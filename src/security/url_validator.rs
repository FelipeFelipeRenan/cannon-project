use colored::Colorize;

pub fn validate_and_extract(url_option: &Option<String>) -> String {
    // Extrai a URL ou mata o processo de forma graciosa
    let url_str = url_option.clone().unwrap_or_else(|| {
        eprintln!(
            "{} É necessário fornecer uma URL via flag (-u) ou no ficheiro YAML (--config)",
            "❌ Erro:".red().bold()
        );
        std::process::exit(1);
    });

    // Blinda contra SSRF (Server-Side Request Forgery)
    if !url_str.starts_with("http://") && !url_str.starts_with("https://") {
        eprintln!(
            "{} A URL do alvo deve começar com http:// ou https://",
            "❌ Erro Crítico:".red().bold()
        );
        std::process::exit(1);
    }

    url_str
}
