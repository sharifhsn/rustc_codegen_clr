use std::fmt::Debug;

#[derive(Debug)]
/// Repersentation of an error which occured while converting MIR to CIL assembly.
pub enum CodegenError {
    UnersolvedGeneric,
    Error(crate::IString),
    Method(MethodCodegenError),
    FunctionABIUnsuported(&'static str),
}

impl From<MethodCodegenError> for CodegenError {
    fn from(value: MethodCodegenError) -> Self {
        Self::Method(value)
    }
}
impl CodegenError {
    pub fn from_panic_message(msg: &str) -> Self {
        Self::Error(msg.into())
    }
}

/// Best-effort extraction of a human-readable message from a panic payload.
///
/// `std` panics carry their message as either `&'static str` (`panic!("literal")`) or
/// `String` (`assert!`, `assert_eq!`, `panic!("{}", x)`, `todo!`/`unimplemented!` with args).
/// The codegen's per-statement/-terminator panic recovery previously only handled `&str`, so
/// every *formatted* panic — which is the majority of the backend's `todo!`/`assert!` holes —
/// was reported as an opaque "non-string message", defeating the "fail loud + specific" goal
/// that turns the build-std walk into an actionable backlog. Handle both here.
#[must_use]
pub fn panic_payload_msg(payload: &(dyn std::any::Any + Send)) -> Option<&str> {
    if let Some(msg) = payload.downcast_ref::<&'static str>() {
        Some(msg)
    } else {
        payload.downcast_ref::<String>().map(String::as_str)
    }
}

pub struct MethodCodegenError {
    file: String,
    line: u32,
    column: u32,
    message: String,
}
impl MethodCodegenError {
    pub fn new(file: &str, line: u32, column: u32, message: String) -> Self {
        Self {
            file: file.into(),
            line,
            column,
            message,
        }
    }
    pub fn report(&self) {
        eprintln!(
            "Method Codegen Error: {file}({line},{column}): {message}",
            file = self.file,
            line = self.line,
            column = self.column,
            message = self.message
        );
    }
}
impl Debug for MethodCodegenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Method Codegen Error: {file}({line},{column}): {message}",
            file = self.file,
            line = self.line,
            column = self.column,
            message = self.message
        )
    }
}
