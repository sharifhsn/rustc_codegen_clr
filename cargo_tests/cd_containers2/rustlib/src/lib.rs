//! The whole Rust side of the round-2 reusable C#->Rust containers — three macro calls emit the
//! size-erased `rcl_map_*` (hash map), `rcl_str_*` (UTF-8 string) and `rcl_vec_*` (list) cores into
//! this cdylib's `MainModule`. The C# consumer uses the shipped `RustDotnet.RustHashMap<K, V>` /
//! `RustDotnet.RustString` / `RustDotnet.RustVec<T>` wrappers over them, with zero hand-written glue.
mycorrhiza::export_rust_containers!();
mycorrhiza::export_rust_hashmap!();
mycorrhiza::export_rust_string!();
