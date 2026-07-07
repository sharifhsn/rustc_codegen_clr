use dotnet_macros::dotnet_entity;

// Typo: `renam` instead of `rename`. Before the fix this silently compiled and kept the default
// PascalCase property name, with no diagnostic that the key was misspelled.
#[dotnet_entity]
struct Person {
    #[dotnet(renam = "Age")]
    age: i32,
}

fn main() {}
