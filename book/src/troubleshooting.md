# Troubleshooting

Start with:

```bash
cargo dotnet doctor
```

## A change appears to have no effect

Delete the consumer's `target/` directory (and a C# host's `bin/` and `obj/`) and rebuild. Backend,
linker, standard-library, or managed-host artifacts from an earlier run can otherwise obscure the
change.

## The compiler reports an internal API error

This backend uses rustc-private APIs and supports a narrow nightly window. Confirm that the active
toolchain matches `rust-toolchain.toml`. Porting notes for a newer nightly are in
[`feasibility/PORT_NOTES.md`](../../feasibility/PORT_NOTES.md).

## A managed type cannot be loaded

Check the type's implementation assembly and managed identity. Some collection types live in
`System.Private.CoreLib`; others live in `System.Collections`. For exported libraries, ensure two
packages do not claim the same CLR assembly and fully qualified type identity.

## Debugging a miscompilation

First disable the CIL optimizer:

```bash
OPTIMIZE_CIL=0 cargo dotnet run
```

This preserves the near one-to-one MIR-to-CIL lowering and makes the failing statement easier to
identify. The repository's [debugging guide](../../docs/DEBUGGING.md) covers IR dumps, typechecking,
and backend panic logs.
