use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Byte order used when encoding multi-byte binary values.
#[derive(Clone)]
pub enum Endian {
    /// Most significant byte is stored first.
    Big,

    /// Least significant byte is stored first.
    Little,
}

/// Binary integer representation supported by payload templates.
#[derive(Clone)]
pub enum BinaryType {
    /// Unsigned 8-bit integer.
    U8,

    /// Unsigned 16-bit integer with the specified byte order.
    U16(Endian),

    /// Unsigned 32-bit integer with the specified byte order.
    U32(Endian),

    /// Unsigned 64-bit integer with the specified byte order.
    U64(Endian),
}
impl BinaryType {
    fn max_value(&self) -> u64 {
        match self {
            BinaryType::U8 => u8::MAX as u64,
            BinaryType::U16(_) => u16::MAX as u64,
            BinaryType::U32(_) => u32::MAX as u64,
            BinaryType::U64(_) => u64::MAX,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            BinaryType::U8 => "u8",
            BinaryType::U16(_) => "u16",
            BinaryType::U32(_) => "u32",
            BinaryType::U64(_) => "u64",
        }
    }
}

const VALID_TAGS: &str =
    "number, uuid, email, username, timestamp, seq, number:<type>, value:<n>[:<type>]";

/// A single component of a compiled payload template.
#[derive(Clone)]
pub enum Chunk {
    /// Static bytes copied directly into the rendered payload.
    StaticText(Vec<u8>),

    /// Generates a random numeric value in textual form.
    TextRandomNumber,

    /// Generates a UUID-like textual identifier.
    TextUuid,

    /// Generates a synthetic email address in textual form.
    TextEmail,

    /// Generates a synthetic username in textual form.
    TextUsername,

    /// Generates the current Unix timestamp in milliseconds.
    TextTimestamp,

    /// Generates a counter that starts at 1 and increases by one for every
    /// rendered payload (shared by all workers), which guarantees uniqueness.
    TextSequence,

    /// Generates a random binary integer using the specified representation.
    BinaryRandomNumber(BinaryType),

    /// Writes a fixed binary integer using the specified representation.
    BinaryFixedValue {
        /// Numeric value to encode.
        value: u64,

        /// Binary representation used to encode the value.
        ty: BinaryType,
    },
}

/// A parsed and reusable request payload template.
///
/// A template is compiled into a sequence of [`Chunk`] values and can then
/// be rendered repeatedly. Static portions are reused while dynamic chunks
/// generate new values for each request.
pub struct PayloadTemplate {
    chunks: Vec<Chunk>,
    warnings: Vec<String>,
    sequence: AtomicU64,
}

impl PayloadTemplate {
    /// A reusable request payload template.
    ///
    /// A template is parsed once and can then be rendered repeatedly into a
    /// caller-provided buffer. Dynamic placeholders are evaluated on each
    /// render, making the same template suitable for high-volume load tests.
    ///
    /// # Examples
    ///
    /// ```
    /// use cannon::payload::generator::PayloadTemplate;
    ///
    /// let template = PayloadTemplate::parse(
    ///     r#"{"user_id":"{{uuid}}","email":"{{email}}"}"#
    /// )?;
    ///
    /// let mut buffer = Vec::new();
    /// template.render(&mut buffer);
    ///
    /// assert!(!buffer.is_empty());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error when the template contains an unterminated `{{ ... }}`
    /// placeholder.
    pub fn parse(template: &str) -> Result<Arc<Self>, Box<dyn std::error::Error>> {
        let mut chunks = Vec::new();
        let mut warnings = Vec::new();
        let mut literal: Vec<u8> = Vec::new();
        let bytes = template.as_bytes();
        let mut position = 0;

        while position < template.len() {
            let Some(relative) = template[position..].find("{{") else {
                literal.extend_from_slice(&bytes[position..]);
                break;
            };

            let start = position + relative;

            // `\{{` is an escaped, literal `{{`.
            if start > position && bytes[start - 1] == b'\\' {
                literal.extend_from_slice(&bytes[position..start - 1]);
                literal.extend_from_slice(b"{{");
                position = start + 2;
                continue;
            }

            literal.extend_from_slice(&bytes[position..start]);

            // The closing `}}` must be searched *after* the opening `{{`;
            // otherwise nested JSON such as `{"a":{"b":1}}` breaks parsing.
            let tag_start = start + 2;
            let Some(relative_end) = template[tag_start..].find("}}") else {
                let snippet: String = template[start..].chars().take(24).collect();

                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("invalid template: unclosed tag starting at '{snippet}'"),
                )
                .into());
            };

            let end = tag_start + relative_end;
            let tag = template[tag_start..end].trim();

            match Self::parse_tag(tag)? {
                Some(chunk) => {
                    if !literal.is_empty() {
                        chunks.push(Chunk::StaticText(std::mem::take(&mut literal)));
                    }

                    chunks.push(chunk);
                }
                None => {
                    warnings.push(format!(
                        "unknown template tag '{{{{{tag}}}}}' will be sent literally \
                         (valid tags: {VALID_TAGS}; write \\{{{{ for a literal '{{{{')"
                    ));

                    literal.extend_from_slice(&bytes[start..end + 2]);
                }
            }

            position = end + 2;
        }

        if !literal.is_empty() {
            chunks.push(Chunk::StaticText(literal));
        }

        Ok(Arc::new(Self {
            chunks,
            warnings,
            sequence: AtomicU64::new(0),
        }))
    }

    /// Non-fatal problems found while parsing, such as tags that are not
    /// recognised and will therefore be sent as literal text.
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    fn parse_tag(tag: &str) -> Result<Option<Chunk>, String> {
        let chunk = match tag {
            "number" => Chunk::TextRandomNumber,
            "uuid" => Chunk::TextUuid,
            "email" => Chunk::TextEmail,
            "username" => Chunk::TextUsername,
            "timestamp" => Chunk::TextTimestamp,
            "seq" => Chunk::TextSequence,
            _ if tag.starts_with("number:") => {
                let type_str = tag["number:".len()..].trim();

                Chunk::BinaryRandomNumber(Self::parse_binary_type(type_str, tag)?)
            }
            _ if tag.starts_with("value:") => {
                let mut parts = tag["value:".len()..].splitn(2, ':');
                let raw = parts.next().unwrap_or("").trim();

                let value = raw.parse::<u64>().map_err(|_| {
                    format!(
                        "invalid value '{raw}' in '{{{{{tag}}}}}': expected an unsigned integer"
                    )
                })?;

                let ty = match parts.next() {
                    Some(type_str) => Self::parse_binary_type(type_str.trim(), tag)?,
                    None => BinaryType::U8,
                };

                let max = ty.max_value();

                if value > max {
                    return Err(format!(
                        "value {value} in '{{{{{tag}}}}}' does not fit in {} (maximum {max})",
                        ty.name()
                    ));
                }

                Chunk::BinaryFixedValue { value, ty }
            }
            _ => return Ok(None),
        };

        Ok(Some(chunk))
    }

    fn parse_binary_type(type_str: &str, tag: &str) -> Result<BinaryType, String> {
        match type_str.to_lowercase().as_str() {
            "u8" => Ok(BinaryType::U8),
            "u16be" => Ok(BinaryType::U16(Endian::Big)),
            "u16le" => Ok(BinaryType::U16(Endian::Little)),
            "u32be" => Ok(BinaryType::U32(Endian::Big)),
            "u32le" => Ok(BinaryType::U32(Endian::Little)),
            "u64be" => Ok(BinaryType::U64(Endian::Big)),
            "u64le" => Ok(BinaryType::U64(Endian::Little)),
            other => Err(format!(
                "unknown binary type '{other}' in '{{{{{tag}}}}}' \
                 (expected u8, u16be, u16le, u32be, u32le, u64be or u64le)"
            )),
        }
    }

    /// Renders the template into `buffer`.
    ///
    /// The buffer is cleared before rendering. Reusing the same buffer avoids
    /// allocating a new `Vec<u8>` for every generated request.
    ///
    /// Dynamic values such as `{{uuid}}`, `{{email}}`, and `{{timestamp}}`
    /// are generated during rendering.
    ///
    /// # Examples
    ///
    /// ```
    /// use cannon::payload::generator::PayloadTemplate;
    ///
    /// let template = PayloadTemplate::parse(r#"{"id":"{{uuid}}"}"#)?;
    ///
    /// let mut buffer = Vec::new();
    /// template.render(&mut buffer);
    ///
    /// assert!(buffer.starts_with(b"{\"id\":\""));
    /// assert!(buffer.ends_with(b"\"}"));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[inline(always)]
    pub fn render(&self, buffer: &mut Vec<u8>) {
        buffer.clear();

        for chunk in &self.chunks {
            match chunk {
                Chunk::StaticText(bytes) => buffer.extend_from_slice(bytes),

                // --- INJEÇÕES HTTP / TEXTO ZERO-COPY ---
                Chunk::TextRandomNumber => {
                    let num = fastrand::u32(1..=999999);
                    let mut num_buf = itoa::Buffer::new();
                    buffer.extend_from_slice(num_buf.format(num).as_bytes());
                }
                Chunk::TextTimestamp => {
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis();
                    let mut num_buf = itoa::Buffer::new();
                    buffer.extend_from_slice(num_buf.format(now).as_bytes());
                }
                Chunk::TextSequence => {
                    let next = self.sequence.fetch_add(1, Ordering::Relaxed) + 1;
                    let mut num_buf = itoa::Buffer::new();
                    buffer.extend_from_slice(num_buf.format(next).as_bytes());
                }
                // 64 random bits keep collisions practically impossible even
                // for multi-million request runs (use {{seq}} for a guarantee).
                Chunk::TextUsername => {
                    buffer.extend_from_slice(b"user_");
                    let num = fastrand::u64(..);
                    let mut num_buf = itoa::Buffer::new();
                    buffer.extend_from_slice(num_buf.format(num).as_bytes());
                }
                Chunk::TextEmail => {
                    buffer.extend_from_slice(b"test_");
                    let num = fastrand::u64(..);
                    let mut num_buf = itoa::Buffer::new();
                    buffer.extend_from_slice(num_buf.format(num).as_bytes());
                    buffer.extend_from_slice(b"@loadtest.com");
                }
                Chunk::TextUuid => {
                    // Pseudo-UUID ultra rápido (Garante formato sem custo de criptografia pesada)
                    let p1 = fastrand::u32(0..=0xFFFFFFFF);
                    let p2 = fastrand::u16(0..=0xFFFF);
                    let p3 = fastrand::u16(0..=0x0FFF) | 0x4000; // Versão 4
                    let p4 = fastrand::u16(0..=0x3FFF) | 0x8000; // Variante
                    let p5_1 = fastrand::u32(0..=0xFFFFFFFF);
                    let p5_2 = fastrand::u16(0..=0xFFFF);

                    // Formata direto para bytes ASCII no buffer usando um macete de macros
                    use std::io::Write;
                    let _ = write!(
                        buffer,
                        "{:08x}-{:04x}-{:04x}-{:04x}-{:08x}{:04x}",
                        p1, p2, p3, p4, p5_1, p5_2
                    );
                }

                // --- INJEÇÕES TCP BINÁRIAS ZERO-COPY ---
                Chunk::BinaryRandomNumber(ty) => {
                    Self::write_binary_value(buffer, fastrand::u64(1..=999999), ty);
                }
                Chunk::BinaryFixedValue { value, ty } => {
                    Self::write_binary_value(buffer, *value, ty);
                }
            }
        }
    }

    #[inline(always)]
    fn write_binary_value(buffer: &mut Vec<u8>, val: u64, ty: &BinaryType) {
        match ty {
            BinaryType::U8 => buffer.push(val as u8),
            BinaryType::U16(endian) => {
                let v = val as u16;
                buffer.extend_from_slice(&match endian {
                    Endian::Big => v.to_be_bytes(),
                    Endian::Little => v.to_le_bytes(),
                });
            }
            BinaryType::U32(endian) => {
                let v = val as u32;
                buffer.extend_from_slice(&match endian {
                    Endian::Big => v.to_be_bytes(),
                    Endian::Little => v.to_le_bytes(),
                });
            }
            BinaryType::U64(endian) => {
                buffer.extend_from_slice(&match endian {
                    Endian::Big => val.to_be_bytes(),
                    Endian::Little => val.to_le_bytes(),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unclosed_template_tag() {
        let result = PayloadTemplate::parse(r#"{"id":"{{uuid"}"#);

        assert!(result.is_err());
    }

    #[test]
    fn parses_valid_template() {
        let result = PayloadTemplate::parse(r#"{"id":"{{uuid}}"}"#);

        assert!(result.is_ok());
    }

    #[test]
    fn preserves_unknown_template_tag() {
        let template = PayloadTemplate::parse("before {{unknown}} after").unwrap();

        let mut buffer = Vec::new();
        template.render(&mut buffer);

        assert_eq!(buffer, b"before {{unknown}} after");
    }

    #[test]
    fn renders_static_text() {
        let template = PayloadTemplate::parse("hello world").unwrap();

        let mut buffer = Vec::new();
        template.render(&mut buffer);

        assert_eq!(buffer, b"hello world");
    }

    #[test]
    fn renders_u8_fixed_value() {
        let template = PayloadTemplate::parse("{{value:255:u8}}").unwrap();

        let mut buffer = Vec::new();
        template.render(&mut buffer);

        assert_eq!(buffer, vec![255]);
    }

    #[test]
    fn renders_u16_big_endian() {
        let template = PayloadTemplate::parse("{{value:258:u16be}}").unwrap();

        let mut buffer = Vec::new();
        template.render(&mut buffer);

        assert_eq!(buffer, vec![0x01, 0x02]);
    }

    #[test]
    fn renders_u16_little_endian() {
        let template = PayloadTemplate::parse("{{value:258:u16le}}").unwrap();

        let mut buffer = Vec::new();
        template.render(&mut buffer);

        assert_eq!(buffer, vec![0x02, 0x01]);
    }

    #[test]
    fn renders_u32_big_endian_decimal() {
        let template = PayloadTemplate::parse("{{value:16909060:u32be}}").unwrap();

        let mut buffer = Vec::new();
        template.render(&mut buffer);

        assert_eq!(buffer, vec![0x01, 0x02, 0x03, 0x04]);
    }

    #[test]
    fn renders_u32_little_endian() {
        let template = PayloadTemplate::parse("{{value:16909060:u32le}}").unwrap();

        let mut buffer = Vec::new();
        template.render(&mut buffer);

        assert_eq!(buffer, vec![0x04, 0x03, 0x02, 0x01]);
    }

    #[test]
    fn renders_u64_big_endian() {
        let template = PayloadTemplate::parse("{{value:72623859790382856:u64be}}").unwrap();

        let mut buffer = Vec::new();
        template.render(&mut buffer);

        assert_eq!(buffer, vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]);
    }

    #[test]
    fn renders_u64_little_endian() {
        let template = PayloadTemplate::parse("{{value:72623859790382856:u64le}}").unwrap();

        let mut buffer = Vec::new();
        template.render(&mut buffer);

        assert_eq!(buffer, vec![0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01]);
    }

    #[test]
    fn renders_binary_random_number_with_expected_size() {
        let cases = [
            ("{{number:u8}}", 1),
            ("{{number:u16be}}", 2),
            ("{{number:u16le}}", 2),
            ("{{number:u32be}}", 4),
            ("{{number:u32le}}", 4),
            ("{{number:u64be}}", 8),
            ("{{number:u64le}}", 8),
        ];

        for (template_text, expected_size) in cases {
            let template = PayloadTemplate::parse(template_text).unwrap();

            let mut buffer = Vec::new();
            template.render(&mut buffer);

            assert_eq!(
                buffer.len(),
                expected_size,
                "unexpected size for {template_text}"
            );
        }
    }

    #[test]
    fn renders_random_text_number() {
        let template = PayloadTemplate::parse("{{number}}").unwrap();

        let mut buffer = Vec::new();
        template.render(&mut buffer);

        let value = std::str::from_utf8(&buffer).unwrap();

        assert!(!value.is_empty());
        assert!(value.parse::<u32>().is_ok());
    }

    #[test]
    fn renders_username() {
        let template = PayloadTemplate::parse("{{username}}").unwrap();

        let mut buffer = Vec::new();
        template.render(&mut buffer);

        let value = std::str::from_utf8(&buffer).unwrap();

        assert!(value.starts_with("user_"));
        assert!(value[5..].parse::<u64>().is_ok());
    }

    #[test]
    fn renders_email() {
        let template = PayloadTemplate::parse("{{email}}").unwrap();

        let mut buffer = Vec::new();
        template.render(&mut buffer);

        let value = std::str::from_utf8(&buffer).unwrap();

        assert!(value.starts_with("test_"));
        assert!(value.ends_with("@loadtest.com"));

        let number = &value[5..value.len() - "@loadtest.com".len()];
        assert!(number.parse::<u64>().is_ok());
    }

    #[test]
    fn renders_uuid_with_expected_shape() {
        let template = PayloadTemplate::parse("{{uuid}}").unwrap();

        let mut buffer = Vec::new();
        template.render(&mut buffer);

        let value = std::str::from_utf8(&buffer).unwrap();

        assert_eq!(value.len(), 36);
        assert_eq!(value.as_bytes()[8], b'-');
        assert_eq!(value.as_bytes()[13], b'-');
        assert_eq!(value.as_bytes()[18], b'-');
        assert_eq!(value.as_bytes()[23], b'-');

        assert_eq!(&value[14..15], "4");

        let variant = value.as_bytes()[19];
        assert!(matches!(variant, b'8' | b'9' | b'a' | b'b'));
    }

    #[test]
    fn renders_timestamp_as_unix_milliseconds() {
        let template = PayloadTemplate::parse("{{timestamp}}").unwrap();

        let mut buffer = Vec::new();
        template.render(&mut buffer);

        let value = std::str::from_utf8(&buffer).unwrap();
        let timestamp = value.parse::<u128>();

        assert!(timestamp.is_ok());
    }

    #[test]
    fn render_reuses_buffer_without_leaking_previous_contents() {
        let template = PayloadTemplate::parse("hello").unwrap();

        let mut buffer = b"old data".to_vec();
        template.render(&mut buffer);

        assert_eq!(buffer, b"hello");
    }

    #[test]
    fn renders_multiple_dynamic_values() {
        let template = PayloadTemplate::parse("{{username}}:{{email}}:{{timestamp}}").unwrap();

        let mut buffer = Vec::new();
        template.render(&mut buffer);

        let value = std::str::from_utf8(&buffer).unwrap();

        let parts: Vec<&str> = value.split(':').collect();

        assert_eq!(parts.len(), 3);
        assert!(parts[0].starts_with("user_"));
        assert!(parts[1].starts_with("test_"));
        assert!(parts[1].ends_with("@loadtest.com"));
        assert!(parts[2].parse::<u128>().is_ok());
    }

    fn render(template: &str) -> String {
        let template = PayloadTemplate::parse(template).unwrap();

        let mut buffer = Vec::new();
        template.render(&mut buffer);

        String::from_utf8(buffer).unwrap()
    }

    #[test]
    fn nested_json_before_a_tag_does_not_panic() {
        // Regression: the closing `}}` of the nested object used to be found
        // before the opening `{{` of the tag and caused a slice panic.
        let output = render(r#"{"a":{"b":{"c":1}},"id":"{{seq}}"}"#);

        assert_eq!(output, r#"{"a":{"b":{"c":1}},"id":"1"}"#);
    }

    #[test]
    fn tag_directly_followed_by_closing_braces() {
        assert_eq!(render(r#"{"a":{"b":{{seq}}}}"#), r#"{"a":{"b":1}}"#);
    }

    #[test]
    fn whitespace_inside_tags_is_ignored() {
        assert_eq!(render("{{ seq }}-{{  seq}}"), "1-2");
    }

    #[test]
    fn backslash_escapes_a_literal_opening_brace_pair() {
        let template = PayloadTemplate::parse(r"a \{{seq}} b {{seq}}").unwrap();

        let mut buffer = Vec::new();
        template.render(&mut buffer);

        assert_eq!(buffer, b"a {{seq}} b 1");
        assert!(template.warnings().is_empty());
    }

    #[test]
    fn unknown_tags_are_kept_literally_but_reported() {
        let template = PayloadTemplate::parse(r#"{"v":"{{uuidd}}"}"#).unwrap();

        let mut buffer = Vec::new();
        template.render(&mut buffer);

        assert_eq!(buffer, br#"{"v":"{{uuidd}}"}"#);
        assert_eq!(template.warnings().len(), 1);
        assert!(template.warnings()[0].contains("uuidd"));
    }

    #[test]
    fn sequence_counts_up_from_one() {
        let template = PayloadTemplate::parse("{{seq}}").unwrap();
        let mut buffer = Vec::new();
        let mut seen = Vec::new();

        for _ in 0..3 {
            template.render(&mut buffer);
            seen.push(String::from_utf8(buffer.clone()).unwrap());
        }

        assert_eq!(seen, ["1", "2", "3"]);
    }

    #[test]
    fn sequence_is_unique_across_threads() {
        let template = PayloadTemplate::parse("{{seq}}").unwrap();

        let mut handles = Vec::new();

        for _ in 0..4 {
            let template = template.clone();

            handles.push(std::thread::spawn(move || {
                let mut buffer = Vec::new();
                let mut values = Vec::new();

                for _ in 0..500 {
                    template.render(&mut buffer);
                    values.push(
                        String::from_utf8(buffer.clone())
                            .unwrap()
                            .parse::<u64>()
                            .unwrap(),
                    );
                }

                values
            }));
        }

        let mut all: Vec<u64> = handles
            .into_iter()
            .flat_map(|handle| handle.join().unwrap())
            .collect();

        all.sort_unstable();
        all.dedup();

        assert_eq!(all.len(), 2000);
        assert_eq!(all[0], 1);
        assert_eq!(all[1999], 2000);
    }

    #[test]
    fn rejects_fixed_value_that_does_not_fit_the_type() {
        let error = PayloadTemplate::parse("{{value:300:u8}}").err().unwrap();

        assert!(error.to_string().contains("does not fit in u8"));
        assert!(PayloadTemplate::parse("{{value:65535:u16be}}").is_ok());
        assert!(PayloadTemplate::parse("{{value:65536:u16be}}").is_err());
    }

    #[test]
    fn rejects_non_numeric_fixed_value() {
        let error = PayloadTemplate::parse("{{value:abc:u8}}").err().unwrap();

        assert!(error.to_string().contains("expected an unsigned integer"));
    }

    #[test]
    fn rejects_unknown_binary_type() {
        let error = PayloadTemplate::parse("{{number:u128}}").err().unwrap();

        assert!(error.to_string().contains("unknown binary type 'u128'"));
        assert!(PayloadTemplate::parse("{{value:1:i8}}").is_err());
    }

    #[test]
    fn fixed_value_defaults_to_u8() {
        assert_eq!(
            PayloadTemplate::parse("{{value:7}}")
                .map(|t| {
                    let mut buffer = Vec::new();
                    t.render(&mut buffer);
                    buffer
                })
                .unwrap(),
            vec![7]
        );
    }

    #[test]
    fn unclosed_tag_error_mentions_where_it_starts() {
        let error = PayloadTemplate::parse(r#"{"id":"{{uuid"}"#).err().unwrap();

        assert!(error.to_string().contains("unclosed tag"));
    }
}
