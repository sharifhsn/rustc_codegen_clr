//! Real proof that Rust can drive `NATS.Net`'s SYNCHRONOUS-era predecessor, `NATS.Client`
//! (the classic C-client-derived .NET binding, last released 1.1.8), publishing a message and
//! receiving it back over a REAL NATS server, entirely without `async`/`await`.
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
//! WHAT `cargo dotnet add-nuget NATS.Client 1.1.8` alone COULD NOT COVER: the reflected bindings
//! (`nuget::nats_client`) faithfully expose `ConnectionFactory`, `Connection`/`IConnection`,
//! `ISyncSubscription`, and `Msg` metadata (subject/reply/headers/ack) — but `Connection.Publish`
//! and `Msg.Data` are BOTH typed `byte[]`, and spinacz's reflector (`DType::from_tpe` in
//! `cargo_tests/spinacz/src/reflect.rs`) unconditionally drops any method whose parameter or
//! return type is a managed array (`get_IsArray`, marshalling tracked as WF-9, not implemented).
//! That silently removes the ENTIRE payload-carrying surface from the generated bindings, even
//! though everything else reflects. `csharp_helper/NatsHelper.cs` closes exactly that one gap
//! with `string`-typed (UTF8) overloads — the same "small helper assembly wired through
//! `.cargo-dotnet-nuget-assets/`" mechanism `cd_efcore`'s `EfHelper` already uses for a different
//! reason (fluent EF wiring). Rust itself never touches a `byte[]`.
//!
//! REQUIRES a real NATS server reachable at `NATS_URL` (default `nats://127.0.0.1:4222`) — see
//! this crate's module doc / the task's report for how one was provisioned for this run.
#![allow(dead_code)]

use mycorrhiza::intrinsics::RustcCLRInteropManagedClass;
use mycorrhiza::system::console::Console;
use mycorrhiza::system::DotNetString;

mod nuget;
use nuget::nats_client::NATS::Client::{IConnection, ISyncSubscription};

// `RustcCLRInteropManagedClass<..>` is a foreign type (defined in `mycorrhiza`), so an inherent
// `impl NatsHelper { .. }` here would violate the orphan rule (E0116) — same reason
// `add-nuget`'s own generated bindings use a local trait instead of an inherent impl (see
// `Namespace::export`'s doc in `cargo_tests/spinacz/src/reflect.rs`).
type NatsHelper = RustcCLRInteropManagedClass<"NatsHelper", "CdNatsHelper.NatsHelper">;

trait NatsHelperMethods {
    fn connect(url: mycorrhiza::system::MString) -> IConnection;
    fn publish_string(conn: IConnection, subject: mycorrhiza::system::MString, payload: mycorrhiza::system::MString);
    fn subscribe_sync(conn: IConnection, subject: mycorrhiza::system::MString) -> ISyncSubscription;
    fn next_message_data_as_string(sub: ISyncSubscription, timeout_ms: i32) -> mycorrhiza::system::MString;
    fn close(conn: IConnection);
}

impl NatsHelperMethods for NatsHelper {
    fn connect(url: mycorrhiza::system::MString) -> IConnection {
        Self::static1::<"Connect", mycorrhiza::system::MString, IConnection>(url)
    }
    fn publish_string(conn: IConnection, subject: mycorrhiza::system::MString, payload: mycorrhiza::system::MString) {
        mycorrhiza::intrinsics::rustc_clr_interop_managed_call3_::<
            "NatsHelper",
            "CdNatsHelper.NatsHelper",
            false,
            "PublishString",
            true,
            (),
            IConnection,
            mycorrhiza::system::MString,
            mycorrhiza::system::MString,
        >(conn, subject, payload)
    }
    fn subscribe_sync(conn: IConnection, subject: mycorrhiza::system::MString) -> ISyncSubscription {
        Self::static2::<"SubscribeSync", IConnection, mycorrhiza::system::MString, ISyncSubscription>(conn, subject)
    }
    fn next_message_data_as_string(sub: ISyncSubscription, timeout_ms: i32) -> mycorrhiza::system::MString {
        Self::static2::<"NextMessageDataAsString", ISyncSubscription, i32, mycorrhiza::system::MString>(sub, timeout_ms)
    }
    fn close(conn: IConnection) {
        Self::static1::<"Close", IConnection, ()>(conn)
    }
}

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
    let conn: IConnection = NatsHelper::connect(url.handle());

    let subject: DotNetString = "cd.nats.smoke.rust".into();
    let sub: ISyncSubscription = NatsHelper::subscribe_sync(conn, subject.handle());

    let payload: DotNetString = "hello from Rust on .NET via NATS.Client (sync)".into();
    NatsHelper::publish_string(conn, subject.handle(), payload.handle());

    // Fully synchronous: blocks the calling thread, no Task/await anywhere in this path.
    let received = NatsHelper::next_message_data_as_string(sub, 5000);
    let received_str = String::from(DotNetString::from_handle(received));
    chk!(received_str.as_str(), "hello from Rust on .NET via NATS.Client (sync)");

    // A second round trip with a distinct payload, to rule out a stale/cached first message.
    let payload2: DotNetString = "second message 42".into();
    NatsHelper::publish_string(conn, subject.handle(), payload2.handle());
    let received2 = NatsHelper::next_message_data_as_string(sub, 5000);
    let received2_str = String::from(DotNetString::from_handle(received2));
    chk!(received2_str.as_str(), "second message 42");

    NatsHelper::close(conn);

    Console::writeln_u64(pass as u64);
    Console::writeln_u64(total as u64);
    if pass == total {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
