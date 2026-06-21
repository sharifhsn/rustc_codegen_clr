//! Raw-mio probe: does mio's Poll/Registry/readiness compile + run on os=dotnet? Scopes the mio
//! dotnet sys arm (epoll-readiness -> .NET Socket.Select). Loopback. Panic-safe.
use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token};
use std::io::{Read, Write};

const SERVER: Token = Token(0);
const CLIENT: Token = Token(1);

fn run() -> std::io::Result<String> {
    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(16);
    let mut listener = TcpListener::bind("127.0.0.1:0".parse().unwrap())?;
    let addr = listener.local_addr()?;
    poll.registry().register(&mut listener, SERVER, Interest::READABLE)?;
    let mut client = TcpStream::connect(addr)?;
    poll.registry().register(&mut client, CLIENT, Interest::WRITABLE)?;

    let mut got = String::new();
    let mut server_conn: Option<TcpStream> = None;
    for _ in 0..50 {
        poll.poll(&mut events, Some(std::time::Duration::from_millis(200)))?;
        for event in events.iter() {
            match event.token() {
                SERVER => {
                    if let Ok((mut s, _)) = listener.accept() {
                        let _ = s.write(b"hi-mio");
                        server_conn = Some(s);
                    }
                }
                CLIENT => {
                    let mut buf = [0u8; 32];
                    if let Ok(n) = client.read(&mut buf) {
                        if n > 0 { got = String::from_utf8_lossy(&buf[..n]).into_owned(); }
                    }
                    let _ = poll.registry().reregister(&mut client, CLIENT, Interest::READABLE);
                }
                _ => {}
            }
        }
        if !got.is_empty() { break; }
    }
    drop(server_conn);
    Ok(got)
}

fn main() {
    println!("== pal_mio start ==");
    match run() {
        Ok(s) => println!("1  mio readiness echo: {:?} (expect \"hi-mio\")", s),
        Err(e) => println!("1  mio ERR: {:?}", e.kind()),
    }
    println!("== pal_mio done ==");
}
