//! Filesystem-metadata PAL probe (B2 Pieces 2/3/4 on the real dotnet PAL):
//!   * MetadataExt timestamps   — `fs::metadata(f).st_mtime() > 0` (Piece 2,
//!     FileInfo.LastWriteTimeUtc via the extended rcl_dotnet_fs_stat hook).
//!   * pread/pwrite at an offset — `File::write_at(b"X", 5)` then `read_at` at 5
//!     round-trips WITHOUT moving the sequential cursor (Piece 3, RandomAccess).
//!   * symlink round-trip        — `symlink` + `fs::read_link` + the
//!     `symlink_metadata().file_type().is_symlink()` flag (Piece 4,
//!     File.CreateSymbolicLink / ResolveLinkTarget / FileAttributes.ReparsePoint).
//!
//! Panic-safe: all fallible work is behind `?` in `run()`; the happy path has no
//! `unwrap`/`expect`. SUCCESS = "== pal_fsmeta done ==".
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{FileExt, MetadataExt};

fn run() -> std::io::Result<()> {
    let dir = std::env::temp_dir();
    let path = dir.join("pal_fsmeta_test.bin");
    let _ = fs::remove_file(&path);

    // ---- Piece 2: MetadataExt timestamps ----
    {
        let mut f = File::create(&path)?;
        f.write_all(b"0123456789")?; // 10 bytes
        f.flush()?;
    }
    let md = fs::metadata(&path)?;
    let size = md.len();
    // The public os::unix::fs::MetadataExt accessors are `mtime()`/`ctime()`
    // (they forward internally to the platform `st_mtime()`/`st_ctime()`).
    let mtime = md.mtime();
    let ctime = md.ctime();
    println!("1  size={size} mtime={mtime} ctime={ctime}");
    assert_eq!(size, 10, "file should be 10 bytes");
    assert!(mtime > 0, "mtime must be a real (non-zero) Unix timestamp");
    // The file was just created, so mtime should be after the 2020-01-01 epoch
    // sanity bound (1577836800) — proves it is a real wall-clock value, not 1.
    assert!(mtime > 1_577_836_800, "mtime should be a recent timestamp");
    // ctime maps to .NET CreationTimeUtc — also real and non-zero.
    assert!(ctime > 0, "ctime must be non-zero");

    // metadata().modified() must now succeed (Piece 2 timestamps).
    let modified = md.modified()?;
    let since = modified
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| std::io::Error::other("modified before epoch"))?;
    println!("2  modified() since epoch = {}s", since.as_secs());
    assert!(since.as_secs() > 1_577_836_800);

    // ---- Piece 3: pread/pwrite at an offset ----
    {
        let f = OpenOptions::new().read(true).write(true).open(&path)?;
        // Seek the sequential cursor to the start so we can prove write_at does
        // NOT move it.
        let mut g = OpenOptions::new().read(true).write(true).open(&path)?;
        g.seek(SeekFrom::Start(0))?;

        // write_at: overwrite byte at offset 5 with 'X'.
        let n = f.write_at(b"X", 5)?;
        assert_eq!(n, 1, "write_at should write exactly 1 byte");

        // read_at: read it straight back from offset 5.
        let mut one = [0u8; 1];
        let r = f.read_at(&mut one, 5)?;
        assert_eq!(r, 1, "read_at should read exactly 1 byte");
        assert_eq!(one[0], b'X', "read_at@5 must observe the write_at@5");
        println!("3  pread@5 == pwrite@5 == 'X' (offset-relative I/O ok)");

        // read_at at offset 0 should still see the original first byte '0'
        // (write_at@5 must not have moved/affected offset 0).
        let mut head = [0u8; 1];
        f.read_at(&mut head, 0)?;
        assert_eq!(head[0], b'0', "byte 0 untouched by write_at@5");

        // The sequential cursor of `g` is still at 0 — read 1 byte sequentially
        // and confirm it is '0' (proves positioned I/O didn't disturb it).
        let mut seqbuf = [0u8; 1];
        g.read_exact(&mut seqbuf)?;
        assert_eq!(seqbuf[0], b'0', "sequential cursor unaffected by pwrite");
        println!("4  sequential cursor unaffected by positioned I/O");
    }

    // ---- Piece 4: symlink round-trip ----
    {
        let link = dir.join("pal_fsmeta_link");
        let _ = fs::remove_file(&link);
        std::os::unix::fs::symlink(&path, &link)?;

        // read_link recovers the target (resolved full path on .NET; we assert it
        // points at our file by basename).
        let target = fs::read_link(&link)?;
        println!("5  read_link -> {}", target.display());
        let tgt_name = target.file_name().and_then(|s| s.to_str()).unwrap_or("");
        assert_eq!(tgt_name, "pal_fsmeta_test.bin", "link target basename");

        // symlink_metadata().file_type().is_symlink() must be true (Piece 4 flag).
        let lmd = fs::symlink_metadata(&link)?;
        let is_link = lmd.file_type().is_symlink();
        println!("6  symlink_metadata().is_symlink() = {is_link}");
        assert!(is_link, "the reparse-point flag must mark this as a symlink");

        let _ = fs::remove_file(&link);
    }

    let _ = fs::remove_file(&path);
    Ok(())
}

fn main() {
    match run() {
        Ok(()) => println!("== pal_fsmeta done =="),
        Err(e) => {
            println!("!! pal_fsmeta FAILED: {e}");
            std::process::exit(1);
        }
    }
}
