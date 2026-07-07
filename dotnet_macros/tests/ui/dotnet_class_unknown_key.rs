use dotnet_macros::dotnet_class;

// Typo: `extendz` instead of `extends`. Before the fix this silently compiled, falling back to the
// default base class with no diagnostic at all.
#[dotnet_class(extendz = "[System.Runtime]System.Object")]
struct Foo {
    x: i32,
}

fn main() {}
