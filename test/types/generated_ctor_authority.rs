#![feature(core_intrinsics)]
#![allow(internal_features)]

mod safe_marked {
    #[doc = "__rustc_codegen_clr_generated_ctor_v1"]
    #[inline(never)]
    pub fn rustc_clr_interop_managed_ctor1_(value: i32) -> i32 {
        value + 1
    }
}

mod unsafe_unmarked {
    #[inline(never)]
    pub unsafe fn rustc_clr_interop_managed_ctor1_(value: i32) -> i32 {
        value + 2
    }
}

mod wrong_abi {
    #[doc = "__rustc_codegen_clr_generated_ctor_v1"]
    #[inline(never)]
    pub unsafe extern "C" fn rustc_clr_interop_managed_ctor1_(value: i32) -> i32 {
        value + 3
    }
}

fn main() {
    let safe = safe_marked::rustc_clr_interop_managed_ctor1_(40);
    let unmarked = unsafe { unsafe_unmarked::rustc_clr_interop_managed_ctor1_(40) };
    let wrong_abi = unsafe { wrong_abi::rustc_clr_interop_managed_ctor1_(40) };
    if safe != 41 || unmarked != 42 || wrong_abi != 43 {
        core::intrinsics::abort();
    }
}
