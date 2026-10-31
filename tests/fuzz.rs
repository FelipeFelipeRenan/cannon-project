//! Randomised robustness tests: the parsers that handle user input must
//! return errors for garbage, never panic.
//!
//! The generator is seeded, so a failure is reproducible: the panic message
//! of the failing test includes the offending input.

use cannon::args::parser::unescape_delimiter;
use cannon::client::target::StatusFilter;
use cannon::payload::generator::PayloadTemplate;
use cannon::report::gate::Baseline;

const ITERATIONS: usize = 20_000;

/// Characters that matter to the parsers, plus a few awkward ones.
const ALPHABET: &[&str] = &[
    "{",
    "}",
    "{{",
    "}}",
    ":",
    ",",
    "\"",
    "\\",
    "a",
    "z",
    "0",
    "9",
    " ",
    "\n",
    "x",
    "u8",
    "u16",
    "u32",
    "u64",
    "i8",
    "value",
    "number",
    "uuid",
    "email",
    "seq",
    "timestamp",
    "300",
    "-1",
    "99999999999999999999",
    "é",
    "🚀",
    "\u{0}",
    "[",
    "]",
    "xx",
    "2xx",
    "9xx",
];

fn random_text(rng: &mut fastrand::Rng, max_pieces: usize) -> String {
    (0..rng.usize(0..=max_pieces))
        .map(|_| ALPHABET[rng.usize(0..ALPHABET.len())])
        .collect()
}

#[test]
fn payload_templates_never_panic() {
    let mut rng = fastrand::Rng::with_seed(0xCA_0001);
    let mut rendered = Vec::new();

    for _ in 0..ITERATIONS {
        let input = random_text(&mut rng, 24);

        // A panic in `parse` or `render` fails the test and prints `input`.
        if let Ok(template) = PayloadTemplate::parse(&input) {
            rendered.clear();
            template.render(&mut rendered);
        }
    }
}

#[test]
fn nested_json_bodies_with_tags_never_panic() {
    let mut rng = fastrand::Rng::with_seed(0xCA_0002);

    for _ in 0..ITERATIONS / 4 {
        let depth = rng.usize(0..8);
        let tag = ["{{uuid}}", "{{seq}}", "{{value:2:u16}}", "{{bogus}}", "{{"][rng.usize(0..5)];

        let mut body = format!("\"k\":\"{tag}\"");

        for level in 0..depth {
            body = format!("\"n{level}\":{{{body}}}");
        }

        let input = format!("{{{body}}}");

        if let Ok(template) = PayloadTemplate::parse(&input) {
            template.render(&mut Vec::new());
        }
    }
}

#[test]
fn status_filters_never_panic() {
    let mut rng = fastrand::Rng::with_seed(0xCA_0003);

    for _ in 0..ITERATIONS {
        let input = random_text(&mut rng, 10);

        if let Ok(filter) = StatusFilter::parse(&input) {
            for status in [0, 99, 100, 200, 404, 599, 600, u16::MAX] {
                let _ = filter.accepts(status);
            }
        }
    }
}

#[test]
fn delimiter_escapes_never_panic() {
    let mut rng = fastrand::Rng::with_seed(0xCA_0004);

    for _ in 0..ITERATIONS {
        let _ = unescape_delimiter(&random_text(&mut rng, 12));
    }
}

#[test]
fn baseline_files_never_panic() {
    let mut rng = fastrand::Rng::with_seed(0xCA_0005);

    for _ in 0..ITERATIONS / 4 {
        let input = format!(
            "{{\"p99_ms\":{},\"rps\":{},\"error_rate_percent\":{}}}",
            random_text(&mut rng, 3),
            random_text(&mut rng, 3),
            random_text(&mut rng, 3)
        );

        let _ = Baseline::parse(&input);
        let _ = Baseline::parse(&random_text(&mut rng, 12));
    }
}

#[test]
fn generated_emails_and_usernames_do_not_collide() {
    let template = PayloadTemplate::parse("{{email}}|{{username}}").unwrap();
    let mut seen = std::collections::HashSet::new();
    let mut buffer = Vec::new();

    for _ in 0..200_000 {
        buffer.clear();
        template.render(&mut buffer);

        assert!(seen.insert(buffer.clone()), "duplicate generated value");
    }
}
