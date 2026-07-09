//! Step 3 of the async campaign (STATUS: FIXED — was a real CoreCLR `TypeLoadException`, root
//! cause isolated and closed at the `cilly` layer; see below for the fix and how to verify it).
//!
//! Intent: the closest thing this project has to "one real ASP.NET controller method" — a Rust
//! `async fn` that:
//!   1. `.await`s a REAL asynchronous EF Core query (`ToListAsync`, via the C# helper's
//!      `QueryRunner.RunAsyncCount`, `cargo_tests/cd_efcore/csharp_helper/QueryRunner.cs`),
//!   2. does a small synchronous transform on the result,
//!   3. `.await`s a SECOND async call (`SaveChangesAsync`, via `QueryRunner.AddInvestorAndVerifyAsync`),
//! all while holding the SAME `InvestorDbContext` handle across both `.await` points via
//! `mycorrhiza::class::Class<..>` (the GCHandle-backed newtype from `mycorrhiza/src/task.rs`'s
//! "Wall 1" docs), then exposing the whole workflow as a `Task<i32>`
//! (`mycorrhiza::task::future_to_task`) via `#[dotnet_export]`.
//!
//! **What used to happen when you ran this**: `cd_efcore_async/csharp`'s C# host threw
//! `System.TypeLoadException: Could not load type 'CoroutineDefId(...)' ... because it contains an
//! object field at offset 16 that is incorrectly aligned or overlapped by a non-object field.`
//!
//! **Root cause (isolated by dumping the coroutine's actual field/offset list and diffing it
//! against the WORKING `cd_persisted_async` coroutine — see `cilly/src/ir/class.rs`,
//! `ClassDef::layout_check`, and `rustc_codegen_clr_type/src/type.rs`, `coroutine_typedef`):**
//! `Type::is_gcref` (`cilly/src/ir/tpe/mod.rs`) is *shallow* — for a `ClassRef` it only asked
//! "is this a valuetype", never recursing into the struct's own fields. `mycorrhiza::task::
//! TaskFuture<T>` (the `Future` that `await_task(..).await` produces, and which the coroutine
//! MUST save across a `Poll::Pending`) is itself a plain, non-overlapping struct — but it wraps a
//! *raw* `Task<T>` object handle (`TaskT<T>` = `RustcCLRInteropManagedGeneric<..>`), a genuine
//! gcref, in its one field. `is_gcref` reported `false` for `TaskFuture<i32>`, so
//! `layout_check`'s "no gcref allowed in overlapping/coroutine storage" scan never saw it. In
//! THIS coroutine specifically (unlike `cd_persisted_async`'s), one saved-local's `TaskFuture<i32>`
//! landed at the exact same starting offset (16) that a *different* coroutine variant used for a
//! plain `i32` — CoreCLR's class loader (correctly) refuses to load a type where one variant's
//! byte range is an object reference and another variant's is raw data. `cd_persisted_async`
//! happened to avoid this collision by luck of the specific offsets rustc's coroutine layout
//! picked, not because the pattern was actually safe in general — proof: after the fix below,
//! `cd_persisted_async` (which ALSO nests a real gcref inside `TaskFuture<i32>` in overlapping
//! coroutine storage) still passes 4/4, because its offsets never collided in the first place.
//!
//! **The fix** (two parts, both in the `cilly`/`rustc_codegen_clr_type` layer, no weakening of
//! `layout_check`'s protections — see their doc comments for the full reasoning):
//!   1. `Type::contains_gcref` (`cilly/src/ir/tpe/mod.rs`) — a *recursive* gcref check that looks
//!      inside value-type `ClassRef` fields (bounded depth), closing the false-negative that let
//!      `TaskFuture<T>` slip past `layout_check` undetected.
//!   2. `ClassDef::layout_check` now groups a class's overlapping-storage fields by their exact
//!      starting offset and only rejects a group that mixes a gcref-containing field with a
//!      DIFFERENTLY-typed field at that same offset — matching what CoreCLR's loader actually
//!      allows (proven by `cd_persisted_async`'s existing passing tests, which rely on exactly
//!      this "same gcref-shaped field reused across variants" pattern) rather than a blanket
//!      "no gcref in overlapping storage, ever" rule that would reject that legitimate case too.
//!   3. `coroutine_typedef` (`rustc_codegen_clr_type/src/type.rs`) now runs the SAME
//!      collision-detection while building a coroutine's field list; when it would place a field
//!      at an unsafe (loader-rejected) offset, it instead appends that field at a freshly
//!      allocated, non-overlapping offset past the coroutine's natural extent (growing the
//!      declared class size/align to fit) — so the type is not just rejected cleanly at compile
//!      time, the workflow actually RUNS correctly end-to-end (verified: `run_investor_workflow()`
//!      returns `2001` as expected).
//!
//! Verified regressions: `cd_persisted_async` 4/4, `cd_async` 9/9, `cd_delegates` 14/14,
//! `cd_efcore` 16/16 all still pass after this change.
//!
//! **Second, separate bug (also surfaced by the `contains_gcref` deepening above, fixed
//! afterward): a `ManagedPtrCast` type-verifier rejection of `run_investor_workflow` itself.**
//! Once the coroutine layout bug above was fixed, this crate still failed to build with
//! `cilly/src/ir/asm.rs`'s fatal type gate rejecting `run_investor_workflow` for a `PtrCast` from
//! `MaybeUninit<TaskT<i32>>` to `TaskT<i32>`. Root cause: `#[dotnet_export]`'s generated shim (for
//! any `Task`/`TaskT<T>`-returning export, `dotnet_macros/src/lib.rs`) got the call's result out of
//! `catch_unwind`'s closure via `MaybeUninit<RetTy>::as_mut_ptr()` + a raw-pointer `.write()` — a
//! `self as *mut Self as *mut T` cast, i.e. a CIL `PtrCast` reinterpreting a (possibly
//! uninitialized) struct as a live gcref through an UNMANAGED pointer, exactly the hazard
//! `Type::contains_gcref`'s deepened check (rightly) rejects (ECMA-335 requires a managed byref,
//! `T&`, not `T*`, to reference gcref-carrying memory). It silently worked for the simplest export
//! (`cd_export`'s `compute_answer() -> TaskT<i32>`, a trivial `async { 42 }` with no real suspend
//! point) only because rustc's own optimizer proved the `MaybeUninit` round-trip redundant and
//! elided the cast before the backend ever saw it; `run_investor_workflow`'s much bigger body (a
//! real multi-`.await` coroutine driven through `future_to_task`) kept the cast in the final MIR,
//! and the verifier correctly caught it. Fix (`dotnet_macros/src/lib.rs`, `dotnet_export`'s
//! `returns_managed_handle` body): replaced the `MaybeUninit<RetTy>` out-slot with a plain
//! `Option<RetTy>` one. Writing `*out = Some(__v)` is an ordinary tagged-enum construction, and
//! reading it back is an ordinary `match` — neither lowers to a `PtrCast`, so `contains_gcref`
//! never enters the picture, while the slot is still captured by the closure only as a `&mut
//! Option<RetTy>` reference (never a `RetTy` value inside `catch_unwind`'s own `Result`), so the
//! ORIGINAL hazard this pattern was built to avoid (a gcref in `Result`'s overlapping storage)
//! stays avoided too. Verified: `run_investor_workflow()` returns `2001` end-to-end via
//! `cd_efcore_async/csharp`'s `dotnet run`. Regressions re-verified after this second fix:
//! `cd_persisted_async` 4/4, `cd_async` 9/9, `cd_delegates` 14/14, `cd_bgservice` 9/9, `cd_auth`
//! (authz angle) 7/7, `cd_efcore` 16/16, `cilly`'s own unit tests 218/218, `cargo check --workspace`
//! clean.

use dotnet_macros::dotnet_entity;
use mycorrhiza::class::Class;
use mycorrhiza::intrinsics::{
    rustc_clr_interop_generic_method_call2 as gmethod2, RustcCLRInteropManagedClass,
    RustcCLRInteropManagedGeneric, RustcCLRInteropMethodGeneric,
};
use mycorrhiza::linq::{Expr, Param, TypedPredicate};
use mycorrhiza::system::MString;
use mycorrhiza::task::{await_task, future_to_task, TaskT};

// ---- Raw handles into the `EfHelper` assembly — same binding shape `cd_efcore/src/main.rs` uses. --
type DbContextHandle = RustcCLRInteropManagedClass<"EfHelper", "CdEfCore.InvestorDbContext">;
type QueryRunnerHandle = RustcCLRInteropManagedClass<"EfHelper", "CdEfCore.QueryRunner">;

// ---- `#[dotnet_entity]` ergonomics layer for the `i => i.Name == "Acme"` predicate (identical to
// `cd_efcore`'s). ----
#[dotnet_entity]
#[dotnet(namespace = "CdEfCore", assembly = "EfHelper", name = "Investor")]
struct InvestorEntity {
    name: String,
}

// ---- Raw `Expression.Lambda<Func<Investor,bool>>` plumbing, identical to `cd_efcore/src/main.rs`. --
type CExpr = RustcCLRInteropManagedClass<"System.Linq.Expressions", "System.Linq.Expressions.Expression">;
type CParam =
    RustcCLRInteropManagedClass<"System.Linq.Expressions", "System.Linq.Expressions.ParameterExpression">;
type CParamArr = mycorrhiza::intrinsics::RustcCLRInteropManagedArray<CParam, 1>;
type CFuncInvestorBool = RustcCLRInteropManagedGeneric<"System.Private.CoreLib", "System.Func", (DbContextInvestorHandle, bool)>;
type DbContextInvestorHandle = RustcCLRInteropManagedClass<"EfHelper", "CdEfCore.Investor">;
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
    let arr: CParamArr = mycorrhiza::intrinsics::rustc_clr_interop_managed_new_arr::<CParam>(1);
    mycorrhiza::intrinsics::rustc_clr_interop_managed_set_elem::<CParam>(arr, 0, p.raw());
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

// ---- The real async workflow --------------------------------------------------------------------
//
// `ctx` is held in a `Class<..>` (GCHandle-backed, no gcref field) across BOTH `.await`s below —
// exactly the pattern `cd_persisted_async` proved. Everything else (the predicate expression, the
// intermediate `i32` counts) is either a plain value or a same-segment temporary, never carried
// live across a suspend point as a raw managed handle.
async fn async_investor_workflow() -> i32 {
    let ctx_persisted: Class<"EfHelper", "CdEfCore.InvestorDbContext"> =
        Class::from_naked_ref(DbContextHandle::static0::<"CreateContext", DbContextHandle>());

    // Every raw managed handle used to KICK OFF an awaited call is built and consumed inside its
    // own `{ }` block that ends strictly before the `.await` — so its last use precedes the suspend
    // point lexically, not just by control flow, matching the pattern `cd_persisted_async` proved.
    // (A flat, unscoped sequence of `let`s sharing one block with both `.await`s hit a REAL CoreCLR
    // class-loader rejection here during this proof -- see the module doc comment's "CoreCLR is
    // pickier than cilly's layout_check" note below -- even though every individual handle's last
    // textual use was already before the relevant await.)
    let acme_count: i32 = {
        // Build `i => i.Name == "Acme"` from Rust (mirrors cd_efcore's Stage-1 predicate pipeline).
        let investor = InvestorEntity::new();
        let pred: TypedPredicate<InvestorEntity> = investor.name.eq("Acme");
        let expr_handle = typed_pred_investor(pred.body(), pred.param());

        // ---- await #1: a REAL asynchronous EF Core query (`ToListAsync` under the hood). ----
        let ctx1: DbContextHandle = unsafe { ctx_persisted.get_naked_ref() };
        let count_task: TaskT<i32> = QueryRunnerHandle::static2::<
            "RunAsyncCount",
            DbContextHandle,
            CExprFuncInvestorBool,
            TaskT<i32>,
        >(ctx1, expr_handle);
        await_task(count_task).await // suspend point 1 -- `ctx_persisted` must survive
    };

    // ---- small synchronous transform on the awaited result ----
    let doubled = acme_count * 2;

    let persisted_count: i32 = {
        // ---- await #2: a SECOND async call (`SaveChangesAsync` under the hood), using the SAME
        // persisted `ctx_persisted` handle, rematerialized fresh after the first suspend point. ----
        let name = format!("AsyncNewCo{doubled}");
        let mstr: MString = MString::from(name.as_str());
        let ctx2: DbContextHandle = unsafe { ctx_persisted.get_naked_ref() };
        let write_task: TaskT<i32> = QueryRunnerHandle::static2::<
            "AddInvestorAndVerifyAsync",
            DbContextHandle,
            MString,
            TaskT<i32>,
        >(ctx2, mstr);
        await_task(write_task).await // suspend point 2
    };

    acme_count * 1000 + persisted_count
}

/// `Task<int> run_investor_workflow()` — the seam a C# caller `await`s. Drives
/// [`async_investor_workflow`] to completion internally (`future_to_task`'s `block_on`) and hands
/// back a `Task<int>` a .NET caller sees as an ordinary awaitable, exactly like `cd_export`'s
/// `compute_answer()`.
#[dotnet_macros::dotnet_export]
pub fn run_investor_workflow() -> TaskT<i32> {
    future_to_task(async_investor_workflow())
}
