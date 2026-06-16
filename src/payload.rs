use std::time::{SystemTime, UNIX_EPOCH};

use rand::{distr::Alphanumeric, RngExt};
use uuid::Uuid;
use std::fmt::Write;

pub fn process_payload(template: &str) -> String {
    if !template.contains("{{"){
        return template.to_string();
    }

    let mut result = String::with_capacity(template.len() + 128);
    let mut rest = template;

    while let Some(start) = rest.find("{{"){
        result.push_str(&rest[..start]);
        rest = &rest[start + 2..];

        if let Some(end) = rest.find("}}") {
            let tag = &rest[..end];
            match tag {
                "uuid" => {
                    let _ = write!(result, "{}", Uuid::new_v4());
                }
                "timestamp" => {
                    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
                    let _ = write!(result, "{}", now);
                }
                "user" | "random" => {
                    let random_str: String = rand::rng().sample_iter(&Alphanumeric).take(8).map(char::from).collect();
                    result.push_str(&random_str.to_lowercase());
                }
                "email" =>{
                    let random_str: String = rand::rng().sample_iter(&Alphanumeric).take(8).map(char::from).collect();
                    result.push_str(&random_str.to_lowercase());
                    result.push_str("@example.com");
                }
                "number" => {
                    let random_num: u32 = rand::rng().random_range(10..9999);
                    let _ = write!(result, "{}", random_num);
                }

                _ =>{
                    result.push_str("{{");
                    result.push_str(tag);
                    result.push_str("}}");
                }
            }
            rest = &rest[end + 2..];
        }else {
            result.push_str("{{");
            break;
        }
    }
    result.push_str(rest);
    result
}
