use heck::{ToSnakeCase, ToUpperCamelCase, ToShoutySnakeCase, ToKebabCase};

fn main() {
    // Pure text case conversions. All heck conversion methods are infallible
    // (they return String), so there is no Result/Option/panic path here.
    let inputs = [
        "HelloWorld",
        "hello world",
        "some_snake_case_value",
        "BackgroundColor",
        "XMLHttpRequest",
        "kebab-case-thing",
    ];

    for input in inputs.iter() {
        let snake = input.to_snake_case();
        let camel = input.to_upper_camel_case();
        let shouty = input.to_shouty_snake_case();
        let kebab = input.to_kebab_case();
        println!(
            "in={} | snake={} | camel={} | shouty={} | kebab={}",
            input, snake, camel, shouty, kebab
        );
    }

    println!("== soak_heck done ==");
}
