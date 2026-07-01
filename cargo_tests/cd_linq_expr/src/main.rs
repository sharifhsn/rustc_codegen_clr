// Building `System.Linq.Expressions` trees from Rust — the shape EF Core / any `IQueryable` provider
// consumes. A provider does NOT run the predicate in-process; it WALKS the tree structure to translate
// it (e.g. to SQL). So this proves the two things a provider needs: (1) the tree's structure is what we
// intended (verified via `Expression.ToString()`), and (2) it is a semantically valid, JIT-compilable
// predicate (verified via `LambdaExpression.Compile()` producing a real, non-null `Func<...>`).
use mycorrhiza::linq::{Expr, Param};
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

    // THE canonical EF filter: a => (a > 5), with a value-type CONSTANT boxed via the `box` primitive.
    let gt5_body = a.expr().gt(Expr::const_i32(5));
    say("gt5 body", &gt5_body.text());
    chk!(gt5_body.text().contains('5'), true);
    chk!(gt5_body.text().contains('>'), true);
    let gt5_lambda = gt5_body.lambda(&[&a]);
    say("gt5 lambda", &gt5_lambda.text());
    chk!(gt5_lambda.compiles(), true); // a real Func<int,bool>: `x => x > 5` is EF-consumable.

    // A composed constant filter: b => ((b >= 18) && (b < 65)) — an age-range predicate.
    let range = b.expr().ge(Expr::const_i32(18)).and(b.expr().lt(Expr::const_i32(65)));
    say("range body", &range.text());
    chk!(range.text().contains("18"), true);
    chk!(range.text().contains("65"), true);
    let range_lambda = range.lambda(&[&b]);
    say("range lambda", &range_lambda.text());
    chk!(range_lambda.compiles(), true);

    // 64-bit constant — x => (x < 1000000000000).
    let big_lambda = x.expr().lt(Expr::const_i64(1_000_000_000_000)).lambda(&[&x]);
    say("big lambda", &big_lambda.text());
    chk!(big_lambda.compiles(), true);

    // MEMBER ACCESS — the realistic EF filter: s => (s.Length > 5). Filters on a PROPERTY of the entity.
    let s = Param::new("System.String", "s");
    let len_body = s.expr().prop("Length").gt(Expr::const_i32(5));
    say("len body", &len_body.text());
    chk!(len_body.text().contains("Length"), true);
    let len_lambda = len_body.lambda(&[&s]);
    say("len lambda", &len_lambda.text());
    chk!(len_lambda.compiles(), true);

    // ACTUAL EXECUTION — compile s => (s.Length > 5) and RUN it. Proves the tree isn't just
    // well-formed, it computes the right answer (what client-side EF evaluation does).
    let len_pred = s.expr().prop("Length").gt(Expr::const_i32(5)).lambda(&[&s]).compile();
    chk!(len_pred.call_str("hello!"), true); // 6 > 5
    chk!(len_pred.call_str("hi"), false); // 2 > 5

    // STRING-EQUALITY filter, executed: name => (name == "target").
    let name = Param::new("System.String", "name");
    let eq_pred = name
        .expr()
        .eq(Expr::const_str("target"))
        .lambda(&[&name])
        .compile();
    say("eq lambda", &name.expr().eq(Expr::const_str("target")).lambda(&[&name]).text());
    chk!(eq_pred.call_str("target"), true);
    chk!(eq_pred.call_str("other"), false);

    // VALUE-TYPE arg, executed: a => (a > 5), invoked with boxed ints.
    let gt5_pred = a.expr().gt(Expr::const_i32(5)).lambda(&[&a]).compile();
    chk!(gt5_pred.call_i32(7), true);
    chk!(gt5_pred.call_i32(3), false);

    Console::writeln_u64(pass as u64);
    Console::writeln_u64(total as u64);
    if pass == total {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
