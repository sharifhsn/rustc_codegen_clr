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
    rustc_clr_interop_generic_method_call2 as gmethod2, rustc_clr_interop_generic_method_call5 as gmethod5,
    rustc_clr_interop_managed_checked_cast as cast, rustc_clr_interop_managed_new_arr as new_arr,
    rustc_clr_interop_managed_set_elem as set_elem, RustcCLRInteropManagedArray, RustcCLRInteropManagedClass,
    RustcCLRInteropManagedGeneric, RustcCLRInteropMethodGeneric,
};
use crate::system::{DotNetString, MString};
use std::marker::PhantomData;

// Managed-handle aliases for the types we touch. `Expression` and its subtypes live in the
// `System.Linq.Expressions` assembly; `Type`/`Delegate` in CoreLib.
type CExpr = RustcCLRInteropManagedClass<"System.Linq.Expressions", "System.Linq.Expressions.Expression">;
type CParam =
    RustcCLRInteropManagedClass<"System.Linq.Expressions", "System.Linq.Expressions.ParameterExpression">;
type CBinary =
    RustcCLRInteropManagedClass<"System.Linq.Expressions", "System.Linq.Expressions.BinaryExpression">;
type CUnary =
    RustcCLRInteropManagedClass<"System.Linq.Expressions", "System.Linq.Expressions.UnaryExpression">;
type CLambda =
    RustcCLRInteropManagedClass<"System.Linq.Expressions", "System.Linq.Expressions.LambdaExpression">;
type CConst =
    RustcCLRInteropManagedClass<"System.Linq.Expressions", "System.Linq.Expressions.ConstantExpression">;
type CMember =
    RustcCLRInteropManagedClass<"System.Linq.Expressions", "System.Linq.Expressions.MemberExpression">;
type CMethodCall =
    RustcCLRInteropManagedClass<"System.Linq.Expressions", "System.Linq.Expressions.MethodCallExpression">;
type CType = RustcCLRInteropManagedClass<"System.Private.CoreLib", "System.Type">;
type CMethodInfo = RustcCLRInteropManagedClass<"System.Private.CoreLib", "System.Reflection.MethodInfo">;
type CDelegate = RustcCLRInteropManagedClass<"System.Private.CoreLib", "System.Delegate">;
type CObject = RustcCLRInteropManagedClass<"System.Private.CoreLib", "System.Object">;
type CConvert = RustcCLRInteropManagedClass<"System.Private.CoreLib", "System.Convert">;
/// A managed `object[]` (`DynamicInvoke`'s argument array).
type CObjArray = RustcCLRInteropManagedArray<CObject, 1>;
/// A managed `Type[]` (for `GetMethod` argument-type lookups).
type CTypeArr = RustcCLRInteropManagedArray<CType, 1>;
/// A managed `Expression[]` (for `Expression.Call`'s arguments array).
type CExprArr = RustcCLRInteropManagedArray<CExpr, 1>;

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

    /// The raw `ParameterExpression` managed handle. An escape hatch for callers building interop
    /// plumbing this module doesn't wrap directly (e.g. constructing `Expression.Lambda<Func<T,bool>>`
    /// for a caller's own entity type `T`, as `TypedPredicate<T>`'s module doc describes) — spelled out
    /// as the full `RustcCLRInteropManagedClass` instantiation (not the private `CParam` alias) so it's
    /// nameable from outside this module.
    #[must_use]
    pub fn raw(
        self,
    ) -> crate::intrinsics::RustcCLRInteropManagedClass<
        "System.Linq.Expressions",
        "System.Linq.Expressions.ParameterExpression",
    > {
        self.inner
    }
}

/// Declare the crate-wide default .NET namespace (and, by the small-project convention documented on
/// [`Field`]/`#[dotnet_entity]`, the default assembly name too) for every `#[dotnet_entity]` struct in
/// this crate that doesn't override it explicitly. Invoke this ONCE, anywhere in the crate (typically
/// near the crate root, e.g. the top of `lib.rs`/`main.rs`) — ordinary Rust name resolution makes the
/// declaration visible to every module regardless of where it appears, exactly like any other
/// crate-root `const` referenced from elsewhere in the same crate:
///
/// ```ignore
/// mycorrhiza::linq::dotnet_namespace!("LinqDemo");
///
/// #[dotnet_entity]
/// struct Person { id: i32, name: String, age: i32, is_active: bool }
/// // -> class "Person", namespace "LinqDemo", assembly "LinqDemo" (namespace == assembly by convention)
/// // -> also generates `Person::new()` (a const fn constructor) + `impl Default for Person`; the
/// //    caller writes `let person = Person::new();` themselves, then `person.age.ge(18)` reads it via
/// //    real field access — no hidden, auto-generated singleton.
/// ```
///
/// Expands to exactly one item: `pub(crate) const __MYCORRHIZA_DOTNET_NAMESPACE_DEFAULT: &str = <lit>;`
/// — this fixed, reserved name is the ONLY contract between this macro and `#[dotnet_entity]`. The
/// entity macro does not need to see this invocation at macro-expansion time (proc-macros have no
/// cross-item visibility into other macro invocations); it just emits code that references
/// `crate::__MYCORRHIZA_DOTNET_NAMESPACE_DEFAULT` BY NAME when a struct doesn't override its
/// namespace/assembly, and ordinary Rust item resolution (at the consuming crate's normal compile time,
/// long after either macro has expanded) finds this `const` no differently than it would find any other
/// two independently-defined items referencing each other. There is no proc-macro-level global state
/// involved.
///
/// If `#[dotnet_entity]` is used on a struct that has NEITHER a `#[dotnet(namespace = "...")]` NOR a
/// `#[dotnet(assembly = "...")]` override, AND this macro was never invoked anywhere in the crate, the
/// generated code fails to compile with a plain "cannot find const
/// `__MYCORRHIZA_DOTNET_NAMESPACE_DEFAULT` in this scope" error. That failure is correct and intended —
/// it means there is no default namespace/assembly to fall back to and none was given explicitly.
#[macro_export]
macro_rules! dotnet_namespace {
    ($ns:literal) => {
        #[doc(hidden)]
        pub(crate) const __MYCORRHIZA_DOTNET_NAMESPACE_DEFAULT: &str = $ns;
    };
}
// `#[macro_export]` places the macro at the CRATE ROOT (`mycorrhiza::dotnet_namespace!`) regardless of
// where in the source tree it's defined — this re-export makes it ALSO reachable at
// `mycorrhiza::linq::dotnet_namespace!`, matching this module's public surface (callers shouldn't need
// to know the macro physically lives in `linq.rs` vs. some other module).
pub use crate::dotnet_namespace;

/// A typed, named .NET property/field on an entity `Root` — e.g. `person.age: Field<Person, i32>` (a
/// real struct field on a `Person` value the caller constructs themselves via `Person::new()`, the
/// explicit constructor `#[dotnet_entity]` generates).
///
/// This is the ergonomic front door to the predicate-building machinery above: instead of hand-writing
/// `Param::new(type_name, "p")` + `.expr().prop("Age")` + `Expr::const_i32(v)` + `TypedPredicate::new(..)`
/// at every call site, a `Field<Root, Val>` value (generated once, by `#[dotnet_entity]` in
/// `dotnet_macros`, as a RETYPED field on the entity struct, populated via that struct's generated
/// `::new()`/`Default::default()`) bundles the owning type's .NET namespace/class/assembly and the
/// property's .NET name, and its methods (`.eq`/`.gt`/`.contains`/`.is_true`/…, added per `Val` below) go
/// straight from a RAW Rust value to a finished `TypedPredicate<Root>` — no `Param`, `Expr`, or
/// property-name string ever touched by a caller. Because the value is constructed explicitly by the
/// caller, building a predicate reads as real Rust field access on a visible binding —
/// `let person = Person::new(); person.age.ge(18) & person.name.contains("a")` — rather than a
/// `::`-qualified associated-const path (an earlier version of this API generated each `Field` as an
/// associated const, e.g. `Person::AGE`, which forced `::` path syntax at every call site; a later
/// version replaced that with a hidden, auto-generated singleton `const`, which user feedback found "too
/// magical" since the binding appeared with no visible declaration — both are fully replaced by the
/// explicit-constructor generation `#[dotnet_entity]` does now).
///
/// The owning type's `Type.GetType`-resolvable spec (`"Namespace.Class, Assembly"`, what [`Param::new`]
/// wants) is assembled from three SEPARATE pieces — `namespace`, `class`, `assembly` — rather than
/// stored pre-joined, because each piece is independently overridable via `#[dotnet_entity]`'s
/// escape-hatch attributes (`#[dotnet(namespace = "...")]` / `#[dotnet(assembly = "...")]` /
/// `#[dotnet(name = "...")]`), and joining three `&'static str` consts into one at Rust-const-eval time
/// would need `concat!`, which only accepts literals — not references to another item's `const`. Since
/// [`Param::new`] already takes a plain (non-const) `&str`, the join happens with an ordinary runtime
/// `format!` inside [`Field::type_name_spec`], not at const-evaluation time; this keeps `Field::new`
/// itself a `const fn` (so `#[dotnet_entity]` can still emit the whole singleton, one `Field::new(..)`
/// per field, as a single `const` value) while letting each piece resolve independently.
///
/// `Root`/`Val` are phantom type parameters (see `TypedPredicate`'s module doc) — `Field` itself carries
/// no runtime payload beyond its `&'static str`s, so it is `Copy` regardless of `Root`/`Val` (manual
/// impls below, matching `TypedPredicate`'s own reasoning for not `#[derive]`ing).
pub struct Field<Root, Val> {
    /// The owning entity's .NET namespace, e.g. `"LinqDemo"`.
    namespace: &'static str,
    /// The owning entity's .NET class name (no namespace prefix), e.g. `"Person"`.
    class: &'static str,
    /// The owning entity's .NET assembly (simple) name, e.g. `"LinqDemo"`.
    assembly: &'static str,
    /// The .NET property/field name, e.g. `"Age"` — fed to `Expr::prop`.
    prop_name: &'static str,
    _root: PhantomData<fn() -> Root>,
    _val: PhantomData<fn() -> Val>,
}

impl<Root, Val> Clone for Field<Root, Val> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<Root, Val> Copy for Field<Root, Val> {}

impl<Root, Val> Field<Root, Val> {
    /// Construct a field descriptor from its three .NET-identity pieces (namespace, bare class name,
    /// assembly simple name) plus the .NET property/field name. Callers should not normally spell this
    /// out by hand — it is what `#[dotnet_entity]` generates one of per struct field, as an initializer
    /// inside the generated singleton's `const` literal — but it is `pub const fn` so that singleton
    /// itself can be a real `const`.
    #[must_use]
    pub const fn new(
        namespace: &'static str,
        class: &'static str,
        assembly: &'static str,
        prop_name: &'static str,
    ) -> Self {
        Field {
            namespace,
            class,
            assembly,
            prop_name,
            _root: PhantomData,
            _val: PhantomData,
        }
    }

    /// The `Type.GetType`-resolvable spec for this field's owning entity — `"Namespace.Class, Assembly"`
    /// — joined at ordinary runtime (see the type-level doc for why this can't be a `const` join).
    fn type_name_spec(self) -> std::string::String {
        format!("{}.{}, {}", self.namespace, self.class, self.assembly)
    }

    /// Build the `Param` this field's property access is expressed over, plus the `x.Prop` member
    /// access itself. A fixed lambda-parameter name (`"p"`) is fine even across independently-called
    /// `Field` methods within one combined predicate — `TypedPredicate`'s `BitAnd`/`BitOr` already
    /// detect and rebind mismatched `Param`s (see `combine` above), so two `Field` calls building two
    /// separate `TypedPredicate<Root>`s that both happen to use `"p"` are handled exactly like any two
    /// independently-authored predicates.
    fn param_and_prop(self) -> (Param, Expr) {
        let p = Param::new(&self.type_name_spec(), "p");
        let e = p.expr().prop(self.prop_name);
        (p, e)
    }
}

macro_rules! field_numeric_cmp {
    ($ty:ty, $const_fn:ident) => {
        impl<Root> Field<Root, $ty> {
            fn cmp<const OP: &'static str>(self, v: $ty) -> TypedPredicate<Root> {
                let (p, e) = self.param_and_prop();
                TypedPredicate::new(p, binop::<OP>(e, Expr::$const_fn(v)))
            }
            /// `field == v`.
            #[must_use]
            pub fn eq(self, v: $ty) -> TypedPredicate<Root> {
                self.cmp::<"Equal">(v)
            }
            /// `field > v`.
            #[must_use]
            pub fn gt(self, v: $ty) -> TypedPredicate<Root> {
                self.cmp::<"GreaterThan">(v)
            }
            /// `field >= v`.
            #[must_use]
            pub fn ge(self, v: $ty) -> TypedPredicate<Root> {
                self.cmp::<"GreaterThanOrEqual">(v)
            }
            /// `field < v`.
            #[must_use]
            pub fn lt(self, v: $ty) -> TypedPredicate<Root> {
                self.cmp::<"LessThan">(v)
            }
            /// `field <= v`.
            #[must_use]
            pub fn le(self, v: $ty) -> TypedPredicate<Root> {
                self.cmp::<"LessThanOrEqual">(v)
            }
        }
    };
}

field_numeric_cmp!(i32, const_i32);
field_numeric_cmp!(i64, const_i64);

impl<Root> Field<Root, String> {
    /// `field == v`.
    #[must_use]
    pub fn eq(self, v: &str) -> TypedPredicate<Root> {
        let (p, e) = self.param_and_prop();
        TypedPredicate::new(p, e.eq(Expr::const_str(v)))
    }
    /// `field.Contains(v)` — the standard EF-translatable substring filter (`LIKE '%v%'` in SQL).
    #[must_use]
    pub fn contains(self, v: &str) -> TypedPredicate<Root> {
        let (p, e) = self.param_and_prop();
        TypedPredicate::new(p, e.call1_same_type("Contains", Expr::const_str(v)))
    }
    /// `field.StartsWith(v)`.
    #[must_use]
    pub fn starts_with(self, v: &str) -> TypedPredicate<Root> {
        let (p, e) = self.param_and_prop();
        TypedPredicate::new(p, e.call1_same_type("StartsWith", Expr::const_str(v)))
    }
    /// `field.EndsWith(v)`.
    #[must_use]
    pub fn ends_with(self, v: &str) -> TypedPredicate<Root> {
        let (p, e) = self.param_and_prop();
        TypedPredicate::new(p, e.call1_same_type("EndsWith", Expr::const_str(v)))
    }
}

impl<Root> Field<Root, bool> {
    /// The field itself, as a predicate body — `p => p.Flag`.
    #[must_use]
    pub fn is_true(self) -> TypedPredicate<Root> {
        let (p, e) = self.param_and_prop();
        TypedPredicate::new(p, e)
    }
    /// The field's negation, as a predicate body — `p => !p.Flag`.
    #[must_use]
    pub fn is_false(self) -> TypedPredicate<Root> {
        let (p, e) = self.param_and_prop();
        TypedPredicate::new(p, e.not())
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

/// Emit one `Expression.<Op>(a) -> UnaryExpression`, upcast to `Expression`.
fn unop<const OP: &'static str>(a: Expr) -> Expr {
    let un = CExpr::static1::<OP, CExpr, CUnary>(a.inner);
    Expr {
        inner: cast::<CExpr, CUnary>(un),
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

    /// `!a` — `Expression.Not(a) -> UnaryExpression`.
    #[must_use]
    pub fn not(self) -> Expr {
        unop::<"Not">(self)
    }

    /// A one-argument instance-method call on this expression where BOTH operands share the same
    /// static .NET type (e.g. `string.Contains(string)`) — `Expression.Call(self, method, [arg])` where
    /// `method` is looked up via `self.Type.GetMethod(name, [self.Type])`. E.g.
    /// `p.prop("Name").call1_same_type("Contains", Expr::const_str("a"))` builds `p.Name.Contains("a")`
    /// — the standard EF-translatable substring-filter shape (`LIKE '%a%'` in SQL), which the
    /// comparison-only combinators above (`gt`/`lt`/`eq`/...) can't express since it's a method call,
    /// not an operator.
    #[must_use]
    pub fn call1_same_type(self, method_name: &str, arg: Expr) -> Expr {
        // `self.Type` — the static CLR type this expression node evaluates to (e.g. `System.String`
        // for `p.Name`), via the `Expression.Type` property every node has.
        let ty: CType = self.inner.instance0::<"get_Type", CType>();
        let targs: CTypeArr = new_arr::<CType>(1);
        set_elem::<CType>(targs, 0, ty);
        // Type.GetMethod(string, Type[]) -> MethodInfo
        let method: CMethodInfo = ty.instance2::<"GetMethod", MString, CTypeArr, CMethodInfo>(
            mstr(method_name),
            targs,
        );
        let args: CExprArr = new_arr::<CExpr>(1);
        set_elem::<CExpr>(args, 0, arg.inner);
        // Expression.Call(Expression instance, MethodInfo method, Expression[] arguments) ->
        // MethodCallExpression — a 3-arg static call, within the raw intrinsics' max arity.
        let call: CMethodCall = crate::intrinsics::rustc_clr_interop_managed_call3_::<
            "System.Linq.Expressions",
            "System.Linq.Expressions.Expression",
            false,
            "Call",
            true,
            CMethodCall,
            CExpr,
            CMethodInfo,
            CExprArr,
        >(self.inner, method, args);
        Expr {
            inner: cast::<CExpr, CMethodCall>(call),
        }
    }

    /// The raw `Expression` managed handle. An escape hatch for callers building interop plumbing this
    /// module doesn't wrap directly (see [`Param::raw`]).
    #[must_use]
    pub fn raw(
        self,
    ) -> crate::intrinsics::RustcCLRInteropManagedClass<
        "System.Linq.Expressions",
        "System.Linq.Expressions.Expression",
    > {
        self.inner
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

// ---- `TypedPredicate<T>` — combinable predicates via `&`/`|`/`!` (`PredicateBuilder`-equivalent) ----
//
// THE REAL C# PROBLEM THIS SOLVES: two independently-built `Expression<Func<T,bool>>` each carry their
// OWN `ParameterExpression` instance for the lambda parameter (`Param::new` allocates a fresh one every
// call). Naively combining their bodies with `Expression.AndAlso(a.Body, b.Body)` produces a tree that
// references TWO DIFFERENT parameters — it's structurally broken: `LambdaExpression.Compile()` either
// throws (`ParameterExpression not bound` style errors) or, when it does not throw, EF Core translates a
// tree where one side's variable is never bound to a query source at all. This is real and well-known:
// LINQKit's `PredicateBuilder` exists SOLELY to work around it, using a small `ExpressionVisitor`
// (`ParameterRebinder`) that walks one tree and rewrites every occurrence of its parameter to the OTHER
// tree's parameter before the two are combined. Hand-rolling this correctly is fiddly (most naive
// attempts forget nested lambdas, or rebind the wrong side, or leak the mismatched-parameter tree through
// `Compile()`/EF translation without erroring loudly) — so this module does it ONCE, here, and Rust
// callers just write `pred_a & pred_b`.
//
// `TypedPredicate<T>` is NOT Rust-generic over the .NET element type in any deep sense — `T` is a phantom
// marker distinguishing predicates meant for different entities at the Rust type level (so you can't
// accidentally combine a `TypedPredicate<Person>` with a `TypedPredicate<Order>`). The actual .NET typing
// of the final `Expression<Func<T,bool>>` (a nested-generic value keyed by a real CLR type, e.g.
// `Func<Person,bool>`) is deferred to wherever the caller hands the built body+param off to a
// `Expression.Lambda<Func<T,bool>>` construction site (mirrors `Expr::typed_pred`'s int-specialized
// version, generalized by the caller for their own entity type) — this keeps `linq.rs` itself free of a
// new const-generic type family per entity. `TypedPredicate` only carries the UNTYPED `Expr` body plus
// the `Param` it was built against, which is exactly what the parameter-rebinding fix needs to operate on.

/// The assembly name of the bundled `ParameterRebinder` helper (a ~15-line `ExpressionVisitor`
/// subclass, the same pattern LINQKit's `PredicateBuilder` uses internally) that
/// [`TypedPredicate`]'s `BitAnd`/`BitOr` combinators call to reconcile two predicates built against
/// different `ParameterExpression` instances. This is a plain runtime dependency of whatever binary
/// consumes `mycorrhiza::linq` (resolved the same way as any other .NET dependency DLL sitting next to
/// the app — normal `AssemblyLoadContext` probing by simple name, no special plumbing) — mycorrhiza does
/// not ship the C# source for it, only this name constant, matching the existing pattern of referencing
/// BCL assemblies by name without embedding their sources. See `docs/PARAMETER_REBINDER.md` (or the
/// consuming app's helper-assembly project) for the exact C# this name must resolve to:
///
/// ```csharp
/// namespace Mycorrhiza.Linq;
///
/// public sealed class ParameterRebinder : ExpressionVisitor
/// {
///     private readonly ParameterExpression _from;
///     private readonly ParameterExpression _to;
///
///     private ParameterRebinder(ParameterExpression from, ParameterExpression to)
///     {
///         _from = from;
///         _to = to;
///     }
///
///     public static Expression Rebind(Expression body, ParameterExpression from, ParameterExpression to)
///         => new ParameterRebinder(from, to).Visit(body)!;
///
///     protected override Expression VisitParameter(ParameterExpression node)
///         => ReferenceEquals(node, _from) ? _to : base.VisitParameter(node);
/// }
/// ```
///
/// Override with a different assembly if the consuming app names its helper assembly differently — this
/// constant is the single point of configuration.
pub const PARAMETER_REBINDER_ASSEMBLY: &str = "Mycorrhiza.Interop.Helpers";
/// The fully-qualified class name of the bundled helper (see [`PARAMETER_REBINDER_ASSEMBLY`]).
pub const PARAMETER_REBINDER_CLASS: &str = "Mycorrhiza.Linq.ParameterRebinder";

/// Rewrite every occurrence of `from` inside `body` to `to` — `ParameterRebinder.Rebind(body, from, to)`.
/// This is the one place parameter-identity is reconciled; everything above just needs to call it before
/// combining two independently-built trees.
fn rebind_param(body: Expr, from: Param, to: Param) -> Expr {
    use crate::intrinsics::rustc_clr_interop_managed_call3_ as call3;
    let inner: CExpr = call3::<
        "Mycorrhiza.Interop.Helpers",
        "Mycorrhiza.Linq.ParameterRebinder",
        false,
        "Rebind",
        true,
        CExpr,
        CExpr,
        CParam,
        CParam,
    >(body.inner, from.inner, to.inner);
    Expr { inner }
}

/// A combinable, entity-typed predicate: the built lambda **body** ([`Expr`]) plus the [`Param`] it was
/// built against. `T` is a phantom marker (see the module-level doc above) distinguishing predicates
/// meant for different .NET entity types at the Rust type level — it carries no runtime representation.
///
/// Combine two predicates with ordinary Rust operators — `pred_a & pred_b`, `pred_a | pred_b`,
/// `!pred_a` — regardless of whether they were built against the SAME or DIFFERENT [`Param`] instances.
/// When different (the common case: two independently-authored builder functions each calling
/// `Param::new` on their own), the combinator transparently rewrites the right-hand side's parameter
/// references to the left-hand side's parameter (see [`rebind_param`]) before combining — this is the
/// actual, LINQKit-equivalent fix for the "two `ParameterExpression` instances" problem, not a shortcut.
pub struct TypedPredicate<T> {
    body: Expr,
    param: Param,
    _marker: PhantomData<fn() -> T>,
}

// Manual `Clone`/`Copy` impls (not `#[derive]`d): `derive` would wrongly require `T: Clone + Copy`,
// but `T` is a phantom marker only — the actual payload (`Expr`, `Param`) is `Copy` regardless of `T`.
impl<T> Clone for TypedPredicate<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for TypedPredicate<T> {}

/// Cosmetic alias: `Filter<Person>` reads a little more like the thing it is (a reusable filter over an
/// entity) than `TypedPredicate<Person>` — purely a naming convenience, identical type either way.
pub type Filter<T> = TypedPredicate<T>;

impl<T> TypedPredicate<T> {
    /// Build a predicate from a lambda body and the parameter it was constructed against — e.g.
    /// `TypedPredicate::new(p, p.expr().prop("Age").ge(Expr::const_i32(18)))` for `p => p.Age >= 18`.
    #[must_use]
    pub fn new(param: Param, body: Expr) -> Self {
        TypedPredicate {
            body,
            param,
            _marker: PhantomData,
        }
    }

    /// The parameter this predicate's body is expressed in terms of.
    #[must_use]
    pub fn param(self) -> Param {
        self.param
    }

    /// A trivially-true predicate (`1 == 1`) over entity type `T` — e.g. for a "no filter applied"
    /// branch. `T` is purely a phantom Rust-level marker (see the module doc), so this needs no real
    /// .NET type or `Field` to build against: the internal `Param` is built against `System.Object`
    /// (always `Type.GetType`-resolvable, regardless of what `T` is) — sound because the body never
    /// performs a member access on the parameter, and combining this predicate with a real one via
    /// `&`/`|` rebinds parameter identity structurally (see [`Self::combine`]/`rebind_param`), never
    /// relying on the two operands' parameters sharing a declared .NET type. Calling `.compile()`
    /// directly on a bare `always()` (without combining it into anything) also works standalone —
    /// `Func<object,bool>` is a perfectly real, executable delegate.
    #[must_use]
    pub fn always() -> Self {
        let p = Param::new("System.Object", "p");
        TypedPredicate::new(p, Expr::const_i32(1).eq(Expr::const_i32(1)))
    }

    /// A trivially-false predicate (`1 == 0`) over entity type `T` — see [`Self::always`] for why no
    /// real .NET type is needed.
    #[must_use]
    pub fn never() -> Self {
        let p = Param::new("System.Object", "p");
        TypedPredicate::new(p, Expr::const_i32(1).eq(Expr::const_i32(0)))
    }

    /// The untyped lambda body — hand this (with [`Self::param`]) to a `Expression.Lambda<Func<T,bool>>`
    /// construction site to produce the final strongly-typed `Expression<Func<T,bool>>` EF Core consumes
    /// (mirrors [`Expr::typed_pred`]'s int-specialized version, generalized to the caller's own entity
    /// type — see the module-level doc for why that final typing step is deliberately NOT done here).
    #[must_use]
    pub fn body(self) -> Expr {
        self.body
    }

    /// The provider-visible rendering of the body (`Expression.ToString()`).
    #[must_use]
    pub fn text(self) -> std::string::String {
        self.body.text()
    }

    /// Reference-compare two `ParameterExpression`s — `object.ReferenceEquals`. `Param::new` allocates a
    /// fresh `ParameterExpression` on every call, so two predicates built independently (even with
    /// identical `type_name`/`name` arguments) almost always have DISTINCT parameter identity; this is
    /// exactly the condition `BitAnd`/`BitOr` must detect and correct for.
    fn same_param(a: Param, b: Param) -> bool {
        let a_obj = cast::<CObject, CParam>(a.inner);
        let b_obj = cast::<CObject, CParam>(b.inner);
        CObject::static2::<"ReferenceEquals", CObject, CObject, bool>(a_obj, b_obj)
    }

    /// Combine two predicates' bodies with `op`, first rewriting `rhs` onto `self`'s parameter if the two
    /// were built against different `ParameterExpression` instances (the ParameterRebinder fix).
    fn combine<const OP: &'static str>(self, rhs: Self) -> Self {
        let rhs_body = if Self::same_param(self.param, rhs.param) {
            rhs.body
        } else {
            rebind_param(rhs.body, rhs.param, self.param)
        };
        TypedPredicate {
            body: binop::<OP>(self.body, rhs_body),
            param: self.param,
            _marker: PhantomData,
        }
    }
}

impl<T> std::ops::BitAnd for TypedPredicate<T> {
    type Output = TypedPredicate<T>;
    /// `self && rhs`, reconciling mismatched parameter identity first — see the type-level docs.
    fn bitand(self, rhs: Self) -> Self::Output {
        self.combine::<"AndAlso">(rhs)
    }
}

impl<T> std::ops::BitOr for TypedPredicate<T> {
    type Output = TypedPredicate<T>;
    /// `self || rhs`, reconciling mismatched parameter identity first — see the type-level docs.
    fn bitor(self, rhs: Self) -> Self::Output {
        self.combine::<"OrElse">(rhs)
    }
}

impl<T> std::ops::Not for TypedPredicate<T> {
    type Output = TypedPredicate<T>;
    /// `!self` — negates the body in place; no parameter rebinding needed (only one operand).
    fn not(self) -> Self::Output {
        TypedPredicate {
            body: self.body.not(),
            param: self.param,
            _marker: PhantomData,
        }
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

// ===========================================================================================
// `GroupBy` / `Join` / `SelectMany` — the three `Queryable` shapes beyond `Where`/`Count` that
// [`IntQuery`] covers. Unlike `IntQuery` (hardcoded to `int`), these are Rust-GENERIC over the
// caller's own .NET element-type handles (mirrors how `cd_efcore`'s `typed_pred_investor` free
// function generalizes [`Expr::typed_pred`] to `Investor` — that same "caller supplies its own
// concrete handle types, this module supplies the WF-9 def-shape machinery" split is used here,
// just promoted into reusable generic functions instead of being duplicated per test crate).
//
// Every operator below is a THIN wrapper around `rustc_clr_interop_generic_method_call{2,5}`: it
// supplies the fixed `(ASSEMBLY, CLASS_PATH, METHOD, KIND, ClassGenerics)` header and the
// definition-shape `Sig` tuple (built from the markers below), while the caller's own concrete
// `TSource`/`TKey`/… handle types flow through as ordinary Rust generic parameters — exactly the
// `!!N`-binds-to-`MethodGenerics[N]` contract `check_generic_marker` enforces (see
// `src/terminator/call.rs`). `Join` needed a new arity rung
// ([`crate::intrinsics::rustc_clr_interop_generic_method_call5`]) since it has five runtime
// arguments (`outer, inner, outerKeySelector, innerKeySelector, resultSelector`) — the backend's
// `call_gmethod` already reads arity from the call's argument count / `Sig` tuple length, so no
// change was needed there, only the new arity-ladder rung in `intrinsics.rs`.
// ===========================================================================================

/// Definition-shape marker: `System.Linq.IQueryable\`1<!!N>` — an `IQueryable` over method-generic
/// slot `N`. Generalizes this module's `CIQueryMG` (which is hardcoded to method-generic slot 0).
pub type QueryableMarker<const N: usize> =
    RustcCLRInteropManagedGeneric<"System.Linq.Expressions", "System.Linq.IQueryable", (RustcCLRInteropMethodGeneric<N>,)>;

/// Definition-shape marker: `System.Collections.Generic.IEnumerable\`1<!!N>`.
pub type EnumerableMarker<const N: usize> = RustcCLRInteropManagedGeneric<
    "System.Private.CoreLib",
    "System.Collections.Generic.IEnumerable",
    (RustcCLRInteropMethodGeneric<N>,),
>;

/// Definition-shape marker: `System.Linq.IGrouping\`2<!!K,!!V>` — the element `GroupBy` produces
/// (`IGrouping<TKey,TSource>`, key first).
pub type GroupingMarker<const K: usize, const V: usize> = RustcCLRInteropManagedGeneric<
    "System.Linq",
    "System.Linq.IGrouping",
    (RustcCLRInteropMethodGeneric<K>, RustcCLRInteropMethodGeneric<V>),
>;

/// Definition-shape marker: `System.Linq.Expressions.Expression\`1<System.Func\`2<!!A,!!B>>` — a
/// single-parameter selector (key selector / collection selector) from method-generic `A` to `B`.
/// Generalizes this module's `CExprFuncMG` (which hardcodes the result to `bool`).
pub type SelectorMarker<const A: usize, const B: usize> = RustcCLRInteropManagedGeneric<
    "System.Linq.Expressions",
    "System.Linq.Expressions.Expression",
    (RustcCLRInteropManagedGeneric<"System.Private.CoreLib", "System.Func", (RustcCLRInteropMethodGeneric<A>, RustcCLRInteropMethodGeneric<B>)>,),
>;

/// Definition-shape marker: `Expression\`1<Func\`3<!!A,!!B,!!C>>` — a two-parameter `resultSelector`
/// (`Join`'s last argument), from method-generics `A,B` to `C`.
pub type ResultSelectorMarker<const A: usize, const B: usize, const C: usize> = RustcCLRInteropManagedGeneric<
    "System.Linq.Expressions",
    "System.Linq.Expressions.Expression",
    (RustcCLRInteropManagedGeneric<"System.Private.CoreLib", "System.Func", (RustcCLRInteropMethodGeneric<A>, RustcCLRInteropMethodGeneric<B>, RustcCLRInteropMethodGeneric<C>)>,),
>;

/// Generalized version of [`Expr::typed_pred`]: wrap `body` into a strongly-typed
/// `Expression<TDelegate>` over `params`, via the generic `Expression.Lambda<TDelegate>(body,
/// ParameterExpression[])` method. Unlike `typed_pred` (hardcoded to `Func<int,bool>`), the
/// delegate/expression types are Rust generic parameters, so this works for ANY selector shape —
/// `Func<TSource,TKey>` (a `GroupBy`/`SelectMany` key/collection selector), `Func<TOuter,TInner,
/// TResult>` (a `Join` result selector, over TWO parameters — `Expression.Lambda`'s own arity is
/// always 2 runtime args, `(body, ParameterExpression[])`, regardless of how many parameters the
/// produced delegate has), or any other `Expression<Func<..>>` a caller needs. Mirrors what
/// `cd_efcore::typed_pred_investor` hand-rolled for one concrete entity type, generalized here so
/// callers no longer need to hand-roll it themselves.
#[must_use]
pub fn typed_lambda<TDelegate, TExprDelegate>(body: Expr, params: &[&Param]) -> TExprDelegate {
    let arr: CParamArr = new_arr::<CParam>(params.len() as i32);
    let mut i = 0i32;
    for p in params {
        set_elem::<CParam>(arr, i, p.inner);
        i += 1;
    }
    gmethod2::<
        "System.Linq.Expressions",
        "System.Linq.Expressions.Expression",
        false,
        "Lambda",
        0,
        (),
        (TDelegate,),
        (CExprMethGen0, CExpr, CParamArr),
        TExprDelegate,
        CExpr,
        CParamArr,
    >(body.inner, arr)
}

/// `Queryable.GroupBy<TSource,TKey>(source, keySelector) -> IQueryable<IGrouping<TKey,TSource>>`.
/// `TSource`/`TKey` are the caller's concrete .NET handle types for the method generics (e.g. a
/// `SubscriptionHandle` / `MString`); `QSource`/`QResult` are the concrete `IQueryable<..>` handle
/// types for the source and the produced `IQueryable<IGrouping<TKey,TSource>>`; `KeySelector` is the
/// concrete `Expression<Func<TSource,TKey>>` handle (built via [`typed_lambda`]).
#[must_use]
pub fn group_by<TSource, TKey, QSource, QResult, KeySelector>(source: QSource, key_selector: KeySelector) -> QResult {
    gmethod2::<
        "System.Linq.Queryable",
        "System.Linq.Queryable",
        false,
        "GroupBy",
        0,
        (),
        (TSource, TKey),
        (GroupingQueryMarker<1, 0>, QueryableMarker<0>, SelectorMarker<0, 1>),
        QResult,
        QSource,
        KeySelector,
    >(source, key_selector)
}

/// Definition-shape marker: `IQueryable\`1<IGrouping\`2<!!K,!!V>>` — `GroupBy`'s return shape.
/// Written as its own alias (rather than inlined at the `group_by` call site) purely for
/// readability — `GroupingQueryMarker<K, V>` reads as "a queryable of groupings" the way the doc
/// comment above describes it.
pub type GroupingQueryMarker<const K: usize, const V: usize> =
    RustcCLRInteropManagedGeneric<"System.Linq.Expressions", "System.Linq.IQueryable", (GroupingMarker<K, V>,)>;

/// `Queryable.SelectMany<TSource,TResult>(source, selector) -> IQueryable<TResult>`, flattening a
/// one-to-many navigation (`selector: Expression<Func<TSource,IEnumerable<TResult>>>`). `TSource`/
/// `TResult` are the caller's concrete method-generic handle types; `QSource`/`QResult` the concrete
/// `IQueryable<..>` handles; `Selector` the concrete selector expression handle.
#[must_use]
pub fn select_many<TSource, TResult, QSource, QResult, Selector>(source: QSource, selector: Selector) -> QResult {
    gmethod2::<
        "System.Linq.Queryable",
        "System.Linq.Queryable",
        false,
        "SelectMany",
        0,
        (),
        (TSource, TResult),
        (
            QueryableMarker<1>,
            QueryableMarker<0>,
            RustcCLRInteropManagedGeneric<
                "System.Linq.Expressions",
                "System.Linq.Expressions.Expression",
                (RustcCLRInteropManagedGeneric<"System.Private.CoreLib", "System.Func", (RustcCLRInteropMethodGeneric<0>, EnumerableMarker<1>)>,),
            >,
        ),
        QResult,
        QSource,
        Selector,
    >(source, selector)
}

/// `Queryable.Join<TOuter,TInner,TKey,TResult>(outer, inner, outerKeySelector, innerKeySelector,
/// resultSelector) -> IQueryable<TResult>` — the EXPLICIT LINQ-join shape (unlike `Include`, it
/// needs no pre-declared navigation property; the two key selectors are built purely from Rust).
/// `TOuter,TInner,TKey,TResult` are the caller's concrete method-generic handle types; `QOuter`/
/// `IInner`/`QResult` the concrete `IQueryable<TOuter>`/`IEnumerable<TInner>`/`IQueryable<TResult>`
/// handles; `KeyOuter`/`KeyInner`/`ResultSel` the concrete selector-expression handles (built via
/// [`typed_lambda`]). Uses [`crate::intrinsics::rustc_clr_interop_generic_method_call5`] — the new
/// arity rung this module's `Join` support needed (see the section doc above).
#[must_use]
pub fn join<TOuter, TInner, TKey, TResult, QOuter, IInner, KeyOuter, KeyInner, ResultSel, QResult>(
    outer: QOuter,
    inner: IInner,
    outer_key_selector: KeyOuter,
    inner_key_selector: KeyInner,
    result_selector: ResultSel,
) -> QResult {
    gmethod5::<
        "System.Linq.Queryable",
        "System.Linq.Queryable",
        false,
        "Join",
        0,
        (),
        (TOuter, TInner, TKey, TResult),
        (
            QueryableMarker<3>,
            QueryableMarker<0>,
            EnumerableMarker<1>,
            SelectorMarker<0, 2>,
            SelectorMarker<1, 2>,
            ResultSelectorMarker<0, 1, 3>,
        ),
        QResult,
        QOuter,
        IInner,
        KeyOuter,
        KeyInner,
        ResultSel,
    >(outer, inner, outer_key_selector, inner_key_selector, result_selector)
}
