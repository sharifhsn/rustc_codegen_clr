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
//! let a = Param::new("System.Int32", "a");
//! let b = Param::new("System.Int32", "b");
//! let pred = a.expr().gt(b.expr()).lambda(&[&a, &b]); // (a, b) => (a > b)
//! assert_eq!(pred.text(), "(a, b) => (a > b)");
//! assert!(pred.compiles());                            // JIT-valid -> a real Func<int,int,bool>
//! ```
//!
//! Everything is built from the ordinary managed-interop primitives (static calls, `newarr`/`stelem`,
//! `castclass`) — no bespoke backend support. `Expression.Constant(object)` for a *value-type* literal
//! (`x => x > 5`) additionally needs a value→`object` box, which the backend does not yet emit; use a
//! parameter-vs-parameter or reference-typed comparison until that lands.

use crate::intrinsics::{
    rustc_clr_interop_managed_checked_cast as cast, rustc_clr_interop_managed_new_arr as new_arr,
    rustc_clr_interop_managed_set_elem as set_elem, RustcCLRInteropManagedArray,
    RustcCLRInteropManagedClass,
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
type CType = RustcCLRInteropManagedClass<"System.Private.CoreLib", "System.Type">;
type CDelegate = RustcCLRInteropManagedClass<"System.Private.CoreLib", "System.Delegate">;
type CObject = RustcCLRInteropManagedClass<"System.Private.CoreLib", "System.Object">;

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

/// Emit one `Expression.<Op>(a, b) -> BinaryExpression`, upcast to `Expression`.
fn binop<const OP: &'static str>(a: Expr, b: Expr) -> Expr {
    let bin = CExpr::static2::<OP, CExpr, CExpr, CBinary>(a.inner, b.inner);
    Expr {
        inner: cast::<CExpr, CBinary>(bin),
    }
}

impl Expr {
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
}
