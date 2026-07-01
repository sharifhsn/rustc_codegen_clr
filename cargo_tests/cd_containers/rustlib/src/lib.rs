//! The whole Rust side of the reusable C#->Rust generic container — one macro call emits the
//! size-erased `rcl_vec_*` core into this cdylib's `MainModule`. Compare to hand-writing the ~90-line
//! byte-vector core per project (as cargo_tests/cd_rustvec still does).
mycorrhiza::export_rust_containers!();
