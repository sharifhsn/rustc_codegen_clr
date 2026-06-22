//! AF_UNIX runtime PAL probe (B2 Piece 1 on the real dotnet PAL):
//! a path-based `UnixListener`/`UnixStream` loopback echo over a temp-dir path.
//!
//! Proves the two-layer AF_UNIX wiring:
//!   * net-PAL `Socket::{new,accept,read,write,send_with_flags,shutdown,
//!     set_nonblocking}` -> the fd-backed BCL Socket (AddressFamily.Unix), and
//!   * POSIX-shim raw `bind`/`listen`/`connect` -> `UnixDomainSocketEndPoint`
//!     (the family-dispatching `rcl_endpoint_from_sockaddr`).
//!
//! Server thread binds + accepts + echoes one message; the client connects to the
//! same path, writes b"ping-uds", and reads it back. Panic-safe (`?` in run(); no
//! unwrap/expect on the happy path). SUCCESS = "== pal_uds done ==".
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};

fn run() -> std::io::Result<()> {
    let mut path = std::env::temp_dir();
    // Unique-ish name (pid) so re-runs don't collide on a stale socket node.
    path.push(format!("pal_uds_{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&path);

    let listener = UnixListener::bind(&path)?;
    println!("1  bound UnixListener at {}", path.display());

    let server_path = path.clone();
    let server = std::thread::spawn(move || -> std::io::Result<()> {
        let (mut s, _peer) = listener.accept()?;
        let mut buf = [0u8; 64];
        let n = s.read(&mut buf)?;
        s.write_all(&buf[..n])?; // echo
        let _ = server_path; // keep the path alive for the listener's lifetime
        Ok(())
    });

    let mut client = UnixStream::connect(&path)?;
    println!("2  connected UnixStream");
    client.write_all(b"ping-uds")?;
    let mut back = [0u8; 64];
    let n = client.read(&mut back)?;
    let echoed = &back[..n];
    println!("3  uds echo: {:?} (expect \"ping-uds\")", String::from_utf8_lossy(echoed));
    assert_eq!(echoed, b"ping-uds", "echo must round-trip over the unix socket");

    server
        .join()
        .map_err(|_| std::io::Error::other("server thread panicked"))??;

    let _ = std::fs::remove_file(&path);
    Ok(())
}

fn main() {
    match run() {
        Ok(()) => println!("== pal_uds done =="),
        Err(e) => {
            println!("!! pal_uds FAILED: {e}");
            std::process::exit(1);
        }
    }
}
