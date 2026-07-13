# Issue dashboard: a complete Rust-on-.NET CLI

This example is deliberately application-shaped rather than a compiler test. It reads an issue
tracker export, parses it with .NET's `System.Text.Json` implementation through `mycorrhiza`, and
uses ordinary Rust to produce a small dashboard.

From the repository root, after completing the main quickstart setup:

```bash
cargo dotnet run examples/issue-dashboard --backend native --dotnet 10
```

With no argument it uses the bundled [`sample/issues.json`](sample/issues.json). Pass another file to
exercise the normal CLI path:

```bash
cargo dotnet run examples/issue-dashboard --backend native --dotnet 10 -- ./issues.json
```

Expected output for the bundled sample:

```text
Project: rust-dotnet-demo
Issues: 4 total, 3 open, 2 high-priority open
Owners:
- Ada: 2 open
- Grace: 1 open
```

The seam is visible in [`src/main.rs`](src/main.rs): `Json::parse` and JSON navigation operate on
managed `System.Text.Json.Nodes.JsonNode` objects, while argument parsing, file I/O, validation,
aggregation, sorting, and output remain normal Rust. Change the schema or aggregation logic and rerun
the same command—there is no C# glue project in this application.

One CLR-specific rule is visible in the error paths: a managed handle such as `Json` cannot be a
payload of Rust's overlapping `Result<T, E>` layout. The example pattern-matches `Option<Json>` at
the managed boundary and only moves Rust-owned strings into ordinary error handling. If an
experiment crosses that boundary, `cargo dotnet doctor <log-file>` recognizes the resulting loader
error and explains the supported pattern.

For the install and provisioning steps, start at the repository [`QUICKSTART.md`](../../QUICKSTART.md).
For more interop patterns, see [`docs/INTEROP_COOKBOOK.md`](../../docs/INTEROP_COOKBOOK.md).
