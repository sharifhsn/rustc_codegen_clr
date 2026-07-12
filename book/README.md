# Rust on .NET book

This directory contains the publishable user guide for `rustc_codegen_clr`.

Build and preview it with:

```bash
mdbook serve --open
```

The generated site is written to `book/book/` and is intentionally ignored by Git.
Examples in the guide should remain small and should link to a runnable fixture under
`cargo_tests/` whenever one exists.
