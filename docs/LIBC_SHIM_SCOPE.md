# A POSIX/libc-over-.NET shim — scope, feasibility, and phased plan

> **Status:** design + feasibility analysis only. No code in this document is built yet.
> **Goal (from the project owner):** a downstream user with a dependency like `mio`,
> `socket2`, or any of the long tail of POSIX-assuming `-sys` crates should flip **one
> simple cfg** (a dotnet target-spec flag that *mirrors* libc) and have the crate work
> **unmodified** — no hand-maintained bindings, no per-crate fork. This is contingent on the
> .NET interface mapping to libc in an **architecturally clean** way. Where it does not, this
> document marks the surface **leaky** or **impossible** rather than faking it.

Related reading: [docs/ARCHITECTURE.md](ARCHITECTURE.md) (IR + .NET-mapping gotchas),
[docs/TRANSLATION_STATUS.md](TRANSLATION_STATUS.md) (the broader Rust↔.NET completeness map;
this doc is the libc-tier deep-dive of its §8 PAL work).

---

## 1. Executive summary

**How much of libc maps cleanly?** A large, useful core — but unevenly, and concentrated in
exactly the clusters that real-world crates touch. Across the six libc clusters scoped here,
roughly **55–70% of the realistically-needed call surface is already implemented** as CIL
bodies inside the backend (the `rcl_dotnet_*` and `pthread_*` hooks in
[`cilly/src/ir/builtins/dotnet.rs`](../cilly/src/ir/builtins/dotnet.rs) and
[`cilly/src/ir/builtins/thread.rs`](../cilly/src/ir/builtins/thread.rs)). These hooks are
backed by the .NET BCL (`System.IO`, `System.Net.Sockets`, `System.Threading`,
`NativeMemory`, `Stopwatch`/`DateTime`) and are already wired onto the .NET target by the
linker ([`cilly/src/bin/linker/main.rs:452,459`](../cilly/src/bin/linker/main.rs)). **The
libc shim is therefore mostly a re-packaging problem**: expose the same CIL bodies under
their bare POSIX C-ABI symbol names, route integer file descriptors through a new fd-table to
the existing opaque-`GCHandle` hooks, and add the one piece the PAL has never had — a
thread-local `errno`.

The honest ceiling has three tiers:

- **CLEAN** (faithful, mostly re-package): heap allocators, the time/clock family, env, the
  socket connection + readiness core, the file data-plane and common path ops, and thread
  lifecycle. These are the bulk of what `mio`/`socket2`/`tokio-net` and ordinary `-sys`
  crates exercise.
- **LEAKY** (mappable, but with a caveat that must be advertised): `errno` itself (the BCL
  signals failure by *throwing*, so every wrapper must `try/catch` and translate
  exception→errno, and the long tail collapses to `EIO`); `EAGAIN` on non-blocking sockets
  (works, but pays exception machinery); `readv`/`writev` (per-iovec loop, not atomic);
  `dup`/`dup2` (alias+refcount, no independent OS description); `struct stat` rich fields;
  permission/`chmod`/`access(R/W/X)` (host-conditional on Unix-host CoreCLR); `posix_spawn`
  (viable via `System.Diagnostics.Process`, but the "pid" is synthetic).
- **IMPOSSIBLE** on stock CoreCLR (must be `ENOSYS`/documented, never faked): `fork`/`vfork`,
  `execve` (in-place image replace), real `pipe()` that participates in the readiness loop,
  `mmap(MAP_FIXED)` / file-backed / shared mmap, `mremap`, `mprotect` guard pages,
  `brk`/`sbrk`, raw signal *delivery*, `st_ino`/`st_dev`/`st_nlink`, hard `link()`, `chown`,
  `socketpair`, edge-triggered/`O(1)` epoll.

**Is the one-cfg-flip DX achievable?** **Yes as the end-state, but it is BLOCKED today** and
cannot be the *first* step. The flip the owner wants is adding `"families": ["unix"]` to
`x86_64-unknown-dotnet.json` — that turns on `cfg(unix)` + `cfg(target_family="unix")`
globally, so unmodified `mio`/`socket2` compile their existing `unix` arms straight onto the
shim. But flipping it *now* breaks **std's own compilation**: I read the live std cfg
cascades and at least eight modules (`sys::process`, `sys::fd`, `sys::pipe`,
`sys::sync::{mutex,rwlock,condvar,once}`, plus the public `std::os::fd` / `std::os::unix`
trees) have **no `target_os="dotnet"` arm** and would mis-select the libc/unix
implementation, which assumes real integer fds and `libc::close`/`pthread_mutex_t` the BCL
PAL cannot supply. The needle-thread is real but it is a **capstone**: build the fd-table +
the missing dotnet PAL arms first (gate stays green with `families` unset), then flip
`families=["unix"]` **last**, once every unix cascade resolves to a BCL-backed
implementation. See §4 for the exact sequencing and the safe interim DX.

**Bottom line for the owner:** the cfg-flip DX *is* attainable and most of libc *does* map
cleanly, because the hard parts (sockets, readiness, files, alloc, time) are already built.
The work is (a) the fd-table spine, (b) a thread-local `errno` + exception→errno translation
(the single biggest net-new, honestly leaky at the tail), and (c) six dotnet PAL cascade arms
so the global cfg flip lands safely. The MVP that unblocks `mio`/`socket2` is ~1.5–2 weeks;
the full libc surface is phased across §5.

---

## 2. The categorized libc map

Per cluster: CLEAN / LEAKY / IMPOSSIBLE, each with the .NET BCL mapping and the existing-hook
reuse. **"% already covered"** is the share of the realistically-needed call surface in that
cluster already implemented as a CIL body (needing only re-packaging + fd-table + errno), as
opposed to net-new BCL work.

### 2.1 fd-generic I/O — the spine (`read`, `write`, `close`, `lseek`, `dup`, `dup2`, `fcntl`, `ioctl`, `readv`/`writev`, `isatty`, `pipe`)
**~65–70% covered.** Every per-cluster syscall resolves an int fd through the fd-table (§3) to
an opaque `GCHandle` + a fd-kind tag, then dispatches to an existing hook.

| Symbol | Verdict | .NET BCL mapping | Reuse |
|---|---|---|---|
| `write(fd,buf,len)` | **CLEAN** | branch on fd-kind | `rcl_dotnet_fs_write` (FILE) / `rcl_dotnet_net_send` (SOCKET) / `rcl_dotnet_write` (1/2) — all exist |
| `read(fd,buf,len)` | **CLEAN** | branch on fd-kind; socket `0`==EOF matches | `rcl_dotnet_fs_read` / `rcl_dotnet_net_recv` exist; **new** `read_stdin` over `Console.OpenStandardInput().Read(Span)` |
| `close(fd)` | **CLEAN** | dispose + free + drop table slot | `rcl_dotnet_fs_close` / `rcl_dotnet_net_close` exist |
| `lseek(fd,off,whence)` | **CLEAN** (file) | `FileStream.Seek`; `SEEK_SET/CUR/END`→`SeekOrigin 0/1/2`; socket→`ESPIPE` | `rcl_dotnet_fs_seek` exists |
| `isatty(fd)` | **CLEAN** | fd 0/1/2 → `Console.Is{Input,Output,Error}Redirected` (negated); else `ENOTTY` | **new** ~15-line hook |
| `ioctl(fd,FIONBIO,*)` | **CLEAN** (the one that matters) | socket → `Socket.Blocking` | `rcl_dotnet_net_set_nonblocking` exists |
| `fcntl(F_SETFL O_NONBLOCK / F_GETFL / F_*FD)` | **LEAKY** | `O_NONBLOCK`→`Socket.Blocking`; flags synthesized from table flags word; `FD_CLOEXEC` stored+echoed (no-op, no exec) | `rcl_dotnet_net_set_nonblocking` + table flags |
| `readv`/`writev` | **LEAKY** | **per-iovec loop** over single-buffer hooks (no managed `IList` — §3); not atomic; matches std's `default_*_vectored` | `fs_read`/`net_recv` (+ `_write`/`_send`) |
| `dup`/`dup2` | **LEAKY** | second table entry aliasing the same `GCHandle` + refcount; shared offset is faithful for *files*, not sockets; no separate OS description | table refcount |
| `ioctl` (≠FIONBIO), true `fcntl` flag round-trip, `F_GETLK`/`SETLK` | **IMPOSSIBLE** | no generic device-control / kernel-flag-word / advisory-lock surface (`FIONREAD`→`Socket.Available` is a clean special-case worth adding) | — |
| `pipe`/`pipe2` | **IMPOSSIBLE** (cleanly) | `System.IO.Pipes` are `Stream`s, not `Socket`s → cannot ride the per-fd `Socket.Poll` readiness loop; escape hatch = loopback socketpair (different semantics) | — |

### 2.2 Sockets + readiness — the most-complete cluster (D1 mio work validated it)
**VERY HIGH reuse (~14 of ~16 focus fns ≥80% covered).** Shim body = fd-table lookup → existing
hook → errno map.

| Symbol(s) | Verdict | .NET BCL mapping | Reuse |
|---|---|---|---|
| `connect`, `send`/`recv` (flags 0), `sendto`/`recvfrom`, `shutdown`, `getsockname`/`getpeername`, `accept`/`accept4`, `close`, `setsockopt(TCP_NODELAY)`, `FIONBIO` | **CLEAN** | `Socket.{Connect,Send,Receive,SendTo,ReceiveFrom,Shutdown,LocalEndPoint,RemoteEndPoint,Accept,Dispose,NoDelay,Blocking}`; `SHUT_RD/WR/RDWR`==`SocketShutdown 0/1/2` | `rcl_dotnet_net_{tcp_connect,udp_connect,send,recv,send_to,recv_from,shutdown,local_addr,peer_addr,accept,close,set_nonblocking,set_nodelay,nodelay}` — **all exist** |
| **readiness**: `epoll_create1`/`ctl`/`wait`, `poll`, `select`, `eventfd` (mio `Waker`) | **CLEAN** (ZERO new BCL) | **per-fd `Socket.Poll(micros, SelectMode)` loop**, not an array call (§3); `eventfd`→self-connected loopback UDP registered readable | `rcl_dotnet_socket_poll` (exists) + the proven `pal_mio` selector shape |
| `getaddrinfo`/`freeaddrinfo` | **CLEAN** | `Dns.GetHostAddresses`→`IPAddress[]` (a fixed array is index-iterable — no-IList does *not* bite); std parses numeric addrs so only fires on real names | **new** hook mirroring `fs_readdir_{count,get}` |
| `socket()`→fd (unbound), `SO_REUSEADDR`, `getsockopt`/`setsockopt` (RCVBUF/SNDBUF/KEEPALIVE/IP_TTL/IPV6_V6ONLY/...) | **LEAKY** | existing hooks act *atomically* (connect creates+connects) → need a **new** `socket_create` + split into create/bind-only/connect-on-existing/listen; each sockopt needs a `(level,optname)`→`SocketOption{Level,Name}` switch | partial: `build_socket()` already factored |
| `sockaddr`↔`(family,ip,port)` | **LEAKY** | hooks take a *decomposed* ABI (std decomposes `SocketAddr` itself); shim must parse opaque `sockaddr*` (`sa_family`, `sin_port` **network order** → `ntohs`/`htons`, `sin_addr`), hardcoding Linux layout | **new** helper |
| `send`/`recv` flags (`MSG_PEEK`/`DONTWAIT`/`WAITALL`/`NOSIGNAL`), `sendmsg`/`recvmsg` (data path) | **LEAKY** | `SocketFlags.Peek`; toggle `Blocking`; loop; no-op; vectored→per-iovec loop | `net_send`/`net_recv` |
| `SO_ERROR`, `recvmsg`/`sendmsg` **ancillary** (`cmsghdr`, `SCM_RIGHTS`), `socketpair(AF_UNIX)`, raw/packet sockets | **IMPOSSIBLE** | no pending-error model; no managed cmsg/fd-passing; no anonymous-pair primitive | — |

> **STATUS — I/O-driven tokio LANDED on the dotnet PAL (`pal_tokio_net`: TcpListener
> bind/accept + TcpStream connect/read/write loopback echo → "ping-tokio").** Four
> non-obvious runtime fixes beyond the libc decls, all in the readiness cluster:
> 1. **`eventfd` = self-readable loopback UDP socket** (`rcl_dotnet_eventfd` BCL hook +
>    `insert_eventfd` registers it `FD_KIND_SOCKET`). The mio `Waker` is now an
>    `OwnedFd` over this fd (the dotnet `waker/dotnet.rs` arm), read/writing the 8-byte
>    counter via `libc::{read,write}` — the stock `File`-backed eventfd waker still
>    can't be used (fs::File is not fd-backed; Option-B follow-up).
> 2. **`fcntl(F_DUPFD_CLOEXEC)`** now creates a real fd-table entry **sharing** the
>    original's handle (tokio's `Selector::try_clone()` dups the epoll fd; the clone
>    must see the same interest dict or `epoll_ctl` hits a null dict → was a crash).
> 3. **`epoll_wait` polls BOTH `SelectRead` and `SelectWrite`** and ORs them — mio
>    registers a stream with the full `EPOLLIN|EPOLLOUT` mask, and the old single-mode
>    derivation mis-polled the listener for *write*-readiness → `accept().await` hung.
> 4. **`EPOLLET` edge-trigger gate** (`RclEpollReg.last_ready`): report an
>    edge-triggered fd only on a rising readiness edge — else the never-drained
>    edge-triggered waker re-fires every sweep and busy-spins the reactor. The head-fd
>    blocking Poll is also capped (`POLL_CAP_MICROS`, no infinite single-fd block).
> tokio's `net::unix` (AF_UNIX + `unix::pipe`) is gated off for dotnet in the vendored
> tokio (`cfg_net_unix!` excludes `target_os="dotnet"`) — pipe is `IoSource<fs::File>`
> (the same fs::File wall); re-enabling it is the Option-B follow-up.

### 2.3 Filesystem + metadata
**~55–60% covered** (~14 of ~24 entry points reuse shipped `rcl_dotnet_fs_*`; proven E2E by
Phase-4 WF-fs, commit `461d38a`).

| Symbol(s) | Verdict | .NET BCL mapping | Reuse |
|---|---|---|---|
| `open`/`openat`/`creat`, `close`, `read`/`write` (+`pread`/`pwrite`/`lseek` as compositions), `fsync`/`fdatasync`, `mkdir`, `rmdir`, `unlink`/`unlinkat`, `rename`/`renameat`, `access(F_OK)`, `opendir`/`readdir`/`closedir` | **CLEAN** | `new FileStream((FileMode)…,(FileAccess)…)`, `Dispose`, `Read`/`Write`, `Flush`, `Directory.CreateDirectory`/`Delete`, `File.Delete`/`Move`, `File.Exists`, `Directory.GetFileSystemEntries` (per-index) | `rcl_dotnet_fs_{open,close,read,write,seek,flush,mkdir,rmdir,unlink,rename,exists,readdir_*}` — **all exist** |
| `ftruncate`/`truncate`, `getcwd`, `chdir`, `realpath`/`canonicalize`, `symlink`/`readlink` | **CLEAN** (small new hooks) | `FileStream.SetLength`; `Directory.GetCurrentDirectory`/`SetCurrentDirectory`; `Path.GetFullPath`; `File.CreateSymbolicLink`/`ResolveLinkTarget` (NET6+) | **new** thin hooks |
| `stat`/`fstat`/`lstat` (rich struct), `chmod`/`access(R/W/X)` | **LEAKY (mostly CLOSED)** | `FileInfo`: `Length`/`LastWriteTimeUtc`/`LastAccessTimeUtc`/`Attributes`→`S_IFDIR/S_IFREG/S_IFLNK`; `File.{Get,Set}UnixFileMode` (NET7+, **Unix-host only**; Windows-host→single ReadOnly bit). **.NET ctime=creation ≠ POSIX ctime=inode-change.** ✅ **CLOSED (B2 Piece 2/4):** `rcl_dotnet_fs_stat` is now the **wide 8-arg** stat — `(size, is_dir, mtime, atime, ctime, is_symlink)` each written via `StInd` (mtime/atime via `File.GetLast{Write,Access}TimeUtc`, ctime via `GetCreationTimeUtc` [creation, NOT inode-change — honest mismatch], is_symlink via `FileAttributes.ReparsePoint`). `std::fs` `modified()`/`accessed()`/`created()`/`file_type().is_symlink()` are all real. Remaining-leaky: `chmod`/`access(R/W/X)` mode bits (synthesized 0o644 on the PAL; Unix-host-best-effort only) | mostly done |
| `mkstemp`, `fdatasync` durability, `d_type`/`d_ino`, mkdir EEXIST semantics | **LEAKY** | `Path.GetTempFileName`+open (O_EXCL atomicity weakened); `Flush(true)` over-syncs; `DT_UNKNOWN` (spec-legal) or extra stat; shim must pre-check `Directory.Exists` for EEXIST | — |
| `st_ino`/`st_dev`/`st_nlink`/`st_blocks`, `link` (hard), `chown`, `umask` (kernel), `statvfs` inode counts, exotic `O_*` (`O_DIRECT`/`O_SYNC`/`O_TMPFILE`), real ctime | **IMPOSSIBLE** | no portable managed source of truth; `link` needs P/Invoke (breaks pure-BCL); `statvfs` space fields *are* derivable from `DriveInfo` (leaky-clean) but inode fields are not | — |

### 2.4 Threads + synchronization
**HIGH reuse on lifecycle/TLS-keys (~9 symbols already real C-ABI bodies), LOW on locks.**

| Symbol(s) | Verdict | .NET BCL mapping | Reuse |
|---|---|---|---|
| `pthread_create`, `pthread_join`, `pthread_detach`, `pthread_attr_{init,setstacksize,destroy}`, `pthread_self`, `pthread_setname_np`, `sched_yield`, `nanosleep`/`usleep`, `pthread_key_{create,delete}`, `pthread_setspecific` | **CLEAN** (mostly DONE) | `new Thread(ThreadStart)`/`Join`/`Yield`/`Sleep`; `ManagedThreadId`; `ConcurrentDictionary` TLS-key map | **already implemented** in `thread.rs` via `instert_threading`; `rcl_dotnet_thread_{spawn,join,yield,sleep}` duplicate it; `pthread_getspecific` = tiny new sibling |
| `pthread_mutex_*`, `pthread_cond_*`, `pthread_rwlock_*`, `sem_*`, `pthread_once` | **LEAKY** | `Monitor`/`SemaphoreSlim(1,1)` (Monitor is re-entrant ≠ `PTHREAD_MUTEX_NORMAL` → use `SemaphoreSlim`); `Monitor.Pulse/Wait`; `ReaderWriterLockSlim`; `SemaphoreSlim`; `Interlocked.CompareExchange`. Caller-allocated opaque structs → **address→handle `ConcurrentDictionary<nint,object>`** (mirrors `pthread_keys`) | **new**; reuses `gc_handle`/`interlocked` ClassRefs + the dict pattern |
| TLS correctness (process-global today), std's own `Mutex`/`Condvar` on `dotnet` | **IMPOSSIBLE as-is** (architectural) | `thread_local` PAL is single-thread-correct (one global map, not per-`ManagedThreadId`) → all threads share one value; std's `dotnet` target lands on `no_threads` which **panics on first real contention**. Fix = `[ThreadStatic]`/`ThreadLocal<T>` + point std's sync at the pthread backend | — |
| `pthread_cancel`/cleanup | **IMPOSSIBLE** | no cooperative-cancel-at-syscall; `Thread.Abort` removed in modern .NET | — |

> Note: `pthread_*` return the error code as their **return value** (0/`EBUSY`/`EINVAL`/`ETIMEDOUT`),
> they do **not** set `errno` — so no `__errno_location` interplay for them. `sem_*` *do* set errno.

### 2.5 Time + clock + sleep
**VERY HIGH reuse (~80–85% covered); ZERO new CIL hooks for the clean tier.** Lowest-effort cluster.

| Symbol(s) | Verdict | .NET BCL mapping | Reuse |
|---|---|---|---|
| `clock_gettime(MONOTONIC)`, `clock_getres(MONOTONIC)` | **CLEAN** | `Stopwatch.GetTimestamp`/`Frequency` → `timespec` via the 128-bit math already in the time PAL | `rcl_dotnet_instant_ticks`/`instant_freq` |
| `clock_gettime(REALTIME)`, `gettimeofday`, `time` | **CLEAN** | `DateTime.UtcNow.Ticks` rebased by the Unix-epoch constant | `rcl_dotnet_unix_ticks` |
| `nanosleep`/`clock_nanosleep`/`usleep`/`sleep` | **LEAKY** | `Thread.Sleep((int)ms)` — sub-ms precision lost (rounds up ≥1ms); never signal-interruptible so `rem`={0,0}, `EINTR` never fires | `rcl_dotnet_thread_sleep` |
| `clock_gettime(PROCESS/THREAD_CPUTIME)` | **LEAKY** | `Process.TotalProcessorTime` (approximate; single managed thread) | — |
| `timerfd_*`, `setitimer`/`alarm`, `clock_settime` | **IMPOSSIBLE** | a .NET `Timer` is not a `Socket` → can't join the per-fd Poll loop; no signal-driven timers; `clock_settime`→`EPERM` | — |

### 2.6 Memory
**Heap allocators ~90% covered; virtual-memory half ~0% and mostly unbuildable.**

| Symbol(s) | Verdict | .NET BCL mapping | Reuse |
|---|---|---|---|
| `malloc`, `free`, `calloc`, `realloc`, `reallocarray`, `aligned_alloc`, `memalign` | **CLEAN** | `NativeMemory.{Alloc,AlignedFree,AlignedRealloc,AllocZeroed}` — RAW `void*`, **no GCHandle/fd-table bookkeeping** (cleanest cluster) | `rcl_dotnet_alloc`/`free` + `__rust_alloc`/`_zeroed`/`_realloc` builtins exist; net-new = C-ABI names + edge cases (size==0, NULL realloc, overflow check) |
| `posix_memalign`, `malloc_usable_size`, anon-private `mmap`/`munmap`, `madvise` | **LEAKY** | `AlignedAlloc` + `StInd` out; `NativeMemory` has no usable-size query (side-table or return-requested); anon `mmap`→page-aligned arena (PROT ignored, partial munmap unsupported); `madvise`→no-op (correct for hints, wrong for `MADV_DONTNEED`-as-zero) | `rcl_dotnet_alloc`/`free` |
| `mprotect`, `mmap(MAP_FIXED)`/file-backed/shared, `mremap`, `brk`/`sbrk` | **IMPOSSIBLE** | no VM-protection / fixed-VA / program-break primitive on .NET; **do NOT fake `mprotect`/`brk` with return-0** — it silently removes guarantees | — |

### 2.7 Process + env + errno + misc
**~40% covered, concentrated in env + random.**

| Symbol(s) | Verdict | .NET BCL mapping | Reuse |
|---|---|---|---|
| `getenv`/`setenv`/`unsetenv`/`putenv` | **CLEAN** (DONE) | `Environment.{GetEnvironmentVariable,SetEnvironmentVariable}` | `rcl_dotnet_getenv`/`setenv`/`unsetenv` + `cotaskmem_free` — 100% reuse (only a `(ptr,len)`↔NUL-string adapter is new) |
| `getrandom`/`getentropy`/`arc4random_buf` | **CLEAN** (DONE) | `RandomNumberGenerator.Fill(Span)` (never blocks → flags ignorable) | `rcl_dotnet_random_fill` |
| `getpid`, `getpagesize`/`sysconf(_SC_PAGESIZE)`, `sysconf(_SC_NPROCESSORS)`, `gethostname`, `exit`/`_exit`, `abort`, `isatty` | **CLEAN** (new, 1:1) | `Environment.{ProcessId,SystemPageSize,ProcessorCount,MachineName,Exit}`; `FailFast`; `Console.Is*Redirected` | `available_parallelism` reused; abort reuses `Interned::abort` |
| `getppid`/`getuid`/`geteuid`/`getgid`/`getegid`, `environ` (char**), `errno`/`__errno_location`/`strerror`, signal `SIG_IGN`/`SIGINT`/`SIGTERM`, `uname`, `posix_spawn`/`waitpid` | **LEAKY** | creds = leaky **constants** (pick 1000 so privilege paths stay unprivileged); `environ` via per-index loop (no-IList); errno = thread-local cell + exception→errno (coarse); `PosixSignalRegistration` backs only SIGINT/TERM/HUP/QUIT; `uname` fillable from `RuntimeInformation`+`OSVersion`; `Process.Start`+pid-table (synthetic pid) | env marshalling primitives; `args` per-index *shape*; `LIBC_MODIFIES_ERRNO` lane exists |
| `fork`/`vfork`, `execve`, `kill(other_pid,*)`, `atexit`, `dlopen`(native `.so`), `backtrace` (real IPs), raw signal *delivery* | **IMPOSSIBLE** | cannot clone/replace the CLR image; no signal delivery path; no DWARF/native IPs; `dlopen` of a *managed* assembly is leaky-hard (reflection + the fragile `calli` fn-ptr path), of a native `.so` impossible | — |

---

## 3. The fd-table design + the no-IList IR constraint

### 3.1 The fd-table (the spine)
The whole tier hangs on one new process-global structure mapping **integer fd ⇄ opaque
`GCHandle`**. Today the net/fs hooks return a `*mut u8` that is a `GCHandle` `IntPtr` pinning a
managed `Socket`/`FileStream`; POSIX callers want an integer fd. The table is the seam between
"existing GCHandle hooks" and "POSIX integer-fd surface."

**Entry shape** (per fd):
- the `GCHandle` `IntPtr` (the opaque managed object),
- a **kind tag** `{STDIN/STDOUT/STDERR sentinel, FILE, SOCKET, EPOLL, EVENTFD, PIPE(future)}` so
  `read`/`write`/`close`/`lseek` dispatch correctly,
- a **flags word** (`O_NONBLOCK` shadow + `FD_CLOEXEC` shadow — .NET can't store these on the
  stream),
- a **refcount** (for `dup` alias correctness so the `GCHandle` frees once),
- for *directory* fds, the originating **path string** (so `openat`/`unlinkat` can resolve
  relative paths).

`fd 0/1/2` are pre-seeded to stdin/stdout/stderr sentinels. Lowest-free-int allocation (POSIX
`select`/`fd_set` requires it). Every *other* cluster registers its returned `GCHandle` into
the table and hands back the int fd — i.e. the net/fs hooks stay GCHandle-returning
internally; the libc-named wrappers (`socket`, `open`, `accept`) call `rcl_fdtable_insert`.

**Feasibility — it is clean.** The IR cannot *construct* a managed array/`IList` from Rust,
**but** a `MissingMethodPatcher` builtin can emit CIL that allocates/touches a static managed
field — proven: `thread.rs` allocs a `static "pthread_keys" ConcurrentDictionary` into a
`StaticFieldDesc`, the IL exporter emits `[ThreadStatic]` for TLS statics
([`il_exporter/mod.rs:186`](../cilly/src/ir/il_exporter/mod.rs)), and `asm.rs` has the
`is_tls` static path. Concrete mapping: a `static System.Collections.Generic.Dictionary<int,object>`
(or a doubling `object[]`) keyed by the int fd. New builtins — all <20-line CIL bodies over
`Dictionary.Add`/`get_Item`/`Remove`: `rcl_fdtable_insert(handle,kind)→i32`,
`rcl_fdtable_get(fd)→handle`, `rcl_fdtable_kind(fd)→i32`, `rcl_fdtable_remove(fd)`. Reuses the
`gc_handle`/`concurent_dictionary` ClassRefs verbatim. **~1 day.**

A *sibling* **pid-table** (int pid ⇄ `Process` `GCHandle`) of identical shape backs
`posix_spawn`/`waitpid`.

### 3.2 errno — the thread-local cell (the crux)
The dotnet PAL has **no `errno`**: [`dotnet_pal/sys/io/error/dotnet.rs`](../dotnet_pal/sys/io/error/dotnet.rs)
hardcodes `errno()==0`, every code → `ErrorKind::Uncategorized`, `error_string` → a fixed
message; the BCL signals failure by **throwing**. A POSIX fd shim is meaningless without errno
because `mio`/`socket2`/`-sys` crates branch on `EAGAIN`/`EWOULDBLOCK`/`EINTR`/`ECONNRESET`
(`EAGAIN` is load-bearing: mio's readiness loop treats it as "not ready, retry", not an error).

Required new work — and the honest hard part:
1. a thread-local `errno` static (the IR has `is_tls` statics; `__errno_location()` returns its
   address — and `LIBC_MODIFIES_ERRNO` + `__errno_location` already exist as a lane in
   [`libc_fns.rs:616`](../cilly/src/libc_fns.rs) / threaded through
   [`patch_missing_methods`](../cilly/src/bin/linker/main.rs:486)),
2. wrap each BCL call in `try/catch` (the IR has this — `insert_interop_try_catch` template),
3. translate the caught `SocketException.SocketErrorCode` / `IOException.HResult` /
   `FileNotFoundException` / `UnauthorizedAccessException` to a POSIX errno,
4. return `-1`.

**Leak, stated plainly:** the exception→errno table is lossy and incomplete. `SocketError.WouldBlock`→`EAGAIN`
and ~20 common codes map cleanly; the long tail (`EINTR` never occurs on .NET; `EPIPE` vs
`ECONNRESET` nuance; Windows-flavored `IOException` HResults even on Unix CoreCLR) is
approximate — curate the ~20 errnos crates actually branch on, default `EIO`. This is the
single biggest net-new piece (~3–5 days) and the main honesty caveat of the whole tier.
`EINTR`-never-fires is actually *fine*: `is_interrupted` stays false and `nanosleep` loops
just never loop.

**UPDATE (PAL-fidelity pass — partially CLOSED).** The fs-side exception→errno map is now
enriched and wired through the std `std::fs` arm:
- `rcl_errno_from_exception` (`cilly/src/ir/builtins/posix.rs`) gained `isinst` arms
  `FileNotFoundException`→`ENOENT`, `DirectoryNotFoundException`→`ENOENT`,
  `UnauthorizedAccessException`→`EACCES`, `PathTooLongException`→`ENAMETOOLONG`, tested
  **before** the general `IOException` tail (which still defaults to `EIO` — the honest
  remainder). New `ClassRef` helpers in `cilly/src/ir/class.rs`.
- The mutating fs hooks `rcl_dotnet_fs_{mkdir,rmdir,unlink,rename}` are now wrapped in
  `errno_wrapped` (set `errno`, return `-1` on a managed fault instead of unwinding);
  `rcl_dotnet_fs_open` catches itself and returns **null** (it returns a pointer, so it
  cannot use the `-1` wrapper). The std arm constructs errors with
  `io::Error::last_os_error()` (`rc()`, `File::open` in `dotnet_pal/sys/fs/dotnet.rs`), so
  callers now see precise `ErrorKind::{NotFound, PermissionDenied, …}` instead of
  `Uncategorized`/`Other`. The libc `open()` face (`open_errno_wrapped`) routes through the
  same rich mapper instead of its old blind `ENOENT`.
- HOST CAVEAT: the *mapping* (exception type → errno) is **host-agnostic** (the BCL throws the
  same types on Unix-host and Windows-host CoreCLR). The *meaning* of `EACCES` /
  `PermissionDenied` is **Unix-host-best-effort** only: a Windows-host CoreCLR has a single
  ReadOnly bit and throws `UnauthorizedAccessException` for ACL denials too, with no rwx/uid/gid
  model. `FilePermissions` mode bits stay synthesized (0o644) on the dotnet PAL, so a
  `set_permissions(0o000)` denial cannot be reproduced there — the `pal_fsmeta` probe gates
  that case to a real Unix host and SKIPs it honestly on the PAL.
- STILL OPEN (honest remainder): the general `IOException`/HResult tail still defaults to
  `EIO` (Windows-flavored HResults even on Unix CoreCLR make `EEXIST`/`EBUSY` HResult-sniffing
  leaky — better delivered by std-side pre-checks than guessed here). `EINTR` never fires.
  Proof: `cargo_tests/pal_fsmeta` (cases 7–9: `metadata`/`File::open`/`remove_file` of a
  missing path → `NotFound`, with `File::open`→`NotFound` being the headline; was `Other`
  pre-fix) + `cargo_tests/pal_libc` (libc `open(missing)` → `errno==ENOENT`).

### 3.3 The no-IList constraint → per-fd `Socket.Poll` loops
The backend IR **cannot construct a managed array / `IList<T>`** from Rust. This drives three
decisions (D1 lessons):

- **`Socket.Select(3 ILists)` is INFEASIBLE.** Readiness over a *set* of fds is a **per-fd
  loop calling `Socket.Poll(micros, SelectMode)`** — one socket at a time
  (`rcl_dotnet_socket_poll`, exists and proven by `pal_mio`). So `epoll_wait`/`poll`/`select`
  are all a per-fd Poll sweep, not a single array call: `epoll_create1`/`ctl` build a Rust-side
  fd-set in the table (no BCL); `epoll_wait`/`poll`/`select` iterate it one fd at a time, the
  head fd absorbing the timeout, writing results into the **caller's** `epoll_event[]` /
  `pollfd[]` / `fd_set` memory via pointer-walk (`LdInd`/`StInd`). `SelectMode 0=Read/1=Write/2=Error`.
  Cost: inherently **level-triggered** and **O(registered)** per sweep — the perf ceiling
  (§2.2 impossible).
- **`readv`/`writev`** cannot use the scatter-gather `IList` overloads → **per-iovec loop**
  over single-buffer hooks (the iovec array is read *from* Rust memory, so `(base,len)` pairs
  are pointer-walked, no managed array needed). Matches std's `default_*_vectored`.
- The **fd-table itself** is one managed `Dictionary`/array allocated+grown **entirely in CIL by
  a builtin** (the IR can't pass an `IList` from Rust, but a builtin emits the alloc) — so the
  table lives on the .NET side, keyed by the int fd Rust holds.

A *fixed* `PlatformArray` (`byte[]`/`string[]`/`IPAddress[]`) **can** be built and index-iterated
in IR (proven by `args`/`readdir`/`accept`), so `getaddrinfo` over `IPAddress[]` and the
`readdir` per-index pattern are fine — it is only `IList<T>` construction that is walled.

---

## 4. The cfg / target-family decision + the exact downstream flip

### 4.1 The finding: there is NO clean flip available *today* — the flip is the capstone
Setting `target_family="unix"` on the dotnet target flips bare `cfg(unix)` **and**
`cfg(target_family="unix")` (rustc emits `--cfg unix` iff `target.families` contains `"unix"`;
the two are equivalent). I read the live std cascades on the pinned nightly's `rust-src`. The
PAL is selected by `dev.sh` injecting `target_os="dotnet" => {…}` as the **first** arm of ~14
`cfg_select!` cascades. `cfg_select!` selects the **first** true arm, so those 14 are **safe**
even when `unix` also matches. The damage is in cascades that have **no dotnet arm** and would
mis-select the libc/unix arm. Confirmed against source:

| std module | first/winning arm (no dotnet arm) | what it wrongly pulls |
|---|---|---|
| `sys/process/mod.rs` | `target_family = "unix" => mod unix` | libc `fork`/`execvp`/`posix_spawn` |
| `sys/fd/mod.rs` | `any(target_family="unix", target_os="wasi") => mod unix` | `FileDesc` over libc `read`/`write`/`close` |
| `sys/pipe/mod.rs` | bare `unix => mod unix` | libc `pipe2` |
| `sys/sync/{mutex,rwlock,condvar,once}/mod.rs` | `any(target_family="unix", …) => mod pthread` (futex arm keys on an explicit `target_os` list, so it stays off — only pthread is the hazard) | libc `pthread_mutex_t`/`pthread_cond_t` |
| `sys/io/mod.rs` (isatty), `net/connection`, `net/hostname` | libc socket/isatty arms under `target_family="unix"` | libc socket surface |
| **`os/mod.rs:84`** `pub mod unix` gated `any(unix, doc)`; `pub mod fd` gated `any(unix,…)` | **the worst part** | `std::os::fd` (`OwnedFd::drop`→`libc::close`) + `std::os::unix` ext-traits (`MetadataExt` reads `libc::stat`; `CommandExt` touches the unix process PAL) keyed to unix-PAL inner shapes |

The deepest coupling: `std::os::fd::net` does
`sys::net::Socket::from_inner(FromInner::from_inner(OwnedFd::from_raw_fd(fd)))` and
`self.as_inner().socket().as_raw_fd()` — but the dotnet net `Socket`
([`dotnet_pal/sys/net/connection/dotnet.rs`](../dotnet_pal/sys/net/connection/dotnet.rs):406)
is `GCHandle`-backed and **implements none of** `FromInner<OwnedFd>`/`AsInner`/`.socket()`. So
**std itself fails to compile before any user crate.** This also confirms the D1 mio finding:
the vendored mio gates its `unix` arm on bare `unix` and its dotnet arm on
`target_os="dotnet"` (a comment notes "os=dotnet has no target_family"); flipping
`target_family="unix"` fires **both** arms → duplicate `mod`/`Selector`/`Event` → mio won't
compile either.

**Verdict: `target_os="dotnet"` + `target_family="unix"` is BLOCKED as-is — very invasive.**
The prerequisite is exactly the libc-shim tier being scoped: the int-fd⇄GCHandle fd-table at
the std layer, plus dotnet PAL arms for the six missing cascades, plus the net `Socket`
implementing the fd traits. The cfg flip **cannot precede** them.

### 4.2 The exact flip (the DX win) — delivered LAST
The downstream user's flip is, in the target spec `x86_64-unknown-dotnet.json`:

```jsonc
"families": ["unix"]    // turns on cfg(unix) + cfg(target_family="unix") globally
```

After which **unmodified** `mio`/`socket2` compile their existing `#[cfg(unix)]` arms straight
onto the shim — zero per-crate forks. **But it is only safe after** the libc-shim tier lands:
1. a dotnet PAL arm injected as the **first** `cfg_select!` arm for
   `sys::{process, fd, pipe, sync, io-isatty, net-hostname}` so the unix/libc arms never win;
2. the dotnet net `Socket` made to implement `FromInner<OwnedFd>`/`AsInner`/`.socket()` over the
   int-fd⇄GCHandle fd-table so `std::os::fd`/`std::os::unix` compile;
3. for mio specifically, the vendored dotnet arm **removed** and mio's own `#[cfg(unix)] mod unix`
   allowed to drive everything through the shim's POSIX-C-ABI symbols
   (`epoll_*`/`read`/`write`/`close` backed by the existing `rcl_dotnet_*` hooks + the per-fd
   `Socket.Poll` loop), deleting the fork.

The "one cfg flip" DX is **real for the consumer crate** (it just observes `cfg(unix)`); the
std side needs these dotnet arms baked into the project's patched std. That is a one-time
project investment, not a downstream burden.

#### Cap-2 attempt outcome (deferred — precise blocker)
The flip was attempted in Cap-2 and **reverted to keep main green**. Findings, for the next run:

* **Field name:** the JSON target-spec key is **`"target-family": ["unix"]`** (an array), NOT
  `"families"` — rustc's `TargetOptions` deserializer rejects `families` (`unknown field`).
* **The cargo wall is real and needs the flip:** upstream `mio`'s libc dep is
  `[target.'cfg(any(unix, target_os = "hermit", target_os = "wasi"))'.dependencies]`, so cargo only
  compiles libc into mio when `cfg(unix)` is true *at dep-resolution* — which only the spec's
  `target-family` provides (RUSTFLAGS/`RUSTC_WRAPPER` `--cfg unix` runs after resolution).
* **The blocker = a wide std cfg(unix) cascade** (beyond `os::unix`): with `target-family=unix`,
  std's own `sys::*` cascades switch to their unix arms and need dotnet arms or break —
  `sys/fs/mod.rs` (`with_native_path`, `OpenOptions::custom_flags`), `sys/paths/{mod,unix}.rs`
  (`current_exe`, `OsStr::{from_bytes,as_bytes}`, `OsString::from_vec`, `OsStringExt`),
  `sys/io/mod.rs` (`errno_location`/`set_errno`), `sys/process/mod.rs` (`getppid`), plus
  `sys/backtrace.rs` + `os/mod.rs:36,85` + `os/fd/raw.rs` + `backtrace/.../elf.rs` referencing
  `crate::os::unix` directly (so even *narrowing* `pub mod unix` off for dotnet breaks these
  std-internal refs). Several pieces (AF_UNIX `os::unix::net`, `MetadataExt`, the `OsStr`
  bytes-vs-wtf8 representation switch, the CStr `with_native_path` path model) have **no clean
  .NET mapping** and are a multi-module std-PAL effort.
* **What landed instead (green, additive):** the POSIX shim is now mio-runtime-complete —
  multi-fd epoll (`posix_epoll.rs`), connect→EINPROGRESS, accept peer-addr + nonblocking,
  SOCK_NONBLOCK socket(), and a latent `SocketException.SocketErrorCode` enum-return fix
  (`ClassRef::socket_error`). Proven by `pal_libc` over the new multi-fd epoll. The
  crate-scoped `RUSTC_WRAPPER` (`feasibility/rcc-rustc-wrapper.sh`) is committed but UNWIRED in
  `dev.sh` (it only helps under the flip). **Next run:** land the std cfg(unix) cascade arms
  (fs/paths/io/process + keep os::unix ON with dotnet arms or a narrower bytes-OsStr bridge),
  THEN re-apply the `target-family` flip + re-wire the wrapper + drop the mio fork together.

### 4.3 Fallbacks (least → most invasive)
1. **Status quo** — keep `target_os="dotnet"`, no family, per-crate `#[cfg(target_os="dotnet")] mod dotnet`
   via `patch.crates-io` vendoring (what mio does now). Clean for std, but it *is* the
   fork-per-crate burden the owner wants gone. Does not meet the goal.
2. **Per-module cfg(unix) injection without a global family** — inject a dotnet arm as the
   first `cfg_select!` arm into the remaining libc cascades + add the fd traits to the net
   `Socket`, leaving `target_family` unset (so `std::os::fd`/`os::unix` and the libc arms never
   auto-activate). Fixes std robustness but **not** the downstream DX (consumer crates still
   need their own `target_os="dotnet"` arm to reach a unix path without `cfg(unix)`).
3. **Scoped family via a build-std std patch** — set `families=["unix"]` in the spec **and**
   ship the libc-shim PAL arms (1)–(3). The **only** path that delivers the true DX
   (unmodified mio via one spec flag). This is the actual deliverable of the libc-shim tier.

**Recommended sequencing:** build the fd-table + the six dotnet PAL cascade arms + net `Socket`
fd-traits **first** (gate stays green with `families` unset), then flip `families=["unix"]`
**last** as the capstone once each unix cascade resolves to a dotnet-backed impl.

**Mio-only de-risk (do this early):** keep `families` unset, but in the still-vendored mio drop
the dotnet arm and let `#[cfg(unix)]` drive it by injecting `--cfg unix` *for the mio crate
alone* via RUSTFLAGS. Proves the shim symbols satisfy mio before committing to a global family flip.

### 4.4 Cap-2.5: near-unmodified mio via a crate-scoped wrapper — NO global flip (DONE, green)

Cap-2.5 delivers fallback **(2)** as a *working downstream DX* without the global `families` flip.
Headline: `cargo_tests/pal_mio` runs (`"hi-mio"` + `"== pal_mio done =="`) on **near-unmodified
upstream mio 1.2.1** through the POSIX shim — the whole D1 `sys/dotnet` fork (7 files) is deleted;
mio's own `#[cfg(unix)]` `selector/epoll.rs` + `net.rs`/`tcp.rs`/`udp.rs`/`io_source.rs` drive it,
byte-identical to crates.io. std stays **pristine** os=dotnet; the spec has **no** `target-family`.

**The DX (3 steps a consumer follows):**
1. `[patch.crates-io] mio = { path = "vendor/mio" }` pointing at the near-unmodified vendored mio.
2. `export RUSTC_WRAPPER=<repo>/feasibility/rcc-rustc-wrapper.sh` — the crate-scoped wrapper that
   adds `-A explicit_builtin_cfgs_in_flags --cfg unix --cfg target_os="linux"` to the **`mio` crate
   only** (keyed on `--crate-name=mio` + `--target` present). Every other crate (std/core/alloc/
   libc/the user crate) is passed through unchanged.
3. In the vendored mio `Cargo.toml`, the libc dep is **unconditional** (`[dependencies.libc]`, not
   `[target.'cfg(any(unix,...))'.dependencies.libc]`). cargo evaluates the target-gated form at
   dep-resolution against the spec (os=dotnet, no family ⇒ `cfg(unix)=false`) and would never
   compile libc into mio; the wrapper's `--cfg` runs *after* resolution and can't fix that. Un-gating
   compiles libc into mio with **no** families flip. (This is the one load-bearing Cargo line.)

**The honest mio patch size** (vs published crates.io mio 1.2.1 — verified with `diff -r`): it is
**NOT a literal 1-line diff**. It is `1 Cargo.toml line` + `~7 functional source lines` across two
files (`net/mod.rs` + `sys/unix/mod.rs`) + `1 new ~20-line file` (`sys/unix/waker/dotnet.rs`). All
three source touches are forced, real walls — documented inline as `// DOTNET PAL ARM (Cap-2.5)`:
* **waker arm** (`sys/unix/mod.rs`): the epoll selector re-exports `Waker` *raw* and needs
  `new(selector, token)` + `wake()`; the only File-free stock waker (`single_threaded.rs`) has only
  `new_unregistered()` (it is the *poll* selector's internal waker), and the stock `eventfd.rs` waker
  needs `std::fs::File: FromRawFd` which the dotnet `fs::File` (GCHandle/FileStream) is not. So a
  minimal `waker/dotnet.rs` supplies exactly that surface (pal_mio never builds a Waker, so it is
  never exercised). A literal-1-line mio needs fd-backed `fs::File` + a real loopback-socket eventfd
  — deferred.
* **uds gate** (`net/mod.rs` + `sys/unix/mod.rs`): `--cfg unix` activates mio's unix-DOMAIN-socket
  module, which needs `std::os::unix::net` (`UnixStream`/`SocketAddr`/`from_abstract_name`) — exactly
  the leaky AF_UNIX surface the libc-shim avoids and the dotnet std PAL does not provide. Gated
  `not(target_os="dotnet")`; pal_mio uses only TCP/UDP.

**libc-once-for-both — the reconciliation (CORRECTED from the §4.2 premise).** The §4.2 plan assumed
the wrapper would give the *single* libc build its real **linux/gnu module** (a "strict superset").
That premise is **false** and was abandoned: forcing libc's linux module while `target_os="dotnet"`
is *also* active makes libc 0.2's `new/` module tree inconsistent (the gnu-gated `pub use
net::route::*` + the `prelude!()` base-type imports `c_int`/... fail — verified `E0433`/`E0432`),
because `target_os` cannot be *unset* via `--cfg`, only added. The clean resolution: the wrapper
does **not** re-cfg libc at all. libc stays on its **dotnet arm** for *every* build, and that single
arm (`dotnet_pal/libc/dotnet.rs`) is the superset declaring the surface for **both** faces —
`std::os::fd`'s `close`/`fcntl`/`F_DUPFD*` **and** mio's `epoll_*`/`socket`/`bind`/`connect`/
`accept`/`accept4`/`setsockopt`/`getsockopt` + `epoll_event`/`sockaddr*`/`EPOLL*`/`AF_*`/`SOCK_*`/
`SO_*` consts. The function **bodies** are resolved at link time by the cilly POSIX shim
(`posix.rs`/`posix_symbols.rs`/`posix_epoll.rs`) by bare C-ABI symbol name, independent of which libc
Rust module is in scope. Struct/const layouts mirror Linux x86_64 (the shim hardcodes that numbering:
`epoll_event` `#[repr(C,packed)]` events:u32@0/data:u64@4 stride 12; `sockaddr_in` family@0/port@2
net-order/addr@4). So **libc-once-for-both = libc-once, period** — one dotnet arm, no multi-OS
module conflict, no symbol collision, no std breakage. The `inject_libc` gate is plain
`target_os="dotnet"`.

**WouldBlock determinism (the other Cap-2.5 fix).** pal_mio was pre-existing ~50% flaky: a
non-blocking `Socket.Receive` after `Socket.Poll` says ready can still race and throw
`SocketException(WouldBlock)`, which propagated uncaught out of `rcl_dotnet_net_recv`. Fixed in two
halves: (A) the backend wraps `rcl_dotnet_net_recv`/`_recv_from` in the POSIX errno catch
(`errno_wrapped`; WouldBlock→EAGAIN, real errno for other SocketExceptions) so a racing recv returns
`-1`/`errno`; (B) the std net PAL (`dotnet_pal/sys/net/connection/dotnet.rs` read/recv/recv_from)
surfaces that as `Err(ErrorKind::WouldBlock)` via `cvt`/`last_os_error` instead of a flat
`ErrorKind::Other`, so mio re-polls. pal_mio is now deterministic (≥5/5 green).

**Verified:** pal_mio ≥5/5 deterministic; pal_net/pal_libc/pal_fd/pal_soak/pal_tokio/pal_probe2 all
"done"; `::stable` gate 426/12 (no real regressions); `target-family`/`families` absent from
`x86_64-unknown-dotnet.json` (no global flip; std unchanged).

**Deferred to a literal-1-line mio / the global-flip end-state (4.2 fallback (3)):** fd-backed
`std::fs::File` (unblocks the stock eventfd waker → drop `waker/dotnet.rs`) + a real
loopback-socket eventfd (tokio's reactor constructs a Waker); a `std::os::unix::net` PAL (unblocks
mio's `uds` → drop the uds gates); the full `families=["unix"]` flip for the broader os::unix DX.

### 4.5 B1: mio CONVERGENCE under the committed global flip (DONE, green)

Package A flipped `target-family=["unix"]` GLOBALLY on `x86_64-unknown-dotnet.json` (committed,
e0a8a39). B1 collapses the Cap-2.5 mio scaffolding onto that flip. **The crate-scoped RUSTC_WRAPPER
is DELETED, and the vendored mio Cargo.toml is byte-identical to crates.io mio 1.2.1.** Two Cap-2.5
crutches dissolve under the flip:

* **The wrapper's `--cfg unix` is redundant** — mio's `#[cfg(unix)]` sys arm now activates straight
  from `--target` (the spec's `target-family`).
* **The Cargo libc un-gate is redundant** — cargo evaluates upstream mio's
  `[target.'cfg(any(unix, hermit, wasi))'.dependencies.libc]` as TRUE at dep-resolution (the spec
  carries `target-family`), so libc is pulled into mio with **zero** Cargo edits.

The wrapper *also* used to set `--cfg target_os="linux"` so mio's backend-selection cascades (which
key on `target_os`, not `unix`) would pick the epoll/accept4/SOCK_NONBLOCK linux paths. With the
wrapper gone, those are replaced by a handful of `target_os="dotnet"` cfg arms baked into the
vendored mio. **Final mio diff vs crates.io 1.2.1 — ZERO Cargo edits + ~10 functional src lines
across 4 files + 1 new ~24-line file:**

| file | change | why |
|---|---|---|
| `sys/unix/mod.rs` | `#[cfg_attr(target_os="dotnet", path="selector/epoll.rs")]` selector arm | mio selects its readiness backend by `target_os`; dotnet is not in {linux,android,illumos,redox}, so no arm matched → `mod selector` failed to resolve. |
| `sys/unix/mod.rs` | `target_os="dotnet"` waker arm → `waker/dotnet.rs` | **the irreducible blocker:** the stock eventfd waker needs `std::fs::File: FromRawFd`; the dotnet `fs::File` is GCHandle/FileStream-backed, not fd-backed (deferred). |
| `sys/unix/mod.rs` + `net/mod.rs` | uds gated `not(target_os="dotnet")` (3 lines) | mio's uds needs `std::os::unix::net::SocketAddr::from_abstract_name` — abstract-namespace AF_UNIX, impossible on stock CoreCLR. |
| `sys/unix/net.rs` | `target_os="dotnet"` in the SOCK_NONBLOCK\|SOCK_CLOEXEC list (1 line) | atomic non-blocking socket creation in `new_socket`; the shim honours both flags. |
| `sys/unix/tcp.rs` | `target_os="dotnet"` in the accept4 list (1 line) | atomic non-blocking accept; the shim implements `accept4`. |
| `sys/unix/waker/dotnet.rs` | NEW ~24-line file | the minimal `Waker{new,wake}` surface the epoll selector re-exports raw. |

**The irreducible remainder** (what keeps mio from being literal zero-patch): (1) the
`target_os`-keyed backend-selection arms (mio simply has no `target_os="dotnet"` concept — these
will always be needed unless the project upstreams a dotnet arm to mio); (2) the waker shim, which
disappears the day the dotnet `std::fs::File` is fd-backed (then the stock eventfd waker works); (3)
the uds gate-off, which disappears with a real `std::os::unix::net` AF_UNIX PAL. Verified: pal_mio
≥4/4 deterministic (`"hi-mio"` echo + `"== pal_mio done =="`, exit 0) under the flip with no wrapper.

---

## 5. Phased implementation plan

Each phase lists the symbols, the reuse, and a **verifiable probe** (a `cargo_tests/` crate that
runs on the real dotnet PAL, the project's acceptance pattern).

### Phase 0 — shared infrastructure (prerequisite for everything; ~1 week)
- **fd-table builtins** (`rcl_fdtable_{insert,get,kind,remove}`) + entry struct (kind/flags/refcount/path)
  + pre-seed 0/1/2. *Reuse:* `gc_handle`/`concurent_dictionary` ClassRefs, the `pthread_keys`
  static pattern. (~1–1.5d, clean)
- **thread-local `errno`** (`is_tls` static) + `__errno_location` + the `try/catch`+exception→errno
  translation switch (`insert_interop_try_catch` template; `LIBC_MODIFIES_ERRNO` lane exists).
  (~3–5d, **leaky** — the dominant honest cost)
- **POSIX-symbol registration**: register the libc-named wrappers as `MissingMethodPatcher`
  overrides (the patcher matches the demangled name at
  [`asm.rs:1120`](../cilly/src/ir/asm.rs) — same mechanism `insert_dotnet_pal` uses).
- **Probe:** a unit crate that opens a file, writes, `lseek`s, reads back, and forces an
  `ENOENT` to assert `errno`/`-1` round-trips.

### Phase 1 — the mio/tokio-net unblocking cluster (the headline; ~1.5–2 weeks incl. Phase 0)
- **fd-generic I/O:** `read`, `write`, `close`, `lseek`, `isatty`, `ioctl(FIONBIO)`,
  `fcntl(F_SETFL O_NONBLOCK/F_GETFL/F_*FD)`. *Reuse:* `fs_{read,write,close,seek}` /
  `net_{send,recv,close,set_nonblocking}` — backends exist; net-new = dispatch wrappers + stdin
  read + isatty.
- **Sockets:** `socket`(create), `bind`, `listen`, `connect`, `accept`/`accept4`, `send`/`recv`,
  `sendto`/`recvfrom`, `shutdown`, `getsockname`/`getpeername`, `setsockopt(TCP_NODELAY,SO_REUSEADDR)`,
  `getaddrinfo`/`freeaddrinfo`, `sockaddr`↔`(family,ip,port)` helper. *Reuse:* the 14 `net_*`
  hooks ≈ verbatim; net-new = ~3–4 lifecycle hooks + sockaddr parse + getaddrinfo.
- **Readiness:** `epoll_create1`/`ctl`/`wait`, `poll`, `select`, `eventfd`. *Reuse:* `socket_poll`
  + the proven `pal_mio` selector shape — **ZERO new BCL**; per-fd Poll loops over caller memory.
- **`errno` `EAGAIN`** on non-blocking sockets (`SocketError.WouldBlock`→`EAGAIN`).
- **Probe:** the `pal_mio` echo+`Waker` workload re-run with the **unmodified** mio unix arm + the
  mio-only `--cfg unix` de-risk (§4.3), then ultimately the `families=["unix"]` capstone.

### Phase 2 — fuller file I/O + metadata
- `open`/`openat`/`creat` (O_* flag translation), `pread`/`pwrite`, `readv`/`writev`,
  `mkdir`/`rmdir`/`unlink`/`rename` (+ `*at`), `access(F_OK)`, `opendir`/`readdir`/`closedir`,
  `ftruncate`/`truncate`, `getcwd`/`chdir`, `realpath`, `symlink`/`readlink`, a **wide `stat`**
  (scalar `StInd` writes), `dup`/`dup2` (alias+refcount). *Reuse:* `fs_*` hooks for ~14 of these;
  net-new = wide stat, set_len, getcwd/chdir/realpath/symlink, dup refcount.
- **Probe:** a crate doing canonicalize + readdir + stat + truncate + a `dup`'d fd write.

### Phase 3 — threads + synchronization (the real-threading gate)
- `pthread_getspecific`, proper `pthread_self`, `sched_yield`/`nanosleep` wrappers (lifecycle is
  already done), then the lock primitives `pthread_mutex_*`/`cond_*`/`rwlock_*`/`sem_*`/`once`
  via the address→handle dict. **Then the correctness gate:** `[ThreadStatic]` TLS + point std's
  `dotnet` sync `cfg_select!` arm at the pthread backend (without which multi-threaded std code
  aborts on the `no_threads` mutex assert).
- *Reuse:* `instert_threading` lifecycle + `gc_handle`/`interlocked` ClassRefs + the dict pattern;
  net-new ClassRefs: `Monitor`, `SemaphoreSlim`, `ReaderWriterLockSlim`.
- **Probe:** two real spawned threads contending on a `std::sync::Mutex` + a `Condvar` handoff.

### Phase 4 — time + clock
- `clock_gettime(REALTIME/MONOTONIC)`, `gettimeofday`, `time`, `clock_getres`, then
  `nanosleep`/`usleep`/`sleep` (ms-granular caveat). *Reuse:* `instant_ticks`/`instant_freq`/
  `unix_ticks`/`thread_sleep` — **ZERO new CIL hooks** for the clean tier; net-new = symbol
  aliases + `timespec`/`timeval` struct writes. (Lowest effort — can be slotted earlier if a
  consumer needs it.)
- **Probe:** monotonic-elapsed + wall-clock + a sub-second sleep assertion.

### Phase 5 — memory + process/env + misc
- **Memory:** `malloc`/`free`/`calloc`/`realloc`/`reallocarray`/`aligned_alloc`/`memalign`
  (reuse `NativeMemory` allocators ≈ verbatim, ~1d), then `posix_memalign`/`malloc_usable_size`
  (side-table) + anon `mmap`/`munmap` arena.
- **Process/env/misc:** env (DONE), random (DONE), `getpid`/`getpagesize`/`sysconf`/`gethostname`/
  `exit`/`abort`/`isatty`, creds-constants, `environ` (per-index), `uname`, signal `SIG_IGN`/
  SIGINT/TERM via `PosixSignalRegistration`, `posix_spawn`/`waitpid` via `Process`+pid-table.
- **Probe:** `getpid`/`gethostname`/`getrandom` smoke + a `posix_spawn` of `echo` with redirected
  stdout read back through the fd-table.

---

## 6. NOT cleanly mappable (the honest ceiling)

Surface these as `ENOSYS`/`-1`/documented-stub — **never** silently fake them:

- **`fork`/`vfork`** — cannot clone a multi-threaded JIT+GC managed runtime in-process; no BCL
  primitive. The canonical hard wall. Mitigation: steer crates to the `posix_spawn` arm.
- **`execve`/`execv`/`execvp`** (in-place image replace) — `.NET` cannot replace the running CLR
  image; `Process.Start`+`Environment.Exit` does **not** preserve pid/fds/parent — a semantic
  lie, offered only as an explicit labeled fallback.
- **`pipe`/`pipe2`** with real readiness — `System.IO.Pipes` are `Stream`s, not `Socket`s, so
  they cannot ride the per-fd `Socket.Poll` loop; only a loopback socketpair approximates the
  waker/self-pipe use, with different buffering/atomicity semantics.
- **`mmap(MAP_FIXED)` / file-backed / `MAP_SHARED` anon / `mremap`** — no fixed-VA placement, no
  raw-page pointer from `MemoryMappedFile` that fits the model, no shared-anon primitive.
- **`mprotect`** guard pages / W^X — no VM-protection API; returning 0 would silently turn guard
  pages into no-ops (dangerous), so it must fail.
- **`brk`/`sbrk`** — no program break / contiguous data segment on a GC + `NativeMemory` heap.
- **Raw signal *delivery*** (`sigaction` handlers firing, `kill(other_pid,*)` for arbitrary
  signals) — no .NET delivery path beyond `PosixSignalRegistration`'s four signals;
  `SIGPIPE`-ignore is a clean no-op (sockets never raise it), the rest are store-and-succeed
  stubs that never fire.
- **`/proc`, `uname`-exact, `st_ino`/`st_dev`/`st_nlink`, hard `link()`, `chown`, `umask` kernel
  semantics, full `statvfs`** — no managed source of truth (some are *partially* fillable —
  `uname` from `RuntimeInformation`, `statvfs` space from `DriveInfo` — but never wholly).
- **`dlopen`** of a native `.so` (impossible) / of a managed assembly (leaky-hard: reflection +
  the historically fragile `calli` fn-ptr path); **`backtrace`** with real instruction pointers
  (no DWARF, managed frames have no stable native IPs).
- **`atexit`** as a faithful C fn-ptr registry tied to all exit paths (a shim registry fires on
  `exit()` only, not `_exit`/`abort`/signal-exit/host-kill).
- **Edge-triggered (`EPOLLET`) / `O(1)` epoll** — the per-fd Poll loop is inherently
  level-triggered sampling and `O(registered)` per sweep; correct but a perf wall at thousands of fds.

---

## 7. How this complements (does not replace) the idiomatic BCL std PAL

The libc shim and the existing dotnet std PAL are **two faces of the same engine, for two
audiences** — they share the BCL backends and must share the fd-table.

- **The idiomatic dotnet PAL** ([`dotnet_pal/sys/*`](../dotnet_pal/sys), selected by
  `target_os="dotnet"`) is what *Rust std itself* uses. It keeps managed objects **opaque**
  (`GCHandle` as `*mut u8`), decomposes high-level types (e.g. `SocketAddr`) before calling the
  BCL, and is the clean, high-fidelity path for `std::fs`/`std::net`/`std::thread`. It is the
  preferred surface and is **not** being replaced.
- **The libc shim** is the **C-ABI compatibility layer underneath** it, for the long tail of
  crates that bypass std and call POSIX directly (`mio`, `socket2`, `-sys` crates, vendored C).
  It re-exposes the *same* CIL bodies under bare POSIX names, threads integer fds through the
  fd-table to the same `GCHandle` hooks, and adds the `errno` contract those callers require.

The single load-bearing shared structure is the **fd-table**: when `families=["unix"]` is set,
std's own `dotnet` net `Socket` must implement `AsRawFd`/`FromRawFd` over the **same**
int-fd⇄GCHandle table the shim uses — otherwise `std::os::fd` consumers and the shim disagree on
what an int fd *means*. Build the fd-table once, in the backend; both faces consult it. This is
why the shim is mostly re-packaging: it does not duplicate the BCL bindings, it gives the
existing ones a second, POSIX-shaped doorway — and the std PAL gains the fd-table bridge it
needs to make the global cfg flip safe. A `.NET` bugfix in a shared hook fixes both; the shim
adds breadth (the cfg-flip DX) without forking the depth std already has.
