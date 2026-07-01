//! Building `System.Linq.Expressions` trees from Rust — the shape LINQ providers (EF Core, `IQueryable`)
//! consume.
//!
//! An `IQueryable` provider does NOT run a predicate in-process: it *walks the expression-tree
//! structure* and translates it (e.g. to SQL). So a Rust-built tree whose structure round-trips
//! (`ToString`) and compiles (`LambdaExpression.Compile`) is exactly what EF Core needs to see. This
//! module gives an ergonomic, allocation-free builder over the `Expression` factory:
//!
//! ```ignore
//! use mycorrhiza::linq::*;
//! // The realistic EF shape — filter on a *property*: x => x.Length > 5
//! let x = Param::new("System.String", "x");
//! let pred = x.expr().prop("Length").gt(Expr::const_i32(5)).lambda(&[&x]);
//! assert_eq!(pred.text(), "x => (x.Length > 5)");
//! let f = pred.compile();
//! assert!(f.call_str("hello!")); // 6 > 5  -> true
//! assert!(!f.call_str("hi"));    // 2 > 5  -> false
//! ```
//!
//! Everything is built from the ordinary managed-interop primitives (static calls, `newarr`/`stelem`,
//! `castclass`, and the value→`object` `box`). The pieces of a real predicate are all here:
//! **parameters** ([`Param`]), **member access** ([`Expr::prop`] — `x.Age`), **value-** and
//! **string-constants** ([`Expr::const_i32`]/[`Expr::const_str`]), **comparison/logical** combinators,
//! and a **[`Expr::lambda`]**. A built tree can be inspected ([`Lambda::text`] = `Expression.ToString`,
//! what a query provider translates) AND executed end-to-end ([`Lambda::compile`] then
//! [`Compiled::call_str`]/[`Compiled::call_i32`], returning the real boolean).
//!
//! The full **EF `IQueryable.Where` handoff** is also here ([`IntQuery`]): a strongly-typed
//! `Expression<Func<int,bool>>` (built via the *generic* `Expression.Lambda<T>` — [`Expr::typed_pred`])
//! is passed to `Queryable.Where<int>`, exactly as EF Core consumes a predicate to translate to SQL.
//! ```ignore
//! let n = IntQuery::range(1, 10)                                    // IQueryable<int> over 1..=10
//!     .where_(a.expr().gt(Expr::const_i32(5)).typed_pred(&a))       // Where(Expression<Func<int,bool>>)
//!     .count();                                                     // == 5  ({6,7,8,9,10})
//! ```
//! This crosses the nested-generic-value production path (`Expression<Func<int,bool>>`, `IQueryable<int>`)
//! end to end — a generic method returning a nested-generic value, held in a Rust local, and fed to
//! another generic method whose parameter is the doubly-nested `Expression<Func<!!0,bool>>`.

use crate::intrinsics::{
    rustc_clr_interop_box as box_value, rustc_clr_interop_generic_method_call1 as gmethod1,
    rustc_clr_interop_generic_method_call2 as gmethod2, rustc_clr_interop_managed_checked_cast as cast,
    rustc_clr_interop_managed_new_arr as new_arr, rustc_clr_interop_managed_set_elem as set_elem,
    RustcCLRInteropManagedArray, RustcCLRInteropManagedClass, RustcCLRInteropManagedGeneric,
    RustcCLRInteropMethodGeneric,
};
use crate::system::{DotNetString, MString};

// Managed-handle aliases for the types we touch. `Expression` and its subtypes live in the
// `System.Linq.Expressions` assembly; `Type`/`Delegate` in CoreLib.
type CExpr = RustcCLRInteropManagedClass<"System.Linq.Expressions", "System.Linq.Expressions.Expression">;
type CParam =
    RustcCLRInteropManagedClass<"System.Linq.Expressions", "System.Linq.Expressions.ParameterExpression">;
type CBinary =
    RustcCLRInteropManagedClass<"System.Linq.Expressions", "System.Linq.Expressions.BinaryExpression">;
type CLambda =
    RustcCLRInteropManagedClass<"System.Linq.Expressions", "System.Linq.Expressions.LambdaExpression">;
type CConst =
    RustcCLRInteropManagedClass<"System.Linq.Expressions", "System.Linq.Expressions.ConstantExpression">;
type CMember =
    RustcCLRInteropManagedClass<"System.Linq.Expressions", "System.Linq.Expressions.MemberExpression">;
type CType = RustcCLRInteropManagedClass<"System.Private.CoreLib", "System.Type">;
type CDelegate = RustcCLRInteropManagedClass<"System.Private.CoreLib", "System.Delegate">;
type CObject = RustcCLRInteropManagedClass<"System.Private.CoreLib", "System.Object">;
type CConvert = RustcCLRInteropManagedClass<"System.Private.CoreLib", "System.Convert">;
/// A managed `object[]` (`DynamicInvoke`'s argument array).
type CObjArray = RustcCLRInteropManagedArray<CObject, 1>;

// ---- Strongly-typed predicate types for the EF `IQueryable<int>.Where` path ----
// `System.Func`2<int32,bool>` — a generic *delegate* instantiation.
type CFuncIB = RustcCLRInteropManagedGeneric<"System.Private.CoreLib", "System.Func", (i32, bool)>;
// `Expression`1<Func`2<int32,bool>>` — the NESTED-generic type EF's `Where` consumes.
type CExprFuncIB =
    RustcCLRInteropManagedGeneric<"System.Linq.Expressions", "System.Linq.Expressions.Expression", (CFuncIB,)>;
// The *def-shape* return of `Expression.Lambda<!!0>` — `Expression`1<!!0>`, where `!!0` is the method
// generic. Bound to `Func<int,bool>` at the call site; `check_generic_marker` proves the binding.
type CExprMethGen0 = RustcCLRInteropManagedGeneric<
    "System.Linq.Expressions",
    "System.Linq.Expressions.Expression",
    (RustcCLRInteropMethodGeneric<0>,),
>;
type CParamArr = RustcCLRInteropManagedArray<CParam, 1>;

// ---- The IQueryable pipeline types (concrete `<int>` + the `<!!0>` def-shapes) ----
type CIEnumInt =
    RustcCLRInteropManagedGeneric<"System.Private.CoreLib", "System.Collections.Generic.IEnumerable", (i32,)>;
type CIQueryInt = RustcCLRInteropManagedGeneric<"System.Linq.Expressions", "System.Linq.IQueryable", (i32,)>;
type CIEnumMG = RustcCLRInteropManagedGeneric<
    "System.Private.CoreLib",
    "System.Collections.Generic.IEnumerable",
    (RustcCLRInteropMethodGeneric<0>,),
>;
type CIQueryMG = RustcCLRInteropManagedGeneric<
    "System.Linq.Expressions",
    "System.Linq.IQueryable",
    (RustcCLRInteropMethodGeneric<0>,),
>;
// `Expression`1<Func`2<!!0,bool>>` — the doubly-nested def-shape of `Where`'s predicate parameter.
type CExprFuncMG = RustcCLRInteropManagedGeneric<
    "System.Linq.Expressions",
    "System.Linq.Expressions.Expression",
    (RustcCLRInteropManagedGeneric<"System.Private.CoreLib", "System.Func", (RustcCLRInteropMethodGeneric<0>, bool)>,),
>;
type CEnumerable = RustcCLRInteropManagedClass<"System.Linq", "System.Linq.Enumerable">;

fn mstr(s: &str) -> MString {
    DotNetString::from(s).handle()
}

fn to_rust(s: MString) -> std::string::String {
    DotNetString::from_handle(s).to_rust_string()
}

/// A node in an expression tree (`System.Linq.Expressions.Expression`).
#[derive(Clone, Copy)]
pub struct Expr {
    inner: CExpr,
}

/// A typed lambda parameter (`ParameterExpression`).
#[derive(Clone, Copy)]
pub struct Param {
    inner: CParam,
}

impl Param {
    /// A parameter of the named .NET type (assembly-resolvable, e.g. `"System.Int32"`,
    /// `"System.String"`) with a display name — `Expression.Parameter(Type.GetType(ty), name)`.
    #[must_use]
    pub fn new(type_name: &str, name: &str) -> Param {
        // `Type.GetType(string, bool throwOnError=false)`.
        let ty = CType::static2::<"GetType", MString, bool, CType>(mstr(type_name), false);
        let inner = CExpr::static2::<"Parameter", CType, MString, CParam>(ty, mstr(name));
        Param { inner }
    }

    /// Use this parameter as an operand (upcast `ParameterExpression` -> `Expression`).
    #[must_use]
    pub fn expr(self) -> Expr {
        Expr {
            inner: cast::<CExpr, CParam>(self.inner),
        }
    }
}

/// Build a `ConstantExpression` from a value-type literal boxed to `object` (the `box` primitive), then
/// upcast to `Expression`. `Expression.Constant(object)` records `ConstantExpression.Type` as the
/// runtime type of the boxed value, so an `int` boxes to a `System.Int32` constant.
fn constant<T>(v: T) -> Expr {
    let obj = box_value::<T>(v);
    let c = CExpr::static1::<"Constant", CObject, CConst>(obj);
    Expr {
        inner: cast::<CExpr, CConst>(c),
    }
}

/// Emit one `Expression.<Op>(a, b) -> BinaryExpression`, upcast to `Expression`.
fn binop<const OP: &'static str>(a: Expr, b: Expr) -> Expr {
    let bin = CExpr::static2::<OP, CExpr, CExpr, CBinary>(a.inner, b.inner);
    Expr {
        inner: cast::<CExpr, CBinary>(bin),
    }
}

impl Expr {
    /// An `Int32` literal — `Expression.Constant((object)v)`. Boxes `v` to `System.Object`, so a real
    /// value-constant filter like `x.gt(Expr::const_i32(5))` (== `x => x > 5`) is expressible.
    #[must_use]
    pub fn const_i32(v: i32) -> Expr {
        constant::<i32>(v)
    }
    /// An `Int64` literal — `Expression.Constant((object)v)`.
    #[must_use]
    pub fn const_i64(v: i64) -> Expr {
        constant::<i64>(v)
    }

    /// A `String` literal — `Expression.Constant((object)s)`. A string is a reference type, so it
    /// upcasts to `object` with a `castclass` (no box needed). Enables `name.eq(Expr::const_str("x"))`.
    #[must_use]
    pub fn const_str(s: &str) -> Expr {
        let obj = cast::<CObject, MString>(mstr(s));
        let c = CExpr::static1::<"Constant", CObject, CConst>(obj);
        Expr {
            inner: cast::<CExpr, CConst>(c),
        }
    }

    /// Access a property or field `name` on this expression — `Expression.PropertyOrField(self, name)`.
    /// This is THE realistic EF shape: `p.prop("Age")` builds the `x.Age` in `x => x.Age > 18`. The
    /// member's static type flows through, so a subsequent comparison type-checks (e.g. a `string`
    /// parameter's `.prop("Length")` is an `int`, comparable to an `int` constant).
    #[must_use]
    pub fn prop(self, name: &str) -> Expr {
        let m = CExpr::static2::<"PropertyOrField", CExpr, MString, CMember>(self.inner, mstr(name));
        Expr {
            inner: cast::<CExpr, CMember>(m),
        }
    }

    /// `a > b`
    #[must_use]
    pub fn gt(self, other: Expr) -> Expr {
        binop::<"GreaterThan">(self, other)
    }
    /// `a < b`
    #[must_use]
    pub fn lt(self, other: Expr) -> Expr {
        binop::<"LessThan">(self, other)
    }
    /// `a >= b`
    #[must_use]
    pub fn ge(self, other: Expr) -> Expr {
        binop::<"GreaterThanOrEqual">(self, other)
    }
    /// `a == b`
    #[must_use]
    pub fn eq(self, other: Expr) -> Expr {
        binop::<"Equal">(self, other)
    }
    /// `a && b`
    #[must_use]
    pub fn and(self, other: Expr) -> Expr {
        binop::<"AndAlso">(self, other)
    }
    /// `a || b`
    #[must_use]
    pub fn or(self, other: Expr) -> Expr {
        binop::<"OrElse">(self, other)
    }

    /// The provider-visible rendering of this node (`Expression.ToString()`).
    #[must_use]
    pub fn text(self) -> std::string::String {
        to_rust(self.inner.virt0::<"ToString", MString>())
    }

    /// Wrap this body in a lambda over `params` — `Expression.Lambda(body, ParameterExpression[])`.
    /// Uses the NON-generic `Lambda` overload returning `LambdaExpression`, which sidesteps producing
    /// a nested-generic `Expression<Func<..>>` value.
    #[must_use]
    pub fn lambda(self, params: &[&Param]) -> Lambda {
        let arr: RustcCLRInteropManagedArray<CParam, 1> = new_arr::<CParam>(params.len() as i32);
        let mut i = 0i32;
        for p in params {
            set_elem::<CParam>(arr, i, p.inner);
            i += 1;
        }
        let inner = CExpr::static2::<
            "Lambda",
            CExpr,
            RustcCLRInteropManagedArray<CParam, 1>,
            CLambda,
        >(self.inner, arr);
        Lambda { inner }
    }

    /// Wrap this body into a STRONGLY-TYPED `Expression<Func<i32,bool>>` over the single `i32`
    /// parameter `p`, via the GENERIC `Expression.Lambda<TDelegate>(body, ParameterExpression[])`
    /// method (`!!0 = Func<int,bool>`). This is the nested-generic value EF's
    /// `IQueryable<int>.Where` consumes — producing it exercises the generic-method + nested-generic
    /// path end-to-end (`call_gmethod` + the `is_assignable_to` structural arm; `check_generic_marker`
    /// proves `!!0` binds to `Func<int,bool>`).
    #[must_use]
    pub fn typed_pred(self, p: &Param) -> Predicate {
        let arr: CParamArr = new_arr::<CParam>(1);
        set_elem::<CParam>(arr, 0, p.inner);
        // Expression.Lambda<Func<int,bool>>(body: Expression, prms: ParameterExpression[])
        //   KIND=0 (static), ClassGenerics=() (Expression is a non-generic declaring class),
        //   MethodGenerics=(Func<int,bool>,), Sig = (ret: Expression<!!0>, body: Expression, prms[]).
        let inner: CExprFuncIB = gmethod2::<
            "System.Linq.Expressions",
            "System.Linq.Expressions.Expression",
            false,
            "Lambda",
            0,
            (),
            (CFuncIB,),
            (CExprMethGen0, CExpr, CParamArr),
            CExprFuncIB,
            CExpr,
            CParamArr,
        >(self.inner, arr);
        Predicate { inner }
    }
}

/// A strongly-typed predicate — `Expression<Func<int,bool>>`, the exact type EF Core's
/// `IQueryable<int>.Where(Expression<Func<int,bool>>)` consumes.
#[derive(Clone, Copy)]
pub struct Predicate {
    inner: CExprFuncIB,
}

impl Predicate {
    /// The provider-visible rendering — upcast the nested-generic `Expression<Func<int,bool>>` to the
    /// base `Expression` and `ToString()`.
    #[must_use]
    pub fn text(self) -> std::string::String {
        let base = cast::<CExpr, CExprFuncIB>(self.inner);
        to_rust(base.virt0::<"ToString", MString>())
    }
}

/// An `IQueryable<int>` — an EF-Core-style query source that consumes `Expression<Func<int,bool>>`
/// predicates (it TRANSLATES them, unlike `IEnumerable.Where`, which takes a compiled `Func`). This is
/// the actual `IQueryable.Where(Expression<Func>)` handoff — the whole point of building expression
/// trees. All three operators are generic methods on `System.Linq.Queryable` (`!!0 = int`).
#[derive(Clone, Copy)]
pub struct IntQuery {
    inner: CIQueryInt,
}

impl IntQuery {
    /// A source `IQueryable<int>` over `start .. start+count` — `Enumerable.Range(start, count)`
    /// (an `IEnumerable<int>`) then `Queryable.AsQueryable<int>` (the generic promotion to a query).
    #[must_use]
    pub fn range(start: i32, count: i32) -> IntQuery {
        let seq: CIEnumInt = CEnumerable::static2::<"Range", i32, i32, CIEnumInt>(start, count);
        // Queryable.AsQueryable<int>(IEnumerable<int>) -> IQueryable<int>
        let inner: CIQueryInt = gmethod1::<
            "System.Linq.Queryable",
            "System.Linq.Queryable",
            false,
            "AsQueryable",
            0,
            (),
            (i32,),
            (CIQueryMG, CIEnumMG),
            CIQueryInt,
            CIEnumInt,
        >(seq);
        IntQuery { inner }
    }

    /// Filter with a predicate expression TREE — `Queryable.Where<int>(this,
    /// Expression<Func<int,bool>>)`. The provider receives the tree (it would translate to SQL); the
    /// in-memory LINQ-to-Objects provider compiles+runs it.
    #[must_use]
    pub fn where_(self, pred: Predicate) -> IntQuery {
        let inner: CIQueryInt = gmethod2::<
            "System.Linq.Queryable",
            "System.Linq.Queryable",
            false,
            "Where",
            0,
            (),
            (i32,),
            (CIQueryMG, CIQueryMG, CExprFuncMG),
            CIQueryInt,
            CIQueryInt,
            CExprFuncIB,
        >(self.inner, pred.inner);
        IntQuery { inner }
    }

    /// Materialize the count — `Queryable.Count<int>(this)`.
    #[must_use]
    pub fn count(self) -> i32 {
        gmethod1::<
            "System.Linq.Queryable",
            "System.Linq.Queryable",
            false,
            "Count",
            0,
            (),
            (i32,),
            (i32, CIQueryMG),
            i32,
            CIQueryInt,
        >(self.inner)
    }
}

/// A compiled-or-uncompiled lambda expression (`LambdaExpression`).
#[derive(Clone, Copy)]
pub struct Lambda {
    inner: CLambda,
}

impl Lambda {
    /// The provider-visible rendering (`x => (x > 5)` etc.).
    #[must_use]
    pub fn text(self) -> std::string::String {
        to_rust(self.inner.virt0::<"ToString", MString>())
    }

    /// Compile the tree and report whether it produced a real, non-null delegate — i.e. the tree is a
    /// semantically valid, JIT-compilable predicate (what a provider relies on for client-side
    /// evaluation). `LambdaExpression.Compile()` throws on a malformed tree, so a non-null result is a
    /// strong witness of well-formedness.
    #[must_use]
    pub fn compiles(self) -> bool {
        let del: CDelegate = self.inner.instance0::<"Compile", CDelegate>();
        let as_obj = cast::<CObject, CDelegate>(del);
        !CObject::static2::<"ReferenceEquals", CObject, CObject, bool>(as_obj, CObject::null())
    }

    /// Compile the tree to an invocable delegate — `LambdaExpression.Compile()`.
    #[must_use]
    pub fn compile(self) -> Compiled {
        Compiled {
            del: self.inner.instance0::<"Compile", CDelegate>(),
        }
    }
}

/// A compiled predicate — the `Func<..>` a provider would run for client-side evaluation. Its
/// arguments cross as a boxed `object[]` and the boolean result unboxes via `Convert.ToBoolean`, so
/// this executes the tree end-to-end and returns the real answer (not just "it compiled").
#[derive(Clone, Copy)]
pub struct Compiled {
    del: CDelegate,
}

impl Compiled {
    /// Invoke a single-parameter `bool` predicate with a **string** argument (a reference type — passed
    /// through as `object` with a `castclass`). E.g. run `x => x.Length > 5` against `"hello"`.
    #[must_use]
    pub fn call_str(self, arg: &str) -> bool {
        let args: CObjArray = new_arr::<CObject>(1);
        set_elem::<CObject>(args, 0, cast::<CObject, MString>(mstr(arg)));
        self.invoke(args)
    }

    /// Invoke a single-parameter `bool` predicate with an **i32** argument (a value type — boxed into
    /// `object` via the `box` primitive). E.g. run `a => a > 5` against `7`.
    #[must_use]
    pub fn call_i32(self, arg: i32) -> bool {
        let args: CObjArray = new_arr::<CObject>(1);
        set_elem::<CObject>(args, 0, box_value::<i32>(arg));
        self.invoke(args)
    }

    fn invoke(self, args: CObjArray) -> bool {
        // `Delegate.DynamicInvoke(object[]) -> object`; the boxed `bool` result is unboxed by
        // `Convert.ToBoolean`.
        let res: CObject = self
            .del
            .instance1::<"DynamicInvoke", CObjArray, CObject>(args);
        CConvert::static1::<"ToBoolean", CObject, bool>(res)
    }
}
