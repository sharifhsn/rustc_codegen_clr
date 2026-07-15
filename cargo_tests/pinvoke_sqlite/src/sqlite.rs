//! Safe, managed-feeling facade over the generated SQLite ABI declarations.

use std::ffi::CStr;
use std::os::raw::{c_char, c_int};

use rust_dotnet_pinvoke::{
    NativeCallError, native_api, status_zero,
};

use crate::native;

native_api! {
    scoped_callback RowCallback as row_callback(
        columns: c_int,
        values: *mut *mut c_char,
        names: *mut *mut c_char,
    ) -> c_int {
        on_panic = 1;
    }

    /// An open SQLite database, closed automatically on drop.
    pub handle Database(native::sqlite3) {
        close = native::sqlite3_close;
    }

    fn open_database(filename: &str) -> Database {
        utf8 filename => filename_pointer;
        out database: *mut native::sqlite3 => database_pointer;
        unsafe_call = native::sqlite3_open(filename_pointer, database_pointer);
        status = status_zero;
        success = handle Database(database);
    }

    fn execute_database(
        database: *mut native::sqlite3,
        sql: &str,
        callback: native::sqlite3_callback,
        context: *mut core::ffi::c_void,
    ) -> () {
        utf8 sql => sql_pointer;
        error_out error_message: *mut core::ffi::c_char => error_message_pointer;
        unsafe_call = native::sqlite3_exec(
            database,
            sql_pointer,
            callback,
            context,
            error_message_pointer,
        );
        status = status_zero;
        error = owned_utf8(free = native::sqlite3_free);
        success = unit;
    }
}

impl Database {
    pub fn open(filename: &str) -> Result<Self, NativeCallError> {
        open_database(filename)
    }

    pub fn execute(&self, sql: &str) -> Result<(), NativeCallError> {
        self.call(sql, None, std::ptr::null_mut())
    }

    pub fn query(
        &self,
        sql: &str,
        mut on_row: impl FnMut(Vec<String>) + 'static,
    ) -> Result<(), NativeCallError> {
        let mut callback = RowCallback::new(move |(columns, values, _names)| {
                assert!(columns >= 0, "SQLite returned a negative column count");
                let values = if columns == 0 {
                    &[][..]
                } else {
                    assert!(!values.is_null(), "SQLite returned a null value array");
                    unsafe { std::slice::from_raw_parts(values, columns as usize) }
                };
                let row = values
                    .iter()
                    .map(|&value| {
                        assert!(!value.is_null(), "SQLite returned a null column");
                        unsafe { CStr::from_ptr(value) }
                            .to_str()
                            .expect("SQLite returned non-UTF-8 text")
                            .to_owned()
                    })
                    .collect();
                on_row(row);
                0
            });
        self.call(sql, Some(row_callback), callback.context())
    }

    pub fn version() -> i32 {
        unsafe { native::sqlite3_libversion_number() }
    }

    fn call(
        &self,
        sql: &str,
        callback: native::sqlite3_callback,
        context: *mut std::ffi::c_void,
    ) -> Result<(), NativeCallError> {
        execute_database(self.as_ptr(), sql, callback, context)
    }
}
