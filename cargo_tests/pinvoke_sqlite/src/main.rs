//! End-to-end proof that application code can use a safe facade over generated P/Invoke bindings.

mod native;
mod sqlite;

use std::cell::Cell;
use std::rc::Rc;

use sqlite::Database;

fn main() {
    let database = Database::open(":memory:").expect("SQLite could not open a database");
    database
        .execute(
            "CREATE TABLE values_table(value INTEGER);\
             INSERT INTO values_table VALUES (19), (23);",
        )
        .expect("SQLite setup failed");

    let total = Rc::new(Cell::new(0));
    let callback_total = Rc::clone(&total);
    database
        .query(
            "SELECT value FROM values_table ORDER BY value",
            move |row| {
                assert_eq!(row.len(), 1);
                let value = row[0]
                    .parse::<i32>()
                    .expect("SQLite returned a non-integer value");
                callback_total.set(callback_total.get() + value);
            },
        )
        .expect("SQLite query failed");
    assert_eq!(total.get(), 42);

    let version = Database::version();
    assert!(version >= 3_000_000, "SQLite returned an invalid version");
    println!("SQLite P/Invoke OK: {version}; query sum={}", total.get());
}
