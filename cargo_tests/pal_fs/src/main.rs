//! H2 Phase 4 (fs) probe: exercise std::fs on the dotnet PAL. Panic-safe — prints Ok/Err per op
//! (never unwraps, so it doesn't trip the default-panic-hook atomic bug). SUCCESS = every op Ok and
//! "== pal_fs done ==". Before the fs PAL arm exists, each prints Err(Unsupported).
use std::io::{Read, Write};

fn main() {
    println!("== pal_fs start ==");
    let path = "/tmp/pal_fs_test.txt";
    let dir = "/tmp/pal_fs_dir";

    println!("1  write:        {:?}", std::fs::write(path, b"hello pal fs"));
    println!("2  read_to_str:  {:?}", std::fs::read_to_string(path));
    println!("3  metadata:     {:?}", std::fs::metadata(path).map(|m| (m.len(), m.is_file())));

    // File::open + Read
    match std::fs::File::open(path) {
        Ok(mut f) => {
            let mut s = String::new();
            println!("4  File read:    {:?} -> {:?}", f.read_to_string(&mut s), s);
        }
        Err(e) => println!("4  File open:    Err({e:?})"),
    }

    // append via OpenOptions
    match std::fs::OpenOptions::new().append(true).open(path) {
        Ok(mut f) => println!("5  append:       {:?}", f.write_all(b"!more")),
        Err(e) => println!("5  append open:  Err({e:?})"),
    }
    println!("5b read-after-append: {:?}", std::fs::read_to_string(path));

    // dirs
    println!("6  create_dir:   {:?}", std::fs::create_dir(dir));
    let _ = std::fs::write(format!("{dir}/a.txt"), b"a");
    println!("7  read_dir cnt: {:?}", std::fs::read_dir(dir).map(|it| it.count()));

    // cleanup
    println!("8  remove_file:  {:?}", std::fs::remove_file(path));
    let _ = std::fs::remove_dir_all(dir);
    println!("9  exists:       {}", std::path::Path::new(path).exists());

    println!("== pal_fs done ==");
}
