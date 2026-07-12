# Managed identity and ABI contract

Status: supported schema 1 contract. New release packages must opt into schema 1 explicitly.
Artifacts without identity metadata retain the compatibility-only global `MainModule` surface.

## Managed identity schema 1

Every Rust `cdylib` compiled for .NET has one explicit managed identity:

```toml
[package.metadata.dotnet]
identity-schema = 1
package-id = "Monark.Aip.PositionParser"
assembly-name = "Monark.Aip.PositionParser"
root-namespace = "Monark.Aip.PositionParser"
module-type = "AipPositionParserNative"
legacy-main-module = false
```

The compiler-facing representation is:

```text
ManagedIdentity {
  schema: u16,
  package_id: String,
  assembly_name: String,
  root_namespace: String,
  module_type: String,
  legacy_main_module: bool,
}
```

Rules:

- `package_id` is the NuGet ID and is independent of CLR assembly filename/identity.
- `assembly_name` is the CLR assembly identity and DLL basename.
- `root_namespace + "." + module_type` is the public static implementation type.
- Namespace segments and `module_type` must be nonempty valid CLR/C# identifiers after Unicode
  normalization. Reserved C# words are escaped only in generated source; metadata stores the raw
  identifier.
- Schema 1 has no guessed string identity fields: schema, package ID, assembly name, root namespace,
  and module type are required; `legacy-main-module` defaults to false. `cargo dotnet pack --id` may
  override only the NuGet package ID for that invocation; it never changes CLR identity.
- Two referenced packages may share function names. CLR assembly names must be unique independently,
  and public type FQNs (`root_namespace + "." + module_type`) must be unique independently. Link and
  package validation reject either collision; comparing only the combined triple is insufficient.
- Identity is embedded in the serialized artifact, artifact receipt, XML member IDs, NuGet
  validation, and generated facade. A mismatch is an artifact ABI error.
- `legacy-main-module = true` emits the historical global `MainModule`. It is never the default for
  a newly generated or release package and cannot be combined with the multi-library acceptance
  claim.

`assembly-name` is independent of both Cargo package name and NuGet package ID. Cargo still emits its
intermediate `lib<crate>.so`; cargo-dotnet publishes `<assembly-name>.dll`, XML/PDB/receipt sidecars,
MSBuild references, and NuGet `lib/<tfm>/` assets under the managed identity. The multi-library and
fresh-package consumers continuously verify this distinction.

The IR may continue to use `MainModule` as an internal sentinel. Exporters must consistently project
that sentinel to the configured namespace/type at every definition and reference boundary.

## Generated facade contract

Consumers call the directly emitted managed class under the configured namespace, never
`global::MainModule`:

```csharp
using Monark.Aip.PositionParser;

PositionProbe result = AipPositionParser.Parse(line);
```

`#[dotnet_class]` plus `#[dotnet_methods]` defines that facade in Rust. A method can remain idiomatic
Rust while selecting its public C# spelling explicitly:

```rust
#[dotnet_methods]
impl InvoiceFacade {
    #[dotnet(name = "CreateInvoice")]
    pub fn create_invoice(/* ... */) -> InvoiceDtoHandle { /* ... */ }
}
```

The managed name, signature, DTOs, namespace, and assembly identity are the stable API. No generated
C# source shim is required or currently promised by schema 1.

## DTO schema 1

Projection is explicit through `#[dotnet_dto]`. Arbitrary `repr(Rust)` or domain objects are never
inferred as public CLR contracts.

| Rust source | CLR projection | Required behavior |
|---|---|---|
| integer/float/bool primitives | matching CLR primitive | exact width; no implicit narrowing |
| `MString` | `System.String` | managed handle; may carry null |
| `Decimal` bridge | `System.Decimal` | exact 96-bit coefficient, sign, and scale; never `f64` |
| `DateOnly` bridge | `System.DateOnly` | validated year/month/day, no timezone |
| `Nullable<T>` for managed value `T` | `System.Nullable<T>` | exact null/present roundtrip |
| managed enum wrapper | referenced CLR enum | the external enum's fixed underlying layout |

`#[dotnet_dto]` accepts a named-field struct and emits a public managed class with a parameterless
constructor, a full primary constructor of any arity, and readable/writable PascalCase properties.
Rust fields remain lower-camel internally, avoiding CLR field/property name ambiguity.

Schema 1 encodes value-type nullability structurally with `System.Nullable<T>`. Reference-type
nullable annotations are deliberately **oblivious** in metadata: `MString` can be null, and consumers
must treat it accordingly. Adding C# nullable-reference attributes is a future schema capability,
not an unimplemented promise in schema 1. Arbitrary Rust-owned layouts and borrowed references are
not DTO ABI types; authors use the explicit managed bridge types above.

## Errors and panics

`#[dotnet_export(error = "exception")]` is the schema 1 error boundary for free functions returning
`Result<T, E>` with a GC-safe value `T`. `Ok(T)` returns the projected value and `Err(E)` throws a
managed exception containing `E: Display`. Omitting the policy is a compile error, and managed-handle
`Ok` payloads are rejected because overlapping `Result` storage is not GC-safe. Stable domain error
codes/details require an explicit DTO chosen by the library author; schema 1 does not invent them.

Panics are implementation failures, not domain errors. With the required unwind panic strategy, the
boundary catches unwindable Rust panics before the ABI, returns or throws the stable
`RustImplementationFailure` category, and suppresses arbitrary panic payloads in release builds.
`panic=abort`, stack overflow, runtime aborts, and other fatal/foreign failures are explicitly outside
this recovery guarantee and terminate according to the runtime's fatal-failure policy.

## Compatibility

- Removing or renaming a public facade member, DTO property, enum member, error code, namespace,
  module type, or assembly name is breaking.
- Adding, removing, renaming, or changing a DTO property is conservatively breaking because it
  changes the generated full constructor and reflected public surface.
- Changing decimal scale semantics, null/empty semantics, date validation, or error classification
  is breaking even if the CLR signature is unchanged.
- CI compares the metadata-only reflected public API with the committed baseline. Any difference is
  conservatively breaking and requires a major SemVer increase.

## Acceptance

The managed-identity fixture loads three assemblies with overlapping member names, including one
whose Cargo, NuGet, CLR assembly, and public type names differ. The typed DTO fixture verifies exact
`Decimal` scale, nullable `DateOnly`, managed strings, arbitrary-arity construction, writable C#
properties, and Rust-snake/C#-Pascal method naming. Compile-fail fixtures cover unsupported export
types and unsafe `Result` shapes; metadata snapshots enforce major-version compatibility.
