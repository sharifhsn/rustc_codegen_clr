# Call Rust from C#

Create a Rust library:

```bash
cargo dotnet new pricing-core --lib
```

Export a function with a managed name:

```rust
use mycorrhiza::dotnet_export;

#[dotnet_export(name = "CalculateTotal")]
pub fn calculate_total(quantity: i32, unit_price: f64) -> f64 {
    f64::from(quantity) * unit_price
}
```

The generated managed facade is callable as an ordinary static method:

```csharp
double total = PricingCore.CalculateTotal(4, 12.50);
Console.WriteLine(total); // 50
```

For structured data, use the typed DTO projection rather than passing pointer/length pairs. The
checked-in [`cd_typed_dto`](../../../cargo_tests/cd_typed_dto/) fixture demonstrates decimal,
nullable date, string, constructor, and property round trips from a separately compiled C# host.

`Result<T, E>` requires an explicit export error policy. With the managed-exception policy, an
error becomes a catchable managed exception; managed-handle payloads are rejected where ownership
cannot be proved safe.

For complete runnable exports, see [`cd_export`](../../../cargo_tests/cd_export/) and
[`cd_interop`](../../../cargo_tests/cd_interop/).
