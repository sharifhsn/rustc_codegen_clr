// Minimal repro for the isolated `calli` fn-pointer-typing bug (DerfWrongPtr) + the sibling
// virtual-call Bad-IL that together blocked spinacz `reflect_params`.
//
// It mirrors `reflect_params` exactly, which is the precise shape that tripped the bug:
//   1. call a managed method returning a MANAGED ARRAY (GetMethods -> MethodInfo[]),
//   2. index an element of that array (managed ld_elem_ref),
//   3. call a method on that element that itself returns a MANAGED ARRAY
//      (GetParameters -> ParameterInfo[]) via the non-virtual `instanceN` helper,
//   4. index the nested array + call a method on its element (get_ParameterType).
//
// BEFORE the fix this produced `System.BadImageFormatException: Bad IL format` at JIT time
// (TYPECHECK_CIL=1 flagged `DerfWrongPtr { expected: FnPtr(..), got: Ptr(..) }`):
//   * the vtable-dispatch lowering built `Ptr(Ptr(FnPtr))` instead of `Ptr(FnPtr)` and then
//     deref'd it as an `FnPtr` (src/terminator/call.rs + src/terminator/mod.rs), and
//   * the non-virtual `instanceN` managed-call lowering emitted `call instance` on a virtual /
//     abstract reference-type slot (`MethodBase::GetParameters`) instead of `callvirt`.
//
// AFTER the fix this JITs and runs, matching native .NET output.
#![allow(internal_features, incomplete_features, unused_variables, dead_code)]
#![feature(adt_const_params, unsized_const_params)]

use mycorrhiza::intrinsics::RustcCLRInteropManagedArray;
use mycorrhiza::system::console::Console;
use mycorrhiza::system::MString;
use mycorrhiza::{
    System::Reflection::Assembly, System::Reflection::MethodInfo,
    System::Reflection::ParameterInfo, System::Type,
};

fn main() {
    let mstr: MString = "System.Private.CoreLib".into();
    let asm = Assembly::static1::<"Load", MString, Assembly>(mstr);
    let types = Assembly::virt0::<"GetTypes", RustcCLRInteropManagedArray<Type, 1>>(asm);
    let tpe = types.index(0);

    // GetMethods -> MethodInfo[]   (managed method returning a managed array)
    let methods = Type::instance0::<"GetMethods", RustcCLRInteropManagedArray<MethodInfo, 1>>(tpe);
    let n = methods.len();
    let mut total_params: u64 = 0;
    let mut first_param_type_seen: u64 = 0;
    let mut m = 0;
    while m < n {
        let mi = methods.index(m); // managed ld_elem_ref of an element
        m += 1;
        // exact reflect_params shape: instanceN GetParameters -> ParameterInfo[]
        let params = MethodInfo::instance0::<
            "GetParameters",
            RustcCLRInteropManagedArray<ParameterInfo, 1>,
        >(mi);
        let pn = params.len();
        total_params += pn as u64;
        if pn > 0 {
            // index the nested array + call a method on its element
            let pi = params.index(0);
            let ptpe = ParameterInfo::virt0::<"get_ParameterType", Type>(pi);
            if !ptpe.is_null() {
                first_param_type_seen += 1;
            }
        }
    }
    // Proven-good managed Console path. For System.Private.CoreLib GetTypes()[0] (== `Interop`,
    // 4 public methods) native .NET prints `1` and `1`.
    Console::writeln_u64(total_params);
    Console::writeln_u64(first_param_type_seen);
}
