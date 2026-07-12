# Interop

Interop works in both directions:

- Export Rust functions, DTOs, classes, and containers to managed consumers.
- Call .NET methods, collections, tasks, delegates, LINQ, and selected BCL APIs from Rust through
  `mycorrhiza`.

Prefer typed projection macros and the idiomatic `mycorrhiza` wrappers. Raw handles and intrinsic
calls remain available for unsupported surfaces, but they move ownership and ABI checks into your
code.
