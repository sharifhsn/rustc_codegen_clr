use cilly::Assembly;

/// Builds an isolated assembly shard and links it into `parent` only after `build` succeeds.
///
/// The closure never receives the parent assembly, so an error or an unwind while building cannot
/// leak partially interned values or definitions into it. `Assembly::link` is the commit boundary.
pub(crate) fn assembly_transaction<T, E>(
    parent: &mut Assembly,
    build: impl FnOnce(&mut Assembly) -> Result<T, E>,
) -> Result<T, E> {
    let mut shard = Assembly::default();
    let value = build(&mut shard)?;
    let base = std::mem::take(parent);
    *parent = base.link(shard);
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::assembly_transaction;
    use cilly::Assembly;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    fn encoded(assembly: &Assembly) -> Vec<u8> {
        postcard::to_stdvec(assembly).expect("test assembly should serialize")
    }

    fn seeded_parent() -> Assembly {
        let mut assembly = Assembly::default();
        assembly.add_section("parent-section", b"parent-value");
        assembly
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
