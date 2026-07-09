// Postgres counterpart of `cargo_tests/cd_efcore/src/main.rs` -- verifies the SAME
// Rust-built-`System.Linq.Expressions`-predicate-through-real-EF-Core mechanism works against a
// REAL Npgsql/PostgreSQL provider and a real running `postgres:16` server, not just SQLite. Mirrors
// that file's entities/predicate/proof shape as closely as possible; only the underlying provider
// (and assembly/namespace names, to keep the two proofs' runtime assets from colliding) differ.
//
// Unlike the Sqlite proof (a fresh in-memory db every process run), this Postgres server is a real,
// persistent instance shared across runs, so `main` calls `QueryRunner.ResetDatabase` first to keep
// repeated `cargo dotnet run` invocations deterministic (see `csharp_helper/QueryRunner.cs`).
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

// ---- Raw handles into the `EfHelperPg` assembly (the C# class library under `csharp_helper/`,
// consumed via the same `.cargo-dotnet-nuget-assets/` marker-dir mechanism as `cd_efcore`). ----
type DbContextHandle = RustcCLRInteropManagedClass<"EfHelperPg", "CdEfCorePg.InvestorDbContext">;
type QueryRunnerHandle = RustcCLRInteropManagedClass<"EfHelperPg", "CdEfCorePg.QueryRunner">;
type QueryResultHandle = RustcCLRInteropManagedClass<"EfHelperPg", "CdEfCorePg.QueryResult">;
type InvestorHandle = RustcCLRInteropManagedClass<"EfHelperPg", "CdEfCorePg.Investor">;
type InvestorArr = RustcCLRInteropManagedArray<InvestorHandle, 1>;
type WriteResultHandle = RustcCLRInteropManagedClass<"EfHelperPg", "CdEfCorePg.WriteResult">;
type IncludeResultHandle = RustcCLRInteropManagedClass<"EfHelperPg", "CdEfCorePg.IncludeResult">;
type SubscriptionHandle = RustcCLRInteropManagedClass<"EfHelperPg", "CdEfCorePg.Subscription">;
type SubscriptionArr = RustcCLRInteropManagedArray<SubscriptionHandle, 1>;

#[dotnet_entity]
#[dotnet(namespace = "CdEfCorePg", assembly = "EfHelperPg", name = "Investor")]
struct InvestorEntity {
    name: String,
}

type CExpr = RustcCLRInteropManagedClass<"System.Linq.Expressions", "System.Linq.Expressions.Expression">;
type CParam =
    RustcCLRInteropManagedClass<"System.Linq.Expressions", "System.Linq.Expressions.ParameterExpression">;
type CParamArr = RustcCLRInteropManagedArray<CParam, 1>;
type CFuncInvestorBool = RustcCLRInteropManagedGeneric<"System.Private.CoreLib", "System.Func", (InvestorHandle, bool)>;
type CExprFuncInvestorBool = RustcCLRInteropManagedGeneric<
    "System.Linq.Expressions",
    "System.Linq.Expressions.Expression",
    (CFuncInvestorBool,),
>;
type CExprMethGen0 = RustcCLRInteropManagedGeneric<
    "System.Linq.Expressions",
    "System.Linq.Expressions.Expression",
    (RustcCLRInteropMethodGeneric<0>,),
>;

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

    let ctx = DbContextHandle::static0::<"CreateContext", DbContextHandle>();
    say("ctx", "created");

    // Postgres is a real, persistent server -- reset it first so repeated runs are deterministic
    // (the Sqlite proof gets this for free from its fresh-per-process in-memory db).
    QueryRunnerHandle::static1::<"ResetDatabase", DbContextHandle, ()>(ctx);
    say("db", "reset");
    // Re-create so the seed-data check below runs against the freshly-truncated tables.
    let ctx = DbContextHandle::static0::<"CreateContext", DbContextHandle>();

    // ---- item 2: materialization check -- read a `string` property off REAL EF-materialized
    // entities backed by a real Postgres row. ----
    let all: InvestorArr = QueryRunnerHandle::static1::<"AllInvestors", DbContextHandle, InvestorArr>(ctx);
    let all_len = ld_len(all);
    say("all_len", &all_len.to_string());
    chk!(all_len, 4); // 4 seeded rows (Acme x2, Globex, Initech)

    let mut acme_count_raw = 0u32;
    for i in 0..all_len {
        let inv: InvestorHandle = ld_elem_ref::<"EfHelperPg", "CdEfCorePg.Investor">(all, i);
        let name = mstr_to_rust(inv.instance0::<"get_Name", mycorrhiza::system::MString>());
        say("materialized investor", &name);
        if name == "Acme" {
            acme_count_raw += 1;
        }
    }
    chk!(acme_count_raw, 2u32);

    // ---- item 3: build `i => i.Name == "Acme"` from Rust, hand it to EF's REAL Npgsql/Postgres
    // provider, and check BOTH the translated (Postgres) SQL and the materialized results. ----
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
    say("translated Postgres SQL", &sql);
    // Proves EF's Npgsql provider ACTUALLY TRANSLATED the Rust-built expression tree to Postgres SQL
    // (not just evaluated it client-side). Postgres's translated SQL uses double-quoted identifiers
    // (`"Name"`) rather than Sqlite's bracket/plain style -- NOT expected to match cd_efcore's SQL
    // byte-for-byte, just to be a real WHERE clause referencing the Name column and literal.
    chk!(sql.to_uppercase().contains("WHERE"), true);
    chk!(sql.contains("Name"), true);
    chk!(sql.contains("Acme"), true);

    let rows: InvestorArr = result.instance0::<"get_Rows", InvestorArr>();
    let rows_len = ld_len(rows);
    say("materialized rows", &rows_len.to_string());
    chk!(rows_len, 2); // native oracle: 2 of the 4 seeded rows have Name == "Acme"

    let mut all_acme = true;
    for i in 0..rows_len {
        let inv: InvestorHandle = ld_elem_ref::<"EfHelperPg", "CdEfCorePg.Investor">(rows, i);
        let name = mstr_to_rust(inv.instance0::<"get_Name", mycorrhiza::system::MString>());
        say("result row", &name);
        if name != "Acme" {
            all_acme = false;
        }
    }
    chk!(all_acme, true);

    // ---- write round-trip: Rust drives a real INSERT (Add + synchronous SaveChanges) against the
    // real Postgres server; persistence is proven by re-querying through a BRAND-NEW
    // `InvestorDbContext` (its own fresh Npgsql connection), same shape as the Sqlite proof. ----
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
    say("new investor name", &new_investor_name);
    say("new investor id", &new_investor_id);
    say("persisted count (fresh DbContext, fresh connection)", &persisted_count.to_string());
    chk!(new_investor_name, "NewCo".to_string());
    chk!(persisted_count, 1);

    // ---- Include()/navigation properties against Postgres -- one round-trip JOIN, not N+1. ----
    let inc_result: IncludeResultHandle = QueryRunnerHandle::static1::<
        "InvestorsWithSubscriptions",
        DbContextHandle,
        IncludeResultHandle,
    >(ctx);
    let inc_sql = mstr_to_rust(inc_result.instance0::<"get_Sql", mycorrhiza::system::MString>());
    say("Include() translated Postgres SQL", &inc_sql);
    chk!(inc_sql.to_uppercase().contains("JOIN"), true);

    let inc_rows: InvestorArr = inc_result.instance0::<"get_Rows", InvestorArr>();
    let inc_rows_len = ld_len(inc_rows);
    say("Include() investor count", &inc_rows_len.to_string());
    chk!(inc_rows_len, 5); // 4 original seeds + the write-path "NewCo" row, same live ctx

    let mut total_subs = 0u32;
    let mut found_acme_seriesa = false;
    for i in 0..inc_rows_len {
        let inv: InvestorHandle = ld_elem_ref::<"EfHelperPg", "CdEfCorePg.Investor">(inc_rows, i);
        let name = mstr_to_rust(inv.instance0::<"get_Name", mycorrhiza::system::MString>());
        let subs: SubscriptionArr = inv.instance0::<"get_SubscriptionsArray", SubscriptionArr>();
        let subs_len = ld_len(subs);
        total_subs += subs_len as u32;
        say(&format!("{name} subscriptions"), &subs_len.to_string());
        for j in 0..subs_len {
            let sub: SubscriptionHandle = ld_elem_ref::<"EfHelperPg", "CdEfCorePg.Subscription">(subs, j);
            let kind = mstr_to_rust(sub.instance0::<"get_Kind", mycorrhiza::system::MString>());
            say("subscription kind", &kind);
            if name == "Acme" && kind == "SeriesA" {
                found_acme_seriesa = true;
            }
        }
    }
    chk!(total_subs, 4u32);
    chk!(found_acme_seriesa, true);

    // ---- migrations proof: `Database.Migrate()` applied `Migrations/*_InitialCreate.cs`
    // (scaffolded via `dotnet ef migrations add` against the Npgsql provider -- Postgres columns
    // use `uuid`/`text`/`timestamp with time zone`, NOT Sqlite's blanket `TEXT` affinity). ----
    let applied_count = QueryRunnerHandle::static1::<"AppliedMigrationCount", DbContextHandle, i32>(ctx);
    say("applied migration count", &applied_count.to_string());
    chk!(applied_count, 1);

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
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
