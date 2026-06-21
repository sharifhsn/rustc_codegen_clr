// DOTNET PAL ARM
//
// `DotnetRawHandle`: a local marker trait that exposes the GCHandle backing a
// std::net socket as a `u64` (the Selector's map key). os=dotnet has no
// target_family, so `std::os::fd::AsRawFd` / `std::os::windows::io::AsRawSocket`
// are both compiled out — there is no std `AsRaw*` trait to key the IoSource off.
// Instead the dotnet std::net types carry an inherent `dotnet_raw_handle(&self)
// -> *mut u8` accessor (see dotnet_pal/sys/net/connection/dotnet.rs); this trait
// widens that handle to u64 for the registry. This is the dotnet analogue of
// mio's windows `RawSocket` model: identity-by-raw-handle, just not via std::os.

use std::net::{TcpListener, TcpStream, UdpSocket};

/// Exposes a std::net socket's raw GCHandle as the Selector key.
pub trait DotnetRawHandle {
    fn dotnet_raw_handle(&self) -> u64;
}

// Each impl forwards to the INHERENT `dotnet_raw_handle(&self) -> *mut u8` method
// dev.sh injects onto the public `std::net::{TcpStream,TcpListener,UdpSocket}`
// wrappers (which forward to the sys PAL accessor). In method-call syntax the
// inherent method shadows this trait method, so `self.dotnet_raw_handle()` binds
// to the `*mut u8` inherent one, NOT to this trait (no recursion). The `let h:
// *mut u8` annotation makes that binding explicit; do not rewrite it as a path
// call `TcpStream::dotnet_raw_handle(self)`, which WOULD pick this trait method.
impl DotnetRawHandle for TcpStream {
    fn dotnet_raw_handle(&self) -> u64 {
        let h: *mut u8 = self.dotnet_raw_handle();
        h as u64
    }
}

impl DotnetRawHandle for TcpListener {
    fn dotnet_raw_handle(&self) -> u64 {
        let h: *mut u8 = self.dotnet_raw_handle();
        h as u64
    }
}

impl DotnetRawHandle for UdpSocket {
    fn dotnet_raw_handle(&self) -> u64 {
        let h: *mut u8 = self.dotnet_raw_handle();
        h as u64
    }
}
