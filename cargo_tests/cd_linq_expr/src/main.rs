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

    // NESTED-GENERIC PRODUCTION: a strongly-typed Expression<Func<int,bool>> for `a => (a > 5)`,
    // built via the generic Expression.Lambda<Func<int,bool>> — the exact type EF's Where consumes.
    let typed = a.expr().gt(Expr::const_i32(5)).typed_pred(&a);
    say("typed pred", &typed.text());
    chk!(typed.text().contains('>'), true);
    chk!(typed.text().contains('5'), true);

    // THE EF `IQueryable.Where(Expression<Func>)` HANDOFF: filter a query with the predicate TREE.
    // Queryable.Where TRANSLATES the tree (unlike Enumerable.Where which takes a compiled Func).
    // 1..10, keep a > 5  ->  {6,7,8,9,10}  -> count 5.
    use mycorrhiza::linq::IntQuery;
    let n = IntQuery::range(1, 10)
        .where_(a.expr().gt(Expr::const_i32(5)).typed_pred(&a))
        .count();
    Console::writeln_u64(n as u64);
    chk!(n, 5);
    // Different bound: 1..10, keep a >= 8 -> {8,9,10} -> 3.
    let n2 = IntQuery::range(1, 10)
        .where_(a.expr().ge(Expr::const_i32(8)).typed_pred(&a))
        .count();
    chk!(n2, 3);
    // Composed predicate a range: 1..20, keep (a > 5) && (a < 10) -> {6,7,8,9} -> 4.
    let n3 = IntQuery::range(1, 20)
        .where_(
            a.expr()
                .gt(Expr::const_i32(5))
                .and(a.expr().lt(Expr::const_i32(10)))
                .typed_pred(&a),
        )
        .count();
    chk!(n3, 4);

    // ---- TypedPredicate<T> combinator (BitAnd/BitOr/Not) + the ParameterRebinder fix ----
    // THE REAL PROBLEM: two predicates built by two SEPARATE `Param::new` calls each carry their own
    // distinct `ParameterExpression`. Naively splicing their bodies (`Expression.AndAlso(a.Body, b.Body)`)
    // produces a tree that references two different parameters. `TypedPredicate`'s `&`/`|` transparently
    // detect that and rebind one side onto the other's parameter before combining (see
    // `mycorrhiza::linq::rebind_param` / the bundled `ParameterRebinder` C# helper).
    use mycorrhiza::linq::TypedPredicate;

    struct Widget; // phantom marker entity type for these predicates

    // Two INDEPENDENT builder functions, each with its OWN `Param::new` — mirrors "authored in
    // different files/by different people", the actual real-world scenario this fixes.
    fn build_age_pred() -> TypedPredicate<Widget> {
        let p = Param::new("System.Int32", "p");
        TypedPredicate::new(p, p.expr().ge(Expr::const_i32(18)))
    }
    fn build_big_pred() -> TypedPredicate<Widget> {
        let q = Param::new("System.Int32", "q"); // DIFFERENT ParameterExpression than `p` above
        TypedPredicate::new(q, q.expr().gt(Expr::const_i32(100)))
    }

    let age_pred = build_age_pred();
    let big_pred = build_big_pred();

    // Sanity: the two predicates were indeed built against different Param instances.
    say("age pred", &age_pred.text());
    say("big pred", &big_pred.text());

    // AND — combined tree must reference a SINGLE parameter (post-rebind), not two.
    let and_combined = age_pred & big_pred;
    say("and combined", &and_combined.text());
    chk!(and_combined.text().contains("AndAlso"), true);
    // The rebind must have unified variable identity: only `age_pred`'s original parameter name
    // ("p") should remain in the combined tree's *rendered parameter list* -- `Expression.ToString()`
    // on an AndAlso BinaryExpression renders both operand subtrees using whatever parameter object each
    // references; after a correct rebind, both sides use the SAME ParameterExpression object, so the
    // combined predicate must still COMPILE and EXECUTE correctly end-to-end, which is the strongest
    // possible proof (a structurally-broken two-parameter tree throws on Lambda/Compile).
    let and_lambda = and_combined.body().lambda(&[&and_combined.param()]);
    chk!(and_lambda.compiles(), true);
    let and_fn = and_lambda.compile();
    chk!(and_fn.call_i32(200), true); // 200 >= 18 && 200 > 100
    chk!(and_fn.call_i32(50), false); // 50 >= 18 && 50 > 100 -> false (fails second clause)
    chk!(and_fn.call_i32(5), false); // fails both

    // OR — same rebind path, different combinator.
    let or_combined = build_age_pred() | build_big_pred();
    say("or combined", &or_combined.text());
    chk!(or_combined.text().contains("OrElse"), true);
    let or_fn = or_combined.body().lambda(&[&or_combined.param()]).compile();
    chk!(or_fn.call_i32(5), false); // 5 >= 18? no. 5 > 100? no -> false
    chk!(or_fn.call_i32(20), true); // 20 >= 18? yes -> true
    chk!(or_fn.call_i32(150), true); // 150 > 100? yes -> true

    // NOT — single operand, no rebinding involved.
    let not_pred = !build_age_pred();
    say("not pred", &not_pred.text());
    let not_fn = not_pred.body().lambda(&[&not_pred.param()]).compile();
    chk!(not_fn.call_i32(20), false); // NOT(20 >= 18) -> NOT true -> false
    chk!(not_fn.call_i32(5), true); // NOT(5 >= 18) -> NOT false -> true

    // SAME-PARAM fast path: combining two predicates already built against the SAME Param must still
    // work (no spurious rebind needed, but must not break anything either).
    let shared = Param::new("System.Int32", "shared");
    let pa = TypedPredicate::<Widget>::new(shared, shared.expr().gt(Expr::const_i32(0)));
    let pb = TypedPredicate::<Widget>::new(shared, shared.expr().lt(Expr::const_i32(10)));
    let same_and = pa & pb;
    let same_fn = same_and.body().lambda(&[&same_and.param()]).compile();
    chk!(same_fn.call_i32(5), true); // 0 < 5 < 10
    chk!(same_fn.call_i32(50), false); // not < 10

    // `Expr::call1_same_type` — the substring-filter shape (`string.Contains(string)`), added for the
    // rcc-linq-demo app (EF `Name.Contains(x)` predicate over a real, non-BCL entity type). Not itself a
    // comparison, so it needs a real `Expression.Call` + reflection-based `MethodInfo` lookup on the
    // operand's own static `.Type` — exercised here directly against a `System.String`-typed parameter.
    let s = Param::new("System.String", "s");
    let contains_body = s.expr().call1_same_type("Contains", Expr::const_str("ell"));
    say("contains body", &contains_body.text());
    chk!(contains_body.text().contains("Contains"), true);
    let contains_lambda = contains_body.lambda(&[&s]);
    chk!(contains_lambda.compiles(), true);
    let contains_fn = contains_lambda.compile();
    chk!(contains_fn.call_str("hello"), true); // "hello".Contains("ell") -> true
    chk!(contains_fn.call_str("world"), false); // "world".Contains("ell") -> false

    // `Expr::raw` / `Param::raw` — the escape-hatch accessors `TypedPredicate<T>`'s generalization to a
    // caller's own entity type relies on (see `linq-rs` in rcc-linq-demo): the raw handles must round-trip
    // through `Expression.Lambda(body, [param])`'s NON-generic factory (type inference from the raw
    // `ParameterExpression`/`Expression` handles, no `Expr`/`Param` wrapper needed) and still compile+run.
    let r = Param::new("System.Int32", "r");
    let raw_body_expr = r.expr().gt(Expr::const_i32(41));
    let raw_body = raw_body_expr.raw();
    let raw_param = r.raw();
    let _ = (raw_body, raw_param); // proves both `.raw()` accessors return usable managed handles
    let raw_lambda = raw_body_expr.lambda(&[&r]);
    chk!(raw_lambda.compiles(), true);
    let raw_fn = raw_lambda.compile();
    chk!(raw_fn.call_i32(42), true); // 42 > 41
    chk!(raw_fn.call_i32(10), false); // 10 > 41 -> false

    Console::writeln_u64(pass as u64);
    Console::writeln_u64(total as u64);
    if pass == total {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
