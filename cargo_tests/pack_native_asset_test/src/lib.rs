//! Proves `cargo dotnet pack`'s add-nuget transparency fix handles the RID-specific NATIVE
//! asset case correctly — the one with real design risk, per the design note in pack.rs: a
//! bundled-raw-dll approach could never replicate NuGet's own `runtimes/<rid>/native/...`
//! targeting, but declaring a real `<dependency>` and letting NuGet's own restore resolve it
//! should. `Microsoft.Data.Sqlite.Core` (added via `add-nuget`) needs a native SQLite driver
//! (SQLitePCLRaw's `e_sqlite3`) at runtime to actually open a connection — if the native asset
//! doesn't land correctly, `open()` throws immediately.
#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, non_snake_case)]

pub mod nuget;

use mycorrhiza::bindings::System::Convert;
use mycorrhiza::system::MString;
use nuget::microsoft_data_sqlite_core::Microsoft::Data::Sqlite::{
    SqliteCommand, SqliteCommand_Methods, SqliteConnection, SqliteConnection_Methods,
};

#[unsafe(no_mangle)]
pub extern "C" fn sqlite_native_smoke() -> i32 {
    let conn = SqliteConnection::new();
    conn.set_connection_string(MString::from("Data Source=:memory:"));
    conn.open(); // throws immediately if the native e_sqlite3 driver didn't resolve.

    let cmd: SqliteCommand = conn.create_command();
    cmd.set_command_text(MString::from("SELECT 21 + 21;"));
    let result = cmd.execute_scalar();
    Convert::to_int32(result)
}
