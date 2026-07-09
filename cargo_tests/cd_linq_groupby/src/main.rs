// Proof: Rust drives `Queryable.GroupBy`/`Join`/`SelectMany` DIRECTLY (via
// `mycorrhiza::linq::{group_by, join, select_many}` — the WF-9 generic-method-call machinery, same
// family as `mycorrhiza::linq::IntQuery::{where_,count}`), against a REAL EF Core `IQueryable<T>`
// (Sqlite provider — the EF `InMemory` provider never translates to SQL). Unlike `cd_efcore`'s
// `Where` proof (where the C# side issues `.Where(expr)` itself, taking only the predicate from
// Rust), the `GroupBy`/`Join`/`SelectMany` CALLS themselves are issued by Rust here — the C# helper
// (`csharp_helper/QueryHelpers.cs`) only hands Rust the two source `IQueryable<T>`s and, at the end,
// materializes the `IQueryable` Rust built + reads `ToQueryString()` off it.
//
// Both the translated SQL AND the materialized results are checked against a fixed, hand-computed
// oracle (the seed data in `csharp_helper/GroupDbContext.cs` is small and enumerated in this file's
// comments), matching `cd_efcore`'s "assert BOTH the SQL and the results" rigor.
//
// SIMPLIFICATIONS (see the task write-up this crate answers):
//   * `Join`'s key selectors use a plain `int` surrogate key (`Investor.Code`/
//     `Subscription.InvestorCode`) instead of the real `Guid` FK (`Investor.Id`/
//     `Subscription.InvestorId`) — this keeps the key-selector `Expression<Func<..,TKey>>` types
//     `i32`-based (already proven via `IntQuery`) instead of needing a new `Expression<Func<..,
//     Guid>>` type-alias family. The real Guid FK still exists on the entities and still backs the
//     nav property `SelectMany` exercises.
//   * `Join`'s `resultSelector` projects to just the INNER (`Subscription`) side —
//     `(o, i) => i` — rather than a `new { ... }` anonymous-type-equivalent projection (which would
//     need a `#[dotnet_class]`-defined result type + `Expression.New`/`MemberInit` construction,
//     scope this crate's `Join` proof doesn't need to carry to demonstrate the JOIN shape itself).

use mycorrhiza::intrinsics::{
    rustc_clr_interop_managed_checked_cast as cast, rustc_clr_interop_managed_ld_elem_ref as ld_elem_ref,
    rustc_clr_interop_managed_ld_len as ld_len, RustcCLRInteropManagedArray, RustcCLRInteropManagedClass,
    RustcCLRInteropManagedGeneric,
};
use mycorrhiza::linq::{self, Expr, Param};
use mycorrhiza::system::console::Console;
use mycorrhiza::system::{DotNetString, MString};

// ---- Raw handles into the `LinqGroupHelper` assembly (csharp_helper/), wired in via the same
// `.cargo-dotnet-nuget-assets/` runtime-asset marker-dir mechanism `cd_efcore`/`cargo dotnet
// add-nuget` use — every DLL under that directory is copied alongside the build output, and the PE
// writer emits a real `AssemblyRef` for "LinqGroupHelper" that the CLR resolves via normal probing.
type DbContextHandle = RustcCLRInteropManagedClass<"LinqGroupHelper", "CdLinqGroup.GroupDbContext">;
type QueryHelpersHandle = RustcCLRInteropManagedClass<"LinqGroupHelper", "CdLinqGroup.QueryHelpers">;
type InvestorHandle = RustcCLRInteropManagedClass<"LinqGroupHelper", "CdLinqGroup.Investor">;
type SubscriptionHandle = RustcCLRInteropManagedClass<"LinqGroupHelper", "CdLinqGroup.Subscription">;
type SubscriptionArr = RustcCLRInteropManagedArray<SubscriptionHandle, 1>;
type GroupSummaryHandle = RustcCLRInteropManagedClass<"LinqGroupHelper", "CdLinqGroup.GroupSummary">;
type GroupSummaryArr = RustcCLRInteropManagedArray<GroupSummaryHandle, 1>;
type GroupSummaryResultHandle = RustcCLRInteropManagedClass<"LinqGroupHelper", "CdLinqGroup.GroupSummaryResult">;
type SubscriptionQueryResultHandle = RustcCLRInteropManagedClass<"LinqGroupHelper", "CdLinqGroup.SubscriptionQueryResult">;

// ---- `IQueryable<T>`/`IEnumerable<T>` concrete instantiations over the real entity types ----
type CIQueryInvestor =
    RustcCLRInteropManagedGeneric<"System.Linq.Expressions", "System.Linq.IQueryable", (InvestorHandle,)>;
type CIQuerySub = RustcCLRInteropManagedGeneric<"System.Linq.Expressions", "System.Linq.IQueryable", (SubscriptionHandle,)>;
type CIEnumSub = RustcCLRInteropManagedGeneric<
    "System.Private.CoreLib",
    "System.Collections.Generic.IEnumerable",
    (SubscriptionHandle,),
>;

// ---- `GroupBy` selector plumbing: `Expression<Func<Subscription,string>>` (`s => s.Kind`) ----
type CFuncSubStr = RustcCLRInteropManagedGeneric<"System.Private.CoreLib", "System.Func", (SubscriptionHandle, MString)>;
type CExprFuncSubStr =
    RustcCLRInteropManagedGeneric<"System.Linq.Expressions", "System.Linq.Expressions.Expression", (CFuncSubStr,)>;
type CIGroupingStrSub = RustcCLRInteropManagedGeneric<"System.Linq", "System.Linq.IGrouping", (MString, SubscriptionHandle)>;
type CIQueryGroup =
    RustcCLRInteropManagedGeneric<"System.Linq.Expressions", "System.Linq.IQueryable", (CIGroupingStrSub,)>;

// ---- `SelectMany` selector plumbing: `Expression<Func<Investor,IEnumerable<Subscription>>>`
// (`i => i.Subscriptions`) ----
type CFuncInvestorEnumSub = RustcCLRInteropManagedGeneric<"System.Private.CoreLib", "System.Func", (InvestorHandle, CIEnumSub)>;
type CExprFuncInvestorEnumSub = RustcCLRInteropManagedGeneric<
    "System.Linq.Expressions",
    "System.Linq.Expressions.Expression",
    (CFuncInvestorEnumSub,),
>;

// ---- `Join` selector plumbing: two `Expression<Func<..,int>>` key selectors (`o => o.Code` /
// `sub => sub.InvestorCode`) + one `Expression<Func<Investor,Subscription,Subscription>>`
// resultSelector (`(o,i) => i` — see the module doc's "SIMPLIFICATIONS" note) ----
type CFuncInvestorI32 = RustcCLRInteropManagedGeneric<"System.Private.CoreLib", "System.Func", (InvestorHandle, i32)>;
type CExprFuncInvestorI32 = RustcCLRInteropManagedGeneric<
    "System.Linq.Expressions",
    "System.Linq.Expressions.Expression",
    (CFuncInvestorI32,),
>;
type CFuncSubI32 = RustcCLRInteropManagedGeneric<"System.Private.CoreLib", "System.Func", (SubscriptionHandle, i32)>;
type CExprFuncSubI32 =
    RustcCLRInteropManagedGeneric<"System.Linq.Expressions", "System.Linq.Expressions.Expression", (CFuncSubI32,)>;
type CFuncInvestorSubSub =
    RustcCLRInteropManagedGeneric<"System.Private.CoreLib", "System.Func", (InvestorHandle, SubscriptionHandle, SubscriptionHandle)>;
type CExprFuncInvestorSubSub = RustcCLRInteropManagedGeneric<
    "System.Linq.Expressions",
    "System.Linq.Expressions.Expression",
    (CFuncInvestorSubSub,),
>;

fn mstr_to_rust(s: MString) -> std::string::String {
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

    // ============================================================================
    // 1. GroupBy — `Queryable.GroupBy<Subscription,string>(source, s => s.Kind)`, called DIRECTLY
    //    from Rust via `mycorrhiza::linq::group_by`.
    // ============================================================================
    let sub_source: CIQuerySub = QueryHelpersHandle::static1::<"SubscriptionsQuery", DbContextHandle, CIQuerySub>(ctx);
    let s_param = Param::new("CdLinqGroup.Subscription, LinqGroupHelper", "s");
    let key_body: Expr = s_param.expr().prop("Kind");
    let key_selector: CExprFuncSubStr = linq::typed_lambda::<CFuncSubStr, CExprFuncSubStr>(key_body, &[&s_param]);
    say("groupby key selector", &key_body.text());
    chk!(key_body.text(), "s.Kind".to_string());

    let groups: CIQueryGroup =
        linq::group_by::<SubscriptionHandle, MString, CIQuerySub, CIQueryGroup, CExprFuncSubStr>(sub_source, key_selector);
    let group_result: GroupSummaryResultHandle =
        QueryHelpersHandle::static1::<"SummarizeGroups", CIQueryGroup, GroupSummaryResultHandle>(groups);

    let group_sql = mstr_to_rust(group_result.instance0::<"get_Sql", MString>());
    say("GroupBy translated SQL", &group_sql);
    chk!(group_sql.to_uppercase().contains("GROUP BY"), true);
    chk!(group_sql.contains("Kind"), true);

    let group_rows: GroupSummaryArr = group_result.instance0::<"get_Groups", GroupSummaryArr>();
    let group_rows_len = ld_len(group_rows);
    say("GroupBy group count", &group_rows_len.to_string());
    chk!(group_rows_len, 2); // 2 distinct `Kind` buckets: SeriesA, SeriesB

    let mut series_a_count = 0i32;
    let mut series_a_sum = 0i32;
    let mut series_b_count = 0i32;
    let mut series_b_sum = 0i32;
    for i in 0..group_rows_len {
        let g: GroupSummaryHandle = ld_elem_ref::<"LinqGroupHelper", "CdLinqGroup.GroupSummary">(group_rows, i);
        let kind = mstr_to_rust(g.instance0::<"get_Kind", MString>());
        let count = g.instance0::<"get_Count", i32>();
        let sum = g.instance0::<"get_Sum", i32>();
        say("GroupBy group", &format!("{kind}: count={count} sum={sum}"));
        if kind == "SeriesA" {
            series_a_count = count;
            series_a_sum = sum;
        } else if kind == "SeriesB" {
            series_b_count = count;
            series_b_sum = sum;
        }
    }
    // Oracle (see `GroupDbContext.CreateContext`'s seed comment): SeriesA = {100,150,300} -> 3/550;
    // SeriesB = {200,50} -> 2/250.
    chk!(series_a_count, 3);
    chk!(series_a_sum, 550);
    chk!(series_b_count, 2);
    chk!(series_b_sum, 250);

    // ============================================================================
    // 2. SelectMany — `Queryable.SelectMany<Investor,Subscription>(source, i =>
    //    i.Subscriptions)`, flattening the one-to-many navigation into a single
    //    `IQueryable<Subscription>`. Called DIRECTLY from Rust via
    //    `mycorrhiza::linq::select_many`.
    // ============================================================================
    let inv_source: CIQueryInvestor =
        QueryHelpersHandle::static1::<"InvestorsQuery", DbContextHandle, CIQueryInvestor>(ctx);
    let i_param = Param::new("CdLinqGroup.Investor, LinqGroupHelper", "i");
    let sel_body: Expr = i_param.expr().prop("Subscriptions");
    let selector: CExprFuncInvestorEnumSub =
        linq::typed_lambda::<CFuncInvestorEnumSub, CExprFuncInvestorEnumSub>(sel_body, &[&i_param]);
    say("selectmany selector", &sel_body.text());
    chk!(sel_body.text(), "i.Subscriptions".to_string());

    let flat: CIQuerySub =
        linq::select_many::<InvestorHandle, SubscriptionHandle, CIQueryInvestor, CIQuerySub, CExprFuncInvestorEnumSub>(
            inv_source, selector,
        );
    let flat_result: SubscriptionQueryResultHandle =
        QueryHelpersHandle::static1::<"SummarizeSubscriptions", CIQuerySub, SubscriptionQueryResultHandle>(flat);

    let flat_sql = mstr_to_rust(flat_result.instance0::<"get_Sql", MString>());
    say("SelectMany translated SQL", &flat_sql);
    // A real flattening query — a single round trip joining Investor to its Subscriptions, not N+1
    // per-investor SELECTs.
    chk!(flat_sql.to_uppercase().contains("JOIN"), true);

    let flat_rows: SubscriptionArr = flat_result.instance0::<"get_Rows", SubscriptionArr>();
    let flat_len = ld_len(flat_rows);
    say("SelectMany row count", &flat_len.to_string());
    chk!(flat_len, 5); // all 5 seeded subscriptions, flattened across both investors

    let mut flat_sum = 0i32;
    for i in 0..flat_len {
        let row: SubscriptionHandle = ld_elem_ref::<"LinqGroupHelper", "CdLinqGroup.Subscription">(flat_rows, i);
        flat_sum += row.instance0::<"get_Amount", i32>();
    }
    chk!(flat_sum, 800); // 100+150+200+300+50

    // ============================================================================
    // 3. Join — `Queryable.Join<Investor,Subscription,int,Subscription>(outer, inner,
    //    o => o.Code, sub => sub.InvestorCode, (o,i) => i)`, the EXPLICIT join shape (no
    //    pre-declared navigation property is consulted — the two key selectors alone drive the
    //    match). Called DIRECTLY from Rust via `mycorrhiza::linq::join`, the new arity-5
    //    generic-method-call rung.
    // ============================================================================
    let inv_source2: CIQueryInvestor =
        QueryHelpersHandle::static1::<"InvestorsQuery", DbContextHandle, CIQueryInvestor>(ctx);
    let sub_source2: CIQuerySub = QueryHelpersHandle::static1::<"SubscriptionsQuery", DbContextHandle, CIQuerySub>(ctx);

    let o_param = Param::new("CdLinqGroup.Investor, LinqGroupHelper", "o");
    let outer_key_body: Expr = o_param.expr().prop("Code");
    let outer_key: CExprFuncInvestorI32 =
        linq::typed_lambda::<CFuncInvestorI32, CExprFuncInvestorI32>(outer_key_body, &[&o_param]);

    let isub_param = Param::new("CdLinqGroup.Subscription, LinqGroupHelper", "isub");
    let inner_key_body: Expr = isub_param.expr().prop("InvestorCode");
    let inner_key: CExprFuncSubI32 =
        linq::typed_lambda::<CFuncSubI32, CExprFuncSubI32>(inner_key_body, &[&isub_param]);

    let ro_param = Param::new("CdLinqGroup.Investor, LinqGroupHelper", "ro");
    let ri_param = Param::new("CdLinqGroup.Subscription, LinqGroupHelper", "ri");
    let result_body: Expr = ri_param.expr(); // simplification: project to the inner (Subscription) side
    let result_selector: CExprFuncInvestorSubSub =
        linq::typed_lambda::<CFuncInvestorSubSub, CExprFuncInvestorSubSub>(result_body, &[&ro_param, &ri_param]);

    say("join outer key selector", &outer_key_body.text());
    say("join inner key selector", &inner_key_body.text());
    chk!(outer_key_body.text(), "o.Code".to_string());
    chk!(inner_key_body.text(), "isub.InvestorCode".to_string());

    // `Queryable.Join`'s `inner` parameter is declared `IEnumerable<TInner>`, not
    // `IQueryable<TInner>` — the cilly typechecker (invariant I1, exact methodref-arg-type
    // matching, NOT CLR-style interface covariance) rejects an `IQueryable<Subscription>` operand
    // at that slot even though `IQueryable<T> : IEnumerable<T>` at the CLR level, so re-type the
    // value explicitly via a checked upcast first (always succeeds — the real runtime type already
    // implements `IEnumerable<Subscription>`).
    let sub_source2_as_enum: CIEnumSub = cast::<CIEnumSub, CIQuerySub>(sub_source2);
    let joined: CIQuerySub = linq::join::<
        InvestorHandle,
        SubscriptionHandle,
        i32,
        SubscriptionHandle,
        CIQueryInvestor,
        CIEnumSub,
        CExprFuncInvestorI32,
        CExprFuncSubI32,
        CExprFuncInvestorSubSub,
        CIQuerySub,
    >(inv_source2, sub_source2_as_enum, outer_key, inner_key, result_selector);
    let join_result: SubscriptionQueryResultHandle =
        QueryHelpersHandle::static1::<"SummarizeSubscriptions", CIQuerySub, SubscriptionQueryResultHandle>(joined);

    let join_sql = mstr_to_rust(join_result.instance0::<"get_Sql", MString>());
    say("Join translated SQL", &join_sql);
    chk!(join_sql.to_uppercase().contains("JOIN"), true);

    let join_rows: SubscriptionArr = join_result.instance0::<"get_Rows", SubscriptionArr>();
    let join_len = ld_len(join_rows);
    say("Join row count", &join_len.to_string());
    chk!(join_len, 5); // every subscription has a matching investor by Code

    let mut join_sum = 0i32;
    for i in 0..join_len {
        let row: SubscriptionHandle = ld_elem_ref::<"LinqGroupHelper", "CdLinqGroup.Subscription">(join_rows, i);
        join_sum += row.instance0::<"get_Amount", i32>();
    }
    chk!(join_sum, 800); // same 5 rows as SelectMany, reached via the explicit Join shape

    Console::writeln_u64(pass as u64);
    Console::writeln_u64(total as u64);
    if pass == total {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
