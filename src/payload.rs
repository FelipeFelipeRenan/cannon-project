use rand::{distributions::Alphanumeric, Rng};

pub fn process_payload(template: &str) -> String {
    let mut result = template.to_string();

    while result.contains("{{user}}") {
        let random_str: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(8)
            .map(char::from)
            .collect();

        result = result.replacen("{{user}}", &random_str.to_lowercase(), 1);
        if result.contains("{{email}}") {
            result = result.replacen("{{email}}", &random_str.to_lowercase(), 1);
        }
    }

    while result.contains("{{number}}") {
        let random_num: u32 = rand::thread_rng().gen_range(10..9999);
        result = result.replacen("{{number}}", &random_num.to_string(), 1);
    }

    result
}
