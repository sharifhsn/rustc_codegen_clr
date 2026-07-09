// A minimal C# shim over `NATS.Client`'s synchronous pub/sub API.
//
// WHY THIS EXISTS: `cargo dotnet add-nuget NATS.Client` reflects `Connection.Publish(string,
// byte[])` / `Msg.Data` (a `byte[]` property) and every other byte[]-typed member of the real
// NATS.Client surface -- but spinacz's reflection generator (`cargo_tests/spinacz/src/reflect.rs`,
// `DType::from_tpe`) unconditionally `Skip`s ANY method whose parameter or return type is a
// managed array (`get_IsArray` check) -- the marshalling for managed arrays is tracked as WF-9
// and not implemented in the general reflector. That drops the message-payload accessor
// entirely: `Connection.Publish(string, byte[])`, `Msg.Data` getter AND setter are all absent
// from the generated `nuget::nats_client` bindings, even though everything else (construction,
// `SubscribeSync`, `NextMessage`, `Msg` metadata) reflects fine.
//
// This tiny helper (same runtime-asset-marker-dir mechanism as `cd_efcore`'s `EfHelper`) closes
// JUST that one gap with `string`-typed overloads (UTF8 in/out), so the actual pub/sub round
// trip is driven end-to-end from Rust via the REAL NATS.Client synchronous API -- no async
// required, and no hand-parsing of the wire protocol.
using System;
using System.Text;
using NATS.Client;

namespace CdNatsHelper
{
    public static class NatsHelper
    {
        public static IConnection Connect(string url)
        {
            var cf = new ConnectionFactory();
            return cf.CreateConnection(url);
        }

        public static void PublishString(IConnection conn, string subject, string payload)
        {
            conn.Publish(subject, Encoding.UTF8.GetBytes(payload));
            conn.Flush();
        }

        public static ISyncSubscription SubscribeSync(IConnection conn, string subject)
        {
            return conn.SubscribeSync(subject);
        }

        public static string NextMessageDataAsString(ISyncSubscription sub, int timeoutMs)
        {
            var msg = sub.NextMessage(timeoutMs);
            return Encoding.UTF8.GetString(msg.Data);
        }

        public static string GetSubject(ISyncSubscription sub)
        {
            return sub.Subject;
        }

        public static void Close(IConnection conn)
        {
            conn.Close();
        }
    }
}
