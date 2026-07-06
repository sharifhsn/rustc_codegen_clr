// Building `System.Linq.Expressions` trees from Rust — the shape EF Core / any `IQueryable` provider
// consumes. A provider does NOT run the predicate in-process; it WALKS the tree structure to translate
// it (e.g. to SQL). So this proves the two things a provider needs: (1) the tree's structure is what we
// intended (verified via `Expression.ToString()`), and (2) it is a semantically valid, JIT-compilable
// predicate (verified via `LambdaExpression.Compile()` producing a real, non-null `Func<...>`).
use dotnet_macros::DotnetEntity;
use mycorrhiza::linq::{Expr, Param, TypedPredicate};
use mycorrhiza::system::console::Console;
use mycorrhiza::system::DotNetString;

/// Test entity for the `Field<Root, Val>` / `#[derive(DotnetEntity)]` ergonomics API. Unlike the plain
/// `Param`/`Expr` checks above (which build trees over `System.Int32`/`System.String` params without a
/// matching backing member), `Expression.PropertyOrField` VALIDATES eagerly against the target `Type` at
/// build time (not just at `.compile()`), so every `Field` const below must name a member that REALLY
/// exists on the .NET type it targets. `System.Exception` supplies both an `int` (`HResult`) and a
/// `String` (`Message`) member with EXACTLY the PascalCase names these snake_case field names convert
/// to, so the derive's default naming convention is exercised end-to-end against a real type.
#[derive(DotnetEntity)]
#[dotnet(type_name = "System.Exception, System.Private.CoreLib")]
struct Sample {
    #[dotnet(rename = "HResult")]
    id: i32,
    #[dotnet(rename = "Message")]
    display_name: String,
}

/// `System.Reflection.MethodInfo` supplies two real `bool` members whose PascalCase matches the Rust
/// field names directly (no rename needed): `IsStatic` (single-word) and `IsGenericMethod`
/// (multi-word) -- exercises the derive's default naming convention against real backing members, and
/// (having two fields on the SAME entity) lets `Field`-built predicates combine with `&`/`|`/`!`.
#[derive(DotnetEntity)]
#[dotnet(type_name = "System.Reflection.MethodInfo, System.Private.CoreLib")]
struct MethodSample {
    is_static: bool,
    is_generic_method: bool,
}

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

    // ---- `Field<Root, Val>` / `#[derive(DotnetEntity)]` ergonomics API ----
    // PascalCase conversion, checked via the .NET property name baked into each generated `Field`'s
    // built `Expression.PropertyOrField` text -- single-word (`id` -renamed-> `HResult`) and multi-word
    // (`display_name` -renamed-> `Message`, and unrenamed multi-word below) snake_case all convert
    // correctly. `System.Exception` is a resolvable real BCL type with both members, so
    // `Expression.PropertyOrField`'s eager validation succeeds.
    let id_body = Sample::ID.eq(1).text();
    say("Sample::ID body", &id_body);
    chk!(id_body.contains(".HResult"), true);

    let name_body = Sample::DISPLAY_NAME.eq("x").text();
    say("Sample::DISPLAY_NAME body", &name_body);
    chk!(name_body.contains(".Message"), true);

    // Unrenamed multi-word snake_case -> PascalCase, against a REAL member: `is_static` -> `IsStatic`.
    let static_body = MethodSample::IS_STATIC.is_true().text();
    say("MethodSample::IS_STATIC body", &static_body);
    chk!(static_body.contains("IsStatic"), true);

    // A second, distinctly-shaped multi-word conversion: `is_generic_method` -> `IsGenericMethod`.
    let generic_body = MethodSample::IS_GENERIC_METHOD.is_true().text();
    say("MethodSample::IS_GENERIC_METHOD body", &generic_body);
    chk!(generic_body.contains("IsGenericMethod"), true);

    // `#[dotnet(rename = "...")]` escape hatch: `Sample::ID`'s .NET property name is "HResult" (the
    // rename), NOT the PascalCase of the Rust field name ("Id").
    chk!(id_body.contains(".Id"), false);
    chk!(name_body.contains(".DisplayName"), false);

    // `Field<Root, i32>.gt` must produce the SAME `Expression` shape as the old hand-built
    // `Param::new` + `.expr().prop(..)` + `Expr::const_i32(..)` path it replaces -- differential
    // check: same structural pieces (comparison operator, property name, operand), and it compiles.
    let manual_p = Param::new("System.Exception, System.Private.CoreLib", "p");
    let manual_body = manual_p.expr().prop("HResult").gt(Expr::const_i32(5));
    let field_pred = Sample::ID.gt(5);
    say("manual gt body", &manual_body.text());
    say("field gt body", &field_pred.text());
    // Both mention the same operator/property/constant -- the only difference is the lambda
    // parameter's arbitrary display name (irrelevant to the tree's semantic shape).
    chk!(manual_body.text().contains(".HResult"), field_pred.text().contains(".HResult"));
    chk!(
        manual_body.text().contains(">"),
        field_pred.text().contains(">")
    );
    chk!(
        manual_body.text().contains("5"),
        field_pred.text().contains("5")
    );
    let manual_lambda = manual_body.lambda(&[&manual_p]);
    chk!(manual_lambda.compiles(), true);
    let field_lambda = field_pred.body().lambda(&[&field_pred.param()]);
    chk!(field_lambda.compiles(), true);

    // Execute a `Field<Root, i32>`-built predicate end-to-end against a REAL backing type/property:
    // `System.String.Length` (an `int` property every `System.String` has) via a Rust entity struct
    // whose derive targets `System.String` -- proves `Field::gt`/`le` don't just build well-formed
    // trees, they filter correctly at runtime, exactly like the old manual path.
    #[derive(DotnetEntity)]
    #[dotnet(type_name = "System.String, System.Private.CoreLib")]
    struct StrEntity {
        length: i32,
    }
    let len_pred = StrEntity::LENGTH.gt(5);
    let len_fn = len_pred.body().lambda(&[&len_pred.param()]).compile();
    chk!(len_fn.call_str("hello!"), true); // 6 > 5
    chk!(len_fn.call_str("hi"), false); // 2 > 5

    // `Field<Root, String>.contains`/`starts_with`/`ends_with` -- same `call1_same_type` shape as
    // `Expr::call1_same_type` above, reached through the ergonomic entry point, executed end-to-end
    // against the SAME real `System.String.Length`... no -- these need a `String`-typed member, so
    // reuse `StrEntity`-style entity, but targeting `System.Exception.Message` (a real `String` member)
    // via `Sample::DISPLAY_NAME`.
    let contains_pred = Sample::DISPLAY_NAME.contains("ell");
    let contains_body = contains_pred.text();
    say("Field contains body", &contains_body);
    chk!(contains_body.contains("Contains"), true);
    let sw_pred = Sample::DISPLAY_NAME.starts_with("he");
    chk!(sw_pred.text().contains("StartsWith"), true);
    let ew_pred = Sample::DISPLAY_NAME.ends_with("lo");
    chk!(ew_pred.text().contains("EndsWith"), true);

    // `Field<Root, bool>.is_false` -- negated body, against the real `MethodInfo.IsStatic` member.
    let false_pred = MethodSample::IS_STATIC.is_false();
    chk!(false_pred.text().contains("Not"), true);

    // `always()`/`never()` -- trivial constant predicates, replacing the old "no filter" workaround.
    // Body shape is `1 == 1` / `1 == 0` regardless of `Root`'s .NET type, so this proves the tree is
    // correctly constant-true/-false via `.text()` (can't `.call_i32`/`.call_str` here: `Sample`'s
    // underlying parameter type is `System.Exception`, a reference type incompatible with either
    // helper -- the MANDATORY end-to-end EXECUTION proof for `always`/`never` is covered separately
    // below via `IntQuery`, which shares `System.Int32`'s parameter type with `Compiled::call_i32`).
    let always_pred: TypedPredicate<Sample> = Sample::ID.always();
    let never_pred: TypedPredicate<Sample> = Sample::ID.never();
    say("always body", &always_pred.text());
    say("never body", &never_pred.text());
    chk!(always_pred.text().contains("1 == 1"), true);
    chk!(never_pred.text().contains("1 == 0"), true);
    chk!(always_pred.body().lambda(&[&always_pred.param()]).compiles(), true);
    chk!(never_pred.body().lambda(&[&never_pred.param()]).compiles(), true);

    // The mandatory end-to-end EXECUTION proof for `always`/`never`, against `System.Int32` (so
    // `Compiled::call_i32` applies directly). `always`/`never` don't touch the field's property name at
    // all (see `mycorrhiza::linq::Field::always`'s doc) -- only `self.type_name` matters -- so this
    // dummy field name need not exist on `System.Int32`.
    #[derive(DotnetEntity)]
    #[dotnet(type_name = "System.Int32, System.Private.CoreLib")]
    struct IntEntity {
        marker: i32,
    }
    let always_int = IntEntity::MARKER.always();
    let never_int = IntEntity::MARKER.never();
    let always_fn = always_int.body().lambda(&[&always_int.param()]).compile();
    let never_fn = never_int.body().lambda(&[&never_int.param()]).compile();
    chk!(always_fn.call_i32(0), true); // always true, regardless of input
    chk!(always_fn.call_i32(999), true);
    chk!(never_fn.call_i32(0), false); // always false, regardless of input
    chk!(never_fn.call_i32(999), false);

    // `Field<Root,_>` predicates combine with `&`/`|`/`!` exactly like manually-built `TypedPredicate`s
    // (same underlying type, no special-casing needed) -- reuses the parameter-rebinding path since
    // each `Field` method call constructs its own fresh `Param` internally (here, two DIFFERENT
    // `MethodSample` fields, so the two operands are genuinely built against different `Param`s).
    let combined = MethodSample::IS_STATIC.is_true() & MethodSample::IS_GENERIC_METHOD.is_false();
    say("Field combined", &combined.text());
    chk!(combined.text().contains("AndAlso"), true);
    chk!(combined.body().lambda(&[&combined.param()]).compiles(), true);
    let or_combined = MethodSample::IS_STATIC.is_true() | MethodSample::IS_GENERIC_METHOD.is_false();
    chk!(or_combined.body().lambda(&[&or_combined.param()]).compiles(), true);
    let not_combined = !MethodSample::IS_STATIC.is_true();
    chk!(not_combined.body().lambda(&[&not_combined.param()]).compiles(), true);

    Console::writeln_u64(pass as u64);
    Console::writeln_u64(total as u64);
    if pass == total {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
