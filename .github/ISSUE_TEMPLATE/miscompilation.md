---
name: Compiler bug or miscompilation
about: Report a crash, wrong result, unsupported code path, or install problem
title: "[BUG] "
labels: miscompilation
assignees: ''

---

## What happened?

Include the command you ran and the complete error or incorrect output.

## Small reproducer

Paste the smallest Cargo project or Rust program that still fails. A repository link is welcome
when reducing it is impractical.

## Expected result

## Diagnostics

Please attach the output of:

```text
cargo dotnet doctor --json
```

Also include whether the same program works with ordinary `cargo run`.

## Anything else
