pub mod slice{
pub mod System{
pub mod Text{
pub type StringBuilder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Text.StringBuilder">;
use super::super::*;
impl StringBuilder {
    pub fn new() -> Self { Self::ctor0() }
    pub fn append(self, a1: i32) -> crate::System::Text::StringBuilder { self.instance1::<"Append", i32, crate::System::Text::StringBuilder>(a1) }
    pub fn get_length(self) -> i32 { self.virt0::<"get_Length", i32>() }
}
}
pub type Console =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Console","System.Console">;
use super::*;
impl Console {
    pub fn write_line(a1: crate::System::String) { Self::static1::<"WriteLine", crate::System::String, ()>(a1) }
}
pub type Math =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Math">;
use super::*;
impl Math {
    pub fn max(a1: i32, a2: i32) -> i32 { Self::static2::<"Max", i32, i32, i32>(a1, a2) }
    pub fn min(a1: i32, a2: i32) -> i32 { Self::static2::<"Min", i32, i32, i32>(a1, a2) }
    pub fn abs(a1: i32) -> i32 { Self::static1::<"Abs", i32, i32>(a1) }
    pub fn sqrt(a1: f64) -> f64 { Self::static1::<"Sqrt", f64, f64>(a1) }
}
pub type String =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.String">;
use super::*;
impl String {
    pub fn get_length(self) -> i32 { self.virt0::<"get_Length", i32>() }
}
}
}
