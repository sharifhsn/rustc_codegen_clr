use regex::Regex;

fn main() {
    // Build the regex. Avoid unwrap/expect that could panic on a bad pattern.
    let re = match Regex::new(r"(\w+)@(\w+)") {
        Ok(r) => r,
        Err(e) => {
            println!("regex compile error: {e}");
            println!("== soak_regex done ==");
            return;
        }
    };

    let text = "contact alice@wonderland or bob@builders today";

    // is_match
    println!("is_match: {}", re.is_match(text));

    // captures: pull out the two groups, handling Option safely.
    match re.captures(text) {
        Some(caps) => {
            let whole = caps.get(0).map(|m| m.as_str()).unwrap_or("<none>");
            let user = caps.get(1).map(|m| m.as_str()).unwrap_or("<none>");
            let domain = caps.get(2).map(|m| m.as_str()).unwrap_or("<none>");
            println!("first match: whole='{whole}' user='{user}' domain='{domain}'");
        }
        None => println!("first match: <none>"),
    }

    // captures_iter: walk every match.
    let mut count = 0usize;
    for caps in re.captures_iter(text) {
        let user = caps.get(1).map(|m| m.as_str()).unwrap_or("<none>");
        let domain = caps.get(2).map(|m| m.as_str()).unwrap_or("<none>");
        println!("match {count}: {user} / {domain}");
        count += 1;
    }
    println!("total matches: {count}");

    // replace_all with a closure referencing capture groups.
    let replaced = re.replace_all(text, "$2.$1");
    println!("replaced: {replaced}");

    println!("== soak_regex done ==");
}
