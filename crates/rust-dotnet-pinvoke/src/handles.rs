//! Ownership wrapper for opaque native handles.

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
