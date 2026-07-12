# Focused CLR public API compatibility gate

`ApiSnapshot` reads ECMA-335 metadata with `System.Reflection.Metadata`; it does not load the
target assembly or execute its code. It emits ordinally sorted canonical records for public types,
constructors, methods, properties, enum members, and nullability/code-analysis attributes. Attribute
payloads are retained as metadata blob hex so changes to nullable context and attribute arguments are
visible without depending on runtime reflection behavior.

Run `bash feasibility/api_compat_acceptance.sh`. The acceptance first compares the focused typed-DTO
fixture with the committed baseline, then builds a temporary binary-breaking variant and proves that
the same comparison rejects it. Update the baseline intentionally only when changing the supported
release-package CLR contract. This is a focused binary surface check, not a complete SemVer or
behavioral-compatibility analyzer; schema fingerprints and semantic DTO rules remain separate gates.
