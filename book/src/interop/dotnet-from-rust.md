# Call .NET from Rust

The `mycorrhiza` crate provides managed handles and idiomatic wrappers. Its prelude brings the most
common interop traits into scope:

```rust
use mycorrhiza::prelude::*;
use mycorrhiza::collections::List;

fn main() {
    let mut values = List::<i32>::new();
    values.add(10);
    values.add(20);

    let sum: i32 = (&values).into_iter().sum();
    println!("{sum}");
}
```

Managed calls can fail by returning null or throwing. Prefer the wrapper APIs that translate those
boundaries into `Option<T>` and `Result<T, ManagedException>` instead of using raw handles.

Available higher-level modules include:

- `collections` and `enumerate` for generic collections and iteration;
- `bcl` for date/time, decimal, JSON, regex, URI, random, and related APIs;
- `delegate` and `task` for callbacks and async bridging;
- `linq` for managed query adapters; and
- `sync` for managed synchronization primitives.

Runnable coverage lives in the `cargo_tests/cd_*` fixtures, especially
[`cd_collections`](../../../cargo_tests/cd_collections/),
[`cd_bcl`](../../../cargo_tests/cd_bcl/), and
[`cd_async`](../../../cargo_tests/cd_async/).
