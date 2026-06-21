//! Net PAL probe: does std::net (TcpListener/TcpStream/UdpSocket) work on the dotnet PAL via
//! System.Net.Sockets? Loopback only (no external network). Panic-safe (? inside run(), no unwrap).
//! SUCCESS = "== pal_net done ==" with the echo + udp results.
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};

fn tcp_echo() -> std::io::Result<String> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    let server = std::thread::spawn(move || -> std::io::Result<()> {
        let (mut s, _peer) = listener.accept()?;
        let mut buf = [0u8; 64];
        let n = s.read(&mut buf)?;
        s.write_all(&buf[..n])?; // echo
        Ok(())
    });
    let mut c = TcpStream::connect(addr)?;
    c.write_all(b"ping-tcp")?;
    let mut resp = [0u8; 64];
    let n = c.read(&mut resp)?;
    let _ = server.join();
    Ok(String::from_utf8_lossy(&resp[..n]).into_owned())
}

fn udp_roundtrip() -> std::io::Result<usize> {
    let a = UdpSocket::bind("127.0.0.1:0")?;
    let b = UdpSocket::bind("127.0.0.1:0")?;
    let b_addr = b.local_addr()?;
    a.send_to(b"ping-udp", b_addr)?;
    let mut buf = [0u8; 64];
    let (n, _from) = b.recv_from(&mut buf)?;
    Ok(n)
}

fn main() {
    println!("== pal_net start ==");
    match tcp_echo() {
        Ok(s) => println!("1  tcp echo: {:?} (expect \"ping-tcp\")", s),
        Err(e) => println!("1  tcp echo ERR: {:?}", e.kind()),
    }
    match udp_roundtrip() {
        Ok(n) => println!("2  udp roundtrip: {} bytes (expect 8)", n),
        Err(e) => println!("2  udp ERR: {:?}", e.kind()),
    }
    println!("== pal_net done ==");
}
