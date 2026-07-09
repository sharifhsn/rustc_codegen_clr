// The LINQKit-equivalent fix for combining two independently-built Expression<Func<T,bool>> trees.
//
// Two predicates built by separate calls to `Param::new` (mycorrhiza::linq) each carry their OWN
// `ParameterExpression` instance for the lambda parameter, even if they describe "the same" logical
// parameter (same name, same type). Naively splicing their bodies together with
// `Expression.AndAlso(a.Body, b.Body)` produces a tree referencing TWO DIFFERENT parameters — broken:
// it either throws on `Compile()`/EF translation, or (worse) silently produces a tree where one operand
// is never bound to anything a query provider can resolve.
//
// `ParameterRebinder` is a small `ExpressionVisitor` that walks a tree and replaces every occurrence of
// one `ParameterExpression` with another, by reference identity. This is the exact ~15-line pattern
// LINQKit's `PredicateBuilder` uses internally. `mycorrhiza::linq::TypedPredicate`'s `BitAnd`/`BitOr`
// combinators call `Rebind` once, on the right-hand operand, before combining — so Rust callers never
// have to know this exists.
//
// This file must match the exact shape documented in `mycorrhiza/src/linq.rs`'s doc comment on
// `PARAMETER_REBINDER_ASSEMBLY`/`PARAMETER_REBINDER_CLASS` — that comment is the contract, this is the
// implementation of it, now actually bundled and built as part of this repo (see
// `tools/cargo-dotnet/src/interop_helpers.rs`).
namespace Mycorrhiza.Linq;

using System.Linq.Expressions;

public sealed class ParameterRebinder : ExpressionVisitor
{
    private readonly ParameterExpression _from;
    private readonly ParameterExpression _to;

    private ParameterRebinder(ParameterExpression from, ParameterExpression to)
    {
        _from = from;
        _to = to;
    }

    /// Rewrite every occurrence of `from` inside `body` to `to`.
    public static Expression Rebind(Expression body, ParameterExpression from, ParameterExpression to)
        => new ParameterRebinder(from, to).Visit(body)!;

    protected override Expression VisitParameter(ParameterExpression node)
        => ReferenceEquals(node, _from) ? _to : base.VisitParameter(node);
}
