//! Real proof that generated NuGet bindings can drive `NATS.Client` directly, publishing and
//! receiving `byte[]` payloads over a real NATS server without a hand-written C# shim.
//!
//! WHY `NATS.Client` AND NOT THE MODERN `NATS.Net`: the modern `NATS.Net` package (the one
//! actually shipped today, id `NATS.Net`) is async-only top to bottom — `PublishAsync`,
//! `SubscribeAsync` returning `IAsyncEnumerable<NatsMsg<T>>`, no synchronous escape hatch at all
//! (checked its reflected surface the same way below — every pub/sub-shaped member returns
//! `Task`/`ValueTask`/an async-iterator type). `NATS.Client` (pre-`NATS.Net`, still on nuget.org,
//! last version 1.1.8) is NATS.io's OLDER, fully synchronous .NET client:
//! `IConnection.Publish(subject, bytes)` and `Connection.SubscribeSync(subject).NextMessage(timeoutMs)`
//! block the calling thread — no `Task`/`await` anywhere in the pub/sub path. This backend has a
//! known async ceiling (a managed object ref cannot live in a coroutine's saved state across
//! `.await` inside an async fn — see mycorrhiza's task.rs doc), so the fully-synchronous NATS
//! client is the honest way to prove real NATS interop from Rust today; `NATS.Net` would need
//! that async work to land first.
//!
//! `cargo dotnet add-nuget NATS.Client 1.1.8` now preserves one-dimensional managed-array
//! signatures and virtual/interface calls with arguments. The generated `IConnection::publish`
//! accepts the real `byte[]` payload and `Msg::get_data` returns it; mycorrhiza's managed-array
//! helpers provide the UTF-8 boundary conversion. This fixture deliberately has no helper DLL.
//!
//! REQUIRES a real NATS server reachable at `NATS_URL` (default `nats://127.0.0.1:4222`) — see
//! this crate's module doc / the task's report for how one was provisioned for this run.
#![allow(dead_code)]

use mycorrhiza::system::DotNetString;
use mycorrhiza::system::console::Console;

mod nuget;
use nuget::nats_client::NATS::Client::{
    ConnectionFactory, ConnectionFactory_Methods, IConnection_Methods, ISyncSubscription_Methods,
    Msg_Methods,
};

fn nats_url() -> String {
    std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".to_string())
}

fn main() -> std::process::ExitCode {
    let mut pass: u32 = 0;
    let mut total: u32 = 0;
    macro_rules! chk {
        ($got:expr, $want:expr) => {{
            total += 1;
            if $got == $want {
                pass += 1;
            } else {
                Console::writeln_u64(900_000_000 + total as u64);
            }
        }};
    }

    let url: DotNetString = nats_url().as_str().into();
    let conn = ConnectionFactory::new().create_connection(url.handle(), false);

    let subject: DotNetString = "cd.nats.smoke.rust".into();
    let sub = conn.subscribe_sync(subject.handle());

    let payload = "hello from Rust on .NET via NATS.Client (sync)";
    conn.publish(
        subject.handle(),
        mycorrhiza::intrinsics::RustcCLRInteropManagedArray::from_utf8(payload),
    );
    conn.flush(5_000);

    let received = sub.next_message().get_data().to_utf8_string();
    chk!(received.as_str(), payload);

    // A second round trip with a distinct payload, to rule out a stale/cached first message.
    let payload2 = "second message 42";
    conn.publish(
        subject.handle(),
        mycorrhiza::intrinsics::RustcCLRInteropManagedArray::from_utf8(payload2),
    );
    conn.flush(5_000);
    let received2 = sub.next_message().get_data().to_utf8_string();
    chk!(received2.as_str(), payload2);

    conn.close();

    Console::writeln_u64(pass as u64);
    Console::writeln_u64(total as u64);
    if pass == total {
        println!("== cd_nats done ==");
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
