use std::time::{SystemTime, UNIX_EPOCH};

use rand::{distr::Alphanumeric, RngExt};
use std::fmt::Write;
use uuid::Uuid;

pub fn process_payload(template: &str) -> String {
    if !template.contains("{{") {
        return template.to_string();
    }

    let mut result = String::with_capacity(template.len() + 128);
    let mut rest = template;

    while let Some(start) = rest.find("{{") {
        result.push_str(&rest[..start]);
        rest = &rest[start + 2..];

        if let Some(end) = rest.find("}}") {
            let tag = &rest[..end];
            match tag {
                "uuid" => {
                    let _ = write!(result, "{}", Uuid::new_v4());
                }
                "timestamp" => {
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_millis();
                    let _ = write!(result, "{}", now);
                }
                "user" | "random" => {
                    let random_str: String = rand::rng()
                        .sample_iter(&Alphanumeric)
                        .take(8)
                        .map(char::from)
                        .collect();
                    result.push_str(&random_str.to_lowercase());
                }
                "email" => {
                    let random_str: String = rand::rng()
                        .sample_iter(&Alphanumeric)
                        .take(8)
                        .map(char::from)
                        .collect();
                    result.push_str(&random_str.to_lowercase());
                    result.push_str("@example.com");
                }
                "number" => {
                    let random_num: u32 = rand::rng().random_range(10..9999);
                    let _ = write!(result, "{}", random_num);
                }

                _ => {
                    result.push_str("{{");
                    result.push_str(tag);
                    result.push_str("}}");
                }
            }
            rest = &rest[end + 2..];
        } else {
            result.push_str("{{");
            break;
        }
    }
    result.push_str(rest);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_tags_remains_unchanged() {
        let input = r#"{"name": "felipe", "role": "engineer"}"#;
        let output = process_payload(input);
        assert_eq!(input, output, "Payload sem tags não deve ser alterado");
    }

    #[test]
    fn test_uuid_replacement() {
        let input = r#"{"id": "{{uuid}}"}"#;
        let output = process_payload(input);
        assert!(
            !output.contains("{{uuid}}"),
            "A tag UUID deve ser substituída"
        );
        assert!(
            output.len() > input.len(),
            "O payload final deve ser maior que o original"
        );
        // Valida se o formato parece um UUID (ex: 550e8400-e29b-41d4-a716-446655440000)
        assert_eq!(output.matches('-').count(), 4);
    }

    #[test]
    fn test_email_replacement() {
        let input = r#"{"email": "{{email}}"}"#;
        let output = process_payload(input);
        assert!(
            !output.contains("{{email}}"),
            "A tag email deve ser substituída"
        );
        assert!(
            output.contains("@example.com"),
            "O email gerado deve conter o domínio base"
        );
    }

    #[test]
    fn test_multiple_tags() {
        let input = r#"{"id": "{{uuid}}", "user": "{{user}}", "age": {{number}}}"#;
        let output = process_payload(input);

        assert!(!output.contains("{{uuid}}"));
        assert!(!output.contains("{{user}}"));
        assert!(!output.contains("{{number}}"));

        // Verifica se manteve a estrutura JSON
        assert!(output.starts_with(r#"{"id": ""#));
        assert!(output.ends_with("}"));
    }

    #[test]
    fn test_unknown_tag_is_ignored() {
        let input = r#"{"value": "{{unknown}}"}"#;
        let output = process_payload(input);
        // O parser deve simplesmente ignorar a tag desconhecida e mantê-la intacta
        assert_eq!(output, r#"{"value": "{{unknown}}"}"#);
    }
}
