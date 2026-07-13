//! A complete, intentionally small Rust-on-.NET application.
//!
//! JSON parsing is performed by the managed `System.Text.Json` implementation through
//! `mycorrhiza`; aggregation and CLI behavior are ordinary Rust.

use mycorrhiza::bcl::json::Json;

const SAMPLE: &str = include_str!("../sample/issues.json");

fn fatal(message: &str) -> ! {
    eprintln!("error: {message}");
    std::process::exit(1)
}

fn required_string(node: &Json, field: &str, context: &str) -> String {
    match node.get(field).and_then(|value| value.as_str()) {
        Some(value) => value,
        None => fatal(&format!("{context} is missing string field `{field}`")),
    }
}

fn load_source() -> (String, String) {
    match std::env::args().nth(1) {
        Some(path) => match std::fs::read_to_string(&path) {
            Ok(source) => (source, path),
            Err(error) => fatal(&format!("could not read {path}: {error}")),
        },
        None => (SAMPLE.to_owned(), "bundled sample".to_owned()),
    }
}

fn main() {
    let (source, source_name) = load_source();
    let document = match Json::parse(&source) {
        Some(document) => document,
        None => fatal(&format!("invalid JSON in {source_name}")),
    };
    let project = required_string(&document, "project", "document");
    let issues = match document.get("issues") {
        Some(issues) => issues,
        None => fatal("document is missing array field `issues`"),
    };

    let total = issues.len();
    let mut open = 0;
    let mut high_priority_open = 0;
    let mut owners: Vec<(String, u32)> = Vec::new();

    for index in 0..total {
        let issue = match issues.index(index) {
            Some(issue) => issue,
            None => fatal(&format!("issues[{index}] is missing")),
        };
        let context = format!("issues[{index}]");
        let status = required_string(&issue, "status", &context);
        if status == "closed" {
            continue;
        }

        open += 1;
        let severity = required_string(&issue, "severity", &context);
        if severity == "high" {
            high_priority_open += 1;
        }

        let owner = required_string(&issue, "owner", &context);
        match owners.iter_mut().find(|(name, _)| *name == owner) {
            Some((_, count)) => *count += 1,
            None => owners.push((owner, 1)),
        }
    }

    owners.sort_by(|left, right| left.0.cmp(&right.0));
    println!("Project: {project}");
    println!("Issues: {total} total, {open} open, {high_priority_open} high-priority open");
    println!("Owners:");
    for (owner, count) in owners {
        println!("- {owner}: {count} open");
    }
}
