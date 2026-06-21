use url::Url;

fn main() {
    // Panic-safe: parse returns Result; handle it instead of unwrap.
    match Url::parse("https://a.b/c?d=e#f") {
        Ok(u) => {
            println!("scheme={}", u.scheme());
            match u.host_str() {
                Some(h) => println!("host={}", h),
                None => println!("host=<none>"),
            }
            println!("path={}", u.path());
            match u.query() {
                Some(q) => println!("query={}", q),
                None => println!("query=<none>"),
            }
            match u.fragment() {
                Some(f) => println!("fragment={}", f),
                None => println!("fragment=<none>"),
            }
        }
        Err(e) => {
            println!("parse error: {}", e);
        }
    }
    println!("== soak_url done ==");
}
