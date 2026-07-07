use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone)]
pub enum Endian {
    Big,
    Little,
}

#[derive(Clone)]
pub enum BinaryType {
    U8,
    U16(Endian),
    U32(Endian),
    U64(Endian),
}

#[derive(Clone)]
pub enum Chunk {
    StaticText(Vec<u8>),

    // Geradores de Texto (Para HTTP/JSON)
    TextRandomNumber,
    TextUuid,
    TextEmail,
    TextUsername,
    TextTimestamp,

    // Geradores Binários (Para TCP)
    BinaryRandomNumber(BinaryType),
    BinaryFixedValue { value: u64, ty: BinaryType },
}

pub struct PayloadTemplate {
    chunks: Vec<Chunk>,
}

impl PayloadTemplate {
    pub fn parse(template: &str) -> Arc<Self> {
        let mut chunks = Vec::new();
        let mut remaining = template;

        // O Parser unificado agora varre a string em busca de qualquer tag {{ ... }}
        while let Some(start_idx) = remaining.find("{{") {
            if start_idx > 0 {
                chunks.push(Chunk::StaticText(
                    remaining.as_bytes()[..start_idx].to_vec(),
                ));
            }

            let end_idx = remaining
                .find("}}")
                .expect("Sintaxe inválida: tag não fechada");
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
                    // Fallback se a tag não for reconhecida, trata como texto estático
                    chunks.push(Chunk::StaticText(format!("{{{{{}}}}}", tag).into_bytes()));
                }
            }

            remaining = &remaining[end_idx + 2..];
        }

        if !remaining.is_empty() {
            chunks.push(Chunk::StaticText(remaining.as_bytes().to_vec()));
        }

        Arc::new(Self { chunks })
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
                        .unwrap()
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
