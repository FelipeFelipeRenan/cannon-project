use std::sync::Arc;

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
    TextRandomNumber,
    BinaryRandomNumber(BinaryType),
    BinaryFixedValue { value: u64, ty: BinaryType },
}

pub struct PayloadTemplate {
    chunks: Vec<Chunk>,
}

impl PayloadTemplate {
    pub fn parse(template: &str) -> Arc<Self> {
        let mut chunks = Vec::new();

        // Se o template contém definições explícitas de tipos binários, ativa o parser avançado
        if template.contains("{{number:") || template.contains("{{value:") {
            let mut remaining = template;

            while let Some(start_idx) = remaining.find("{{") {
                if start_idx > 0 {
                    // Texto estático: Fatiamos os bytes DIRETAMENTE como o Clippy mandou
                    chunks.push(Chunk::StaticText(
                        remaining.as_bytes()[..start_idx].to_vec(),
                    ));
                }

                let end_idx = remaining
                    .find("}}")
                    .expect("Sintaxe inválida: tag não fechada");
                let tag = &remaining[start_idx + 2..end_idx];

                if tag.starts_with("number:") {
                    let type_str = tag.strip_prefix("number:").unwrap();
                    chunks.push(Chunk::BinaryRandomNumber(Self::parse_binary_type(type_str)));
                } else if tag.starts_with("value:") {
                    let parts: Vec<&str> = tag.strip_prefix("value:").unwrap().split(':').collect();
                    let val = parts[0].parse::<u64>().unwrap_or(0);
                    let type_str = parts.get(1).copied().unwrap_or("u8");
                    chunks.push(Chunk::BinaryFixedValue {
                        value: val,
                        ty: Self::parse_binary_type(type_str),
                    });
                }

                remaining = &remaining[end_idx + 2..];
            }

            if !remaining.is_empty() {
                chunks.push(Chunk::StaticText(remaining.as_bytes().to_vec()));
            }
        } else {
            // Fallback para o modo Texto puro (HTTP tradicional / strings sobre TCP)
            let parts: Vec<&str> = template.split("{{number}}").collect();
            for (i, part) in parts.iter().enumerate() {
                if !part.is_empty() {
                    chunks.push(Chunk::StaticText(part.as_bytes().to_vec()));
                }
                if i < parts.len() - 1 {
                    chunks.push(Chunk::TextRandomNumber);
                }
            }
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
                Chunk::TextRandomNumber => {
                    let num = fastrand::u32(1..=9999);
                    let mut num_buf = itoa::Buffer::new();
                    buffer.extend_from_slice(num_buf.format(num).as_bytes());
                }
                Chunk::BinaryRandomNumber(ty) => {
                    let num = fastrand::u64(1..=999999);
                    Self::write_binary_value(buffer, num, ty);
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
