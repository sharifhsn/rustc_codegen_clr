use std::fmt;

// A source error type that we can wrap via #[from].
#[derive(Debug)]
struct ParseProblem {
    detail: String,
}

impl fmt::Display for ParseProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "parse problem: {}", self.detail)
    }
}

impl std::error::Error for ParseProblem {}

// The derive(thiserror::Error) enum: Display via #[error("...")], a #[from] variant,
// and a plain variant with a formatted field.
#[derive(thiserror::Error, Debug)]
enum DataError {
    #[error("parsing failed")]
    Parse(#[from] ParseProblem),

    #[error("value {value} is out of range")]
    OutOfRange { value: i64 },

    #[error("empty input")]
    Empty,
}

// A function that produces the #[from] conversion path via the `?` operator.
fn validate(input: &str) -> Result<i64, DataError> {
    if input.is_empty() {
        return Err(DataError::Empty);
    }

    // Parse without panicking; map a parse failure into our source error,
    // which the `?` operator converts via the generated From impl.
    let parsed: i64 = input
        .parse::<i64>()
        .map_err(|e| ParseProblem { detail: format!("{}", e) })?;

    if parsed < 0 || parsed > 100 {
        return Err(DataError::OutOfRange { value: parsed });
    }

    Ok(parsed)
}

fn report(input: &str) {
    match validate(input) {
        Ok(v) => println!("ok({}) = {}", input, v),
        Err(e) => {
            // Exercise the generated Display impl.
            println!("err({}) = {}", input, e);
            // Exercise the generated Error::source() (Some only for the #[from] variant).
            match std::error::Error::source(&e) {
                Some(src) => println!("  source = {}", src),
                None => println!("  source = <none>"),
            }
        }
    }
}

fn main() {
    // Valid value -> Ok path.
    report("42");
    // Empty -> Empty variant.
    report("");
    // Non-numeric -> #[from] conversion (Parse variant + source chain).
    report("not_a_number");
    // Out of range -> struct-field variant with formatted Display.
    report("999");

    println!("== soak_thiserror done ==");
}
