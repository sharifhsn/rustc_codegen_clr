//! End-goal probe: I/O-driven tokio (TcpListener/TcpStream via mio reactor) over loopback.
//! Pulls mio transitively -> scopes Phase D (mio dotnet arm). Panic-safe.
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

async fn run() -> std::io::Result<String> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let server = tokio::spawn(async move {
        let (mut s, _) = listener.accept().await?;
        let mut buf = [0u8; 64];
        let n = s.read(&mut buf).await?;
        s.write_all(&buf[..n]).await?;
        Ok::<(), std::io::Error>(())
    });
    let mut c = TcpStream::connect(addr).await?;
    c.write_all(b"ping-tokio").await?;
    let mut resp = [0u8; 64];
    let n = c.read(&mut resp).await?;
    let _ = server.await;
    Ok(String::from_utf8_lossy(&resp[..n]).into_owned())
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    println!("== pal_tokio_net start ==");
    match run().await {
        Ok(s) => println!("1  tokio tcp echo: {:?} (expect \"ping-tokio\")", s),
        Err(e) => println!("1  tokio tcp ERR: {:?}", e.kind()),
    }
    println!("== pal_tokio_net done ==");
}
