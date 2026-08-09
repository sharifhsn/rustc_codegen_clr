use cilly::Assembly;

/// Builds an isolated shard without touching its eventual parent.
///
/// Keeping this phase separate from [`commit_assembly_shard`] is important for panic recovery:
/// callers may catch a panic while *building* a shard, but must not catch a panic from the consuming
/// `Assembly::link` commit unless they retained their own parent snapshot.
pub(crate) fn build_assembly_shard<T, E>(
    build: impl FnOnce(&mut Assembly) -> Result<T, E>,
) -> Result<(T, Assembly), E> {
    let mut shard = Assembly::default();
    let value = build(&mut shard)?;
    Ok((value, shard))
}

/// Commits a successfully built shard.
///
/// `Assembly::link` consumes the old parent. Therefore a link panic cannot be rolled back through
/// this API; it must propagate and fail codegen. In particular, never put this call inside the
/// best-effort item-lowering `catch_unwind`, because doing so would resume with an empty parent and
/// lose every item committed earlier in the CGU.
pub(crate) fn commit_assembly_shard(parent: &mut Assembly, shard: Assembly) {
    let base = std::mem::take(parent);
    *parent = base.link(shard);
}

/// Builds an isolated assembly shard and links it into `parent` only after `build` succeeds.
///
/// The closure never receives the parent assembly, so an error or an unwind while building cannot
/// leak partially interned values or definitions into it. `Assembly::link` is the commit boundary.
pub(crate) fn assembly_transaction<T, E>(
    parent: &mut Assembly,
    build: impl FnOnce(&mut Assembly) -> Result<T, E>,
) -> Result<T, E> {
    let (value, shard) = build_assembly_shard(build)?;
    commit_assembly_shard(parent, shard);
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::assembly_transaction;
    use cilly::{Access, Assembly, ClassDef, Int, Type};
    use std::panic::{AssertUnwindSafe, catch_unwind};

    fn encoded(assembly: &Assembly) -> Vec<u8> {
        postcard::to_stdvec(assembly).expect("test assembly should serialize")
    }

    fn seeded_parent() -> Assembly {
        let mut assembly = Assembly::default();
        assembly.add_section("parent-section", b"parent-value");
        assembly
    }

    fn add_collision_class(assembly: &mut Assembly, field_type: Type) {
        let name = assembly.alloc_string("TransactionCollision");
        let field_name = assembly.alloc_string("value");
        assembly
            .class_def(ClassDef::new(
                name,
                true,
                0,
                None,
                vec![(field_type, field_name, Some(0))],
                vec![],
                Access::Private,
                None,
                None,
                true,
            ))
            .unwrap();
    }

    #[test]
    fn successful_transaction_commits_shard() {
        let mut parent = seeded_parent();
        let before = parent.arena_counts();
        let before_bytes = encoded(&parent);

        let value = assembly_transaction(&mut parent, |shard| {
            shard.add_section("committed-section", b"committed-value");
            Ok::<_, ()>(42)
        })
        .unwrap();

        assert_eq!(value, 42);
        assert_eq!(parent.arena_counts().sections, before.sections + 1);
        assert_ne!(encoded(&parent), before_bytes);
    }

    #[test]
    fn error_rolls_back_counts_and_serialization() {
        let mut parent = seeded_parent();
        let counts = parent.arena_counts();
        let bytes = encoded(&parent);

        let result = assembly_transaction(&mut parent, |shard| {
            shard.add_section("discarded-section", b"discarded-error-value");
            Err::<(), _>("expected error")
        });

        assert_eq!(result, Err("expected error"));
        assert_eq!(parent.arena_counts(), counts);
        assert_eq!(encoded(&parent), bytes);
    }

    #[test]
    fn panic_rolls_back_counts_and_serialization() {
        let mut parent = seeded_parent();
        let counts = parent.arena_counts();
        let bytes = encoded(&parent);

        let result = catch_unwind(AssertUnwindSafe(|| {
            let _: Result<(), ()> = assembly_transaction(&mut parent, |shard| {
                shard.add_section("discarded-section", b"discarded-panic-value");
                panic!("expected panic")
            });
        }));

        assert!(result.is_err());
        assert_eq!(parent.arena_counts(), counts);
        assert_eq!(encoded(&parent), bytes);
    }

    #[test]
    fn commit_panic_propagates_instead_of_becoming_recoverable_success() {
        let mut parent = seeded_parent();
        add_collision_class(&mut parent, Type::Int(Int::I32));

        let result = catch_unwind(AssertUnwindSafe(|| {
            let _: Result<(), ()> = assembly_transaction(&mut parent, |shard| {
                // Same managed identity, incompatible definition: relocation must reject the
                // commit. The caller observes that panic; production code deliberately does not
                // catch this consuming commit boundary and therefore cannot continue after losing
                // the taken parent assembly.
                add_collision_class(shard, Type::Int(Int::I64));
                Ok(())
            });
        }));

        assert!(
            result.is_err(),
            "a failing link commit must propagate its panic"
        );
    }

    #[test]
    fn shard_commit_order_is_deterministic() {
        fn assemble(order: &[&str]) -> Vec<u8> {
            let mut assembly = Assembly::default();
            for value in order {
                assembly_transaction(&mut assembly, |shard| {
                    // Every shard writes the same section. `Assembly::link` applies shards in
                    // commit order, so the final serialized value records the last committer.
                    shard.add_section("commit-order", value.as_bytes());
                    Ok::<(), ()>(())
                })
                .unwrap();
            }
            encoded(&assembly)
        }

        let order = ["first-shard", "second-shard", "third-shard"];
        assert_eq!(assemble(&order), assemble(&order));

        let reversed = ["third-shard", "second-shard", "first-shard"];
        assert_ne!(assemble(&order), assemble(&reversed));
    }
}
