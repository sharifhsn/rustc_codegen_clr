use dotnet_macros::dotnet_class;

// `value_type` expects a bool literal; a string was silently ignored before the fix (kept the
// `false` default with no diagnostic).
#[dotnet_class(value_type = "yes")]
struct Foo {
    x: i32,
}

fn main() {}
