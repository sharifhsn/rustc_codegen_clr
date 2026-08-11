use cilly::{Assembly, AssemblyLinkError};

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
/// Expected cross-shard conflicts are detected before the consuming relocation begins, so every
/// returned error leaves `parent` unchanged without cloning it. Unexpected relocation invariant
/// panics remain fail-stop.
pub(crate) fn try_commit_assembly_shard(
    parent: &mut Assembly,
    shard: Assembly,
) -> Result<(), AssemblyLinkError> {
    parent.try_link_in_place(shard).map(|_| ())
}

pub(crate) fn commit_assembly_shard(parent: &mut Assembly, shard: Assembly) {
    if let Err(error) = try_commit_assembly_shard(parent, shard) {
        panic!("assembly shard commit failed: {error}");
    }
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
    use super::{assembly_transaction, try_commit_assembly_shard};
    use cilly::{
        Access, Assembly, AssemblyLinkError, ClassDef, ClassRef, Int, MethodDef, MethodImpl,
        NativeImport, PInvokeCallConv, Type, cilnode::MethodKind,
    };
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

    fn add_class_with_base(assembly: &mut Assembly, base_name: &str) {
        let base_name = assembly.alloc_string(base_name);
        let base = assembly.alloc_class_ref(ClassRef::new(base_name, None, false, vec![].into()));
        let name = assembly.alloc_string("TransactionBaseCollision");
        assembly
            .class_def(ClassDef::new(
                name,
                false,
                0,
                Some(base),
                vec![],
                vec![],
                Access::Private,
                None,
                None,
                true,
            ))
            .unwrap();
    }

    fn add_collision_method(assembly: &mut Assembly, access: Access) {
        let name = assembly.alloc_string("TransactionMethodCollision");
        let class = assembly
            .class_def(ClassDef::new(
                name,
                false,
                0,
                None,
                vec![],
                vec![],
                Access::Private,
                None,
                None,
                true,
            ))
            .unwrap();
        let signature = assembly.sig([Type::Int(Int::I32)], Type::Void);
        let method_name = assembly.alloc_string("conflict");
        assembly.new_method(MethodDef::new(
            access,
            class,
            method_name,
            signature,
            MethodKind::Static,
            MethodImpl::Missing,
            vec![None],
        ));
    }

    fn add_class_with_access(assembly: &mut Assembly, access: Access) {
        let name = assembly.alloc_string("TransactionDefinitionCollision");
        assembly
            .class_def(ClassDef::new(
                name,
                false,
                0,
                None,
                vec![],
                vec![],
                access,
                None,
                None,
                true,
            ))
            .unwrap();
    }

    fn native_import(library: &str) -> NativeImport {
        NativeImport {
            rust_symbol: "transaction_native".into(),
            entry_point: "transaction_native".into(),
            library: library.into(),
            call_conv: PInvokeCallConv::Cdecl,
            preserve_errno: false,
        }
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
    fn failed_field_commit_preserves_parent_counts_and_serialization() {
        let mut parent = seeded_parent();
        add_collision_class(&mut parent, Type::Int(Int::I32));
        let counts = parent.arena_counts();
        let bytes = encoded(&parent);

        let mut shard = Assembly::default();
        add_collision_class(&mut shard, Type::Int(Int::I64));
        let result = try_commit_assembly_shard(&mut parent, shard);

        assert!(matches!(
            result,
            Err(AssemblyLinkError::ClassFieldConflict { .. })
        ));
        assert_eq!(parent.arena_counts(), counts);
        assert_eq!(encoded(&parent), bytes);
    }

    #[test]
    fn failed_base_commit_preserves_parent_counts_and_serialization() {
        let mut parent = seeded_parent();
        add_class_with_base(&mut parent, "BaseOne");
        let counts = parent.arena_counts();
        let bytes = encoded(&parent);

        let mut shard = Assembly::default();
        add_class_with_base(&mut shard, "BaseTwo");
        let result = try_commit_assembly_shard(&mut parent, shard);

        assert!(matches!(
            result,
            Err(AssemblyLinkError::ClassBaseConflict { .. })
        ));
        assert_eq!(parent.arena_counts(), counts);
        assert_eq!(encoded(&parent), bytes);
    }

    #[test]
    fn failed_method_commit_preserves_parent_counts_and_serialization() {
        let mut parent = seeded_parent();
        add_collision_method(&mut parent, Access::Public);
        let counts = parent.arena_counts();
        let bytes = encoded(&parent);

        let mut shard = Assembly::default();
        add_collision_method(&mut shard, Access::Private);
        let result = try_commit_assembly_shard(&mut parent, shard);

        assert!(matches!(
            result,
            Err(AssemblyLinkError::MethodConflict { .. })
        ));
        assert_eq!(parent.arena_counts(), counts);
        assert_eq!(encoded(&parent), bytes);
    }

    #[test]
    fn failed_class_definition_commit_preserves_parent_counts_and_serialization() {
        let mut parent = seeded_parent();
        add_class_with_access(&mut parent, Access::Public);
        let counts = parent.arena_counts();
        let bytes = encoded(&parent);

        let mut shard = Assembly::default();
        add_class_with_access(&mut shard, Access::Private);
        let result = try_commit_assembly_shard(&mut parent, shard);

        assert!(matches!(
            result,
            Err(AssemblyLinkError::ClassDefinitionConflict { .. })
        ));
        assert_eq!(parent.arena_counts(), counts);
        assert_eq!(encoded(&parent), bytes);
    }

    #[test]
    fn failed_native_import_commit_preserves_parent_counts_and_serialization() {
        let mut parent = seeded_parent();
        parent.add_native_import(native_import("library-one"));
        let counts = parent.arena_counts();
        let bytes = encoded(&parent);

        let mut shard = Assembly::default();
        shard.add_native_import(native_import("library-two"));
        let result = try_commit_assembly_shard(&mut parent, shard);

        assert!(matches!(
            result,
            Err(AssemblyLinkError::NativeImportConflict { .. })
        ));
        assert_eq!(parent.arena_counts(), counts);
        assert_eq!(encoded(&parent), bytes);
    }

    #[test]
    fn assembly_transaction_panics_on_link_error_without_losing_parent() {
        let mut parent = seeded_parent();
        add_collision_class(&mut parent, Type::Int(Int::I32));
        let counts = parent.arena_counts();
        let bytes = encoded(&parent);

        let result = catch_unwind(AssertUnwindSafe(|| {
            let _: Result<(), ()> = assembly_transaction(&mut parent, |shard| {
                add_collision_class(shard, Type::Int(Int::I64));
                Ok(())
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
