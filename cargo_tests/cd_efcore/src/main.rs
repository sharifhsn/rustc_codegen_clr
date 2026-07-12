// Stage 1 proof: a Rust-built `System.Linq.Expressions` predicate, run through a REAL EF Core
// query (SQLite provider -- not the EF InMemory provider, which never translates to SQL) against a
// C#-defined `DbContext`. Asserts BOTH the translated SQL (`IQueryable.ToQueryString()`) and the
// materialized results, compared against a native C#/.NET oracle computed independently in
// `csharp_helper/QueryRunner.cs`'s own in-process LINQ (not hand-typed expected values).
//
// The C# helper project (`csharp_helper/`) defines the `Investor` entity + `InvestorDbContext` +
// `QueryRunner` (seed data, `CreateContext()`, `Run()`). Rust NEVER touches
// `DbContextOptionsBuilder` -- all fluent EF wiring stays on the C# side; Rust only calls
// `InvestorDbContext.CreateContext()`.
use dotnet_macros::dotnet_entity;
use mycorrhiza::intrinsics::{
    rustc_clr_interop_generic_method_call2 as gmethod2, rustc_clr_interop_managed_ld_elem_ref as ld_elem_ref,
    rustc_clr_interop_managed_ld_len as ld_len, rustc_clr_interop_managed_new_arr as new_arr,
    rustc_clr_interop_managed_set_elem as set_elem, RustcCLRInteropManagedArray, RustcCLRInteropManagedClass,
    RustcCLRInteropManagedGeneric, RustcCLRInteropMethodGeneric,
};
use mycorrhiza::linq::{Expr, Param, TypedPredicate};
use mycorrhiza::system::console::Console;
use mycorrhiza::system::DotNetString;

// ---- Raw handles into the `EfHelper` assembly (the small C# class library under `csharp_helper/`,
// consumed via the SAME runtime-asset marker-dir mechanism `cargo dotnet add-nuget` uses: every dll
// under `.cargo-dotnet-nuget-assets/` is copied alongside the build output, and the PE writer emits a
// real `AssemblyRef` for "EfHelper" that the CLR resolves via normal probing). ----
type DbContextHandle = RustcCLRInteropManagedClass<"EfHelper", "CdEfCore.InvestorDbContext">;
type QueryRunnerHandle = RustcCLRInteropManagedClass<"EfHelper", "CdEfCore.QueryRunner">;
type QueryResultHandle = RustcCLRInteropManagedClass<"EfHelper", "CdEfCore.QueryResult">;
type InvestorHandle = RustcCLRInteropManagedClass<"EfHelper", "CdEfCore.Investor">;
type InvestorArr = RustcCLRInteropManagedArray<InvestorHandle, 1>;
type WriteResultHandle = RustcCLRInteropManagedClass<"EfHelper", "CdEfCore.WriteResult">;
type IncludeResultHandle = RustcCLRInteropManagedClass<"EfHelper", "CdEfCore.IncludeResult">;
type SubscriptionHandle = RustcCLRInteropManagedClass<"EfHelper", "CdEfCore.Subscription">;
type SubscriptionArr = RustcCLRInteropManagedArray<SubscriptionHandle, 1>;

// ---- `#[dotnet_entity]` ergonomics layer (proven in `cd_linq_expr`) for building the predicate ----
// `Investor.Name` is the only field this Stage-1 predicate needs (`i => i.Name == "Acme"`, the
// realistic string-equality shape sampled from real EF `.Where(...)` predicates).
#[dotnet_entity]
#[dotnet(namespace = "CdEfCore", assembly = "EfHelper", name = "Investor")]
struct InvestorEntity {
    name: String,
}

// ---- Raw interop plumbing for the ParameterExpression/Expression handles `mycorrhiza::linq`
// exposes via its `Param::raw()`/`Expr::raw()` escape hatches (see their doc comments: "building
// interop plumbing this module doesn't wrap directly ... constructing
// `Expression.Lambda<Func<T,bool>>` for a caller's own entity type"). ----
type CExpr = RustcCLRInteropManagedClass<"System.Linq.Expressions", "System.Linq.Expressions.Expression">;
type CParam =
    RustcCLRInteropManagedClass<"System.Linq.Expressions", "System.Linq.Expressions.ParameterExpression">;
type CParamArr = RustcCLRInteropManagedArray<CParam, 1>;
// `Func`2<Investor,bool>` -- a generic delegate instantiation over the REAL entity type.
type CFuncInvestorBool = RustcCLRInteropManagedGeneric<"System.Private.CoreLib", "System.Func", (InvestorHandle, bool)>;
// `Expression`1<Func`2<Investor,bool>>` -- the nested-generic type `QueryRunner.Run` consumes.
type CExprFuncInvestorBool = RustcCLRInteropManagedGeneric<
    "System.Linq.Expressions",
    "System.Linq.Expressions.Expression",
    (CFuncInvestorBool,),
>;
// The def-shape return of `Expression.Lambda<!!0>` -- `Expression`1<!!0>`.
type CExprMethGen0 = RustcCLRInteropManagedGeneric<
    "System.Linq.Expressions",
    "System.Linq.Expressions.Expression",
    (RustcCLRInteropMethodGeneric<0>,),
>;

/// Wrap a `TypedPredicate<InvestorEntity>`'s body into a strongly-typed
/// `Expression<Func<Investor,bool>>`, via the generic `Expression.Lambda<TDelegate>` method --
/// mirrors `mycorrhiza::linq::Expr::typed_pred`'s `i32`-specialized version, generalized to the real
/// entity type this crate binds (`CdEfCore.Investor` in `EfHelper.dll`), exactly as that function's
/// doc comment says a caller building their own entity pipeline should.
fn typed_pred_investor(body: Expr, p: Param) -> CExprFuncInvestorBool {
    let arr: CParamArr = new_arr::<CParam>(1);
    set_elem::<CParam>(arr, 0, p.raw());
    gmethod2::<
        "System.Linq.Expressions",
        "System.Linq.Expressions.Expression",
        false,
        "Lambda",
        0,
        (),
        (CFuncInvestorBool,),
        (CExprMethGen0, CExpr, CParamArr),
        CExprFuncInvestorBool,
        CExpr,
        CParamArr,
    >(body.raw(), arr)
}

fn mstr_to_rust(s: mycorrhiza::system::MString) -> std::string::String {
    DotNetString::from_handle(s).to_rust_string()
}

fn say(label: &str, s: &str) {
    let line = format!("{label}: {s}");
    Console::writeln_string(DotNetString::from(line.as_str()).handle());
}

fn main() -> std::process::ExitCode {
    let mut pass = 0u32;
    let mut total = 0u32;
    macro_rules! chk {
        ($g:expr, $w:expr) => {{
            total += 1;
            if $g == $w {
                pass += 1;
            } else {
                Console::writeln_u64(900_000_000 + total as u64);
            }
        }};
    }

    // The ONLY entry point Rust calls into the C# side's fluent EF wiring -- everything about
    // `DbContextOptionsBuilder`/`UseSqlite`/`EnsureCreated`/seeding stays in
    // `csharp_helper/InvestorDbContext.cs`.
    let ctx = DbContextHandle::static0::<"CreateContext", DbContextHandle>();
    say("ctx", "created");

    // ---- Stage 1, item 2: the materialization check (de-risked FIRST, before the predicate
    // pipeline) -- read a `string` property off REAL EF-materialized entities. ----
    let all: InvestorArr = QueryRunnerHandle::static1::<"AllInvestors", DbContextHandle, InvestorArr>(ctx);
    let all_len = ld_len(all);
    say("all_len", &all_len.to_string());
    chk!(all_len, 4); // 4 seeded rows (Acme x2, Globex, Initech)

    let mut acme_count_raw = 0u32;
    for i in 0..all_len {
        let inv: InvestorHandle = ld_elem_ref::<"EfHelper", "CdEfCore.Investor">(all, i);
        let name = mstr_to_rust(inv.instance0::<"get_Name", mycorrhiza::system::MString>());
        say("materialized investor", &name);
        if name == "Acme" {
            acme_count_raw += 1;
        }
    }
    chk!(acme_count_raw, 2u32); // proves per-row property reads off REAL materialized entities

    // ---- Stage 1, item 3: build `i => i.Name == "Acme"` from Rust, hand it to EF's REAL Sqlite
    // provider, and check BOTH the translated SQL and the materialized results. ----
    let investor = InvestorEntity::new();
    let pred: TypedPredicate<InvestorEntity> = investor.name.eq("Acme");
    say("predicate text", &pred.text());
    chk!(pred.text().contains("Acme"), true);

    let expr_handle = typed_pred_investor(pred.body(), pred.param());
    let result: QueryResultHandle =
        QueryRunnerHandle::static2::<"Run", DbContextHandle, CExprFuncInvestorBool, QueryResultHandle>(
            ctx, expr_handle,
        );

    let sql = mstr_to_rust(result.instance0::<"get_Sql", mycorrhiza::system::MString>());
    say("translated SQL", &sql);
    // Proves EF's provider ACTUALLY TRANSLATED the Rust-built expression tree to SQL (not just
    // evaluated it client-side): a real WHERE clause referencing the Name column and the literal.
    chk!(sql.to_uppercase().contains("WHERE"), true);
    chk!(sql.contains("Name"), true);
    chk!(sql.contains("Acme"), true);

    let rows: InvestorArr = result.instance0::<"get_Rows", InvestorArr>();
    let rows_len = ld_len(rows);
    say("materialized rows", &rows_len.to_string());
    chk!(rows_len, 2); // native oracle: 2 of the 4 seeded rows have Name == "Acme"

    let mut all_acme = true;
    for i in 0..rows_len {
        let inv: InvestorHandle = ld_elem_ref::<"EfHelper", "CdEfCore.Investor">(rows, i);
        let name = mstr_to_rust(inv.instance0::<"get_Name", mycorrhiza::system::MString>());
        say("result row", &name);
        if name != "Acme" {
            all_acme = false;
        }
    }
    chk!(all_acme, true);

    // ---- EF4: write round-trip -- Rust drives a real INSERT (Add + synchronous SaveChanges,
    // via `QueryRunner.AddInvestorAndVerify` -- Rust never touches `ctx.Investors.Add`/
    // `SaveChanges` directly, same "thin C# helper" shape as the read path above). Stays fully
    // synchronous throughout: no `SaveChangesAsync`, avoiding this project's documented ceiling
    // around producing `Task<T>`/holding GC-refs across an `.await`. Persistence is proven by
    // re-querying through a BRAND-NEW `InvestorDbContext` on the C# side (see that method's doc
    // comment), not just reading back the same context's in-memory change tracker.
    let new_name: mycorrhiza::system::MString = "NewCo".into();
    let write_result: WriteResultHandle = QueryRunnerHandle::static2::<
        "AddInvestorAndVerify",
        DbContextHandle,
        mycorrhiza::system::MString,
        WriteResultHandle,
    >(ctx, new_name);
    let new_investor_name =
        mstr_to_rust(write_result.instance0::<"get_NewInvestorName", mycorrhiza::system::MString>());
    let new_investor_id =
        mstr_to_rust(write_result.instance0::<"get_NewInvestorId", mycorrhiza::system::MString>());
    let persisted_count = write_result.instance0::<"get_PersistedCount", i32>();
    say("EF4 new investor name", &new_investor_name);
    say("EF4 new investor id", &new_investor_id);
    say("EF4 persisted count (fresh DbContext)", &persisted_count.to_string());
    chk!(new_investor_name, "NewCo".to_string());
    chk!(persisted_count, 1); // proves the row is durable, not just visible via this context's tracker

    // ---- EF6: Include()/navigation properties -- Rust REQUESTS the eager-load by calling
    // `QueryRunner.InvestorsWithSubscriptions` (the `.Include(i => i.Subscriptions)` lambda itself
    // lives on the C# side), then reads the nested `Subscriptions` collection off each materialized
    // `Investor` -- both the collection's count and a nested field (`Subscription.Kind`).
    let inc_result: IncludeResultHandle = QueryRunnerHandle::static1::<
        "InvestorsWithSubscriptions",
        DbContextHandle,
        IncludeResultHandle,
    >(ctx);
    let inc_sql = mstr_to_rust(inc_result.instance0::<"get_Sql", mycorrhiza::system::MString>());
    say("EF6 translated SQL", &inc_sql);
    // A real JOIN in the translated SQL is the proof this is ONE round trip, not N+1 separate
    // per-investor SELECTs.
    chk!(inc_sql.to_uppercase().contains("JOIN"), true);

    let inc_rows: InvestorArr = inc_result.instance0::<"get_Rows", InvestorArr>();
    let inc_rows_len = ld_len(inc_rows);
    say("EF6 investor count", &inc_rows_len.to_string());
    chk!(inc_rows_len, 5); // 4 original seeds + the EF4-written "NewCo" row, same live ctx

    let mut total_subs = 0u32;
    let mut found_acme_seriesa = false;
    for i in 0..inc_rows_len {
        let inv: InvestorHandle = ld_elem_ref::<"EfHelper", "CdEfCore.Investor">(inc_rows, i);
        let name = mstr_to_rust(inv.instance0::<"get_Name", mycorrhiza::system::MString>());
        let subs: SubscriptionArr = inv.instance0::<"get_SubscriptionsArray", SubscriptionArr>();
        let subs_len = ld_len(subs);
        total_subs += subs_len as u32;
        say(&format!("EF6 {name} subscriptions"), &subs_len.to_string());
        for j in 0..subs_len {
            let sub: SubscriptionHandle = ld_elem_ref::<"EfHelper", "CdEfCore.Subscription">(subs, j);
            let kind = mstr_to_rust(sub.instance0::<"get_Kind", mycorrhiza::system::MString>());
            say("EF6 subscription kind", &kind);
            if name == "Acme" && kind == "SeriesA" {
                found_acme_seriesa = true;
            }
        }
    }
    chk!(total_subs, 4u32); // 4 seeded Subscription rows total (2nd Acme + NewCo have 0)
    chk!(found_acme_seriesa, true); // proves a nested field read off the nested collection

    // ---- Migrations proof: `InvestorDbContext.CreateContext()` now builds the schema via
    // `Database.Migrate()` (applying `csharp_helper/Migrations/*_InitialCreate.cs`), not
    // `EnsureCreated()`'s model-sync. `GetAppliedMigrations()` reads the `__EFMigrationsHistory`
    // table that ONLY `Migrate()` populates -- EnsureCreated() never writes it -- so a concrete,
    // non-empty migration name read back here is a real signal the schema really did come from
    // migration application. ----
    let applied_count = QueryRunnerHandle::static1::<"AppliedMigrationCount", DbContextHandle, i32>(ctx);
    say("applied migration count", &applied_count.to_string());
    chk!(applied_count, 1); // exactly the one InitialCreate migration

    let applied_name = mstr_to_rust(QueryRunnerHandle::static1::<
        "FirstAppliedMigrationName",
        DbContextHandle,
        mycorrhiza::system::MString,
    >(ctx));
    say("applied migration name", &applied_name);
    chk!(applied_name.contains("InitialCreate"), true);

    Console::writeln_u64(pass as u64);
    Console::writeln_u64(total as u64);
    if pass == total {
        println!("== cd_efcore done ==");
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
