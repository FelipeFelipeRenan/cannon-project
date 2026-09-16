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
        let mut remaining = template;

        // O Parser unificado agora varre a string em busca de qualquer tag {{ ... }}
        while let Some(start_idx) = remaining.find("{{") {
            if start_idx > 0 {
                chunks.push(Chunk::StaticText(
                    remaining.as_bytes()[..start_idx].to_vec(),
                ));
            }

            let end_idx = remaining.find("}}").ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Invalid syntax: unclosed tag",
                )
            })?;
            let tag = &remaining[start_idx + 2..end_idx];

            match tag {
                "number" => chunks.push(Chunk::TextRandomNumber),
                "uuid" => chunks.push(Chunk::TextUuid),
                "email" => chunks.push(Chunk::TextEmail),
                "username" => chunks.push(Chunk::TextUsername),
                "timestamp" => chunks.push(Chunk::TextTimestamp),
                _ if tag.starts_with("number:") => {
                    let type_str = tag.strip_prefix("number:").unwrap();
                    chunks.push(Chunk::BinaryRandomNumber(Self::parse_binary_type(type_str)));
                }
                _ if tag.starts_with("value:") => {
                    let parts: Vec<&str> = tag.strip_prefix("value:").unwrap().split(':').collect();
                    let val = parts[0].parse::<u64>().unwrap_or(0);
                    let type_str = parts.get(1).copied().unwrap_or("u8");
                    chunks.push(Chunk::BinaryFixedValue {
                        value: val,
                        ty: Self::parse_binary_type(type_str),
                    });
                }
                _ => {
                    // Fallback if tag is not recognized, treat as static text
                    chunks.push(Chunk::StaticText(format!("{{{{{}}}}}", tag).into_bytes()));
                }
            }

            remaining = &remaining[end_idx + 2..];
        }

        if !remaining.is_empty() {
            chunks.push(Chunk::StaticText(remaining.as_bytes().to_vec()));
        }

        Ok(Arc::new(Self { chunks }))
    }

    fn parse_binary_type(s: &str) -> BinaryType {
        match s.to_lowercase().as_str() {
            "u8" => BinaryType::U8,
            "u16be" => BinaryType::U16(Endian::Big),
            "u16le" => BinaryType::U16(Endian::Little),
            "u32be" => BinaryType::U32(Endian::Big),
            "u32le" => BinaryType::U32(Endian::Little),
            "u64be" => BinaryType::U64(Endian::Big),
            "u64le" => BinaryType::U64(Endian::Little),
            _ => BinaryType::U8,
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
                Chunk::TextUsername => {
                    buffer.extend_from_slice(b"user_");
                    let num = fastrand::u32(1000..=99999);
                    let mut num_buf = itoa::Buffer::new();
                    buffer.extend_from_slice(num_buf.format(num).as_bytes());
                }
                Chunk::TextEmail => {
                    buffer.extend_from_slice(b"test_");
                    let num = fastrand::u32(1000..=99999);
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
        assert!(value[5..].parse::<u32>().is_ok());
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
        assert!(number.parse::<u32>().is_ok());
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
}
