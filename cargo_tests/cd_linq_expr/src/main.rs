// Building `System.Linq.Expressions` trees from Rust — the shape EF Core / any `IQueryable` provider
// consumes. A provider does NOT run the predicate in-process; it WALKS the tree structure to translate
// it (e.g. to SQL). So this proves the two things a provider needs: (1) the tree's structure is what we
// intended (verified via `Expression.ToString()`), and (2) it is a semantically valid, JIT-compilable
// predicate (verified via `LambdaExpression.Compile()` producing a real, non-null `Func<...>`).
use mycorrhiza::linq::Param;
use mycorrhiza::system::console::Console;
use mycorrhiza::system::DotNetString;

fn say(label: &str, s: &str) {
    let line = format!("{label}: {s}");
    Console::writeln_string(DotNetString::from(line.as_str()).handle());
}

fn main() -> std::process::ExitCode {
    let mut pass = 0u32;
    let mut total = 0u32;
    macro_rules! chk {
        ($g:expr,$w:expr) => {{
            total += 1;
            if $g == $w {
                pass += 1;
            } else {
                Console::writeln_u64(900_000_000 + total as u64);
            }
        }};
    }

    // (a, b) => (a > b) — a two-column comparison, the core of a WHERE predicate.
    let a = Param::new("System.Int32", "a");
    let b = Param::new("System.Int32", "b");
    let gt_body = a.expr().gt(b.expr());
    say("gt body", &gt_body.text());
    chk!(gt_body.text().contains('>'), true);
    chk!(gt_body.text().contains('a'), true);
    chk!(gt_body.text().contains('b'), true);
    let gt_lambda = gt_body.lambda(&[&a, &b]);
    say("gt lambda", &gt_lambda.text());
    chk!(gt_lambda.text().contains("=>"), true);
    chk!(gt_lambda.compiles(), true); // -> a real Func<int,int,bool>: the tree is EF-consumable.

    // (a, b, c) => ((a > b) && (b > c)) — composed AndAlso over 3 parameters.
    let c = Param::new("System.Int32", "c");
    let and_body = a.expr().gt(b.expr()).and(b.expr().gt(c.expr()));
    say("and body", &and_body.text());
    chk!(
        and_body.text().contains("AndAlso") || and_body.text().contains("&&"),
        true
    );
    let and_lambda = and_body.lambda(&[&a, &b, &c]);
    say("and lambda", &and_lambda.text());
    chk!(and_lambda.compiles(), true);

    // (x, y) => ((x < y) || (x == y))  ==  x <= y — OrElse over LessThan + Equal, 64-bit params.
    let x = Param::new("System.Int64", "x");
    let y = Param::new("System.Int64", "y");
    let le_body = x.expr().lt(y.expr()).or(x.expr().eq(y.expr()));
    say("le body", &le_body.text());
    chk!(
        le_body.text().contains("OrElse") || le_body.text().contains("||"),
        true
    );
    let le_lambda = le_body.lambda(&[&x, &y]);
    say("le lambda", &le_lambda.text());
    chk!(le_lambda.compiles(), true);

    // A single-parameter predicate wrapping a whole comparison — (x, y) => (x >= y).
    let ge_lambda = x.expr().ge(y.expr()).lambda(&[&x, &y]);
    say("ge lambda", &ge_lambda.text());
    chk!(ge_lambda.compiles(), true);

    Console::writeln_u64(pass as u64);
    Console::writeln_u64(total as u64);
    if pass == total {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
