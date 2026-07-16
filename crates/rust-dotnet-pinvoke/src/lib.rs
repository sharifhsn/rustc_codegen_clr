#![cfg_attr(not(feature = "std"), no_std)]
//! Explicit helpers for building safe wrappers around raw P/Invoke declarations.
//!
//! This crate does not declare imports or infer ownership. Its types make common validation and
//! cleanup steps reusable while leaving the native call itself visibly `unsafe`.

#[cfg(feature = "alloc")]
extern crate alloc;

/// A native API status code retained without lossy conversion.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeStatusError(pub i32);
impl NativeStatusError {
    /// Returns the original native status code.
    pub const fn code(self) -> i32 {
        self.0
    }
}

impl core::fmt::Display for NativeStatusError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "native call failed with status {}", self.0)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for NativeStatusError {}

/// Validation failure while borrowing a native string buffer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StringError {
    /// Bytes before the terminator are not valid UTF-8.
    InvalidUtf8,
    /// Code units before the terminator are not valid UTF-16.
    InvalidUtf16,
    /// A byte buffer has no NUL terminator.
    MissingNul,
    /// A UTF-16 buffer has no NUL terminator.
    UnterminatedUtf16,
    /// An owned string contains an interior NUL and cannot be passed as a C string.
    InteriorNul,
}

/// Error returned by the managed-feeling native facade helpers.
///
/// Unlike a bare status code, this distinguishes invalid Rust arguments, a native failure, and a
/// native API violating its documented non-null return contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeCallError {
    Status(NativeStatusError),
    String(StringError),
    NullHandle,
    NullString,
    #[cfg(feature = "alloc")]
    StatusMessage {
        status: NativeStatusError,
        message: alloc::string::String,
    },
    #[cfg(feature = "alloc")]
    UnexpectedMessage(alloc::string::String),
}

impl core::fmt::Display for NativeCallError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Status(status) => status.fmt(formatter),
            Self::String(error) => error.fmt(formatter),
            Self::NullHandle => {
                formatter.write_str("native call succeeded but returned a null handle")
            }
            Self::NullString => {
                formatter.write_str("native call succeeded but returned a null string")
            }
            #[cfg(feature = "alloc")]
            Self::StatusMessage { status, message } => {
                write!(
                    formatter,
                    "native call failed with status {}: {message}",
                    status.code()
                )
            }
            #[cfg(feature = "alloc")]
            Self::UnexpectedMessage(message) => {
                write!(
                    formatter,
                    "native call reported success with an error message: {message}"
                )
            }
        }
    }
}

impl From<NativeStatusError> for NativeCallError {
    fn from(value: NativeStatusError) -> Self {
        Self::Status(value)
    }
}

impl From<StringError> for NativeCallError {
    fn from(value: StringError) -> Self {
        Self::String(value)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for NativeCallError {}

impl core::fmt::Display for StringError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidUtf8 => "native string is not valid UTF-8",
            Self::InvalidUtf16 => "native string is not valid UTF-16",
            Self::MissingNul => "native byte string has no NUL terminator",
            Self::UnterminatedUtf16 => "native UTF-16 string has no NUL terminator",
            Self::InteriorNul => "native string contains an interior NUL",
        })
    }
}

/// Interprets zero as success and preserves any non-zero native status code.
pub const fn status_zero(code: i32) -> Result<(), NativeStatusError> {
    if code == 0 {
        Ok(())
    } else {
        Err(NativeStatusError(code))
    }
}

/// Interprets non-negative values as success and preserves a negative native status code.
pub const fn status_nonnegative(code: i32) -> Result<i32, NativeStatusError> {
    if code >= 0 {
        Ok(code)
    } else {
        Err(NativeStatusError(code))
    }
}

#[cfg(feature = "std")]
impl std::error::Error for StringError {}

/// Borrows the UTF-8 prefix before the first NUL in `bytes`.
pub fn cstr_utf8(bytes: &[u8]) -> Result<&str, StringError> {
    let end = bytes
        .iter()
        .position(|&b| b == 0)
        .ok_or(StringError::MissingNul)?;
    core::str::from_utf8(&bytes[..end]).map_err(|_| StringError::InvalidUtf8)
}

/// Borrows the UTF-16 code units before the first NUL in `units`.
///
/// The result is not decoded because callers may need platform-specific handling of unpaired
/// surrogates. With `std`, pass the returned slice to `String::from_utf16` when strict decoding is
/// appropriate.
pub fn utf16_nul(units: &[u16]) -> Result<&[u16], StringError> {
    units
        .iter()
        .position(|&u| u == 0)
        .map(|end| &units[..end])
        .ok_or(StringError::UnterminatedUtf16)
}

/// An owned, NUL-terminated UTF-8 string suitable for `*const c_char` parameters.
#[cfg(feature = "alloc")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Utf8CString(alloc::vec::Vec<u8>);

#[cfg(feature = "alloc")]
impl Utf8CString {
    /// Copies a Rust string and appends its terminating NUL.
    pub fn new(value: &str) -> Result<Self, StringError> {
        if value.as_bytes().contains(&0) {
            return Err(StringError::InteriorNul);
        }
        let mut bytes = alloc::vec::Vec::with_capacity(value.len() + 1);
        bytes.extend_from_slice(value.as_bytes());
        bytes.push(0);
        Ok(Self(bytes))
    }

    /// Returns a stable pointer for the duration of the borrow.
    pub fn as_ptr(&self) -> *const core::ffi::c_char {
        self.0.as_ptr().cast()
    }

    /// Returns the byte length excluding the terminating NUL.
    pub fn len(&self) -> usize {
        self.0.len() - 1
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn as_bytes_with_nul(&self) -> &[u8] {
        &self.0
    }
}

/// Runs `call` while a checked, NUL-terminated UTF-8 argument is alive.
#[cfg(feature = "alloc")]
pub fn with_utf8_cstr<R>(
    value: &str,
    call: impl FnOnce(*const core::ffi::c_char) -> R,
) -> Result<R, StringError> {
    let value = Utf8CString::new(value)?;
    Ok(call(value.as_ptr()))
}

/// An owned, NUL-terminated UTF-16 string suitable for Windows wide-string parameters.
#[cfg(feature = "alloc")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Utf16CString(alloc::vec::Vec<u16>);

#[cfg(feature = "alloc")]
impl Utf16CString {
    pub fn new(value: &str) -> Result<Self, StringError> {
        if value.chars().any(|ch| ch == '\0') {
            return Err(StringError::InteriorNul);
        }
        let mut units: alloc::vec::Vec<u16> = value.encode_utf16().collect();
        units.push(0);
        Ok(Self(units))
    }

    pub fn as_ptr(&self) -> *const u16 {
        self.0.as_ptr()
    }

    pub fn len(&self) -> usize {
        self.0.len() - 1
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn as_units_with_nul(&self) -> &[u16] {
        &self.0
    }
}

/// Runs `call` while a checked, NUL-terminated UTF-16 argument is alive.
#[cfg(feature = "alloc")]
pub fn with_utf16_cstr<R>(
    value: &str,
    call: impl FnOnce(*const u16) -> R,
) -> Result<R, StringError> {
    let value = Utf16CString::new(value)?;
    Ok(call(value.as_ptr()))
}

/// Uninitialized storage for an ordinary native out-parameter.
pub struct Out<T>(core::mem::MaybeUninit<T>);

impl<T> Out<T> {
    pub const fn new() -> Self {
        Self(core::mem::MaybeUninit::uninit())
    }

    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.0.as_mut_ptr()
    }

    /// Reads the initialized value after the native function reports success.
    ///
    /// # Safety
    ///
    /// The native call must have initialized the entire value.
    pub unsafe fn assume_init(self) -> T {
        unsafe { self.0.assume_init() }
    }
}

impl<T> Default for Out<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Calls a native function with an out pointer and exposes the value only after success.
///
/// # Safety
///
/// On `Ok(())`, `call` must have initialized the entire value at the supplied pointer. It must not
/// retain the pointer after returning.
pub unsafe fn try_out<T, E>(call: impl FnOnce(*mut T) -> Result<(), E>) -> Result<T, E> {
    let mut out = Out::<T>::new();
    call(out.as_mut_ptr())?;
    Ok(unsafe { out.assume_init() })
}

/// Copies a native-owned UTF-8 error string and then frees it exactly once.
///
/// A null pointer produces `Ok(None)`. Invalid UTF-8 is reported after the native allocation has
/// still been released.
///
/// # Safety
///
/// `pointer` must be null or point to a live NUL-terminated byte string allocated for `free`.
#[cfg(feature = "std")]
pub unsafe fn take_utf8_string(
    pointer: *mut core::ffi::c_char,
    free: impl FnOnce(*mut core::ffi::c_void),
) -> Result<Option<alloc::string::String>, StringError> {
    if pointer.is_null() {
        return Ok(None);
    }
    let result = unsafe { std::ffi::CStr::from_ptr(pointer) }
        .to_str()
        .map(alloc::string::ToString::to_string)
        .map_err(|_| StringError::InvalidUtf8);
    free(pointer.cast());
    result.map(Some)
}

/// Copies a native-owned, NUL-terminated UTF-16 string and then frees it exactly once.
///
/// A null pointer produces `Ok(None)`. Invalid UTF-16 is reported after the native allocation has
/// still been released.
///
/// # Safety
///
/// `pointer` must be null or point to a live NUL-terminated UTF-16 allocation owned by `free`.
#[cfg(feature = "std")]
pub unsafe fn take_utf16_string(
    pointer: *mut u16,
    free: impl FnOnce(*mut core::ffi::c_void),
) -> Result<Option<alloc::string::String>, StringError> {
    if pointer.is_null() {
        return Ok(None);
    }
    let mut len = 0usize;
    while unsafe { pointer.add(len).read() } != 0 {
        len += 1;
    }
    let units = unsafe { core::slice::from_raw_parts(pointer, len) };
    let result = alloc::string::String::from_utf16(units).map_err(|_| StringError::InvalidUtf16);
    free(pointer.cast());
    result.map(Some)
}

/// Owns a heap-stable Rust closure used as a native callback context.
///
/// Keep this value alive until the native library has unregistered the callback. The generated
/// trampoline can recover and invoke the closure through [`Callback::invoke_abort_on_panic`].
#[cfg(feature = "alloc")]
pub struct Callback<Args, Return> {
    inner: alloc::boxed::Box<CallbackInner<Args, Return>>,
}

#[cfg(feature = "alloc")]
struct CallbackInner<Args, Return> {
    callback: alloc::boxed::Box<dyn FnMut(Args) -> Return>,
}

#[cfg(feature = "alloc")]
impl<Args, Return> Callback<Args, Return> {
    pub fn new(callback: impl FnMut(Args) -> Return + 'static) -> Self {
        Self {
            inner: alloc::boxed::Box::new(CallbackInner {
                callback: alloc::boxed::Box::new(callback),
            }),
        }
    }

    pub fn context(&mut self) -> *mut core::ffi::c_void {
        (&mut *self.inner as *mut CallbackInner<Args, Return>).cast()
    }

    /// Invokes a context previously returned by [`Callback::context`].
    ///
    /// # Safety
    ///
    /// `context` must come from a live `Callback<Args, Return>` and may not be invoked
    /// concurrently unless the closure provides its own synchronization.
    pub unsafe fn invoke(context: *mut core::ffi::c_void, args: Args) -> Return {
        let inner = unsafe { &mut *context.cast::<CallbackInner<Args, Return>>() };
        (inner.callback)(args)
    }
}

/// A heap-stable callback callable concurrently from arbitrary native threads.
///
/// Use this through [`CallbackRegistration`]. Mutable state must use explicit synchronization in
/// the closure itself; requiring `Fn + Send + Sync` avoids hidden locking and reflects the native
/// API's actual concurrency contract.
#[cfg(feature = "std")]
pub struct ThreadSafeCallback<Args, Return> {
    inner: alloc::boxed::Box<ThreadSafeCallbackInner<Args, Return>>,
}

#[cfg(feature = "std")]
struct ThreadSafeCallbackInner<Args, Return> {
    callback: alloc::boxed::Box<dyn Fn(Args) -> Return + Send + Sync>,
}

#[cfg(feature = "std")]
impl<Args, Return> ThreadSafeCallback<Args, Return> {
    fn new(callback: impl Fn(Args) -> Return + Send + Sync + 'static) -> Self {
        Self {
            inner: alloc::boxed::Box::new(ThreadSafeCallbackInner {
                callback: alloc::boxed::Box::new(callback),
            }),
        }
    }

    fn context(&self) -> *mut core::ffi::c_void {
        (&*self.inner as *const ThreadSafeCallbackInner<Args, Return>)
            .cast_mut()
            .cast()
    }

    /// Invokes a context owned by a live [`CallbackRegistration`].
    ///
    /// # Safety
    ///
    /// `context` must belong to a live thread-safe registration, and native unregistration must
    /// guarantee quiescence before that registration is dropped.
    pub unsafe fn invoke(context: *mut core::ffi::c_void, args: Args) -> Return {
        let inner = unsafe { &*context.cast::<ThreadSafeCallbackInner<Args, Return>>() };
        (inner.callback)(args)
    }
}

/// Owns a callback together with the native token that retains its context.
///
/// `try_unregister` is the normal shutdown path. Failure returns the still-live registration so
/// the caller can retry. `Drop` performs a best-effort unregister; if it fails or panics, the guard
/// leaks its callback, token, and unregister closure rather than freeing memory native code may
/// still call.
#[cfg(feature = "std")]
#[must_use = "dropping the guard unregisters the native callback"]
pub struct CallbackRegistration<Args, Return, Token, Unregister, UnregisterError>
where
    Unregister: FnMut(&mut Token) -> Result<(), UnregisterError>,
{
    callback: Option<ThreadSafeCallback<Args, Return>>,
    token: Option<Token>,
    unregister: Option<Unregister>,
    error: core::marker::PhantomData<fn() -> UnregisterError>,
}

/// An unregister error that retains ownership of the live registration for retry.
#[cfg(feature = "std")]
#[must_use = "the registration remains live and must be retried or deliberately dropped"]
pub struct UnregisterFailure<Registration, Error> {
    registration: Registration,
    error: Error,
}

#[cfg(feature = "std")]
impl<Registration, Error> UnregisterFailure<Registration, Error> {
    pub fn error(&self) -> &Error {
        &self.error
    }

    pub fn registration(&self) -> &Registration {
        &self.registration
    }

    pub fn into_registration(self) -> Registration {
        self.registration
    }

    pub fn into_parts(self) -> (Registration, Error) {
        (self.registration, self.error)
    }
}

#[cfg(feature = "std")]
impl<Registration, Error: core::fmt::Debug> core::fmt::Debug
    for UnregisterFailure<Registration, Error>
{
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("UnregisterFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "std")]
impl<Registration, Error: core::fmt::Display> core::fmt::Display
    for UnregisterFailure<Registration, Error>
{
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            formatter,
            "native callback remains registered: {}",
            self.error
        )
    }
}

#[cfg(feature = "std")]
impl<Registration, Error> std::error::Error for UnregisterFailure<Registration, Error>
where
    Error: std::error::Error + core::fmt::Debug + 'static,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

#[cfg(feature = "std")]
impl<Args, Return, Token, Unregister, UnregisterError>
    CallbackRegistration<Args, Return, Token, Unregister, UnregisterError>
where
    Unregister: FnMut(&mut Token) -> Result<(), UnregisterError>,
{
    /// Registers a callback whose native owner may invoke it after `register` returns.
    ///
    /// # Safety
    ///
    /// On registration failure, `register` must not retain `context`. On unregister success,
    /// `unregister` must prevent every future invocation and wait for all in-flight invocations to
    /// finish. If the native API separates stop and join, the closure must perform both.
    pub unsafe fn register<RegistrationError>(
        callback: impl Fn(Args) -> Return + Send + Sync + 'static,
        register: impl FnOnce(*mut core::ffi::c_void) -> Result<Token, RegistrationError>,
        unregister: Unregister,
    ) -> Result<Self, RegistrationError> {
        let callback = ThreadSafeCallback::new(callback);
        let token = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            register(callback.context())
        })) {
            Ok(result) => result?,
            Err(panic) => {
                // A panicking foreign registration closure cannot tell us whether it retained the
                // context. Keep that context valid rather than risking callback-after-free.
                core::mem::forget(callback);
                std::panic::resume_unwind(panic)
            }
        };
        Ok(Self {
            callback: Some(callback),
            token: Some(token),
            unregister: Some(unregister),
            error: core::marker::PhantomData,
        })
    }

    pub fn is_registered(&self) -> bool {
        self.token.is_some()
    }

    /// Unregisters, waits for callback quiescence, and then frees the callback context.
    ///
    /// Failure preserves the entire live registration in [`UnregisterFailure`] for retry.
    pub fn try_unregister(mut self) -> Result<(), UnregisterFailure<Self, UnregisterError>> {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let unregister = self
                .unregister
                .as_mut()
                .expect("registered callback has no unregister function");
            let token = self
                .token
                .as_mut()
                .expect("registered callback has no native token");
            unregister(token)
        }));
        match result {
            Ok(Ok(())) => {
                self.token.take();
                self.callback.take();
                self.unregister.take();
                Ok(())
            }
            Ok(Err(error)) => Err(UnregisterFailure {
                registration: self,
                error,
            }),
            Err(panic) => {
                self.leak_native_state();
                std::panic::resume_unwind(panic)
            }
        }
    }

    /// Alias for [`Self::try_unregister`] for resource-style APIs.
    pub fn close(self) -> Result<(), UnregisterFailure<Self, UnregisterError>> {
        self.try_unregister()
    }

    fn leak_native_state(&mut self) {
        if let Some(callback) = self.callback.take() {
            core::mem::forget(callback);
        }
        if let Some(token) = self.token.take() {
            core::mem::forget(token);
        }
        if let Some(unregister) = self.unregister.take() {
            core::mem::forget(unregister);
        }
    }
}

#[cfg(feature = "std")]
impl<Args, Return, Token, Unregister, UnregisterError> Drop
    for CallbackRegistration<Args, Return, Token, Unregister, UnregisterError>
where
    Unregister: FnMut(&mut Token) -> Result<(), UnregisterError>,
{
    fn drop(&mut self) {
        if self.token.is_none() {
            return;
        }
        let unregistered = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let unregister = self
                .unregister
                .as_mut()
                .expect("registered callback has no unregister function");
            let token = self
                .token
                .as_mut()
                .expect("registered callback has no native token");
            unregister(token).is_ok()
        }))
        .unwrap_or(false);
        if !unregistered {
            self.leak_native_state();
        }
    }
}

/// A retained native registration that can stop quiescently while preserving itself on failure.
///
/// API-specific registration guards can implement this trait without exposing their raw token.
/// [`CallbackRegistration`] implements it directly. The tuple error deliberately returns ownership
/// of `Self`; [`NativeJob::try_stop`] restores that live registration before surfacing the error.
#[cfg(feature = "std")]
pub trait RetryableStop: Sized {
    type Error;

    fn try_stop(self) -> Result<(), (Self, Self::Error)>;
}

#[cfg(feature = "std")]
impl<Args, Return, Token, Unregister, UnregisterError> RetryableStop
    for CallbackRegistration<Args, Return, Token, Unregister, UnregisterError>
where
    Unregister: FnMut(&mut Token) -> Result<(), UnregisterError>,
{
    type Error = UnregisterError;

    fn try_stop(self) -> Result<(), (Self, Self::Error)> {
        self.try_unregister().map_err(UnregisterFailure::into_parts)
    }
}

/// Observable lifecycle state of a [`NativeJob`].
#[cfg(feature = "std")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeJobStatus {
    Running,
    Succeeded,
    Failed,
    Stopped,
}

/// Cooperative-cancellation marker returned by [`NativeJobController::ensure_not_canceled`].
/// It contains no managed state and is safe to use from a retained native callback or map into an
/// async export's explicit canceled-task policy.
#[cfg(feature = "std")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeCancellationRequested;

#[cfg(feature = "std")]
enum NativeJobCompletion<ResultValue, OperationError> {
    Running,
    Succeeded(Option<ResultValue>),
    Failed(Option<OperationError>),
    Stopped,
}

#[cfg(feature = "std")]
struct NativeJobInner<ResultValue, OperationError, ProgressValue> {
    completion: std::sync::Mutex<NativeJobCompletion<ResultValue, OperationError>>,
    cancellation_requested: std::sync::atomic::AtomicBool,
    progress: alloc::sync::Arc<dyn Fn(ProgressValue) + Send + Sync>,
}

/// Thread-safe control handle captured by an API-specific retained callback.
///
/// It contains only native Rust synchronization state. A managed adapter may root CLR cancellation
/// and progress objects outside this type, then forward them through the supplied progress closure.
#[cfg(feature = "std")]
pub struct NativeJobController<ResultValue, OperationError, ProgressValue> {
    inner: alloc::sync::Arc<NativeJobInner<ResultValue, OperationError, ProgressValue>>,
}

#[cfg(feature = "std")]
impl<ResultValue, OperationError, ProgressValue> Clone
    for NativeJobController<ResultValue, OperationError, ProgressValue>
{
    fn clone(&self) -> Self {
        Self {
            inner: alloc::sync::Arc::clone(&self.inner),
        }
    }
}

#[cfg(feature = "std")]
impl<ResultValue, OperationError, ProgressValue>
    NativeJobController<ResultValue, OperationError, ProgressValue>
{
    /// Request cooperative cancellation through a callback-safe control handle.
    pub fn request_cancellation(&self) {
        self.inner
            .cancellation_requested
            .store(true, std::sync::atomic::Ordering::Release);
    }

    pub fn is_cancellation_requested(&self) -> bool {
        self.inner
            .cancellation_requested
            .load(std::sync::atomic::Ordering::Acquire)
    }

    /// `?`-friendly cooperative cancellation check for callback and worker code.
    pub fn ensure_not_canceled(&self) -> Result<(), NativeCancellationRequested> {
        if self.is_cancellation_requested() {
            Err(NativeCancellationRequested)
        } else {
            Ok(())
        }
    }

    pub fn report_progress(&self, value: ProgressValue) {
        (self.inner.progress)(value);
    }

    /// Complete the job once. A late/duplicate result is returned to the callback rather than
    /// silently dropped.
    pub fn complete(&self, value: ResultValue) -> Result<(), ResultValue> {
        let mut completion = self
            .inner
            .completion
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if matches!(*completion, NativeJobCompletion::Running) {
            *completion = NativeJobCompletion::Succeeded(Some(value));
            Ok(())
        } else {
            Err(value)
        }
    }

    /// Fail the job once. A late/duplicate error is returned to the callback rather than silently
    /// discarded.
    pub fn fail(&self, error: OperationError) -> Result<(), OperationError> {
        let mut completion = self
            .inner
            .completion
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if matches!(*completion, NativeJobCompletion::Running) {
            *completion = NativeJobCompletion::Failed(Some(error));
            Ok(())
        } else {
            Err(error)
        }
    }
}

/// Standard ownership/state layer for a retained native callback operation.
///
/// `Registration` owns native quiescence. The controller captured by callbacks reports progress,
/// terminal result/error, and observes cooperative cancellation. Stop failure restores the still-
/// live registration so callers can retry safely; dropping the job delegates to the registration's
/// conservative `Drop` policy.
#[cfg(feature = "std")]
#[must_use = "a live native job owns a retained callback registration"]
pub struct NativeJob<Registration, ResultValue, OperationError, ProgressValue>
where
    Registration: RetryableStop,
{
    registration: Option<Registration>,
    controller: NativeJobController<ResultValue, OperationError, ProgressValue>,
}

#[cfg(feature = "std")]
impl<Registration, ResultValue, OperationError, ProgressValue>
    NativeJob<Registration, ResultValue, OperationError, ProgressValue>
where
    Registration: RetryableStop,
{
    /// Build the controller first, let the API-specific closure register its native callback, then
    /// return one object that owns both state and registration.
    pub fn start<StartError>(
        progress: impl Fn(ProgressValue) + Send + Sync + 'static,
        start: impl FnOnce(
            NativeJobController<ResultValue, OperationError, ProgressValue>,
        ) -> Result<Registration, StartError>,
    ) -> Result<Self, StartError> {
        let controller = NativeJobController {
            inner: alloc::sync::Arc::new(NativeJobInner {
                completion: std::sync::Mutex::new(NativeJobCompletion::Running),
                cancellation_requested: std::sync::atomic::AtomicBool::new(false),
                progress: alloc::sync::Arc::new(progress),
            }),
        };
        let registration = start(controller.clone())?;
        Ok(Self {
            registration: Some(registration),
            controller,
        })
    }

    pub fn controller(&self) -> NativeJobController<ResultValue, OperationError, ProgressValue> {
        self.controller.clone()
    }

    pub fn request_cancellation(&self) {
        self.controller.request_cancellation();
    }

    pub fn is_cancellation_requested(&self) -> bool {
        self.controller.is_cancellation_requested()
    }

    pub fn is_registered(&self) -> bool {
        self.registration.is_some()
    }

    pub fn status(&self) -> NativeJobStatus {
        match &*self
            .controller
            .inner
            .completion
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
        {
            NativeJobCompletion::Running => NativeJobStatus::Running,
            NativeJobCompletion::Succeeded(_) => NativeJobStatus::Succeeded,
            NativeJobCompletion::Failed(_) => NativeJobStatus::Failed,
            NativeJobCompletion::Stopped => NativeJobStatus::Stopped,
        }
    }

    pub fn take_result(&self) -> Option<ResultValue> {
        let mut completion = self
            .controller
            .inner
            .completion
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match &mut *completion {
            NativeJobCompletion::Succeeded(value) => value.take(),
            _ => None,
        }
    }

    pub fn take_error(&self) -> Option<OperationError> {
        let mut completion = self
            .controller
            .inner
            .completion
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match &mut *completion {
            NativeJobCompletion::Failed(error) => error.take(),
            _ => None,
        }
    }

    /// Stop and wait for callback quiescence. Failure restores the still-live registration inside
    /// this job before returning the API-specific error, so the same object remains retryable.
    pub fn try_stop(&mut self) -> Result<(), Registration::Error> {
        let Some(registration) = self.registration.take() else {
            return Ok(());
        };
        match registration.try_stop() {
            Ok(()) => {
                let mut completion = self
                    .controller
                    .inner
                    .completion
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if matches!(*completion, NativeJobCompletion::Running) {
                    *completion = NativeJobCompletion::Stopped;
                }
                Ok(())
            }
            Err((registration, error)) => {
                self.registration = Some(registration);
                Err(error)
            }
        }
    }
}

/// Generates a panic-contained C callback trampoline whose first native parameter is the opaque
/// context returned by [`Callback::context`]. The remaining parameters are delivered to the Rust
/// closure as a tuple.
///
/// ```ignore
/// callback_trampoline! {
///     pub unsafe extern "C" fn on_value(value: i32) -> i32;
/// }
/// let mut callback = Callback::<(i32,), i32>::new(|(value,)| value + 1);
/// native_register(Some(on_value), callback.context());
/// ```
#[cfg(feature = "std")]
#[macro_export]
macro_rules! callback_trampoline {
    (
        $visibility:vis unsafe extern "C" fn $name:ident(
            $($argument:ident : $argument_type:ty),* $(,)?
        ) -> $return_type:ty;
    ) => {
        $visibility unsafe extern "C" fn $name(
            context: *mut ::core::ffi::c_void,
            $($argument: $argument_type),*
        ) -> $return_type {
            unsafe {
                $crate::Callback::<($($argument_type,)*), $return_type>::invoke_abort_on_panic(
                    context,
                    ($($argument,)*),
                )
            }
        }
    };
}

/// Generates a panic-contained callback trampoline that returns a caller-selected fallback rather
/// than aborting the process. This is appropriate for C APIs whose callback return value can stop
/// or reject the operation.
#[cfg(feature = "std")]
#[macro_export]
macro_rules! callback_trampoline_return {
    (
        $visibility:vis unsafe extern "C" fn $name:ident(
            $($argument:ident : $argument_type:ty),* $(,)?
        ) -> $return_type:ty;
        on_panic = $fallback:expr;
    ) => {
        $visibility unsafe extern "C" fn $name(
            context: *mut ::core::ffi::c_void,
            $($argument: $argument_type),*
        ) -> $return_type {
            match ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(|| unsafe {
                $crate::Callback::<($($argument_type,)*), $return_type>::invoke(
                    context,
                    ($($argument,)*),
                )
            })) {
                Ok(value) => value,
                Err(_) => $fallback,
            }
        }
    };
}

/// Generates an abort-on-panic trampoline for a callback retained by native code and invoked from
/// arbitrary threads. The callback context must be owned by [`CallbackRegistration`].
#[cfg(feature = "std")]
#[macro_export]
macro_rules! thread_safe_callback_trampoline {
    (
        $visibility:vis unsafe extern "C" fn $name:ident(
            $($argument:ident : $argument_type:ty),* $(,)?
        ) -> $return_type:ty;
    ) => {
        $visibility unsafe extern "C" fn $name(
            context: *mut ::core::ffi::c_void,
            $($argument: $argument_type),*
        ) -> $return_type {
            match ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(|| unsafe {
                $crate::ThreadSafeCallback::<($($argument_type,)*), $return_type>::invoke(
                    context,
                    ($($argument,)*),
                )
            })) {
                Ok(value) => value,
                Err(_) => ::std::process::abort(),
            }
        }
    };
}

/// Generates a thread-safe retained-callback trampoline that maps a panic to a native failure
/// value. This prevents unwinding across the ABI while allowing the native worker to stop cleanly.
#[cfg(feature = "std")]
#[macro_export]
macro_rules! thread_safe_callback_trampoline_return {
    (
        $visibility:vis unsafe extern "C" fn $name:ident(
            $($argument:ident : $argument_type:ty),* $(,)?
        ) -> $return_type:ty;
        on_panic = $fallback:expr;
    ) => {
        $visibility unsafe extern "C" fn $name(
            context: *mut ::core::ffi::c_void,
            $($argument: $argument_type),*
        ) -> $return_type {
            match ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(|| unsafe {
                $crate::ThreadSafeCallback::<($($argument_type,)*), $return_type>::invoke(
                    context,
                    ($($argument,)*),
                )
            })) {
                Ok(value) => value,
                Err(_) => $fallback,
            }
        }
    };
}

#[cfg(feature = "std")]
impl<Args, Return> Callback<Args, Return> {
    /// Invokes a callback and aborts if it panics, preventing unwinding across the native ABI.
    ///
    /// # Safety
    ///
    /// The context requirements are the same as [`Callback::invoke`].
    pub unsafe fn invoke_abort_on_panic(context: *mut core::ffi::c_void, args: Args) -> Return {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            Self::invoke(context, args)
        })) {
            Ok(value) => value,
            Err(_) => std::process::abort(),
        }
    }
}

/// An owned opaque native handle closed by the function supplied at construction.
///
/// `into_raw` transfers ownership back to the caller and suppresses cleanup.
pub struct OwnedHandle<T, F: Fn(*mut T)> {
    ptr: Option<core::ptr::NonNull<T>>,
    close: F,
}
impl<T, F: Fn(*mut T)> OwnedHandle<T, F> {
    /// Takes ownership of a non-null opaque handle.
    ///
    /// Returns `None` for a null pointer, so null-as-failure APIs cannot accidentally create an
    /// apparently valid owned handle.
    ///
    /// # Safety
    ///
    /// `ptr` must be uniquely owned and valid for `close` exactly once. `close` must accept handles
    /// produced by the same native API and must not unwind across the native boundary.
    pub unsafe fn from_raw(ptr: *mut T, close: F) -> Option<Self> {
        core::ptr::NonNull::new(ptr).map(|ptr| Self {
            ptr: Some(ptr),
            close,
        })
    }

    /// Borrows the raw handle without transferring ownership.
    pub fn as_ptr(&self) -> *mut T {
        self.ptr
            .expect("owned handle was already released")
            .as_ptr()
    }

    /// Transfers the raw handle to the caller without invoking `close`.
    #[must_use]
    pub fn into_raw(mut self) -> *mut T {
        self.ptr
            .take()
            .expect("owned handle was already released")
            .as_ptr()
    }
}
impl<T, F: Fn(*mut T)> Drop for OwnedHandle<T, F> {
    fn drop(&mut self) {
        if let Some(ptr) = self.ptr.take() {
            (self.close)(ptr.as_ptr());
        }
    }
}

/// Declares small safe facades over raw native calls from explicit ownership and status policies.
///
/// This macro never inspects a C header and never guesses whether a pointer is borrowed, owned, or
/// initialized. The declaration must name every converted UTF-8/UTF-16 argument, every out value,
/// the status policy, free function for native-owned strings, and the successful result
/// projection. `unsafe_call` is executed only after the declared argument storage exists; out
/// values become visible only after the status policy succeeds.
///
/// ```ignore
/// native_api! {
///     pub handle Database(raw::sqlite3) {
///         close = raw::sqlite3_close;
///     }
///
///     pub fn open(filename: &str) -> Database {
///         utf8 filename => filename_ptr;
///         out database: *mut raw::sqlite3 => database_out;
///         unsafe_call = raw::sqlite3_open(filename_ptr, database_out);
///         status = status_zero;
///         success = handle Database(database);
///     }
/// }
/// ```
///
/// The generated function returns `Result<Success, NativeCallError>`. A custom status policy may
/// be any path or closure returning `Result<_, NativeStatusError>`. Handle projection rejects a
/// null pointer even when the native status reports success.
///
/// Missing policies fail at the declaration rather than producing an incomplete wrapper:
///
/// ```compile_fail
/// use rust_dotnet_pinvoke::native_api;
///
/// native_api! {
///     fn incomplete() -> () {
///         unsafe_call = 0;
///         success = unit;
///     }
/// }
/// ```
#[macro_export]
macro_rules! native_api {
    () => {};

    (
        $(#[$metadata:meta])*
        $visibility:vis handle $name:ident($target:ty) {
            close = $close:path;
        }
        $($rest:tt)*
    ) => {
        $crate::native_handle! {
            $(#[$metadata])*
            $visibility struct $name($target);
            close = $close;
        }
        $crate::native_api! { $($rest)* }
    };

    (
        $(#[$metadata:meta])*
        $visibility:vis scoped_callback $storage:ident as $trampoline:ident(
            $($argument:ident : $argument_type:ty),* $(,)?
        ) -> $return_type:ty {
            on_panic = $fallback:expr;
        }
        $($rest:tt)*
    ) => {
        $(#[$metadata])*
        $visibility type $storage = $crate::Callback<($($argument_type,)*), $return_type>;
        $crate::callback_trampoline_return! {
            $visibility unsafe extern "C" fn $trampoline(
                $($argument: $argument_type),*
            ) -> $return_type;
            on_panic = $fallback;
        }
        $crate::native_api! { $($rest)* }
    };

    (
        $(#[$metadata:meta])*
        $visibility:vis retained_callback $registration:ident, $stop_failure:ident
            as $trampoline:ident(
                $($callback_argument:ident : $callback_argument_type:ty),* $(,)?
            ) -> $return_type:ty
        {
            start($($start_argument:ident : $start_argument_type:ty),* $(,)?);
            token = $token_type:ty;
            register($context:ident, $out_token:ident) = $register:expr;
            unregister($token:ident) = $unregister:expr;
            status = $status:expr;
            on_panic = $fallback:expr;
            quiescence = unregister_waits;
            threading = send;
        }
        $($rest:tt)*
    ) => {
        $crate::thread_safe_callback_trampoline_return! {
            $visibility unsafe extern "C" fn $trampoline(
                $($callback_argument: $callback_argument_type),*
            ) -> $return_type;
            on_panic = $fallback;
        }

        $(#[$metadata])*
        $visibility struct $registration(
            ::core::option::Option<
                $crate::CallbackRegistration<
                    ($($callback_argument_type,)*),
                    $return_type,
                    $token_type,
                    ::std::boxed::Box<
                        dyn ::core::ops::FnMut(
                            &mut $token_type,
                        ) -> ::core::result::Result<(), $crate::NativeStatusError>
                            + ::core::marker::Send,
                    >,
                    $crate::NativeStatusError,
                >,
            >,
        );

        // SAFETY: `threading = send` is a required declaration contract. The native token may be
        // moved to a different managed/native thread for stop, and its unregister closure is Send.
        unsafe impl ::core::marker::Send for $registration {}

        /// A failed native stop that preserves the still-live registration for retry.
        $visibility struct $stop_failure {
            registration: $registration,
            error: $crate::NativeStatusError,
        }

        impl $stop_failure {
            $visibility fn error(&self) -> $crate::NativeStatusError {
                self.error
            }

            $visibility fn into_registration(self) -> $registration {
                self.registration
            }
        }

        impl ::core::fmt::Debug for $stop_failure {
            fn fmt(
                &self,
                formatter: &mut ::core::fmt::Formatter<'_>,
            ) -> ::core::fmt::Result {
                formatter
                    .debug_struct(stringify!($stop_failure))
                    .field("error", &self.error)
                    .finish_non_exhaustive()
            }
        }

        impl ::core::fmt::Display for $stop_failure {
            fn fmt(
                &self,
                formatter: &mut ::core::fmt::Formatter<'_>,
            ) -> ::core::fmt::Result {
                write!(formatter, "native callback remains registered: {}", self.error)
            }
        }

        impl ::std::error::Error for $stop_failure {}

        impl $registration {
            $visibility fn start(
                callback: impl ::core::ops::Fn(
                        $($callback_argument_type),*
                    ) -> $return_type
                    + ::core::marker::Send
                    + ::core::marker::Sync
                    + 'static,
                $($start_argument: $start_argument_type),*
            ) -> ::core::result::Result<Self, $crate::NativeStatusError> {
                let unregister: ::std::boxed::Box<
                    dyn ::core::ops::FnMut(
                        &mut $token_type,
                    ) -> ::core::result::Result<(), $crate::NativeStatusError>
                        + ::core::marker::Send,
                > = ::std::boxed::Box::new(move |$token| {
                    ($status)(unsafe { $unregister })
                        .map(|_| ())
                        .map_err(::core::convert::Into::into)
                });
                let inner = unsafe {
                    $crate::CallbackRegistration::register(
                        move |($($callback_argument,)*)| {
                            callback($($callback_argument),*)
                        },
                        move |$context| {
                            $crate::try_out(|$out_token| {
                                ($status)(unsafe { $register })
                                    .map(|_| ())
                                    .map_err(::core::convert::Into::into)
                            })
                        },
                        unregister,
                    )
                }?;
                ::core::result::Result::Ok(Self(::core::option::Option::Some(inner)))
            }

            $visibility fn is_registered(&self) -> bool {
                self.0.as_ref().is_some_and(|inner| inner.is_registered())
            }

            $visibility fn stop(mut self) -> ::core::result::Result<(), $stop_failure> {
                let inner = self
                    .0
                    .take()
                    .expect("callback registration was already stopped");
                match inner.try_unregister() {
                    ::core::result::Result::Ok(()) => ::core::result::Result::Ok(()),
                    ::core::result::Result::Err(failure) => {
                        let (inner, error) = failure.into_parts();
                        self.0 = ::core::option::Option::Some(inner);
                        ::core::result::Result::Err($stop_failure {
                            registration: self,
                            error,
                        })
                    }
                }
            }
        }

        impl $crate::RetryableStop for $registration {
            type Error = $crate::NativeStatusError;

            fn try_stop(
                self,
            ) -> ::core::result::Result<(), (Self, Self::Error)> {
                self.stop().map_err(|failure| {
                    let error = failure.error();
                    (failure.into_registration(), error)
                })
            }
        }

        $crate::native_api! { $($rest)* }
    };

    (
        $(#[$metadata:meta])*
        $visibility:vis fn $name:ident($($argument:ident : $argument_type:ty),* $(,)?)
            -> $success_type:ty
        {
            utf8 $utf8_argument:ident => $utf8_pointer:ident;
            $(out $out_value:ident : $out_type:ty => $out_pointer:ident;)+
            unsafe_call = $call:expr;
            status = $status:expr;
            success = handle $handle:ident($handle_value:ident);
        }
        $($rest:tt)*
    ) => {
        $(#[$metadata])*
        $visibility fn $name(
            $($argument: $argument_type),*
        ) -> ::core::result::Result<$success_type, $crate::NativeCallError> {
            $crate::with_utf8_cstr($utf8_argument, |$utf8_pointer| {
                $(
                    let mut $out_value = $crate::Out::<$out_type>::new();
                    let $out_pointer = $out_value.as_mut_ptr();
                )+
                let native_status = unsafe { $call };
                let _ = ($status)(native_status).map_err($crate::NativeCallError::from)?;
                $(let $out_value = unsafe { $out_value.assume_init() };)+
                unsafe { $handle::from_raw($handle_value) }
                    .ok_or($crate::NativeCallError::NullHandle)
            })
            .map_err($crate::NativeCallError::from)?
        }
        $crate::native_api! { $($rest)* }
    };

    (
        $(#[$metadata:meta])*
        $visibility:vis fn $name:ident($($argument:ident : $argument_type:ty),* $(,)?)
            -> $success_type:ty
        {
            utf8 $utf8_argument:ident => $utf8_pointer:ident;
            out $owned_value:ident : *mut core::ffi::c_char => $owned_pointer:ident;
            unsafe_call = $call:expr;
            status = $status:expr;
            success = owned_utf8($result_value:ident, free = $free:expr, null = error);
        }
        $($rest:tt)*
    ) => {
        $(#[$metadata])*
        $visibility fn $name(
            $($argument: $argument_type),*
        ) -> ::core::result::Result<$success_type, $crate::NativeCallError> {
            $crate::with_utf8_cstr($utf8_argument, |$utf8_pointer| {
                let mut $owned_value = $crate::Out::<*mut core::ffi::c_char>::new();
                let $owned_pointer = $owned_value.as_mut_ptr();
                let native_status = unsafe { $call };
                let _ = ($status)(native_status).map_err($crate::NativeCallError::from)?;
                let $result_value = unsafe { $owned_value.assume_init() };
                unsafe {
                    $crate::take_utf8_string($result_value, |pointer| {
                        let _ = ($free)(pointer);
                    })
                }
                .map_err($crate::NativeCallError::from)?
                .ok_or($crate::NativeCallError::NullString)
            })
            .map_err($crate::NativeCallError::from)?
        }
        $crate::native_api! { $($rest)* }
    };

    (
        $(#[$metadata:meta])*
        $visibility:vis fn $name:ident($($argument:ident : $argument_type:ty),* $(,)?)
            -> $success_type:ty
        {
            utf16 $utf16_argument:ident => $utf16_pointer:ident;
            out $owned_value:ident : *mut u16 => $owned_pointer:ident;
            unsafe_call = $call:expr;
            status = $status:expr;
            success = owned_utf16($result_value:ident, free = $free:expr, null = error);
        }
        $($rest:tt)*
    ) => {
        $(#[$metadata])*
        $visibility fn $name(
            $($argument: $argument_type),*
        ) -> ::core::result::Result<$success_type, $crate::NativeCallError> {
            $crate::with_utf16_cstr($utf16_argument, |$utf16_pointer| {
                let mut $owned_value = $crate::Out::<*mut u16>::new();
                let $owned_pointer = $owned_value.as_mut_ptr();
                let native_status = unsafe { $call };
                let _ = ($status)(native_status).map_err($crate::NativeCallError::from)?;
                let $result_value = unsafe { $owned_value.assume_init() };
                unsafe {
                    $crate::take_utf16_string($result_value, |pointer| {
                        let _ = ($free)(pointer);
                    })
                }
                .map_err($crate::NativeCallError::from)?
                .ok_or($crate::NativeCallError::NullString)
            })
            .map_err($crate::NativeCallError::from)?
        }
        $crate::native_api! { $($rest)* }
    };

    (
        $(#[$metadata:meta])*
        $visibility:vis fn $name:ident($($argument:ident : $argument_type:ty),* $(,)?)
            -> $success_type:ty
        {
            utf16 $utf16_argument:ident => $utf16_pointer:ident;
            $(out $out_value:ident : $out_type:ty => $out_pointer:ident;)+
            unsafe_call = $call:expr;
            status = $status:expr;
            success = handle $handle:ident($handle_value:ident);
        }
        $($rest:tt)*
    ) => {
        $(#[$metadata])*
        $visibility fn $name(
            $($argument: $argument_type),*
        ) -> ::core::result::Result<$success_type, $crate::NativeCallError> {
            $crate::with_utf16_cstr($utf16_argument, |$utf16_pointer| {
                $(
                    let mut $out_value = $crate::Out::<$out_type>::new();
                    let $out_pointer = $out_value.as_mut_ptr();
                )+
                let native_status = unsafe { $call };
                let _ = ($status)(native_status).map_err($crate::NativeCallError::from)?;
                $(let $out_value = unsafe { $out_value.assume_init() };)+
                unsafe { $handle::from_raw($handle_value) }
                    .ok_or($crate::NativeCallError::NullHandle)
            })
            .map_err($crate::NativeCallError::from)?
        }
        $crate::native_api! { $($rest)* }
    };

    (
        $(#[$metadata:meta])*
        $visibility:vis fn $name:ident($($argument:ident : $argument_type:ty),* $(,)?) -> ()
        {
            utf8 $utf8_argument:ident => $utf8_pointer:ident;
            error_out $error_value:ident : *mut core::ffi::c_char => $error_pointer:ident;
            unsafe_call = $call:expr;
            status = $status:expr;
            error = owned_utf8(free = $free:expr);
            success = unit;
        }
        $($rest:tt)*
    ) => {
        $(#[$metadata])*
        $visibility fn $name(
            $($argument: $argument_type),*
        ) -> ::core::result::Result<(), $crate::NativeCallError> {
            $crate::with_utf8_cstr($utf8_argument, |$utf8_pointer| {
                let mut $error_value = $crate::Out::<*mut core::ffi::c_char>::new();
                let $error_pointer = $error_value.as_mut_ptr();
                let native_status = unsafe { $call };
                let status_result = ($status)(native_status);
                let $error_value = unsafe { $error_value.assume_init() };
                let message = unsafe {
                    $crate::take_utf8_string($error_value, |pointer| {
                        let _ = ($free)(pointer);
                    })
                }
                .map_err($crate::NativeCallError::from)?;
                match (status_result, message) {
                    (::core::result::Result::Ok(_), ::core::option::Option::None) => {
                        ::core::result::Result::Ok(())
                    }
                    (::core::result::Result::Err(status), ::core::option::Option::Some(message)) => {
                        ::core::result::Result::Err($crate::NativeCallError::StatusMessage {
                            status,
                            message,
                        })
                    }
                    (::core::result::Result::Err(status), ::core::option::Option::None) => {
                        ::core::result::Result::Err(status.into())
                    }
                    (::core::result::Result::Ok(_), ::core::option::Option::Some(message)) => {
                        ::core::result::Result::Err(
                            $crate::NativeCallError::UnexpectedMessage(message),
                        )
                    }
                }
            })
            .map_err($crate::NativeCallError::from)?
        }
        $crate::native_api! { $($rest)* }
    };

    (
        $(#[$metadata:meta])*
        $visibility:vis fn $name:ident($($argument:ident : $argument_type:ty),* $(,)?) -> ()
        {
            utf16 $utf16_argument:ident => $utf16_pointer:ident;
            error_out $error_value:ident : *mut u16 => $error_pointer:ident;
            unsafe_call = $call:expr;
            status = $status:expr;
            error = owned_utf16(free = $free:expr);
            success = unit;
        }
        $($rest:tt)*
    ) => {
        $(#[$metadata])*
        $visibility fn $name(
            $($argument: $argument_type),*
        ) -> ::core::result::Result<(), $crate::NativeCallError> {
            $crate::with_utf16_cstr($utf16_argument, |$utf16_pointer| {
                let mut $error_value = $crate::Out::<*mut u16>::new();
                let $error_pointer = $error_value.as_mut_ptr();
                let native_status = unsafe { $call };
                let status_result = ($status)(native_status);
                let $error_value = unsafe { $error_value.assume_init() };
                let message = unsafe {
                    $crate::take_utf16_string($error_value, |pointer| {
                        let _ = ($free)(pointer);
                    })
                }
                .map_err($crate::NativeCallError::from)?;
                match (status_result, message) {
                    (::core::result::Result::Ok(_), ::core::option::Option::None) => {
                        ::core::result::Result::Ok(())
                    }
                    (::core::result::Result::Err(status), ::core::option::Option::Some(message)) => {
                        ::core::result::Result::Err($crate::NativeCallError::StatusMessage {
                            status,
                            message,
                        })
                    }
                    (::core::result::Result::Err(status), ::core::option::Option::None) => {
                        ::core::result::Result::Err(status.into())
                    }
                    (::core::result::Result::Ok(_), ::core::option::Option::Some(message)) => {
                        ::core::result::Result::Err(
                            $crate::NativeCallError::UnexpectedMessage(message),
                        )
                    }
                }
            })
            .map_err($crate::NativeCallError::from)?
        }
        $crate::native_api! { $($rest)* }
    };

    (
        $(#[$metadata:meta])*
        $visibility:vis fn $name:ident($($argument:ident : $argument_type:ty),* $(,)?)
            -> $success_type:ty
        {
            $(out $out_value:ident : $out_type:ty => $out_pointer:ident;)+
            unsafe_call = $call:expr;
            status = $status:expr;
            success = value $value:ident;
        }
        $($rest:tt)*
    ) => {
        $(#[$metadata])*
        $visibility fn $name(
            $($argument: $argument_type),*
        ) -> ::core::result::Result<$success_type, $crate::NativeCallError> {
            $(
                let mut $out_value = $crate::Out::<$out_type>::new();
                let $out_pointer = $out_value.as_mut_ptr();
            )+
            let native_status = unsafe { $call };
            let _ = ($status)(native_status).map_err($crate::NativeCallError::from)?;
            $(let $out_value = unsafe { $out_value.assume_init() };)+
            ::core::result::Result::Ok($value)
        }
        $crate::native_api! { $($rest)* }
    };

    (
        $(#[$metadata:meta])*
        $visibility:vis fn $name:ident($($argument:ident : $argument_type:ty),* $(,)?)
            -> $success_type:ty
        {
            $(out $out_value:ident : $out_type:ty => $out_pointer:ident;)+
            unsafe_call = $call:expr;
            status = $status:expr;
            success = tuple($($value:ident),+ $(,)?);
        }
        $($rest:tt)*
    ) => {
        $(#[$metadata])*
        $visibility fn $name(
            $($argument: $argument_type),*
        ) -> ::core::result::Result<$success_type, $crate::NativeCallError> {
            $(
                let mut $out_value = $crate::Out::<$out_type>::new();
                let $out_pointer = $out_value.as_mut_ptr();
            )+
            let native_status = unsafe { $call };
            let _ = ($status)(native_status).map_err($crate::NativeCallError::from)?;
            $(let $out_value = unsafe { $out_value.assume_init() };)+
            ::core::result::Result::Ok(($($value),+))
        }
        $crate::native_api! { $($rest)* }
    };

    (
        $(#[$metadata:meta])*
        $visibility:vis handle $name:ident($target:ty) { $($policies:tt)* }
        $($rest:tt)*
    ) => {
        ::core::compile_error!(::core::concat!(
            "native_api!: handle `", ::core::stringify!($name),
            "` has an incomplete or contradictory ownership policy. A safe handle requires exactly `close = path;`; raw native handles may remain in ordinary bindgen declarations. Observed policy: `",
            ::core::stringify!($($policies)*), "`"
        ));
    };

    (
        $(#[$metadata:meta])*
        $visibility:vis scoped_callback $storage:ident as $trampoline:ident(
            $($arguments:tt)*
        ) -> $return_type:ty { $($policies:tt)* }
        $($rest:tt)*
    ) => {
        ::core::compile_error!(::core::concat!(
            "native_api!: scoped callback `", ::core::stringify!($storage),
            "` has an incomplete or contradictory callback policy. A safe scoped callback requires `on_panic = fallback;` and a fixed `extern \"C\"` trampoline; raw callback declarations remain a supported escape hatch. Observed policy: `",
            ::core::stringify!($($policies)*), "`"
        ));
    };

    (
        $(#[$metadata:meta])*
        $visibility:vis retained_callback $registration:ident, $stop_failure:ident
            as $trampoline:ident($($arguments:tt)*) -> $return_type:ty
        { $($policies:tt)* }
        $($rest:tt)*
    ) => {
        ::core::compile_error!(::core::concat!(
            "native_api!: retained callback `", ::core::stringify!($registration),
            "` has an incomplete or contradictory lifetime policy. Retained callbacks must explicitly declare start arguments, token type, register and unregister calls, status mapping, panic fallback, `quiescence = unregister_waits;`, and `threading = send;`; use a raw declaration when the native API cannot make those guarantees. Observed policy: `",
            ::core::stringify!($($policies)*), "`"
        ));
    };

    (
        $(#[$metadata:meta])*
        $visibility:vis fn $name:ident($($argument:ident : $argument_type:ty),* $(,)?)
            -> $success_type:ty
        { $($policies:tt)* }
        $($rest:tt)*
    ) => {
        ::core::compile_error!(::core::concat!(
            "native_api!: function `", ::core::stringify!($name),
            "` has an incomplete or contradictory safe-facade policy. Every facade requires one `unsafe_call`, one `status` mapper, and one matching `success` projection; converted strings need utf8/utf16 storage, out values must match the projection, owned strings require an explicit free function, and error strings require `error_out` plus `error`. Raw `extern`/bindgen declarations remain valid and callable without a facade. Observed policies: `",
            ::core::stringify!($($policies)*), "`"
        ));
    };

    ($($invalid:tt)+) => {
        ::core::compile_error!(
            "native_api!: unrecognized declaration. Expected `handle`, `scoped_callback`, \
             `retained_callback`, or `fn` with explicit unsafe_call/status/success policies. \
             Ordinary raw extern/bindgen declarations do not need native_api! and remain the \
             supported escape hatch for APIs without a complete safe policy"
        );
    };
}

/// Declares a typed, non-null native handle with deterministic cleanup.
///
/// The generated type exposes safe borrowing and explicit `close`; construction from a raw pointer
/// remains unsafe because ownership cannot be inferred from an ABI declaration.
#[macro_export]
macro_rules! native_handle {
    (
        $(#[$metadata:meta])*
        $visibility:vis struct $name:ident($target:ty);
        close = $close:path;
    ) => {
        $(#[$metadata])*
        $visibility struct $name(::core::option::Option<::core::ptr::NonNull<$target>>);

        impl $name {
            /// Takes unique ownership of a native handle.
            ///
            /// # Safety
            ///
            /// `pointer` must be owned, valid for `$close`, and not closed elsewhere.
            pub unsafe fn from_raw(pointer: *mut $target) -> ::core::option::Option<Self> {
                ::core::ptr::NonNull::new(pointer).map(|pointer| Self(Some(pointer)))
            }

            pub fn as_ptr(&self) -> *mut $target {
                self.0.expect("native handle was already closed").as_ptr()
            }

            #[must_use]
            pub fn into_raw(mut self) -> *mut $target {
                self.0.take().expect("native handle was already closed").as_ptr()
            }

            /// Closes the handle now. Use a dedicated API wrapper when close failures are
            /// recoverable; destructors cannot report native status reliably.
            pub fn close(mut self) {
                let pointer = self.0.take().expect("native handle was already closed");
                let _ = unsafe { $close(pointer.as_ptr()) };
            }
        }

        impl ::core::fmt::Debug for $name {
            fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                formatter.debug_tuple(stringify!($name)).field(&self.0).finish()
            }
        }

        impl ::core::ops::Drop for $name {
            fn drop(&mut self) {
                if let Some(pointer) = self.0.take() {
                    let _ = unsafe { $close(pointer.as_ptr()) };
                }
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;

    static TYPED_CLOSES: AtomicUsize = AtomicUsize::new(0);
    static STRING_FREES: AtomicUsize = AtomicUsize::new(0);

    unsafe extern "C" fn close_typed_handle(_: *mut u8) -> i32 {
        TYPED_CLOSES.fetch_add(1, Ordering::Relaxed);
        0
    }

    unsafe extern "C" fn open_facade_handle(
        name: *const core::ffi::c_char,
        output: *mut *mut u8,
    ) -> i32 {
        let name = unsafe { std::ffi::CStr::from_ptr(name) };
        if name.to_bytes() != b"facade" {
            return 7;
        }
        unsafe { output.write(core::ptr::dangling_mut()) };
        0
    }

    unsafe extern "C" fn fail_facade_handle(
        _name: *const core::ffi::c_char,
        _output: *mut *mut u8,
    ) -> i32 {
        23
    }

    unsafe extern "C" fn open_facade_handle_wide(name: *const u16, output: *mut *mut u8) -> i32 {
        if name.is_null() || unsafe { name.read() } != 'λ' as u16 {
            return 8;
        }
        unsafe { output.write(core::ptr::dangling_mut()) };
        0
    }

    unsafe extern "C" fn facade_status_message(
        name: *const core::ffi::c_char,
        message: *mut *mut core::ffi::c_char,
    ) -> i32 {
        let name = unsafe { std::ffi::CStr::from_ptr(name) }.to_bytes();
        match name {
            b"ok" => unsafe { message.write(core::ptr::null_mut()) },
            b"unexpected" => unsafe {
                message.write(
                    std::ffi::CString::new("unexpected detail")
                        .unwrap()
                        .into_raw(),
                )
            },
            _ => unsafe {
                message.write(std::ffi::CString::new("native detail").unwrap().into_raw())
            },
        }
        if name == b"fail" { 9 } else { 0 }
    }

    unsafe extern "C" fn free_facade_utf8(pointer: *mut core::ffi::c_void) {
        STRING_FREES.fetch_add(1, Ordering::Relaxed);
        unsafe { drop(std::ffi::CString::from_raw(pointer.cast())) };
    }

    unsafe extern "C" fn facade_wide_status_message(
        name: *const u16,
        message: *mut *mut u16,
    ) -> i32 {
        assert!(!name.is_null());
        let value: Box<[u16]> = "wide detail"
            .encode_utf16()
            .chain(core::iter::once(0))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        unsafe { message.write(Box::into_raw(value).cast::<u16>()) };
        5
    }

    unsafe extern "C" fn free_facade_utf16(pointer: *mut core::ffi::c_void) {
        STRING_FREES.fetch_add(1, Ordering::Relaxed);
        let pointer = pointer.cast::<u16>();
        let mut len = 0usize;
        while unsafe { pointer.add(len).read() } != 0 {
            len += 1;
        }
        let slice = core::ptr::slice_from_raw_parts_mut(pointer, len + 1);
        unsafe { drop(Box::from_raw(slice)) };
    }

    unsafe extern "C" fn split_facade_value(input: i32, first: *mut i32, second: *mut i64) -> i32 {
        unsafe {
            first.write(input + 1);
            second.write(i64::from(input) * 2);
        }
        17
    }

    const fn status_seventeen(code: i32) -> Result<(), NativeStatusError> {
        if code == 17 {
            Ok(())
        } else {
            Err(NativeStatusError(code))
        }
    }

    native_handle! {
        struct TypedHandle(u8);
        close = close_typed_handle;
    }

    native_api! {
        /// Handle and open wrapper generated from explicit facade policies.
        handle FacadeHandle(u8) {
            close = close_typed_handle;
        }

        fn facade_open(name: &str) -> FacadeHandle {
            utf8 name => name_pointer;
            out handle: *mut u8 => handle_pointer;
            unsafe_call = open_facade_handle(name_pointer, handle_pointer);
            status = status_zero;
            success = handle FacadeHandle(handle);
        }

        fn facade_open_failure(name: &str) -> FacadeHandle {
            utf8 name => name_pointer;
            out handle: *mut u8 => handle_pointer;
            unsafe_call = fail_facade_handle(name_pointer, handle_pointer);
            status = status_zero;
            success = handle FacadeHandle(handle);
        }

        fn facade_open_wide(name: &str) -> FacadeHandle {
            utf16 name => name_pointer;
            out handle: *mut u8 => handle_pointer;
            unsafe_call = open_facade_handle_wide(name_pointer, handle_pointer);
            status = status_zero;
            success = handle FacadeHandle(handle);
        }

        fn facade_message(name: &str) -> () {
            utf8 name => name_pointer;
            error_out message: *mut core::ffi::c_char => message_pointer;
            unsafe_call = facade_status_message(name_pointer, message_pointer);
            status = status_zero;
            error = owned_utf8(free = free_facade_utf8);
            success = unit;
        }

        fn facade_wide_message(name: &str) -> () {
            utf16 name => name_pointer;
            error_out message: *mut u16 => message_pointer;
            unsafe_call = facade_wide_status_message(name_pointer, message_pointer);
            status = status_zero;
            error = owned_utf16(free = free_facade_utf16);
            success = unit;
        }

        fn facade_split(input: i32) -> (i32, i64) {
            out first: i32 => first_pointer;
            out second: i64 => second_pointer;
            unsafe_call = split_facade_value(input, first_pointer, second_pointer);
            status = status_seventeen;
            success = tuple(first, second);
        }
    }

    callback_trampoline! {
        unsafe extern "C" fn add_one_callback(value: i32) -> i32;
    }
    callback_trampoline_return! {
        unsafe extern "C" fn recover_callback(value: i32) -> i32;
        on_panic = -1;
    }
    #[test]
    fn strings_are_borrowed_and_checked() {
        assert_eq!(cstr_utf8(b"ok\0tail"), Ok("ok"));
        assert_eq!(cstr_utf8(b"\xff\0"), Err(StringError::InvalidUtf8));
        assert_eq!(utf16_nul(&[65, 0, 66]), Ok(&[65][..]));
        assert_eq!(utf16_nul(&[65]), Err(StringError::UnterminatedUtf16));
        let utf8 = Utf8CString::new("hello").unwrap();
        assert_eq!(utf8.as_bytes_with_nul(), b"hello\0");
        assert_eq!(Utf8CString::new("a\0b"), Err(StringError::InteriorNul));
        let utf16 = Utf16CString::new("A😀").unwrap();
        assert_eq!(utf16.as_units_with_nul().last(), Some(&0));
        assert_eq!(with_utf8_cstr("hello", |ptr| ptr.is_null()), Ok(false));
    }
    #[test]
    fn handle_closes_or_releases() {
        use core::cell::Cell;
        let calls = Cell::new(0);
        let ptr = 7usize as *mut u8;
        let h = unsafe { OwnedHandle::from_raw(ptr, |_| calls.set(calls.get() + 1)) }.unwrap();
        drop(h);
        assert_eq!(calls.get(), 1);
        let h = unsafe { OwnedHandle::from_raw(ptr, |_| calls.set(calls.get() + 1)) }.unwrap();
        assert_eq!(h.into_raw(), ptr);
        assert_eq!(calls.get(), 1);
        assert!(
            unsafe { OwnedHandle::from_raw(core::ptr::null_mut::<u8>(), |_: *mut u8| {}) }
                .is_none()
        );
    }

    #[test]
    fn status_out_and_callback_helpers_preserve_contracts() {
        assert_eq!(status_zero(0), Ok(()));
        assert_eq!(status_zero(7), Err(NativeStatusError(7)));
        let mut out = Out::<u32>::new();
        unsafe { out.as_mut_ptr().write(42) };
        assert_eq!(unsafe { out.assume_init() }, 42);

        let mut callback = Callback::<(i32,), i32>::new(|(value,)| value + 1);
        let context = callback.context();
        assert_eq!(
            unsafe { Callback::<(i32,), i32>::invoke_abort_on_panic(context, (4,)) },
            5
        );
        assert_eq!(unsafe { add_one_callback(context, 9) }, 10);

        let out = unsafe {
            try_out(|pointer: *mut u32| {
                pointer.write(17_u32);
                status_zero(0)
            })
        };
        assert_eq!(out, Ok(17));
        let failed: Result<u32, _> = unsafe { try_out(|_| status_zero(9)) };
        assert_eq!(failed, Err(NativeStatusError(9)));

        let mut panicking = Callback::<(i32,), i32>::new(|_| panic!("callback failure"));
        assert_eq!(unsafe { recover_callback(panicking.context(), 1) }, -1);
    }

    #[test]
    fn typed_handle_macro_closes_exactly_once() {
        TYPED_CLOSES.store(0, Ordering::Relaxed);
        let pointer = core::ptr::dangling_mut::<u8>();
        let handle = unsafe { TypedHandle::from_raw(pointer) }.unwrap();
        assert_eq!(handle.as_ptr(), pointer);
        drop(handle);
        assert_eq!(TYPED_CLOSES.load(Ordering::Relaxed), 1);

        let handle = unsafe { TypedHandle::from_raw(pointer) }.unwrap();
        handle.close();
        assert_eq!(TYPED_CLOSES.load(Ordering::Relaxed), 2);

        let handle = unsafe { TypedHandle::from_raw(pointer) }.unwrap();
        assert_eq!(handle.into_raw(), pointer);
        assert_eq!(TYPED_CLOSES.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn declarative_native_api_projects_utf8_handles_outs_and_custom_status() {
        TYPED_CLOSES.store(0, Ordering::Relaxed);
        STRING_FREES.store(0, Ordering::Relaxed);

        let handle = facade_open("facade").unwrap();
        assert_eq!(handle.as_ptr(), core::ptr::dangling_mut());
        drop(handle);
        assert_eq!(TYPED_CLOSES.load(Ordering::Relaxed), 1);

        let wide_handle = facade_open_wide("λ").unwrap();
        drop(wide_handle);
        assert_eq!(TYPED_CLOSES.load(Ordering::Relaxed), 2);

        facade_open("facade").unwrap().close();
        assert_eq!(TYPED_CLOSES.load(Ordering::Relaxed), 3);
        let raw = facade_open("facade").unwrap().into_raw();
        assert_eq!(TYPED_CLOSES.load(Ordering::Relaxed), 3);
        unsafe { FacadeHandle::from_raw(raw) }.unwrap().close();
        assert_eq!(TYPED_CLOSES.load(Ordering::Relaxed), 4);

        assert!(matches!(
            facade_open_failure("facade"),
            Err(NativeCallError::Status(NativeStatusError(23)))
        ));
        assert_eq!(facade_split(20), Ok((21, 40)));

        assert_eq!(facade_message("ok"), Ok(()));
        assert!(matches!(
            facade_message("fail"),
            Err(NativeCallError::StatusMessage {
                status: NativeStatusError(9),
                ref message,
            }) if message == "native detail"
        ));
        assert!(matches!(
            facade_message("unexpected"),
            Err(NativeCallError::UnexpectedMessage(ref message))
                if message == "unexpected detail"
        ));
        assert!(matches!(
            facade_wide_message("λ"),
            Err(NativeCallError::StatusMessage {
                status: NativeStatusError(5),
                ref message,
            }) if message == "wide detail"
        ));
        assert_eq!(STRING_FREES.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn native_owned_string_is_freed_on_success_and_decode_failure() {
        use core::cell::Cell;
        use std::ffi::CString;

        let frees = Cell::new(0);
        let string = CString::new("native message").unwrap().into_raw();
        let value = unsafe {
            take_utf8_string(string, |pointer| {
                frees.set(frees.get() + 1);
                drop(CString::from_raw(pointer.cast()));
            })
        };
        assert_eq!(value, Ok(Some("native message".to_owned())));

        let invalid = unsafe { CString::from_vec_with_nul_unchecked(vec![0xff, 0]) }.into_raw();
        let value = unsafe {
            take_utf8_string(invalid, |pointer| {
                frees.set(frees.get() + 1);
                drop(CString::from_raw(pointer.cast()));
            })
        };
        assert_eq!(value, Err(StringError::InvalidUtf8));

        let wide = "native wide"
            .encode_utf16()
            .chain(core::iter::once(0))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let wide = Box::into_raw(wide).cast::<u16>();
        let value = unsafe {
            take_utf16_string(wide, |pointer| {
                frees.set(frees.get() + 1);
                let pointer = pointer.cast::<u16>();
                let len = "native wide".encode_utf16().count() + 1;
                drop(Box::from_raw(core::ptr::slice_from_raw_parts_mut(
                    pointer, len,
                )));
            })
        };
        assert_eq!(value, Ok(Some("native wide".to_owned())));

        let invalid = Box::into_raw(Box::<[u16]>::from([0xd800, 0])).cast::<u16>();
        let value = unsafe {
            take_utf16_string(invalid, |pointer| {
                frees.set(frees.get() + 1);
                drop(Box::from_raw(core::ptr::slice_from_raw_parts_mut(
                    pointer.cast::<u16>(),
                    2,
                )));
            })
        };
        assert_eq!(value, Err(StringError::InvalidUtf16));
        assert_eq!(frees.get(), 4);
    }

    struct DropFlag(Arc<AtomicBool>);

    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }

    #[test]
    fn retained_callback_registration_failure_drops_callback() {
        let dropped = Arc::new(AtomicBool::new(false));
        let drop_flag = DropFlag(Arc::clone(&dropped));
        let registration: Result<CallbackRegistration<(i32,), i32, (), _, ()>, NativeStatusError> = unsafe {
            CallbackRegistration::register(
                move |(value,)| {
                    let _keep_alive = &drop_flag;
                    value
                },
                |_| Err(NativeStatusError(7)),
                |_| Ok(()),
            )
        };
        assert!(matches!(registration, Err(NativeStatusError(7))));
        assert!(dropped.load(Ordering::Acquire));
    }

    struct WorkerToken {
        stop: Arc<AtomicBool>,
        worker: Option<std::thread::JoinHandle<()>>,
        unregister_attempts: usize,
    }

    struct FakeRegistration {
        stop_attempts: Arc<AtomicUsize>,
        drops: Arc<AtomicUsize>,
        fail_first_stop: bool,
    }

    impl RetryableStop for FakeRegistration {
        type Error = NativeStatusError;

        fn try_stop(mut self) -> Result<(), (Self, Self::Error)> {
            self.stop_attempts.fetch_add(1, Ordering::Relaxed);
            if self.fail_first_stop {
                self.fail_first_stop = false;
                return Err((self, NativeStatusError(9)));
            }
            Ok(())
        }
    }

    impl Drop for FakeRegistration {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn native_job_forwards_progress_cancellation_and_one_terminal_result() {
        let progress = Arc::new(std::sync::Mutex::new(Vec::new()));
        let observed_progress = Arc::clone(&progress);
        let stop_attempts = Arc::new(AtomicUsize::new(0));
        let drops = Arc::new(AtomicUsize::new(0));
        let mut job = NativeJob::<FakeRegistration, i32, &'static str, i32>::start(
            move |value| {
                observed_progress.lock().unwrap().push(value);
            },
            |_| {
                Ok::<_, ()>(FakeRegistration {
                    stop_attempts: Arc::clone(&stop_attempts),
                    drops: Arc::clone(&drops),
                    fail_first_stop: false,
                })
            },
        )
        .unwrap();
        let controller = job.controller();

        assert_eq!(job.status(), NativeJobStatus::Running);
        assert_eq!(controller.ensure_not_canceled(), Ok(()));
        controller.report_progress(10);
        controller.report_progress(20);
        assert_eq!(*progress.lock().unwrap(), vec![10, 20]);

        job.request_cancellation();
        assert!(job.is_cancellation_requested());
        assert!(controller.is_cancellation_requested());
        assert_eq!(
            controller.ensure_not_canceled(),
            Err(NativeCancellationRequested)
        );

        assert_eq!(controller.complete(42), Ok(()));
        assert_eq!(controller.complete(99), Err(99));
        assert_eq!(controller.fail("late failure"), Err("late failure"));
        assert_eq!(job.status(), NativeJobStatus::Succeeded);
        assert_eq!(job.take_result(), Some(42));
        assert_eq!(job.take_result(), None);
        assert_eq!(job.status(), NativeJobStatus::Succeeded);

        job.try_stop().unwrap();
        assert!(!job.is_registered());
        assert_eq!(job.status(), NativeJobStatus::Succeeded);
        assert_eq!(stop_attempts.load(Ordering::Relaxed), 1);
        assert_eq!(drops.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn native_job_preserves_one_terminal_error() {
        let mut job = NativeJob::<FakeRegistration, (), String, ()>::start(
            |_| {},
            |_| {
                Ok::<_, ()>(FakeRegistration {
                    stop_attempts: Arc::new(AtomicUsize::new(0)),
                    drops: Arc::new(AtomicUsize::new(0)),
                    fail_first_stop: false,
                })
            },
        )
        .unwrap();
        let controller = job.controller();

        assert_eq!(controller.fail("native failure".to_owned()), Ok(()));
        assert_eq!(controller.complete(()), Err(()));
        assert_eq!(job.status(), NativeJobStatus::Failed);
        assert_eq!(job.take_error().as_deref(), Some("native failure"));
        assert_eq!(job.take_error(), None);
        job.try_stop().unwrap();
        assert_eq!(job.status(), NativeJobStatus::Failed);
    }

    #[test]
    fn native_job_restores_registration_after_failed_stop_and_delegates_drop() {
        let stop_attempts = Arc::new(AtomicUsize::new(0));
        let drops = Arc::new(AtomicUsize::new(0));
        let mut job = NativeJob::<FakeRegistration, (), (), ()>::start(
            |_| {},
            |_| {
                Ok::<_, ()>(FakeRegistration {
                    stop_attempts: Arc::clone(&stop_attempts),
                    drops: Arc::clone(&drops),
                    fail_first_stop: true,
                })
            },
        )
        .unwrap();

        assert_eq!(job.try_stop(), Err(NativeStatusError(9)));
        assert!(job.is_registered());
        assert_eq!(job.status(), NativeJobStatus::Running);
        assert_eq!(drops.load(Ordering::Relaxed), 0);

        job.try_stop().unwrap();
        assert!(!job.is_registered());
        assert_eq!(job.status(), NativeJobStatus::Stopped);
        assert_eq!(stop_attempts.load(Ordering::Relaxed), 2);
        assert_eq!(drops.load(Ordering::Relaxed), 1);

        let drop_only = NativeJob::<FakeRegistration, (), (), ()>::start(
            |_| {},
            |_| {
                Ok::<_, ()>(FakeRegistration {
                    stop_attempts: Arc::clone(&stop_attempts),
                    drops: Arc::clone(&drops),
                    fail_first_stop: false,
                })
            },
        )
        .unwrap();
        drop(drop_only);
        assert_eq!(drops.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn native_job_stop_is_quiescent_after_retryable_callback_unregister() {
        let progress = Arc::new(AtomicUsize::new(0));
        let observed_progress = Arc::clone(&progress);
        let mut job = NativeJob::<_, (), (), usize>::start(
            move |value| {
                observed_progress.fetch_add(value, Ordering::Relaxed);
            },
            |controller| unsafe {
                CallbackRegistration::register(
                    move |(value,): (i32,)| {
                        controller.report_progress(value as usize);
                        0
                    },
                    |context| {
                        let stop = Arc::new(AtomicBool::new(false));
                        let worker_stop = Arc::clone(&stop);
                        let context = context as usize;
                        let worker = std::thread::spawn(move || {
                            while !worker_stop.load(Ordering::Acquire) {
                                ThreadSafeCallback::<(i32,), i32>::invoke(
                                    context as *mut core::ffi::c_void,
                                    (1,),
                                );
                                std::thread::yield_now();
                            }
                        });
                        Ok::<_, NativeStatusError>(WorkerToken {
                            stop,
                            worker: Some(worker),
                            unregister_attempts: 0,
                        })
                    },
                    |token: &mut WorkerToken| {
                        token.unregister_attempts += 1;
                        if token.unregister_attempts == 1 {
                            return Err(NativeStatusError(1));
                        }
                        token.stop.store(true, Ordering::Release);
                        token.worker.take().unwrap().join().unwrap();
                        Ok(())
                    },
                )
            },
        )
        .unwrap();

        while progress.load(Ordering::Relaxed) == 0 {
            std::thread::yield_now();
        }
        assert_eq!(job.try_stop(), Err(NativeStatusError(1)));
        assert!(job.is_registered());
        job.try_stop().unwrap();
        assert!(!job.is_registered());
        assert_eq!(job.status(), NativeJobStatus::Stopped);

        let stopped_at = progress.load(Ordering::Relaxed);
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert_eq!(progress.load(Ordering::Relaxed), stopped_at);
    }

    #[test]
    fn retained_callback_unregister_failure_is_retryable_and_quiescent() {
        let calls = Arc::new(AtomicUsize::new(0));
        let callback_calls = Arc::clone(&calls);
        let registration = unsafe {
            CallbackRegistration::register(
                move |(value,): (i32,)| {
                    callback_calls.fetch_add(value as usize, Ordering::Relaxed);
                    0
                },
                |context| {
                    let stop = Arc::new(AtomicBool::new(false));
                    let worker_stop = Arc::clone(&stop);
                    let context = context as usize;
                    let worker = std::thread::spawn(move || {
                        while !worker_stop.load(Ordering::Acquire) {
                            ThreadSafeCallback::<(i32,), i32>::invoke(
                                context as *mut core::ffi::c_void,
                                (1,),
                            );
                            std::thread::yield_now();
                        }
                    });
                    Ok::<_, NativeStatusError>(WorkerToken {
                        stop,
                        worker: Some(worker),
                        unregister_attempts: 0,
                    })
                },
                |token: &mut WorkerToken| {
                    token.unregister_attempts += 1;
                    if token.unregister_attempts == 1 {
                        return Err(NativeStatusError(1));
                    }
                    token.stop.store(true, Ordering::Release);
                    token.worker.take().unwrap().join().unwrap();
                    Ok(())
                },
            )
        }
        .unwrap();

        while calls.load(Ordering::Relaxed) == 0 {
            std::thread::yield_now();
        }
        let failure = registration.try_unregister().unwrap_err();
        assert_eq!(failure.error(), &NativeStatusError(1));
        assert!(failure.registration().is_registered());
        let registration = failure.into_registration();
        registration.try_unregister().unwrap();
        let stopped_at = calls.load(Ordering::Relaxed);
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert_eq!(calls.load(Ordering::Relaxed), stopped_at);
    }

    #[test]
    fn retained_callback_drop_leaks_on_unregister_failure() {
        let dropped = Arc::new(AtomicBool::new(false));
        let drop_flag = DropFlag(Arc::clone(&dropped));
        let registration = unsafe {
            CallbackRegistration::register(
                move |()| {
                    let _keep_alive = &drop_flag;
                },
                |_| Ok::<_, ()>(()),
                |_| Err::<(), _>(NativeStatusError(2)),
            )
        }
        .unwrap();
        drop(registration);
        assert!(!dropped.load(Ordering::Acquire));
    }

    #[test]
    fn retained_callback_leaks_if_registration_or_unregister_panics() {
        let registration_dropped = Arc::new(AtomicBool::new(false));
        let drop_flag = DropFlag(Arc::clone(&registration_dropped));
        let result = std::panic::catch_unwind(|| unsafe {
            let _: Result<CallbackRegistration<(), (), (), _, ()>, ()> =
                CallbackRegistration::register(
                    move |()| {
                        let _keep_alive = &drop_flag;
                    },
                    |_| panic!("registration panic"),
                    |_| Ok(()),
                );
        });
        assert!(result.is_err());
        assert!(!registration_dropped.load(Ordering::Acquire));

        let unregister_dropped = Arc::new(AtomicBool::new(false));
        let drop_flag = DropFlag(Arc::clone(&unregister_dropped));
        let registration = unsafe {
            CallbackRegistration::register(
                move |()| {
                    let _keep_alive = &drop_flag;
                },
                |_| Ok::<_, ()>(()),
                |_| -> Result<(), ()> { panic!("unregister panic") },
            )
        }
        .unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = registration.try_unregister();
        }));
        assert!(result.is_err());
        assert!(!unregister_dropped.load(Ordering::Acquire));
    }
}
