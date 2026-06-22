//! J2 — a fresh tokio loopback TCP echo, distinct from cargo_tests/pal_tokio_net.
//! Proves `cargo dotnet run` builds + runs a crate with a real syscall-level dep
//! (tokio rt/macros/net/io-util, pulling mio + socket2 transitively) through the
//! AUTO-APPLIED dotnet_overlays — with ZERO hand-config beyond a normal tokio dep.
//!
//! Protocol (deliberately different from pal_tokio_net's single "ping-tokio"):
//! the client sends THREE distinct line-framed messages; the server echoes each
//! back uppercased so the round-trip is verifiably transformed, not a passthrough.
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const MESSAGES: [&str; 3] = ["alpha", "bravo", "charlie"];

async fn run() -> std::io::Result<Vec<String>> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    // Server: accept one connection, echo each received chunk back UPPERCASED.
    let server = tokio::spawn(async move {
        let (mut sock, _peer) = listener.accept().await?;
        let mut buf = [0u8; 128];
        loop {
            let n = sock.read(&mut buf).await?;
            if n == 0 {
                break; // client closed -> done
            }
            let upper: Vec<u8> = buf[..n].iter().map(|b| b.to_ascii_uppercase()).collect();
            sock.write_all(&upper).await?;
        }
        Ok::<(), std::io::Error>(())
    });

    // Client: send each message, read its transformed echo back.
    let mut client = TcpStream::connect(addr).await?;
    let mut replies = Vec::new();
    for msg in MESSAGES {
        client.write_all(msg.as_bytes()).await?;
        let mut resp = vec![0u8; msg.len()];
        client.read_exact(&mut resp).await?;
        replies.push(String::from_utf8_lossy(&resp).into_owned());
    }
    // Drop the write half so the server's read returns 0 and its task ends.
    client.shutdown().await?;
    let _ = server.await;
    Ok(replies)
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    println!("== cd_tokio start ==");
    match run().await {
        Ok(replies) => {
            println!("echoed (uppercased): {:?}", replies);
            let expect: Vec<String> = MESSAGES.iter().map(|m| m.to_ascii_uppercase()).collect();
            assert_eq!(replies, expect, "echo round-trip mismatch");
            println!("cd_tokio: all 3 echoes correct");
        }
        Err(e) => {
            println!("cd_tokio tcp ERR: {:?}", e.kind());
            std::process::exit(1);
        }
    }
    println!("== cd_tokio done ==");
}
