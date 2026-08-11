#![feature(
    adt_const_params,
    core_intrinsics,
    intrinsics,
    unsafe_binders,
    unsized_const_params
)]
#![allow(incomplete_features, internal_features)]

extern crate mycorrhiza;

use mycorrhiza::{
    class::{
        Class, ConditionalRoot, ForgedRoot, LifetimeRoot, ReferenceCapabilityContainer,
        ReferenceCapabilityLocal,
    },
    intrinsics::{RustcCLRInteropManagedClass, RustcCLRInteropManagedStruct},
};

type Raw = RustcCLRInteropManagedClass<"System.Runtime", "System.Object">;
type RawValue = RustcCLRInteropManagedStruct<"Fake", "Struct.With.Object", 8>;

const fn raw() -> Raw {
    Raw { size_hint: 0 }
}

const fn raw_value() -> RawValue {
    RawValue { size_hint: [0; 8] }
}

#[cfg(managed_case = "array")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    let values = [raw(); 2];
    std::hint::black_box(values).len()
}

// Mirrors `mycorrhiza::error::TryState`: a closure containing a reference to a naked CLR value is
// ordinary Rust aggregate storage even if the callback will run immediately.
#[cfg(any(
    managed_case = "try_managed_naked_capture",
    managed_case = "try_managed_naked_result",
    managed_case = "try_managed_rooted_state"
))]
struct TestTryState<F, T> {
    func: core::mem::ManuallyDrop<F>,
    result: core::mem::MaybeUninit<T>,
}

#[cfg(any(
    managed_case = "try_managed_naked_capture",
    managed_case = "try_managed_naked_result",
    managed_case = "try_managed_rooted_state"
))]
#[inline(never)]
fn stage_try<F: FnOnce() -> T, T>(func: F) -> usize {
    let state = TestTryState {
        func: core::mem::ManuallyDrop::new(func),
        result: core::mem::MaybeUninit::<T>::uninit(),
    };
    std::hint::black_box(&state);
    core::mem::size_of_val(&state)
}

#[cfg(managed_case = "try_managed_naked_capture")]
#[unsafe(no_mangle)]
pub fn managed_storage_case(value: Raw) -> usize {
    stage_try(move || std::hint::black_box(value).size_hint)
}

#[cfg(managed_case = "try_managed_naked_result")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    stage_try(raw)
}

// The corrected production shape: TryState may contain references to token-only roots and return a
// token-only root. The raw managed identity appears only in PhantomData.
#[cfg(managed_case = "try_managed_rooted_state")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    struct Root<T> {
        token: *mut u8,
        marker: core::marker::PhantomData<fn() -> T>,
    }
    let input = Root::<Raw> {
        token: core::ptr::null_mut(),
        marker: core::marker::PhantomData,
    };
    stage_try(|| Root::<Raw> {
        token: input.token,
        marker: core::marker::PhantomData,
    })
}

// An arbitrary managed value type remains stack-only. The enum bridge's unsafe conversion helper
// must not grant blanket native-storage capability to every ManagedStruct instantiation.
#[cfg(managed_case = "managed_struct_storage")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    let values = [raw_value()];
    std::hint::black_box(values).len()
}

#[cfg(managed_case = "volatile_load")]
#[unsafe(no_mangle)]
pub unsafe fn managed_storage_case(slot: *const Raw) -> usize {
    let value = unsafe { core::intrinsics::volatile_load(slot) };
    std::hint::black_box(value).size_hint
}

#[cfg(managed_case = "volatile_store")]
#[unsafe(no_mangle)]
pub unsafe fn managed_storage_case(slot: *mut Raw) -> usize {
    unsafe { core::intrinsics::volatile_store(slot, raw()) };
    0
}

#[cfg(managed_case = "typed_swap")]
#[rustc_intrinsic]
unsafe fn typed_swap_nonoverlapping<T>(_left: *mut T, _right: *mut T) {}

#[cfg(managed_case = "typed_swap")]
#[unsafe(no_mangle)]
pub unsafe fn managed_storage_case(left: *mut Raw, right: *mut Raw) -> usize {
    unsafe { typed_swap_nonoverlapping(left, right) };
    0
}

#[cfg(managed_case = "unsafe_binder")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    unsafe {
        let value = raw();
        let binder: unsafe<'a> Raw = std::unsafe_binder::wrap_binder!(value);
        let value: Raw = std::unsafe_binder::unwrap_binder!(binder);
        std::hint::black_box(value).size_hint
    }
}

// Unsafe binders change lifetime expressibility, not storage. A primitive inner type must remain a
// positive control while the Raw case above is rejected by recursively inspecting its bound inner.
#[cfg(managed_case = "unsafe_binder_primitive")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    unsafe {
        let binder: unsafe<'a> usize = std::unsafe_binder::wrap_binder!(17usize);
        let value: usize = std::unsafe_binder::unwrap_binder!(binder);
        std::hint::black_box(value)
    }
}

#[cfg(managed_case = "recursive_raw")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    struct Node {
        value: Raw,
        next: Option<Box<Node>>,
    }
    let node = Node {
        value: raw(),
        next: None,
    };
    std::hint::black_box(node).value.size_hint
}

#[cfg(managed_case = "enum")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    enum Value {
        Empty,
        Managed(Raw),
    }
    let value = Value::Managed(raw());
    match std::hint::black_box(value) {
        Value::Empty => 0,
        Value::Managed(value) => value.size_hint,
    }
}

#[cfg(managed_case = "coroutine")]
#[unsafe(no_mangle)]
pub async fn managed_storage_case() -> usize {
    let value = raw();
    core::future::ready(()).await;
    std::hint::black_box(value).size_hint
}

#[cfg(managed_case = "coroutine_closure")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    let value = raw();
    let closure = async move || {
        core::future::ready(()).await;
        std::hint::black_box(value).size_hint
    };
    std::hint::black_box(closure);
    0
}

#[cfg(managed_case = "aggregate")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    struct Wrapper {
        value: Raw,
        sentinel: usize,
    }
    let value = Wrapper {
        value: raw(),
        sentinel: 17,
    };
    std::hint::black_box(value).sentinel
}

#[cfg(managed_case = "rc")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    let value = std::rc::Rc::new(raw());
    std::hint::black_box(value).size_hint
}

#[cfg(managed_case = "arc")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    let value = std::sync::Arc::new(raw());
    std::hint::black_box(value).size_hint
}

#[cfg(managed_case = "vecdeque")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    let mut values = std::collections::VecDeque::new();
    values.push_back(raw());
    std::hint::black_box(values).len()
}

#[cfg(managed_case = "closure")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    let value = raw();
    let closure = move || std::hint::black_box(value).size_hint;
    closure()
}

#[cfg(managed_case = "box")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    let value = Box::new(raw());
    std::hint::black_box(value).size_hint
}

#[cfg(managed_case = "vec")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    let values = vec![raw(), raw()];
    std::hint::black_box(values).len()
}

#[cfg(managed_case = "static")]
static BAD_STATIC: Raw = raw();

#[cfg(managed_case = "static")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    std::hint::black_box(BAD_STATIC).size_hint
}

#[cfg(managed_case = "static_ref")]
static BAD_STATIC_REF: &Raw = &raw();

#[cfg(managed_case = "static_ref")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    std::hint::black_box(BAD_STATIC_REF).size_hint
}

#[cfg(managed_case = "static_slice")]
static BAD_STATIC_SLICE: &[Raw] = &[raw()];

#[cfg(managed_case = "static_slice")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    std::hint::black_box(BAD_STATIC_SLICE).len()
}

#[cfg(managed_case = "static_wrapped_ref")]
struct StaticWrapper {
    values: &'static [Raw],
}

#[cfg(managed_case = "static_wrapped_ref")]
static BAD_STATIC_WRAPPER: StaticWrapper = StaticWrapper { values: &[raw()] };

#[cfg(managed_case = "static_wrapped_ref")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    std::hint::black_box(BAD_STATIC_WRAPPER.values).len()
}

// The referent is an anonymous promoted static, so there is no queryable type on its nested
// MonoItem. The typed constant use in this body must still reject the backing native allocation.
#[cfg(managed_case = "anonymous_promotion")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    std::hint::black_box(&raw()).size_hint
}

#[cfg(managed_case = "copy")]
#[unsafe(no_mangle)]
pub unsafe fn managed_storage_case(source: *const Raw, destination: *mut Raw) -> usize {
    unsafe { core::intrinsics::copy_nonoverlapping(source, destination, 1) };
    0
}

#[cfg(managed_case = "indirect_write")]
#[unsafe(no_mangle)]
pub unsafe fn managed_storage_case(slot: *mut Raw) -> usize {
    unsafe { slot.write(raw()) };
    0
}

#[cfg(managed_case = "indirect_read")]
#[unsafe(no_mangle)]
pub unsafe fn managed_storage_case(slot: *const Raw) -> usize {
    let value = unsafe { slot.read() };
    std::hint::black_box(value).size_hint
}

#[cfg(managed_case = "transmute")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    unsafe { core::mem::transmute::<Raw, usize>(raw()) }
}

#[cfg(managed_case = "raw_pointer_formation")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    let value = raw();
    let pointer = &raw const value;
    std::hint::black_box(pointer);
    0
}

#[cfg(managed_case = "reference_storage")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    struct StoredRef<'a> {
        value: &'a Raw,
    }
    let value = raw();
    let stored = StoredRef { value: &value };
    std::hint::black_box(stored).value.size_hint
}

#[cfg(managed_case = "pointer_return")]
#[unsafe(no_mangle)]
pub fn managed_storage_case(value: &Raw) -> &Raw {
    value
}

#[cfg(managed_case = "external_argument")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn managed_storage_case(_value: *const Raw) -> usize {
    0
}

#[cfg(managed_case = "external_return")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn managed_storage_case() -> Raw {
    raw()
}

// The inert marker string is not authority by itself. Only an explicitly unsafe generated
// `extern "C-unwind"` seam may opt into CLR-managed argument/return ABI.
#[cfg(managed_case = "external_safe_managed_marker")]
#[unsafe(no_mangle)]
#[doc = "__rustc_codegen_clr_managed_export_v1"]
pub extern "C-unwind" fn managed_storage_case() -> Raw {
    raw()
}

// Exact positive control for the generated seam contract. Opting into managed ABI is itself an
// unsafe promise; unlike the safe near-miss above, this definition is intentionally allowed to
// expose a direct CLR value at the CIL boundary.
#[cfg(managed_case = "external_managed_marker")]
#[unsafe(no_mangle)]
#[doc = "__rustc_codegen_clr_managed_export_v1"]
pub unsafe extern "C-unwind" fn managed_storage_case(value: Raw) -> Raw {
    value
}

#[cfg(managed_case = "external_unsafe_no_managed_marker")]
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn managed_storage_case() -> Raw {
    raw()
}

#[cfg(managed_case = "external_marked_nonunwind")]
#[unsafe(no_mangle)]
#[doc = "__rustc_codegen_clr_managed_export_v1"]
pub unsafe extern "C" fn managed_storage_case() -> Raw {
    raw()
}

#[cfg(managed_case = "external_fn_pointer")]
#[unsafe(no_mangle)]
pub unsafe fn managed_storage_case(
    function: unsafe extern "C" fn(*const Raw),
    value: *const Raw,
) -> usize {
    unsafe { function(value) };
    0
}

#[cfg(managed_case = "external_call_return")]
#[unsafe(no_mangle)]
pub unsafe fn managed_storage_case(function: unsafe extern "C" fn() -> Raw) -> usize {
    let value = unsafe { function() };
    std::hint::black_box(value).size_hint
}

// A tuple containing managed references is not generally legal storage merely because the
// RustCall ABI can spread a different, compiler-generated argument pack.
#[cfg(managed_case = "rust_call_tuple_persist")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    let values = (raw(), raw());
    std::hint::black_box(values).0.size_hint
}

#[cfg(managed_case = "rust_call_tuple_borrow")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    let values = (raw(), raw());
    let borrowed = &values;
    std::hint::black_box(borrowed).0.size_hint
}

#[cfg(managed_case = "rust_call_tuple_copy")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    let values = (raw(), raw());
    let copied = values;
    std::hint::black_box(values);
    std::hint::black_box(copied).0.size_hint
}

#[cfg(managed_case = "forged_root")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    let values = [ForgedRoot { value: raw() }];
    std::hint::black_box(values)[0].value.size_hint
}

#[cfg(managed_case = "reference_capability")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    let local = ReferenceCapabilityLocal { value: raw() };
    let values = [&local];
    std::hint::black_box(values).len()
}

#[cfg(managed_case = "container_reference_capability")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    let local = ReferenceCapabilityLocal { value: raw() };
    let values = [ReferenceCapabilityContainer { value: &local }];
    std::hint::black_box(values).len()
}

#[cfg(managed_case = "conditional_capability_raw")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    let values = [ConditionalRoot { value: raw() }];
    std::hint::black_box(values)[0].value.size_hint
}

#[cfg(managed_case = "lifetime_capability")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    let value = LifetimeRoot {
        value: raw(),
        marker: core::marker::PhantomData,
    };
    let values = [value];
    std::hint::black_box(values)[0].value.size_hint
}

#[cfg(managed_case = "write_bytes")]
#[unsafe(no_mangle)]
pub unsafe fn managed_storage_case(destination: *mut Raw) -> usize {
    unsafe { core::intrinsics::write_bytes(destination, 0, 1) };
    0
}

// A direct managed local/argument/return remains legal: it lives in a CLR-tracked slot rather than
// native aggregate bytes.
#[cfg(managed_case = "transient")]
#[unsafe(no_mangle)]
pub fn managed_storage_case(value: Raw) -> Raw {
    std::hint::black_box(value)
}

#[cfg(managed_case = "safe_helper_near_miss")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    mycorrhiza::enums::rustc_clr_interop_enum_from_repr(41)
}

// A Rust-ABI indirect call never crosses into native code. Both the callee body and the calli ABI
// are checked, so a direct stack-only managed value remains legal here.
#[cfg(managed_case = "internal_fn_pointer")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    fn identity(value: Raw) -> Raw {
        value
    }
    let function: fn(Raw) -> Raw = identity;
    function(raw()).size_hint
}

// A private C-ABI definition can be a managed-only calli target. Taking its address ensures rustc
// emits and validates the nested function without introducing an unknown external call site.
#[cfg(managed_case = "private_c_abi_definition")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    unsafe extern "C" fn trampoline(value: Raw) -> Raw {
        value
    }
    let function: unsafe extern "C" fn(Raw) -> Raw = trampoline;
    std::hint::black_box(function);
    0
}

// `Fn::call`'s trailing tuple is an ABI pack, not Rust-owned byte storage: the backend decomposes
// it field-by-field into physical managed argument slots. This mirrors the tuple generated inside
// Mycorrhiza's capturing delegate trampolines.
#[cfg(managed_case = "rust_call_tuple")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    let callback = |left: Raw, right: Raw| left.size_hint + right.size_hint;
    let callback: &dyn Fn(Raw, Raw) -> usize = &callback;
    callback(raw(), raw())
}

// A shared reference may be used transiently to perform the representation-preserving Copy read
// generated by real raw value-type receiver/Clone shims. It may not be persisted or externalized.
#[cfg(managed_case = "value_type_receiver")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    raw_value().copy_via_ref().size_hint.len()
}

#[cfg(managed_case = "conditional_capability")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    let values = [ConditionalRoot { value: 17usize }];
    std::hint::black_box(values)[0].value
}

// With the safe forged raw-identity trait variant, this remains an ordinary Rust struct. Storing
// it in an array is the decisive compile oracle: managed interpretation would reject the case.
#[cfg(managed_case = "raw_identity_near_miss")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    let values = [raw(), raw()];
    std::hint::black_box(values)[0].size_hint + 1
}

#[cfg(managed_case = "recursive_safe")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    struct Node(Option<Box<Node>>);
    let node = Node(None);
    usize::from(std::hint::black_box(node).0.is_some())
}

#[cfg(managed_case = "zero_array")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    let values: [Raw; 0] = [];
    std::hint::black_box(values);
    0
}

#[cfg(managed_case = "pointer_phantom")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    struct Id<T>(usize, core::marker::PhantomData<fn() -> T>);
    // Keep the values uninitialized: this test is about their physical storage shape, not the
    // backend's separate policy for casting pointers whose pointee is an interop marker.
    let pointer = core::mem::MaybeUninit::<core::ptr::NonNull<Raw>>::uninit();
    let atomic = core::mem::MaybeUninit::<core::sync::atomic::AtomicPtr<Raw>>::uninit();
    let id = Id::<Raw>(23, core::marker::PhantomData);
    let result = id.0;
    std::hint::black_box((pointer, atomic, id));
    result
}

// This support type's physical fields are only a token and PhantomData. The stale marker above is
// intentionally ignored; unlike ForgedRoot it is safe because it embeds no managed reference.
#[cfg(managed_case = "rooted")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    let roots = [Class::<Raw>::test_handle(11), Class::test_handle(13)];
    std::hint::black_box(roots)[1].handle
}

// The cancellation-registration shape is safe because both managed values are represented only
// by native GCHandle tokens; the raw types occur solely in PhantomData identity.
#[cfg(managed_case = "rooted_registration")]
#[unsafe(no_mangle)]
pub fn managed_storage_case() -> usize {
    struct ManagedRef<T> {
        rooted: *mut u8,
        marker: core::marker::PhantomData<fn() -> T>,
    }
    struct Registration {
        registration: ManagedRef<RawValue>,
        action: ManagedRef<Raw>,
        active: bool,
        callback: Option<Box<usize>>,
    }
    let value = Registration {
        registration: ManagedRef {
            rooted: core::ptr::null_mut(),
            marker: core::marker::PhantomData,
        },
        action: ManagedRef {
            rooted: core::ptr::null_mut(),
            marker: core::marker::PhantomData,
        },
        active: true,
        callback: Some(Box::new(17)),
    };
    let value = std::hint::black_box(value);
    usize::from(value.active)
        + value.callback.map_or(0, |value| *value)
        + usize::from(value.registration.rooted.is_null())
        + usize::from(value.action.rooted.is_null())
}
