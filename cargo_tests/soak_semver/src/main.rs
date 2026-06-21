use semver::{Version, VersionReq};

fn main() {
    // Parse a couple of versions (handle Result, no unwrap).
    let a = match Version::parse("1.2.3") {
        Ok(v) => v,
        Err(e) => {
            println!("parse 1.2.3 failed: {}", e);
            return;
        }
    };
    let b = match Version::parse("1.10.0-alpha.1") {
        Ok(v) => v,
        Err(e) => {
            println!("parse 1.10.0-alpha.1 failed: {}", e);
            return;
        }
    };

    println!("a = {} (major={} minor={} patch={})", a, a.major, a.minor, a.patch);
    println!("b = {} (pre={})", b, b.pre.as_str());

    // Compare versions (Ord).
    println!("a < b: {}", a < b);
    println!("a == a: {}", a == a.clone());

    // Parse a requirement and test matches.
    let req = match VersionReq::parse(">=1.2, <2.0") {
        Ok(r) => r,
        Err(e) => {
            println!("req parse failed: {}", e);
            return;
        }
    };
    println!("req = {}", req);
    println!("req matches a (1.2.3): {}", req.matches(&a));

    let c = match Version::parse("2.5.0") {
        Ok(v) => v,
        Err(e) => {
            println!("parse 2.5.0 failed: {}", e);
            return;
        }
    };
    println!("req matches c (2.5.0): {}", req.matches(&c));

    // Caret requirement.
    let caret = match VersionReq::parse("^1.4.2") {
        Ok(r) => r,
        Err(e) => {
            println!("caret parse failed: {}", e);
            return;
        }
    };
    let d = match Version::parse("1.9.9") {
        Ok(v) => v,
        Err(e) => {
            println!("parse 1.9.9 failed: {}", e);
            return;
        }
    };
    println!("caret = {} matches d (1.9.9): {}", caret, caret.matches(&d));

    // Exercise an invalid parse path (handled, not panicking).
    match Version::parse("not.a.version") {
        Ok(v) => println!("unexpectedly parsed: {}", v),
        Err(_) => println!("invalid version correctly rejected"),
    }

    println!("== soak_semver done ==");
}
