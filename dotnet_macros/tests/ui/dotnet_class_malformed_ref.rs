use dotnet_macros::dotnet_class;

// `implements` spec with an unterminated `[` — before the fix, `split_dotnet_ref` silently treated
// this as a bracket-less type name (empty assembly, garbage type name), registering nonsense with no
// diagnostic.
#[dotnet_class(implements = "[BadSpec")]
struct Foo {
    x: i32,
}

fn main() {}
