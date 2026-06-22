//! .NET-specific `MetadataExt` — the `platform::fs::MetadataExt` the cross-unix
//! `os/unix/fs.rs` impl delegates to (`self.st_dev()` etc.). PACKAGE A/B.
//!
//! The cross-unix `MetadataExt for fs::Metadata` (in `os/unix/fs.rs`) is written
//! as thin forwards onto these `st_*` accessors, which each `platform` supplies.
//! On .NET there is no POSIX `stat`, so we synthesize the `st_*` values from the
//! dotnet `sys::fs::FileAttr` (which carries real `size` + `is_dir`):
//!
//! * `st_size`  -> the real file length (BCL `FileInfo.Length` / `FileStream`).
//! * `st_mode`  -> a synthetic mode: the directory/regular `S_IFDIR`/`S_IFREG`
//!   type bits OR'd with `0o755`/`0o644` default perms (.NET has no POSIX perm
//!   bits — LEAKY L2).
//! * everything else (`st_dev/ino/nlink/uid/gid/rdev`, all timestamps, blocks) is
//!   **0** — IMPOSSIBLE on CoreCLR (I1/I2): no inode identity, ownership, link
//!   count, or block accounting. `st_blksize` reports a conventional 4096.
//!
//! Real timestamps (`FileInfo.LastWriteTimeUtc` etc.) are a clean follow-up but
//! are NOT required for COMPILE; this is the Package-A/B compile-stub.
#![stable(feature = "metadata_ext", since = "1.1.0")]

use crate::fs::Metadata;
use crate::sys::AsInner;

// Synthetic POSIX type bits (Linux S_IFMT layout) for st_mode.
const S_IFDIR: u32 = 0o040000;
const S_IFREG: u32 = 0o100000;

/// OS-specific extensions to [`fs::Metadata`] for the .NET PAL.
///
/// [`fs::Metadata`]: crate::fs::Metadata
#[stable(feature = "metadata_ext", since = "1.1.0")]
pub trait MetadataExt {
    #[stable(feature = "metadata_ext2", since = "1.8.0")]
    fn st_dev(&self) -> u64;
    #[stable(feature = "metadata_ext2", since = "1.8.0")]
    fn st_ino(&self) -> u64;
    #[stable(feature = "metadata_ext2", since = "1.8.0")]
    fn st_mode(&self) -> u32;
    #[stable(feature = "metadata_ext2", since = "1.8.0")]
    fn st_nlink(&self) -> u64;
    #[stable(feature = "metadata_ext2", since = "1.8.0")]
    fn st_uid(&self) -> u32;
    #[stable(feature = "metadata_ext2", since = "1.8.0")]
    fn st_gid(&self) -> u32;
    #[stable(feature = "metadata_ext2", since = "1.8.0")]
    fn st_rdev(&self) -> u64;
    #[stable(feature = "metadata_ext2", since = "1.8.0")]
    fn st_size(&self) -> u64;
    #[stable(feature = "metadata_ext2", since = "1.8.0")]
    fn st_atime(&self) -> i64;
    #[stable(feature = "metadata_ext2", since = "1.8.0")]
    fn st_atime_nsec(&self) -> i64;
    #[stable(feature = "metadata_ext2", since = "1.8.0")]
    fn st_mtime(&self) -> i64;
    #[stable(feature = "metadata_ext2", since = "1.8.0")]
    fn st_mtime_nsec(&self) -> i64;
    #[stable(feature = "metadata_ext2", since = "1.8.0")]
    fn st_ctime(&self) -> i64;
    #[stable(feature = "metadata_ext2", since = "1.8.0")]
    fn st_ctime_nsec(&self) -> i64;
    #[stable(feature = "metadata_ext2", since = "1.8.0")]
    fn st_blksize(&self) -> u64;
    #[stable(feature = "metadata_ext2", since = "1.8.0")]
    fn st_blocks(&self) -> u64;
}

#[stable(feature = "metadata_ext", since = "1.1.0")]
impl MetadataExt for Metadata {
    fn st_dev(&self) -> u64 {
        0 // no managed file-id (I1)
    }
    fn st_ino(&self) -> u64 {
        0 // no inode identity (I1)
    }
    fn st_mode(&self) -> u32 {
        // Synthesize the type bits from the dotnet FileAttr's dir/file flag plus a
        // conventional default perm set (no real POSIX perm bits — L2).
        if self.as_inner().file_type().is_dir() {
            S_IFDIR | 0o755
        } else {
            S_IFREG | 0o644
        }
    }
    fn st_nlink(&self) -> u64 {
        1 // no link count (I2)
    }
    fn st_uid(&self) -> u32 {
        0 // no POSIX ownership (I2)
    }
    fn st_gid(&self) -> u32 {
        0 // no POSIX ownership (I2)
    }
    fn st_rdev(&self) -> u64 {
        0
    }
    fn st_size(&self) -> u64 {
        // REAL: the dotnet FileAttr carries the file length.
        self.as_inner().size()
    }
    fn st_atime(&self) -> i64 {
        0 // timestamps deferred (FileInfo.LastAccessTimeUtc is a follow-up)
    }
    fn st_atime_nsec(&self) -> i64 {
        0
    }
    fn st_mtime(&self) -> i64 {
        0
    }
    fn st_mtime_nsec(&self) -> i64 {
        0
    }
    fn st_ctime(&self) -> i64 {
        0
    }
    fn st_ctime_nsec(&self) -> i64 {
        0
    }
    fn st_blksize(&self) -> u64 {
        4096 // conventional block size
    }
    fn st_blocks(&self) -> u64 {
        0
    }
}
