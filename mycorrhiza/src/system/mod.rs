use runtime::interop_services::Marshal;

pub mod console;
pub mod diagnostics;
pub mod runtime;
pub mod text;
// `System.String` physically lives in `System.Private.CoreLib` (it's only type-*forwarded* from
// `System.Runtime`). Binding the assembly to `System.Runtime` makes instance-method calls emit a
// `call instance ... [System.Runtime]System.String::method` methodref that the JIT rejects as
// "Bad IL format" once the value is a real CoreLib String (e.g. a `get_FullName()` result). Use
// the defining assembly, matching every other `System.String` binding in the tree.
pub type MString =
    crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib", "System.String">;

impl From<&str> for MString {
    fn from(val: &str) -> Self {
        Marshal::static2::<"PtrToStringUTF8", isize, i32, MString>(
            val.as_ptr() as isize,
            val.len() as i32,
        )
    }
}
