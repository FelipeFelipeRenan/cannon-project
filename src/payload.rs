use std::time::{SystemTime, UNIX_EPOCH};

use rand::{distr::Alphanumeric, Rng};
use uuid::Uuid;

pub fn process_payload(template: &str) -> String {
    let mut result = template.to_string();

    while result.contains("{{uuid}}") {
        result = result.replacen("{{uuid}}", &Uuid::new_v4().to_string(), 1);
    }

    while result.contains("{{timestamp}}") {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        result = result.replacen("{{timestamp}}", &now.to_string(), 1);
    }

    while result.contains("{{user}}") {
        let random_str: String = rand::rng()
            .sample_iter(&Alphanumeric)
            .take(8)
            .map(char::from)
            .collect();

        result = result.replacen("{{user}}", &random_str.to_lowercase(), 1);
    }

    while result.contains("{{email}}") {
        let random_str: String = rand::rng()
            .sample_iter(&Alphanumeric)
            .take(8)
            .map(char::from)
            .collect();

        let email = format!("{}@example.com", random_str.to_lowercase());
        result = result.replacen("{{email}}", &email, 1);
    }
    while result.contains("{{number}}") {
        let random_num: u32 = rand::rng().random_range(10..9999);
        result = result.replacen("{{number}}", &random_num.to_string(), 1);
    }

    result
}
