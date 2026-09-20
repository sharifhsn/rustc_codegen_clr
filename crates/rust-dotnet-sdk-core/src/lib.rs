pub mod host {
    use std::env;
    use std::path::PathBuf;

    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    pub struct HostFacts {
        pub os: &'static str,
        pub arch: &'static str,
        pub dylib_ext: &'static str,
        pub exe_ext: &'static str,
        pub host_rid: &'static str,
    }

    impl HostFacts {
        pub fn for_test(os: &'static str) -> Self {
            let arch = if os == "macos" { "aarch64" } else { "x86_64" };
            Self::for_target(os, arch).unwrap_or_else(|| Self::unsupported(os, arch))
        }

        /// Return facts only for the exact host triples carried by the public SDK.
        pub fn for_target(os: &'static str, arch: &'static str) -> Option<Self> {
            let (dylib_ext, exe_ext, host_rid) = match (os, arch) {
                ("linux", "x86_64") => ("so", "", "linux-x64"),
                ("macos", "aarch64") => ("dylib", "", "osx-arm64"),
                ("windows", "x86_64") => ("dll", ".exe", "win-x64"),
                _ => return None,
            };
            Some(Self {
                os,
                arch,
                dylib_ext,
                exe_ext,
                host_rid,
            })
        }

        fn unsupported(os: &'static str, arch: &'static str) -> Self {
            let (dylib_ext, exe_ext) = match os {
                "macos" => ("dylib", ""),
                "windows" => ("dll", ".exe"),
                _ => ("so", ""),
            };
            Self {
                os,
                arch,
                dylib_ext,
                exe_ext,
                host_rid: "unsupported",
            }
        }

        pub fn is_supported(self) -> bool {
            Self::for_target(self.os, self.arch).is_some()
        }

        pub fn detect() -> Self {
            Self::for_target(env::consts::OS, env::consts::ARCH)
                .unwrap_or_else(|| Self::unsupported(env::consts::OS, env::consts::ARCH))
        }
        pub fn backend_dylib_name(&self) -> String {
            if self.os == "windows" {
                format!("rustc_codegen_clr.{}", self.dylib_ext)
            } else {
                format!("librustc_codegen_clr.{}", self.dylib_ext)
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::HostFacts;

        #[test]
        fn accepts_only_public_release_hosts() {
            assert_eq!(
                HostFacts::for_target("linux", "x86_64").unwrap().host_rid,
                "linux-x64"
            );
            assert_eq!(
                HostFacts::for_target("macos", "aarch64").unwrap().host_rid,
                "osx-arm64"
            );
            assert_eq!(
                HostFacts::for_target("windows", "x86_64").unwrap().host_rid,
                "win-x64"
            );
            for (os, arch) in [
                ("linux", "aarch64"),
                ("macos", "x86_64"),
                ("windows", "aarch64"),
                ("freebsd", "x86_64"),
            ] {
                assert!(HostFacts::for_target(os, arch).is_none(), "{os}-{arch}");
            }
        }
    }
    pub fn home_dir() -> Option<PathBuf> {
        env::var_os("HOME")
            .or_else(|| env::var_os("USERPROFILE"))
            .map(PathBuf::from)
    }
}

/// Cross-product process resolution. All SDK components must honor the host selected by the
/// .NET SDK (`DOTNET_HOST_PATH`) before consulting user-local or ambient PATH installations.
pub mod dotnet {
    use std::path::{Component, PathBuf};
    use std::process::Command;

    use anyhow::{Context as _, Result, bail};
    use sha2::{Digest, Sha256};

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct DotnetTool {
        executable: PathBuf,
    }

    impl DotnetTool {
        pub fn from_executable(executable: PathBuf) -> Self {
            Self {
                executable: resolve_executable(executable),
            }
        }

        pub fn resolve(tfm: Option<&str>) -> Self {
            if let Some(host) =
                std::env::var_os("DOTNET_HOST_PATH").filter(|value| !value.is_empty())
            {
                return Self::from_executable(PathBuf::from(host));
            }
            let requested_major = tfm
                .and_then(|tfm| tfm.strip_prefix("net"))
                .and_then(|version| version.split('.').next());
            if let (Some(home), Some(major)) = (
                std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")),
                requested_major,
            ) {
                let root = PathBuf::from(home).join(".dotnet");
                let host = root.join(if cfg!(windows) {
                    "dotnet.exe"
                } else {
                    "dotnet"
                });
                let shared = root.join("shared/Microsoft.NETCore.App");
                let has_runtime = std::fs::read_dir(shared).is_ok_and(|entries| {
                    entries.flatten().any(|entry| {
                        entry
                            .file_name()
                            .to_str()
                            .is_some_and(|version| version.starts_with(&format!("{major}.")))
                    })
                });
                if host.is_file() && has_runtime {
                    return Self::from_executable(host);
                }
            }
            Self::from_executable(PathBuf::from(if cfg!(windows) {
                "dotnet.exe"
            } else {
                "dotnet"
            }))
        }

        pub fn command(&self) -> Command {
            Command::new(&self.executable)
        }

        pub fn executable(&self) -> &std::path::Path {
            &self.executable
        }

        pub fn identity(&self) -> Result<String> {
            let executable = std::fs::canonicalize(&self.executable).with_context(|| {
                format!("resolving selected .NET host {}", self.executable.display())
            })?;
            let executable_before = crate::safe_fs::read_regular_nofollow(&executable)
                .with_context(|| format!("fingerprinting .NET host {}", executable.display()))?;
            let version = self.query("--version")?;
            let info = self.query("--info")?;
            let sdk_payload = selected_sdk_base(&info)
                .map(fingerprint_selected_sdk)
                .transpose()?
                .unwrap_or_else(|| {
                    format!(
                        "sdk_info_sha256={:x}",
                        Sha256::digest(normalized_output(&info).as_bytes())
                    )
                });
            let executable_after = crate::safe_fs::read_regular_nofollow(&executable)
                .with_context(|| format!("rechecking .NET host {}", executable.display()))?;
            if executable_before != executable_after {
                bail!("selected .NET host changed while its SDK identity was queried");
            }
            Ok(format!(
                "cargo-dotnet-dotnet-tool-v2\nexecutable={}\nexecutable_sha256={:x}\nversion={}\n{}",
                executable.display(),
                Sha256::digest(&executable_before),
                normalized_output(&version),
                sdk_payload,
            ))
        }

        fn query(&self, argument: &str) -> Result<String> {
            let output = self
                .command()
                .env("DOTNET_NOLOGO", "1")
                .env("DOTNET_CLI_UI_LANGUAGE", "en-US")
                .arg(argument)
                .output()
                .with_context(|| format!("querying {} {argument}", self.executable.display()))?;
            if !output.status.success() {
                bail!("`{} {argument}` failed", self.executable.display());
            }
            String::from_utf8(output.stdout)
                .with_context(|| format!("dotnet {argument} returned non-UTF-8 output"))
        }
    }

    fn normalized_output(value: &str) -> String {
        value.replace("\r\n", "\n").trim().to_string()
    }

    fn selected_sdk_base(info: &str) -> Option<PathBuf> {
        info.lines().find_map(|line| {
            line.trim()
                .strip_prefix("Base Path:")
                .map(str::trim)
                .filter(|path| !path.is_empty())
                .map(PathBuf::from)
        })
    }

    fn fingerprint_selected_sdk(base: PathBuf) -> Result<String> {
        let base = std::fs::canonicalize(&base)
            .with_context(|| format!("resolving selected .NET SDK base {}", base.display()))?;
        let metadata = std::fs::symlink_metadata(&base)?;
        if crate::safe_fs::metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() {
            bail!(
                "selected .NET SDK base is not a regular directory: {}",
                base.display()
            );
        }
        let mut hash = Sha256::new();
        let mut files = 0_u64;
        for relative in [
            "dotnet.dll",
            "MSBuild.dll",
            "Sdks/Microsoft.NET.Sdk/Sdk/Sdk.props",
        ] {
            let relative = std::path::Path::new(relative);
            let candidate = base.join(relative);
            match std::fs::symlink_metadata(&candidate) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
                Ok(metadata)
                    if crate::safe_fs::metadata_is_link_or_reparse(&metadata)
                        || !metadata.is_file() =>
                {
                    bail!(
                        "selected .NET SDK identity anchor is not a regular file: {}",
                        candidate.display()
                    );
                }
                Ok(_) => {}
            }
            let (_, bytes) = crate::safe_fs::snapshot_regular_within(&base, relative)?;
            hash.update((relative.as_os_str().as_encoded_bytes().len() as u64).to_le_bytes());
            hash.update(relative.as_os_str().as_encoded_bytes());
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
            files += 1;
        }
        if files == 0 {
            bail!(
                "selected .NET SDK base has none of its identity anchors: {}",
                base.display()
            );
        }
        Ok(format!(
            "sdk_base={}\nsdk_anchor_count={files}\nsdk_payload_sha256={:x}",
            base.display(),
            hash.finalize()
        ))
    }

    fn resolve_executable(executable: PathBuf) -> PathBuf {
        resolve_executable_with_path(executable, std::env::var_os("PATH"))
    }

    fn resolve_executable_with_path(
        executable: PathBuf,
        path: Option<std::ffi::OsString>,
    ) -> PathBuf {
        if executable.is_absolute()
            || executable
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
            || executable.components().count() > 1
        {
            return std::fs::canonicalize(&executable).unwrap_or(executable);
        }
        path.into_iter()
            .flat_map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
            .map(|directory| directory.join(&executable))
            .find(|candidate| candidate.is_file())
            .and_then(|candidate| std::fs::canonicalize(candidate).ok())
            .unwrap_or(executable)
    }

    #[cfg(all(test, unix))]
    mod tests {
        use super::*;

        #[test]
        fn identity_canonicalizes_aliases_and_hashes_same_version_host_bytes() {
            use std::os::unix::fs::{PermissionsExt as _, symlink};

            let temp = tempfile::tempdir().unwrap();
            let first = temp.path().join("dotnet-a");
            let second = temp.path().join("dotnet-b");
            std::fs::write(&first, b"#!/bin/sh\n# first\necho 10.0.1\n").unwrap();
            std::fs::write(&second, b"#!/bin/sh\n# second\necho 10.0.1\n").unwrap();
            for path in [&first, &second] {
                let mut permissions = std::fs::metadata(path).unwrap().permissions();
                permissions.set_mode(0o755);
                std::fs::set_permissions(path, permissions).unwrap();
            }
            let alias = temp.path().join("dotnet-alias");
            symlink(&first, &alias).unwrap();
            let first_identity = DotnetTool::from_executable(first).identity().unwrap();
            assert_eq!(
                first_identity,
                DotnetTool::from_executable(alias).identity().unwrap()
            );
            assert_ne!(
                first_identity,
                DotnetTool::from_executable(second).identity().unwrap()
            );
        }

        #[test]
        fn identity_changes_when_selected_sdk_payload_changes() {
            use std::os::unix::fs::PermissionsExt as _;

            let temp = tempfile::tempdir().unwrap();
            let sdk = temp.path().join("sdk");
            std::fs::create_dir(&sdk).unwrap();
            std::fs::write(sdk.join("dotnet.dll"), b"sdk-one").unwrap();
            let host = temp.path().join("dotnet");
            std::fs::write(
                &host,
                format!(
                    "#!/bin/sh\ncase \"$1\" in\n  --version) echo 10.0.1 ;;\n  --info) echo ' Base Path: {}' ;;\n  --list-sdks) echo '10.0.1 [{}]' ;;\nesac\n",
                    sdk.display(),
                    temp.path().display(),
                ),
            )
            .unwrap();
            let mut permissions = std::fs::metadata(&host).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&host, permissions).unwrap();
            let tool = DotnetTool::from_executable(host);
            let first = tool.identity().unwrap();
            std::fs::write(sdk.join("dotnet.dll"), b"sdk-two").unwrap();
            let second = tool.identity().unwrap();
            assert_ne!(first, second);
        }

        #[test]
        fn path_resolution_binds_same_version_hosts_to_the_selected_executable() {
            use std::os::unix::fs::PermissionsExt as _;

            let temp = tempfile::tempdir().unwrap();
            let first_dir = temp.path().join("first");
            let second_dir = temp.path().join("second");
            std::fs::create_dir(&first_dir).unwrap();
            std::fs::create_dir(&second_dir).unwrap();
            for (directory, marker) in [(&first_dir, "first"), (&second_dir, "second")] {
                let host = directory.join("dotnet");
                std::fs::write(&host, format!("#!/bin/sh\n# {marker}\necho 10.0.1\n")).unwrap();
                let mut permissions = std::fs::metadata(&host).unwrap().permissions();
                permissions.set_mode(0o755);
                std::fs::set_permissions(host, permissions).unwrap();
            }
            let first_path = std::env::join_paths([&first_dir, &second_dir]).unwrap();
            let second_path = std::env::join_paths([&second_dir, &first_dir]).unwrap();
            let first = DotnetTool::from_executable(resolve_executable_with_path(
                PathBuf::from("dotnet"),
                Some(first_path),
            ));
            let second = DotnetTool::from_executable(resolve_executable_with_path(
                PathBuf::from("dotnet"),
                Some(second_path),
            ));
            assert_ne!(first.executable(), second.executable());
            assert_ne!(first.identity().unwrap(), second.identity().unwrap());
        }
    }
}

/// Handle-based reads for attacker-influenceable cache and staging leaves.
pub mod safe_fs {
    #[cfg(unix)]
    use std::ffi::OsString;
    #[cfg(windows)]
    use std::ffi::{OsStr, OsString};
    use std::fs::{File, OpenOptions};
    use std::io::{Read as _, Write as _};
    use std::path::{Component, Path, PathBuf};

    use anyhow::{Context as _, Result, bail};

    pub fn open_regular_nofollow(path: &Path) -> Result<File> {
        open_regular_nofollow_with(path, false)
    }

    pub fn open_regular_nofollow_read_write(path: &Path) -> Result<File> {
        open_regular_nofollow_with(path, true)
    }

    pub fn create_or_open_regular_nofollow(path: &Path) -> Result<File> {
        match OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(path)
        {
            Ok(file) => Ok(file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                open_regular_nofollow_read_write(path)
            }
            Err(error) => {
                Err(error).with_context(|| format!("creating regular file {}", path.display()))
            }
        }
    }

    /// One opened directory authority retained for an entire recursive operation.
    ///
    /// A caller must not enumerate a pathname and then open each leaf relative to a newly-resolved
    /// copy of that pathname: replacing the root between those operations can splice a foreign or
    /// mixed tree into an otherwise no-follow walk. This capability keeps the originally-opened
    /// root alive and resolves every descendant from it for the lifetime of the operation.
    #[derive(Debug)]
    pub struct DirectoryCapability {
        root: PathBuf,
        handle: File,
        #[cfg(windows)]
        identity: (u64, u64),
    }

    /// A node yielded by [`DirectoryCapability::walk_regular_tree`]. Directory events bracket all
    /// of their descendants; file handles are opened no-follow and remain the only byte authority.
    pub enum TreeWalkNode<'a> {
        DirectoryEnter(&'a File),
        File(&'a mut File),
        DirectoryLeave(&'a File),
    }

    impl DirectoryCapability {
        pub fn open(root: &Path) -> Result<Self> {
            let before = std::fs::symlink_metadata(root)
                .with_context(|| format!("inspecting capability root {}", root.display()))?;
            if metadata_is_link_or_reparse(&before) || !before.is_dir() {
                bail!(
                    "capability root is not a regular directory: {}",
                    root.display()
                );
            }

            #[cfg(windows)]
            let before_identity = windows_file_identity(&open_directory_nofollow(root)?)?;
            #[cfg(unix)]
            let handle = {
                use rustix::fs::{Mode, OFlags, openat};

                File::from(
                    openat(
                        rustix::fs::CWD,
                        root,
                        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                        Mode::empty(),
                    )
                    .with_context(|| format!("opening capability root {}", root.display()))?,
                )
            };
            #[cfg(not(unix))]
            let handle = open_directory_nofollow(root)?;

            let opened = handle.metadata()?;
            #[cfg(unix)]
            let identity_matches = same_file(&before, &opened);
            #[cfg(windows)]
            let identity_matches = windows_file_identity(&handle)? == before_identity;
            #[cfg(not(any(unix, windows)))]
            let identity_matches = same_file(&before, &opened);
            if metadata_is_link_or_reparse(&opened) || !opened.is_dir() || !identity_matches {
                bail!(
                    "capability root changed while it was opened: {}",
                    root.display()
                );
            }

            Ok(Self {
                root: root.to_path_buf(),
                #[cfg(windows)]
                identity: windows_file_identity(&handle)?,
                handle,
            })
        }

        pub fn root(&self) -> &Path {
            &self.root
        }

        /// Derive a retained descendant-directory authority from this capability. The descendant
        /// is opened relative to the original directory handle on Unix and is identity/containment
        /// checked against it on Windows; callers never need to reopen the source root.
        pub fn subdirectory(&self, relative: &Path) -> Result<Self> {
            validate_normalized_directory_relative(relative)?;
            #[cfg(unix)]
            let handle = {
                use rustix::fs::{Mode, OFlags, openat};

                let mut directory: std::os::fd::OwnedFd = self.handle.try_clone()?.into();
                for component in relative.components() {
                    let Component::Normal(name) = component else {
                        unreachable!("relative directory was validated above")
                    };
                    directory = openat(
                        &directory,
                        name,
                        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                        Mode::empty(),
                    )
                    .with_context(|| {
                        format!(
                            "opening capability subdirectory {}",
                            self.root.join(relative).display()
                        )
                    })?;
                }
                File::from(directory)
            };
            #[cfg(not(unix))]
            let handle = self.open_directory(relative)?;
            let metadata = handle.metadata()?;
            if metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() {
                bail!(
                    "capability subdirectory is not a regular directory: {}",
                    self.root.join(relative).display()
                );
            }
            Ok(Self {
                root: self.root.join(relative),
                #[cfg(windows)]
                identity: windows_file_identity(&handle)?,
                handle,
            })
        }

        /// Reject a source pathname that no longer names the directory retained by this
        /// capability. Operations may safely finish from an already-open handle after a rename,
        /// but setup/publication callers use this check to fail instead of accepting a same-path
        /// replacement as the source of a coherent revision.
        pub fn ensure_path_still_bound(&self) -> Result<()> {
            #[cfg(unix)]
            {
                use rustix::fs::{Mode, OFlags, openat};

                let current = File::from(
                    openat(
                        rustix::fs::CWD,
                        &self.root,
                        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                        Mode::empty(),
                    )
                    .with_context(|| {
                        format!("reopening capability root pathname {}", self.root.display())
                    })?,
                );
                if !same_file(&self.handle.metadata()?, &current.metadata()?) {
                    bail!(
                        "capability root pathname was rebound: {}",
                        self.root.display()
                    );
                }
                Ok(())
            }
            #[cfg(not(unix))]
            self.ensure_root_path_bound()
        }

        pub fn open_regular(&self, relative: &Path) -> Result<(PathBuf, File)> {
            #[cfg(unix)]
            {
                use std::os::unix::fs::FileTypeExt as _;

                use rustix::fs::{Mode, OFlags, openat};

                validate_normalized_relative(relative)?;
                let mut directory: std::os::fd::OwnedFd = self.handle.try_clone()?.into();
                let components = relative.components().collect::<Vec<_>>();
                for component in &components[..components.len() - 1] {
                    let Component::Normal(name) = component else {
                        unreachable!("relative path was validated above")
                    };
                    directory = openat(
                        &directory,
                        *name,
                        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                        Mode::empty(),
                    )
                    .with_context(|| {
                        format!(
                            "opening capability directory without following links: {}",
                            self.root.join(relative).display()
                        )
                    })?;
                }
                let Component::Normal(name) = components.last().expect("relative path is nonempty")
                else {
                    unreachable!("relative path was validated above")
                };
                let file = File::from(
                    openat(
                        &directory,
                        *name,
                        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
                        Mode::empty(),
                    )
                    .with_context(|| {
                        format!(
                            "opening capability file without following links: {}",
                            self.root.join(relative).display()
                        )
                    })?,
                );
                let metadata = file.metadata()?;
                if metadata_is_link_or_reparse(&metadata)
                    || !metadata.is_file()
                    || metadata.file_type().is_socket()
                    || metadata.file_type().is_fifo()
                {
                    bail!(
                        "capability path is not a regular file: {}",
                        self.root.join(relative).display()
                    );
                }
                Ok((self.root.join(relative), file))
            }

            #[cfg(not(unix))]
            {
                validate_normalized_relative(relative)?;
                self.ensure_root_path_bound()?;
                let mut ancestor = PathBuf::new();
                for component in relative
                    .components()
                    .take(relative.components().count() - 1)
                {
                    let Component::Normal(name) = component else {
                        unreachable!("relative path was validated above")
                    };
                    ancestor.push(name);
                    let opened = self.open_directory(&ancestor)?;
                    let metadata = opened.metadata()?;
                    if metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() {
                        bail!(
                            "capability ancestor is not a regular directory: {}",
                            self.root.join(&ancestor).display()
                        );
                    }
                }
                let candidate = self.root.join(relative);
                let file = open_regular_nofollow(&candidate)?;
                validate_final_handle_within(&self.handle, &file)?;
                self.ensure_root_path_bound()?;
                Ok((candidate, file))
            }
        }

        pub fn snapshot_regular(&self, relative: &Path) -> Result<(PathBuf, Vec<u8>)> {
            let (candidate, mut file) = self.open_regular(relative)?;
            let bytes = read_opened_regular(&mut file, &candidate)?;
            Ok((candidate, bytes))
        }

        /// Publish one byte slice beneath this capability. Ancestors are opened (or created) one
        /// component at a time without following links, and the final replacement is relative to
        /// the retained parent handle so a destination symlink is replaced rather than followed.
        #[cfg(unix)]
        pub fn publish_bytes(&self, relative: &Path, bytes: &[u8]) -> Result<PathBuf> {
            use std::sync::atomic::{AtomicU64, Ordering};

            use rustix::fs::{AtFlags, Mode, OFlags, mkdirat, openat, renameat, unlinkat};

            static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

            validate_normalized_relative(relative)?;
            let components = relative.components().collect::<Vec<_>>();
            let mut directory: std::os::fd::OwnedFd = self.handle.try_clone()?.into();
            for component in &components[..components.len() - 1] {
                let Component::Normal(name) = component else {
                    unreachable!("relative path was validated above")
                };
                let opened = openat(
                    &directory,
                    *name,
                    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                    Mode::empty(),
                );
                directory = match opened {
                    Ok(directory) => directory,
                    Err(error) if error == rustix::io::Errno::NOENT => {
                        mkdirat(&directory, *name, Mode::from_raw_mode(0o755)).with_context(
                            || {
                                format!(
                                    "creating capability output directory {}",
                                    self.root.join(relative).display()
                                )
                            },
                        )?;
                        openat(
                            &directory,
                            *name,
                            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                            Mode::empty(),
                        )?
                    }
                    Err(error) => {
                        return Err(error).with_context(|| {
                            format!(
                                "opening capability output directory {}",
                                self.root.join(relative).display()
                            )
                        });
                    }
                };
            }
            let Component::Normal(destination_name) =
                components.last().expect("relative path is nonempty")
            else {
                unreachable!("relative path was validated above")
            };
            let temporary_name = OsString::from(format!(
                ".cargo-dotnet-publish-{}-{}.tmp",
                std::process::id(),
                NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
            ));
            let mut output = File::from(
                openat(
                    &directory,
                    &temporary_name,
                    OFlags::WRONLY
                        | OFlags::CREATE
                        | OFlags::EXCL
                        | OFlags::NOFOLLOW
                        | OFlags::CLOEXEC,
                    Mode::from_raw_mode(0o600),
                )
                .with_context(|| {
                    format!(
                        "creating staged capability output {}",
                        self.root.join(relative).display()
                    )
                })?,
            );
            let publish = (|| -> Result<()> {
                output.write_all(bytes)?;
                output.sync_all()?;
                drop(output);
                renameat(&directory, &temporary_name, &directory, *destination_name).with_context(
                    || {
                        format!(
                            "publishing capability output {}",
                            self.root.join(relative).display()
                        )
                    },
                )?;
                let published = File::from(openat(
                    &directory,
                    *destination_name,
                    OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
                    Mode::empty(),
                )?);
                let metadata = published.metadata()?;
                if metadata_is_link_or_reparse(&metadata) || !metadata.is_file() {
                    bail!("published capability output is not a regular file");
                }
                File::from(rustix::io::dup(&directory)?).sync_all()?;
                Ok(())
            })();
            if publish.is_err() {
                let _ = unlinkat(&directory, &temporary_name, AtFlags::empty());
            }
            publish?;
            Ok(self.root.join(relative))
        }

        #[cfg(windows)]
        pub fn publish_bytes(&self, relative: &Path, bytes: &[u8]) -> Result<PathBuf> {
            validate_normalized_relative(relative)?;
            self.ensure_root_path_bound()?;
            let mut parent = self.root.clone();
            let mut retained = vec![(self.root.clone(), self.handle.try_clone()?)];
            let components = relative.components().collect::<Vec<_>>();
            for component in &components[..components.len() - 1] {
                let Component::Normal(name) = component else {
                    unreachable!("relative path was validated above")
                };
                parent.push(name);
                let opened = create_or_open_windows_directory_within(
                    &retained.last().expect("root retained").1,
                    name,
                )
                .with_context(|| {
                    format!(
                        "creating or opening capability output ancestor {}",
                        parent.display()
                    )
                })?;
                let metadata = opened.metadata()?;
                if metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() {
                    bail!(
                        "capability output ancestor is not a regular directory: {}",
                        parent.display()
                    );
                }
                validate_final_handle_within(&retained.last().expect("root retained").1, &opened)?;
                retained.push((parent.clone(), opened));
            }
            let Component::Normal(destination_name) =
                components.last().expect("relative path is nonempty")
            else {
                unreachable!("relative path was validated above")
            };
            let retained_parent = &retained.last().expect("parent retained").1;
            let mut temporary =
                create_windows_staged_file_within(retained_parent).with_context(|| {
                    format!("creating staged capability output in {}", parent.display())
                })?;
            let publish = (|| -> Result<()> {
                temporary.write_all(bytes)?;
                temporary.sync_all()?;
                validate_final_handle_within(retained_parent, &temporary)?;
                self.revalidate_output_ancestors(&retained)?;
                rename_open_windows_file_within(
                    &temporary,
                    retained_parent,
                    destination_name,
                    true,
                )
                .with_context(|| {
                    format!(
                        "publishing staged capability output {}",
                        self.root.join(relative).display()
                    )
                })?;
                self.revalidate_output_ancestors(&retained)?;
                validate_final_handle_within(retained_parent, &temporary)?;
                let metadata = temporary.metadata()?;
                if metadata_is_link_or_reparse(&metadata) || !metadata.is_file() {
                    bail!("published capability output is not a regular file");
                }
                Ok(())
            })();
            if publish.is_err() {
                let _ = mark_open_windows_file_for_deletion(&temporary);
            }
            publish?;
            Ok(self.root.join(relative))
        }

        #[cfg(not(any(unix, windows)))]
        pub fn publish_bytes(&self, relative: &Path, bytes: &[u8]) -> Result<PathBuf> {
            validate_normalized_relative(relative)?;
            self.ensure_root_path_bound()?;
            let destination = self.root.join(relative);
            let parent = destination
                .parent()
                .context("capability output has no parent")?;
            std::fs::create_dir_all(parent)?;
            let mut temporary = tempfile::Builder::new()
                .prefix(".cargo-dotnet-publish-")
                .suffix(".tmp")
                .tempfile_in(parent)?;
            temporary.write_all(bytes)?;
            temporary.as_file().sync_all()?;
            self.ensure_root_path_bound()?;
            let temporary_path = temporary.into_temp_path();
            atomic_replace_file(&temporary_path, &destination)?;
            self.ensure_root_path_bound()?;
            Ok(destination)
        }

        #[cfg(not(unix))]
        fn revalidate_output_ancestors(&self, retained: &[(PathBuf, File)]) -> Result<()> {
            self.ensure_root_path_bound()?;
            for (index, (path, retained_handle)) in retained.iter().enumerate() {
                let current = open_directory_nofollow(path)?;
                let metadata = current.metadata()?;
                if metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() {
                    bail!(
                        "capability output ancestor was replaced by a link or reparse point: {}",
                        path.display()
                    );
                }
                #[cfg(windows)]
                let matches =
                    windows_file_identity(&current)? == windows_file_identity(retained_handle)?;
                #[cfg(not(windows))]
                let matches = same_file(&current.metadata()?, &retained_handle.metadata()?);
                if !matches {
                    bail!(
                        "capability output ancestor pathname was rebound: {}",
                        path.display()
                    );
                }
                if index != 0 {
                    validate_final_handle_within(&self.handle, &current)?;
                }
            }
            Ok(())
        }

        pub fn walk_regular_tree<F>(
            &self,
            excluded_root_entries: &[&str],
            visitor: &mut F,
        ) -> Result<()>
        where
            F: for<'a> FnMut(&Path, TreeWalkNode<'a>) -> Result<()>,
        {
            #[cfg(unix)]
            {
                self.walk_unix_directory(
                    Path::new(""),
                    &self.handle,
                    excluded_root_entries,
                    visitor,
                )
            }
            #[cfg(not(unix))]
            {
                self.walk_path_directory(
                    Path::new(""),
                    &self.handle,
                    excluded_root_entries,
                    visitor,
                )
            }
        }

        #[cfg(unix)]
        fn walk_unix_directory<F>(
            &self,
            relative: &Path,
            directory: &File,
            excluded_root_entries: &[&str],
            visitor: &mut F,
        ) -> Result<()>
        where
            F: for<'a> FnMut(&Path, TreeWalkNode<'a>) -> Result<()>,
        {
            use std::os::unix::ffi::OsStrExt as _;
            use std::os::unix::fs::FileTypeExt as _;

            use rustix::fs::{Dir, Mode, OFlags, openat};

            let mut stream = Dir::read_from(directory)?;
            let mut names = Vec::<OsString>::new();
            while let Some(entry) = stream.read() {
                let entry = entry?;
                let bytes = entry.file_name().to_bytes();
                if bytes != b"." && bytes != b".." {
                    names.push(std::ffi::OsStr::from_bytes(bytes).to_owned());
                }
            }
            names.sort();
            for name in names {
                if relative.as_os_str().is_empty()
                    && excluded_root_entries
                        .iter()
                        .any(|excluded| name == std::ffi::OsStr::new(excluded))
                {
                    continue;
                }
                let child_relative = relative.join(&name);
                let opened_directory = openat(
                    directory,
                    &name,
                    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                    Mode::empty(),
                );
                if let Ok(descriptor) = opened_directory {
                    let child = File::from(descriptor);
                    let metadata = child.metadata()?;
                    if metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() {
                        bail!(
                            "tree capability entry is not a regular directory: {}",
                            self.root.join(&child_relative).display()
                        );
                    }
                    visitor(&child_relative, TreeWalkNode::DirectoryEnter(&child))?;
                    self.walk_unix_directory(
                        &child_relative,
                        &child,
                        excluded_root_entries,
                        visitor,
                    )?;
                    visitor(&child_relative, TreeWalkNode::DirectoryLeave(&child))?;
                    continue;
                }

                let child = File::from(
                    openat(
                        directory,
                        &name,
                        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
                        Mode::empty(),
                    )
                    .with_context(|| {
                        format!(
                            "opening tree capability entry without following links: {}",
                            self.root.join(&child_relative).display()
                        )
                    })?,
                );
                let metadata = child.metadata()?;
                if metadata_is_link_or_reparse(&metadata)
                    || !metadata.is_file()
                    || metadata.file_type().is_socket()
                    || metadata.file_type().is_fifo()
                {
                    bail!(
                        "tree capability contains an unsupported entry: {}",
                        self.root.join(&child_relative).display()
                    );
                }
                let mut child = child;
                visitor(&child_relative, TreeWalkNode::File(&mut child))?;
            }
            Ok(())
        }

        #[cfg(not(unix))]
        fn walk_path_directory<F>(
            &self,
            relative: &Path,
            directory: &File,
            excluded_root_entries: &[&str],
            visitor: &mut F,
        ) -> Result<()>
        where
            F: for<'a> FnMut(&Path, TreeWalkNode<'a>) -> Result<()>,
        {
            self.ensure_root_path_bound()?;
            self.ensure_directory_path_bound(relative, directory)?;
            let mut names = std::fs::read_dir(self.root.join(relative))?
                .map(|entry| entry.map(|entry| entry.file_name()))
                .collect::<std::io::Result<Vec<_>>>()?;
            // `read_dir` is pathname-based on non-Unix hosts. Bind that enumeration back to the
            // retained directory handle before trusting any of its names. Every recursive entry
            // repeats this check, so a child replaced after DirectoryEnter is rejected before its
            // replacement can contribute bytes to the walk.
            self.ensure_directory_path_bound(relative, directory)?;
            names.sort();
            for name in names {
                if relative.as_os_str().is_empty()
                    && excluded_root_entries
                        .iter()
                        .any(|excluded| name == std::ffi::OsStr::new(excluded))
                {
                    continue;
                }
                let child_relative = relative.join(&name);
                if let Ok(child) = self.open_directory(&child_relative) {
                    visitor(&child_relative, TreeWalkNode::DirectoryEnter(&child))?;
                    self.walk_path_directory(
                        &child_relative,
                        &child,
                        excluded_root_entries,
                        visitor,
                    )?;
                    visitor(&child_relative, TreeWalkNode::DirectoryLeave(&child))?;
                    continue;
                }
                let (_, mut child) = self.open_regular(&child_relative)?;
                visitor(&child_relative, TreeWalkNode::File(&mut child))?;
            }
            self.ensure_directory_path_bound(relative, directory)
        }

        #[cfg(not(unix))]
        fn ensure_directory_path_bound(&self, relative: &Path, retained: &File) -> Result<()> {
            self.ensure_root_path_bound()?;
            let current = self.open_directory(relative)?;
            let metadata = current.metadata()?;
            if metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() {
                bail!(
                    "tree capability directory changed while it was walked: {}",
                    self.root.join(relative).display()
                );
            }
            #[cfg(windows)]
            let matches = windows_file_identity(&current)? == windows_file_identity(retained)?;
            #[cfg(not(windows))]
            let matches = same_file(&current.metadata()?, &retained.metadata()?);
            if !matches {
                bail!(
                    "tree capability directory pathname was rebound while it was walked: {}",
                    self.root.join(relative).display()
                );
            }
            Ok(())
        }

        #[cfg(not(unix))]
        fn open_directory(&self, relative: &Path) -> Result<File> {
            validate_normalized_directory_relative(relative)?;
            if relative.as_os_str().is_empty() {
                return Ok(self.handle.try_clone()?);
            }
            self.ensure_root_path_bound()?;
            let candidate = self.root.join(relative);
            let directory = open_directory_nofollow(&candidate)?;
            let metadata = directory.metadata()?;
            if metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() {
                bail!(
                    "capability path is not a regular directory: {}",
                    candidate.display()
                );
            }
            validate_final_handle_within(&self.handle, &directory)?;
            Ok(directory)
        }

        #[cfg(windows)]
        fn ensure_root_path_bound(&self) -> Result<()> {
            let current = open_directory_nofollow(&self.root)?;
            let metadata = current.metadata()?;
            if metadata_is_link_or_reparse(&metadata)
                || !metadata.is_dir()
                || windows_file_identity(&current)? != self.identity
            {
                bail!(
                    "capability root pathname was rebound: {}",
                    self.root.display()
                );
            }
            Ok(())
        }

        #[cfg(not(any(unix, windows)))]
        fn ensure_root_path_bound(&self) -> Result<()> {
            let current = open_directory_nofollow(&self.root)?;
            let before = self.handle.metadata()?;
            let after = current.metadata()?;
            if metadata_is_link_or_reparse(&after) || !after.is_dir() || !same_file(&before, &after)
            {
                bail!(
                    "capability root pathname was rebound: {}",
                    self.root.display()
                );
            }
            Ok(())
        }
    }

    pub fn publish_bytes_within(root: &Path, relative: &Path, bytes: &[u8]) -> Result<PathBuf> {
        DirectoryCapability::open(root)?.publish_bytes(relative, bytes)
    }

    fn open_regular_nofollow_with(path: &Path, write: bool) -> Result<File> {
        let before = std::fs::symlink_metadata(path)
            .with_context(|| format!("inspecting regular file {}", path.display()))?;
        if metadata_is_link_or_reparse(&before) || !before.is_file() {
            bail!("path is not a regular non-symlink file: {}", path.display());
        }
        #[cfg(windows)]
        let before_identity = {
            let mut options = OpenOptions::new();
            options.read(true);
            configure_nofollow(&mut options);
            let validation = options.open(path).with_context(|| {
                format!(
                    "opening regular file for Windows identity: {}",
                    path.display()
                )
            })?;
            windows_file_identity(&validation)?
        };
        let mut options = OpenOptions::new();
        options.read(true).write(write);
        configure_nofollow(&mut options);
        let file = options.open(path).with_context(|| {
            format!(
                "opening regular file without following links: {}",
                path.display()
            )
        })?;
        let after = file.metadata()?;
        #[cfg(unix)]
        let identity_matches = same_file(&before, &after);
        #[cfg(windows)]
        let identity_matches = windows_file_identity(&file)? == before_identity;
        #[cfg(not(any(unix, windows)))]
        let identity_matches = true;
        if metadata_is_link_or_reparse(&after) || !after.is_file() || !identity_matches {
            bail!(
                "regular file changed while it was opened: {}",
                path.display()
            );
        }
        Ok(file)
    }

    pub fn read_regular_nofollow(path: &Path) -> Result<Vec<u8>> {
        let mut file = open_regular_nofollow(path)?;
        read_opened_regular(&mut file, path)
    }

    /// Read an already-authorized regular-file handle and reject concurrent byte-state changes.
    pub fn read_opened_regular(file: &mut File, display_path: &Path) -> Result<Vec<u8>> {
        let before = file.metadata()?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        let after = file.metadata()?;
        if bytes.len() as u64 != after.len() || !same_contents_state(&before, &after) {
            bail!(
                "regular file changed while its opened handle was read: {}",
                display_path.display()
            );
        }
        Ok(bytes)
    }

    /// Open a regular file by walking every relative component from an already-scoped root.
    ///
    /// On Unix this is an `openat` capability walk: every ancestor and the leaf are opened with
    /// `O_NOFOLLOW`, so an attacker cannot redirect a cache read by replacing an ancestor after
    /// pathname validation. Other platforms retain an opened leaf handle and validate both its
    /// stable file identity and its final resolved path against the root before returning it.
    pub fn open_regular_within(root: &Path, relative: &Path) -> Result<(PathBuf, File)> {
        open_regular_within_impl(root, relative)
    }

    fn validate_normalized_relative(relative: &Path) -> Result<()> {
        if relative.as_os_str().is_empty()
            || relative.is_absolute()
            || relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            bail!(
                "path is not a normalized relative file: {}",
                relative.display()
            );
        }
        Ok(())
    }

    fn validate_normalized_directory_relative(relative: &Path) -> Result<()> {
        if relative.is_absolute()
            || relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            bail!(
                "path is not a normalized relative directory: {}",
                relative.display()
            );
        }
        Ok(())
    }

    #[cfg(unix)]
    fn open_regular_within_impl(root: &Path, relative: &Path) -> Result<(PathBuf, File)> {
        open_regular_within_unix(root, relative)
    }

    #[cfg(unix)]
    fn open_regular_within_unix(root: &Path, relative: &Path) -> Result<(PathBuf, File)> {
        use std::os::unix::fs::FileTypeExt as _;

        use rustix::fs::{Mode, OFlags, openat};

        validate_normalized_relative(relative)?;
        let root_metadata = std::fs::symlink_metadata(root)
            .with_context(|| format!("inspecting containing root {}", root.display()))?;
        if metadata_is_link_or_reparse(&root_metadata) || !root_metadata.is_dir() {
            bail!(
                "containing root is not a regular directory: {}",
                root.display()
            );
        }
        let root_descriptor = openat(
            rustix::fs::CWD,
            root,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .with_context(|| format!("opening containing root {}", root.display()))?;
        let root_file = File::from(root_descriptor);
        let opened_root_metadata = root_file.metadata()?;
        if metadata_is_link_or_reparse(&opened_root_metadata)
            || !opened_root_metadata.is_dir()
            || !same_file(&root_metadata, &opened_root_metadata)
        {
            bail!(
                "containing root changed while it was opened: {}",
                root.display()
            );
        }
        let mut directory: std::os::fd::OwnedFd = root_file.into();
        let components = relative.components().collect::<Vec<_>>();
        for component in &components[..components.len() - 1] {
            let Component::Normal(name) = component else {
                unreachable!("relative path was validated above")
            };
            directory = openat(
                &directory,
                *name,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .with_context(|| {
                format!(
                    "opening contained directory without following links: {}",
                    root.join(relative).display()
                )
            })?;
        }
        let Component::Normal(name) = components.last().expect("relative path is nonempty") else {
            unreachable!("relative path was validated above")
        };
        let descriptor = openat(
            &directory,
            *name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::empty(),
        )
        .with_context(|| {
            format!(
                "opening contained file without following links: {}",
                root.join(relative).display()
            )
        })?;
        let file = File::from(descriptor);
        let metadata = file.metadata()?;
        if metadata_is_link_or_reparse(&metadata)
            || !metadata.is_file()
            || metadata.file_type().is_socket()
            || metadata.file_type().is_fifo()
        {
            bail!(
                "contained path is not a regular file: {}",
                root.join(relative).display()
            );
        }
        Ok((root.join(relative), file))
    }

    #[cfg(not(unix))]
    fn open_regular_within_impl(root: &Path, relative: &Path) -> Result<(PathBuf, File)> {
        open_regular_within_other(root, relative)
    }

    #[cfg(not(unix))]
    fn open_regular_within_other(root: &Path, relative: &Path) -> Result<(PathBuf, File)> {
        validate_normalized_relative(relative)?;
        let root_metadata = std::fs::symlink_metadata(root)
            .with_context(|| format!("inspecting containing root {}", root.display()))?;
        if metadata_is_link_or_reparse(&root_metadata) || !root_metadata.is_dir() {
            bail!(
                "containing root is not a regular directory: {}",
                root.display()
            );
        }
        #[cfg(windows)]
        let before_root_identity = {
            let validation = open_directory_nofollow(root)?;
            windows_file_identity(&validation)?
        };
        let root_handle = open_directory_nofollow(root)?;
        let opened_root_metadata = root_handle.metadata()?;
        #[cfg(windows)]
        let root_identity_matches = windows_file_identity(&root_handle)? == before_root_identity;
        #[cfg(not(windows))]
        let root_identity_matches = same_file(&root_metadata, &opened_root_metadata);
        if metadata_is_link_or_reparse(&opened_root_metadata)
            || !opened_root_metadata.is_dir()
            || !root_identity_matches
        {
            bail!(
                "containing root changed while it was opened: {}",
                root.display()
            );
        }
        let mut ancestor_relative = PathBuf::new();
        for component in relative
            .components()
            .take(relative.components().count() - 1)
        {
            let Component::Normal(name) = component else {
                unreachable!("relative path was validated above")
            };
            ancestor_relative.push(name);
            let ancestor = root.join(&ancestor_relative);
            let metadata = std::fs::symlink_metadata(&ancestor)?;
            if metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() {
                bail!(
                    "contained ancestor is not a regular directory: {}",
                    ancestor.display()
                );
            }
        }
        let candidate = root.join(relative);
        let file = open_regular_nofollow(&candidate)?;
        validate_final_handle_within(&root_handle, &file)?;
        Ok((candidate, file))
    }

    #[cfg(windows)]
    fn open_directory_nofollow(path: &Path) -> Result<File> {
        use std::os::windows::fs::OpenOptionsExt as _;

        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)
            .with_context(|| {
                format!(
                    "opening containing directory without reparse: {}",
                    path.display()
                )
            })
    }

    #[cfg(windows)]
    fn create_or_open_windows_directory_within(parent: &File, name: &OsStr) -> Result<File> {
        const FILE_LIST_DIRECTORY: u32 = 0x0000_0001;
        const FILE_TRAVERSE: u32 = 0x0000_0020;
        const FILE_READ_ATTRIBUTES: u32 = 0x0000_0080;
        const SYNCHRONIZE: u32 = 0x0010_0000;
        const FILE_OPEN_IF: u32 = 3;
        const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0000_0010;
        const FILE_DIRECTORY_FILE: u32 = 0x0000_0001;

        let file = nt_create_relative(
            parent,
            name,
            FILE_LIST_DIRECTORY | FILE_TRAVERSE | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            FILE_OPEN_IF,
            FILE_ATTRIBUTE_DIRECTORY,
            FILE_DIRECTORY_FILE,
        )?;
        let metadata = file.metadata()?;
        if metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() {
            bail!(
                "handle-relative capability ancestor is not a regular directory: {}",
                name.to_string_lossy()
            );
        }
        Ok(file)
    }

    #[cfg(windows)]
    fn create_windows_staged_file_within(parent: &File) -> Result<File> {
        use std::sync::atomic::{AtomicU64, Ordering};

        const DELETE: u32 = 0x0001_0000;
        const SYNCHRONIZE: u32 = 0x0010_0000;
        const GENERIC_READ: u32 = 0x8000_0000;
        const GENERIC_WRITE: u32 = 0x4000_0000;
        const FILE_CREATE: u32 = 2;
        const FILE_ATTRIBUTE_NORMAL: u32 = 0x0000_0080;
        const FILE_NON_DIRECTORY_FILE: u32 = 0x0000_0040;
        static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

        for _ in 0..128 {
            let name = OsString::from(format!(
                ".cargo-dotnet-publish-{}-{}.tmp",
                std::process::id(),
                NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
            ));
            match nt_create_relative(
                parent,
                &name,
                GENERIC_READ | GENERIC_WRITE | DELETE | SYNCHRONIZE,
                FILE_CREATE,
                FILE_ATTRIBUTE_NORMAL,
                FILE_NON_DIRECTORY_FILE,
            ) {
                Ok(file) => {
                    let metadata = file.metadata()?;
                    if metadata_is_link_or_reparse(&metadata) || !metadata.is_file() {
                        bail!("handle-relative staged capability output is not a regular file");
                    }
                    return Ok(file);
                }
                Err(error)
                    if error
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|error| error.kind() == std::io::ErrorKind::AlreadyExists) =>
                {
                    continue;
                }
                Err(error) => return Err(error),
            }
        }
        bail!("could not allocate a unique handle-relative staged capability output")
    }

    #[cfg(windows)]
    fn nt_create_relative(
        parent: &File,
        name: &OsStr,
        desired_access: u32,
        create_disposition: u32,
        file_attributes: u32,
        file_kind_options: u32,
    ) -> Result<File> {
        use std::os::windows::ffi::OsStrExt as _;
        use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _};

        #[repr(C)]
        struct UnicodeString {
            length: u16,
            maximum_length: u16,
            buffer: *mut u16,
        }
        #[repr(C)]
        struct ObjectAttributes {
            length: u32,
            root_directory: *mut core::ffi::c_void,
            object_name: *mut UnicodeString,
            attributes: u32,
            security_descriptor: *mut core::ffi::c_void,
            security_quality_of_service: *mut core::ffi::c_void,
        }
        #[repr(C)]
        struct IoStatusBlock {
            status_or_pointer: usize,
            information: usize,
        }
        const OBJ_CASE_INSENSITIVE: u32 = 0x0000_0040;
        const OBJ_DONT_REPARSE: u32 = 0x0000_1000;
        const FILE_SHARE_READ: u32 = 0x0000_0001;
        const FILE_SHARE_WRITE: u32 = 0x0000_0002;
        const FILE_SHARE_DELETE: u32 = 0x0000_0004;
        const FILE_DIRECTORY_FILE: u32 = 0x0000_0001;
        const FILE_SYNCHRONOUS_IO_NONALERT: u32 = 0x0000_0020;
        const FILE_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        #[link(name = "ntdll")]
        unsafe extern "system" {
            fn NtCreateFile(
                file_handle: *mut *mut core::ffi::c_void,
                desired_access: u32,
                object_attributes: *mut ObjectAttributes,
                io_status_block: *mut IoStatusBlock,
                allocation_size: *mut i64,
                file_attributes: u32,
                share_access: u32,
                create_disposition: u32,
                create_options: u32,
                ea_buffer: *mut core::ffi::c_void,
                ea_length: u32,
            ) -> i32;
            fn RtlNtStatusToDosError(status: i32) -> u32;
        }

        let mut wide = name.encode_wide().collect::<Vec<_>>();
        if wide.is_empty() || wide.iter().any(|unit| *unit == 0) {
            bail!("Windows handle-relative name is empty or contains NUL");
        }
        let byte_len = wide
            .len()
            .checked_mul(std::mem::size_of::<u16>())
            .context("Windows handle-relative name is too long")?;
        let byte_len =
            u16::try_from(byte_len).context("Windows handle-relative name is too long")?;
        let mut unicode = UnicodeString {
            length: byte_len,
            maximum_length: byte_len,
            buffer: wide.as_mut_ptr(),
        };
        let mut attributes = ObjectAttributes {
            length: u32::try_from(std::mem::size_of::<ObjectAttributes>())?,
            root_directory: parent.as_raw_handle(),
            object_name: &mut unicode,
            attributes: OBJ_CASE_INSENSITIVE | OBJ_DONT_REPARSE,
            security_descriptor: std::ptr::null_mut(),
            security_quality_of_service: std::ptr::null_mut(),
        };
        let mut status_block = IoStatusBlock {
            status_or_pointer: 0,
            information: 0,
        };
        let mut handle = std::ptr::null_mut();
        let status = unsafe {
            NtCreateFile(
                &mut handle,
                desired_access,
                &mut attributes,
                &mut status_block,
                std::ptr::null_mut(),
                file_attributes,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                create_disposition,
                file_kind_options
                    | FILE_SYNCHRONOUS_IO_NONALERT
                    | if file_kind_options & FILE_DIRECTORY_FILE == 0 {
                        FILE_OPEN_REPARSE_POINT
                    } else {
                        0
                    },
                std::ptr::null_mut(),
                0,
            )
        };
        if status < 0 {
            let win32 = unsafe { RtlNtStatusToDosError(status) };
            return Err(std::io::Error::from_raw_os_error(win32 as i32)).with_context(|| {
                format!(
                    "opening handle-relative Windows path {} (NTSTATUS {status:#x})",
                    name.to_string_lossy()
                )
            });
        }
        if handle.is_null() {
            bail!("NtCreateFile succeeded without returning a handle");
        }
        Ok(unsafe { File::from_raw_handle(handle) })
    }

    #[cfg(windows)]
    fn mark_open_windows_file_for_deletion(file: &File) -> Result<()> {
        use std::os::windows::io::AsRawHandle as _;

        #[repr(C)]
        struct FileDispositionInfo {
            delete_file: i32,
        }
        const FILE_DISPOSITION_INFO_CLASS: u32 = 4;
        unsafe extern "system" {
            fn SetFileInformationByHandle(
                file: *mut core::ffi::c_void,
                information_class: u32,
                information: *mut core::ffi::c_void,
                buffer_size: u32,
            ) -> i32;
        }
        let mut information = FileDispositionInfo { delete_file: 1 };
        if unsafe {
            SetFileInformationByHandle(
                file.as_raw_handle(),
                FILE_DISPOSITION_INFO_CLASS,
                (&mut information as *mut FileDispositionInfo).cast(),
                u32::try_from(std::mem::size_of::<FileDispositionInfo>())?,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error())
                .context("marking failed staged Windows output for deletion");
        }
        Ok(())
    }

    #[cfg(windows)]
    fn rename_open_windows_file_within(
        file: &File,
        parent: &File,
        destination_name: &OsStr,
        replace_if_exists: bool,
    ) -> Result<()> {
        use std::os::windows::ffi::OsStrExt as _;
        use std::os::windows::io::AsRawHandle as _;

        #[repr(C)]
        struct FileRenameInfo {
            replace_if_exists: u8,
            root_directory: *mut core::ffi::c_void,
            file_name_length: u32,
            file_name: [u16; 1],
        }
        const FILE_RENAME_INFO_CLASS: u32 = 3;
        unsafe extern "system" {
            fn SetFileInformationByHandle(
                file: *mut core::ffi::c_void,
                information_class: u32,
                information: *mut core::ffi::c_void,
                buffer_size: u32,
            ) -> i32;
        }

        let name = destination_name.encode_wide().collect::<Vec<_>>();
        if name.is_empty() {
            bail!("Windows capability destination filename is empty");
        }
        let name_bytes = name
            .len()
            .checked_mul(std::mem::size_of::<u16>())
            .context("Windows destination filename is too long")?;
        let buffer_size = std::mem::offset_of!(FileRenameInfo, file_name)
            .checked_add(name_bytes)
            .context("Windows rename buffer is too large")?;
        let words = buffer_size.div_ceil(std::mem::size_of::<usize>());
        let mut storage = vec![0_usize; words];
        let information = storage.as_mut_ptr().cast::<FileRenameInfo>();
        unsafe {
            (*information).replace_if_exists = u8::from(replace_if_exists);
            (*information).root_directory = parent.as_raw_handle();
            (*information).file_name_length = u32::try_from(name_bytes)?;
            std::ptr::copy_nonoverlapping(
                name.as_ptr(),
                (*information).file_name.as_mut_ptr(),
                name.len(),
            );
            if SetFileInformationByHandle(
                file.as_raw_handle(),
                FILE_RENAME_INFO_CLASS,
                information.cast(),
                u32::try_from(buffer_size)?,
            ) == 0
            {
                return Err(std::io::Error::last_os_error())
                    .context("renaming opened Windows file relative to retained parent");
            }
        }
        Ok(())
    }

    #[cfg(not(any(unix, windows)))]
    fn open_directory_nofollow(path: &Path) -> Result<File> {
        File::open(path)
            .with_context(|| format!("opening containing directory: {}", path.display()))
    }

    #[cfg(windows)]
    fn validate_final_handle_within(root: &File, file: &File) -> Result<()> {
        use std::os::windows::io::AsRawHandle as _;

        const FILE_NAME_NORMALIZED: u32 = 0;
        unsafe extern "system" {
            fn GetFinalPathNameByHandleW(
                file: *mut core::ffi::c_void,
                path: *mut u16,
                length: u32,
                flags: u32,
            ) -> u32;
        }
        fn final_path(handle: *mut core::ffi::c_void) -> Result<PathBuf> {
            let required = unsafe {
                GetFinalPathNameByHandleW(handle, std::ptr::null_mut(), 0, FILE_NAME_NORMALIZED)
            };
            if required == 0 {
                return Err(std::io::Error::last_os_error())
                    .context("querying final Windows handle path");
            }
            let mut buffer = vec![0_u16; required as usize + 1];
            let written = unsafe {
                GetFinalPathNameByHandleW(handle, buffer.as_mut_ptr(), buffer.len() as u32, 0)
            };
            if written == 0 || written as usize >= buffer.len() {
                return Err(std::io::Error::last_os_error())
                    .context("reading final Windows handle path");
            }
            buffer.truncate(written as usize);
            Ok(PathBuf::from(String::from_utf16(&buffer)?))
        }
        fn normalized(path: &Path) -> String {
            let value = path.to_string_lossy().replace('/', "\\");
            value
                .strip_prefix(r"\\?\UNC\")
                .map(|tail| format!(r"\\{tail}"))
                .or_else(|| value.strip_prefix(r"\\?\").map(str::to_owned))
                .unwrap_or(value)
                .trim_end_matches('\\')
                .to_ascii_lowercase()
        }

        let final_file = normalized(&final_path(file.as_raw_handle())?);
        let final_root = normalized(&final_path(root.as_raw_handle())?);
        let root_prefix = format!("{final_root}\\");
        if !final_file.starts_with(&root_prefix) {
            bail!("opened Windows file handle escapes its containing root");
        }
        Ok(())
    }

    #[cfg(not(any(unix, windows)))]
    fn validate_final_handle_within(_root: &File, _file: &File) -> Result<()> {
        Ok(())
    }

    /// Snapshot one regular leaf beneath `root` without trusting a canonicalized pathname after
    /// it has been resolved. The opened handle is the source of the returned bytes; pathname and
    /// root identities are rechecked before those bytes are released to the caller.
    pub fn snapshot_regular_within(root: &Path, relative: &Path) -> Result<(PathBuf, Vec<u8>)> {
        let (candidate, mut file) = open_regular_within(root, relative)?;
        let bytes = read_opened_regular(&mut file, &candidate)?;
        Ok((candidate, bytes))
    }

    #[cfg(unix)]
    pub fn atomic_replace_file(replacement: &Path, destination: &Path) -> Result<()> {
        open_regular_nofollow(replacement)?;
        if destination.exists() {
            open_regular_nofollow(destination)?;
        }
        std::fs::rename(replacement, destination).with_context(|| {
            format!(
                "atomically replacing {} with {}",
                destination.display(),
                replacement.display()
            )
        })
    }

    #[cfg(windows)]
    pub fn atomic_replace_file(replacement: &Path, destination: &Path) -> Result<()> {
        use std::os::windows::ffi::OsStrExt as _;
        const MOVEFILE_REPLACE_EXISTING: u32 = 0x0000_0001;
        const MOVEFILE_WRITE_THROUGH: u32 = 0x0000_0008;
        unsafe extern "system" {
            fn ReplaceFileW(
                replaced: *const u16,
                replacement: *const u16,
                backup: *const u16,
                flags: u32,
                exclude: *mut core::ffi::c_void,
                reserved: *mut core::ffi::c_void,
            ) -> i32;
            fn MoveFileExW(existing: *const u16, new_name: *const u16, flags: u32) -> i32;
        }
        fn wide(path: &Path) -> Vec<u16> {
            path.as_os_str().encode_wide().chain(Some(0)).collect()
        }

        open_regular_nofollow(replacement)?;
        if destination.exists() {
            open_regular_nofollow(destination)?;
        }
        let replacement = wide(replacement);
        let destination = wide(destination);
        let success = unsafe {
            let replaced = ReplaceFileW(
                destination.as_ptr(),
                replacement.as_ptr(),
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            if replaced != 0 {
                replaced
            } else {
                MoveFileExW(
                    replacement.as_ptr(),
                    destination.as_ptr(),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            }
        };
        if success == 0 {
            return Err(std::io::Error::last_os_error()).context("atomic Windows file replacement");
        }
        Ok(())
    }

    #[cfg(not(any(unix, windows)))]
    pub fn atomic_replace_file(replacement: &Path, destination: &Path) -> Result<()> {
        open_regular_nofollow(replacement)?;
        if destination.exists() {
            open_regular_nofollow(destination)?;
        }
        std::fs::rename(replacement, destination).with_context(|| {
            format!(
                "replacing {} with {}",
                destination.display(),
                replacement.display()
            )
        })
    }

    pub fn metadata_is_link_or_reparse(metadata: &std::fs::Metadata) -> bool {
        if metadata.file_type().is_symlink() {
            return true;
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt as _;
            const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
            return metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
        }
        #[cfg(not(windows))]
        false
    }

    #[cfg(unix)]
    fn configure_nofollow(options: &mut OpenOptions) {
        use std::os::unix::fs::OpenOptionsExt as _;

        #[cfg(any(target_os = "macos", target_os = "ios"))]
        const O_NOFOLLOW: i32 = 0x0000_0100;
        #[cfg(any(target_os = "linux", target_os = "android"))]
        const O_NOFOLLOW: i32 = 0x0002_0000;
        #[cfg(not(any(
            target_os = "macos",
            target_os = "ios",
            target_os = "linux",
            target_os = "android"
        )))]
        const O_NOFOLLOW: i32 = 0;
        options.custom_flags(O_NOFOLLOW);
    }

    #[cfg(windows)]
    fn configure_nofollow(options: &mut OpenOptions) {
        use std::os::windows::fs::OpenOptionsExt as _;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }

    #[cfg(not(any(unix, windows)))]
    fn configure_nofollow(_options: &mut OpenOptions) {}

    #[cfg(unix)]
    fn same_file(before: &std::fs::Metadata, after: &std::fs::Metadata) -> bool {
        use std::os::unix::fs::MetadataExt as _;
        before.dev() == after.dev() && before.ino() == after.ino()
    }

    #[cfg(not(any(unix, windows)))]
    fn same_file(_before: &std::fs::Metadata, _after: &std::fs::Metadata) -> bool {
        true
    }

    #[cfg(windows)]
    fn windows_file_identity(file: &File) -> Result<(u64, u64)> {
        use std::os::windows::io::AsRawHandle as _;

        #[repr(C)]
        struct FileTime {
            low: u32,
            high: u32,
        }
        #[repr(C)]
        struct ByHandleFileInformation {
            attributes: u32,
            creation_time: FileTime,
            last_access_time: FileTime,
            last_write_time: FileTime,
            volume_serial_number: u32,
            file_size_high: u32,
            file_size_low: u32,
            number_of_links: u32,
            file_index_high: u32,
            file_index_low: u32,
        }
        unsafe extern "system" {
            fn GetFileInformationByHandle(
                file: *mut core::ffi::c_void,
                information: *mut ByHandleFileInformation,
            ) -> i32;
        }

        let mut information = std::mem::MaybeUninit::<ByHandleFileInformation>::uninit();
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), information.as_mut_ptr()) }
            == 0
        {
            return Err(std::io::Error::last_os_error())
                .context("querying stable Windows file identity");
        }
        let information = unsafe { information.assume_init() };
        let index =
            (u64::from(information.file_index_high) << 32) | u64::from(information.file_index_low);
        Ok((u64::from(information.volume_serial_number), index))
    }

    /// Stable identity of an already-opened file handle. Pathname metadata is deliberately not
    /// accepted: Windows exposes volume/file indices only through by-handle APIs on stable Rust.
    pub fn opened_file_identity(file: &File) -> Result<String> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;
            let metadata = file.metadata()?;
            Ok(format!(
                "{}:{}:{}:{}",
                metadata.dev(),
                metadata.ino(),
                metadata.ctime(),
                metadata.ctime_nsec()
            ))
        }
        #[cfg(windows)]
        {
            let (volume, index) = windows_file_identity(file)?;
            Ok(format!("{volume}:{index}"))
        }
        #[cfg(not(any(unix, windows)))]
        {
            let metadata = file.metadata()?;
            Ok(format!("{}:{:?}", metadata.len(), metadata.modified().ok()))
        }
    }

    #[cfg(unix)]
    fn same_contents_state(before: &std::fs::Metadata, after: &std::fs::Metadata) -> bool {
        use std::os::unix::fs::MetadataExt as _;
        same_file(before, after)
            && before.len() == after.len()
            && before.mtime() == after.mtime()
            && before.mtime_nsec() == after.mtime_nsec()
            && before.ctime() == after.ctime()
            && before.ctime_nsec() == after.ctime_nsec()
    }

    #[cfg(windows)]
    fn same_contents_state(before: &std::fs::Metadata, after: &std::fs::Metadata) -> bool {
        use std::os::windows::fs::MetadataExt as _;
        before.file_size() == after.file_size()
            && before.last_write_time() == after.last_write_time()
            && before.creation_time() == after.creation_time()
    }

    #[cfg(not(any(unix, windows)))]
    fn same_contents_state(before: &std::fs::Metadata, after: &std::fs::Metadata) -> bool {
        before.len() == after.len() && before.modified().ok() == after.modified().ok()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[cfg(unix)]
        #[test]
        fn capability_publication_replaces_leaf_symlink_and_rejects_linked_ancestor() {
            use std::os::unix::fs::symlink;

            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("root");
            let outside = temp.path().join("outside");
            std::fs::create_dir(&root).unwrap();
            std::fs::create_dir(&outside).unwrap();
            let sentinel = outside.join("sentinel");
            std::fs::write(&sentinel, b"keep").unwrap();

            symlink(&sentinel, root.join("leaf")).unwrap();
            let capability = DirectoryCapability::open(&root).unwrap();
            capability
                .publish_bytes(Path::new("leaf"), b"published")
                .unwrap();
            assert_eq!(std::fs::read(root.join("leaf")).unwrap(), b"published");
            assert_eq!(std::fs::read(&sentinel).unwrap(), b"keep");

            symlink(&outside, root.join("linked")).unwrap();
            assert!(
                capability
                    .publish_bytes(Path::new("linked/payload"), b"forbidden")
                    .is_err()
            );
            assert!(!outside.join("payload").exists());
            assert_eq!(std::fs::read(&sentinel).unwrap(), b"keep");
        }

        #[cfg(windows)]
        fn assert_no_publish_temporaries(root: &Path) {
            for entry in std::fs::read_dir(root).unwrap() {
                let entry = entry.unwrap();
                assert!(
                    !entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".cargo-dotnet-publish-"),
                    "staged publication file was left behind: {}",
                    entry.path().display()
                );
                if entry.file_type().unwrap().is_dir()
                    && !metadata_is_link_or_reparse(
                        &std::fs::symlink_metadata(entry.path()).unwrap(),
                    )
                {
                    assert_no_publish_temporaries(&entry.path());
                }
            }
        }

        #[cfg(windows)]
        #[test]
        fn windows_publication_replaces_regular_and_reparse_leaves_atomically() {
            use std::os::windows::fs::symlink_file;

            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("root");
            let outside = temp.path().join("outside");
            std::fs::create_dir(&root).unwrap();
            std::fs::create_dir(&outside).unwrap();
            let capability = DirectoryCapability::open(&root).unwrap();
            let relative = Path::new("nested/leaf");
            let leaf = root.join(relative);

            capability.publish_bytes(relative, b"initial").unwrap();
            assert_eq!(std::fs::read(&leaf).unwrap(), b"initial");
            assert_no_publish_temporaries(&root);

            capability.publish_bytes(relative, b"replacement").unwrap();
            assert_eq!(std::fs::read(&leaf).unwrap(), b"replacement");
            assert_no_publish_temporaries(&root);

            let sentinel = outside.join("sentinel");
            std::fs::write(&sentinel, b"outside-must-not-change").unwrap();
            std::fs::remove_file(&leaf).unwrap();
            symlink_file(&sentinel, &leaf)
                .expect("Windows CI must permit creating the leaf reparse regression fixture");
            assert!(metadata_is_link_or_reparse(
                &std::fs::symlink_metadata(&leaf).unwrap()
            ));

            capability
                .publish_bytes(relative, b"reparse-replaced")
                .unwrap();
            assert_eq!(std::fs::read(&leaf).unwrap(), b"reparse-replaced");
            assert_eq!(
                std::fs::read(&sentinel).unwrap(),
                b"outside-must-not-change"
            );
            assert!(!metadata_is_link_or_reparse(
                &std::fs::symlink_metadata(&leaf).unwrap()
            ));
            assert_no_publish_temporaries(&root);
        }
    }
}

/// Versioned inventory for a portable rust-dotnet SDK home.
///
/// This is intentionally in the dependency-free SDK core crate so the bundle writer, installer,
/// doctor, build-path resolver, and packaging code all consume the same layout contract. Schema 1
/// is the immutable 0.0.1 bundle shape; all newly-produced SDKs use schema 2 and carry an explicit
/// layout.
pub mod sdk {
    use std::path::{Component, Path};

    use anyhow::{Result, bail};
    use serde::{Deserialize, Serialize};

    use crate::host::HostFacts;

    pub const LEGACY_SDK_MANIFEST_SCHEMA: u32 = 1;
    pub const CURRENT_SDK_MANIFEST_SCHEMA: u32 = 2;
    pub const SDK_MANIFEST_KIND: &str = "cargo-dotnet-install-home";
    pub const SDK_MANIFEST_FILE: &str = "BUNDLE-LOCK.json";

    #[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
    pub struct SdkFile {
        pub path: String,
        pub bytes: u64,
        pub sha256: String,
        pub executable: bool,
    }

    /// A semantic leaf without which a schema-2 SDK is not product-complete.
    ///
    /// The manifest still inventories every file byte-for-byte. This smaller contract prevents a
    /// self-consistent but useless bundle (for example, an empty `crates/` tree) from verifying.
    #[derive(Debug, Clone, Eq, PartialEq)]
    pub struct RequiredSdkLeaf {
        pub path: String,
        pub executable: bool,
    }

    #[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
    pub struct SdkLayout {
        pub version: String,
        pub legacy_core: String,
        pub legacy_launcher: String,
        pub backend: String,
        pub linker: String,
        pub cargo_dotnet: String,
        pub target_spec: String,
        pub pal_root: String,
        pub overlays_root: String,
        pub msbuild_root: String,
        pub crates_root: String,
        pub interop_helpers_root: String,
    }

    impl SdkLayout {
        pub fn for_host(facts: &HostFacts) -> Self {
            Self {
                version: "VERSION".into(),
                legacy_core: "core.sh".into(),
                legacy_launcher: "cargo-dotnet".into(),
                backend: format!("bin/{}", facts.backend_dylib_name()),
                linker: format!("bin/linker{}", facts.exe_ext),
                cargo_dotnet: format!("bin/cargo-dotnet{}", facts.exe_ext),
                target_spec: "target/x86_64-unknown-dotnet.json".into(),
                pal_root: "dotnet_pal".into(),
                overlays_root: "dotnet_overlays".into(),
                msbuild_root: "msbuild".into(),
                crates_root: "crates".into(),
                interop_helpers_root: "mycorrhiza_interop_helpers".into(),
            }
        }

        /// Roots copied into an SDK bundle. Keeping this list here prevents create, setup, doctor,
        /// and installed-mode resolution from silently growing different product layouts.
        pub fn inventory_roots(&self) -> [&str; 12] {
            [
                &self.version,
                &self.legacy_core,
                &self.legacy_launcher,
                &self.backend,
                &self.linker,
                &self.cargo_dotnet,
                &self.target_spec,
                &self.pal_root,
                &self.overlays_root,
                &self.msbuild_root,
                &self.crates_root,
                &self.interop_helpers_root,
            ]
        }

        pub fn required_files(&self) -> [&str; 4] {
            [
                &self.version,
                &self.backend,
                &self.linker,
                &self.target_spec,
            ]
        }

        /// Exact semantic anchors for current SDKs. All leaves must be non-empty and carry the
        /// declared executable bit. Keep this list here so bundle creation, verification, setup,
        /// doctor, and package inventory cannot drift independently.
        pub fn required_leaves(&self, host_os: &str) -> Vec<RequiredSdkLeaf> {
            let mut leaves = vec![
                RequiredSdkLeaf {
                    path: self.version.clone(),
                    executable: false,
                },
                RequiredSdkLeaf {
                    path: self.legacy_core.clone(),
                    executable: false,
                },
                RequiredSdkLeaf {
                    path: self.legacy_launcher.clone(),
                    executable: true,
                },
                RequiredSdkLeaf {
                    path: self.backend.clone(),
                    executable: host_os != "windows",
                },
                RequiredSdkLeaf {
                    path: self.linker.clone(),
                    executable: true,
                },
                RequiredSdkLeaf {
                    path: self.cargo_dotnet.clone(),
                    executable: true,
                },
                RequiredSdkLeaf {
                    path: self.target_spec.clone(),
                    executable: false,
                },
            ];
            for (root, path) in [
                (&self.pal_root, "os/dotnet/mod.rs"),
                (&self.overlays_root, "REGISTRY.toml"),
                (&self.msbuild_root, "RustDotnet.targets"),
                (&self.msbuild_root, "RustDotnet.props"),
                (&self.crates_root, "mycorrhiza/Cargo.toml"),
                (&self.crates_root, "mycorrhiza/src/lib.rs"),
                (&self.crates_root, "dotnet_macros/Cargo.toml"),
                (&self.crates_root, "dotnet_macros/src/lib.rs"),
                (&self.crates_root, "rust-dotnet-pinvoke/Cargo.toml"),
                (&self.crates_root, "rust-dotnet-pinvoke/src/lib.rs"),
                (
                    &self.crates_root,
                    "rust-dotnet-native-contract-macros/Cargo.toml",
                ),
                (
                    &self.crates_root,
                    "rust-dotnet-native-contract-macros/src/lib.rs",
                ),
                (
                    &self.interop_helpers_root,
                    "Mycorrhiza.Interop.Helpers.csproj",
                ),
                (&self.interop_helpers_root, "ParameterRebinder.cs"),
            ] {
                leaves.push(RequiredSdkLeaf {
                    path: format!("{root}/{path}"),
                    executable: false,
                });
            }
            leaves
        }

        pub fn legacy_required_directories(&self) -> [&str; 2] {
            [&self.pal_root, &self.overlays_root]
        }

        pub fn validate(&self) -> Result<()> {
            for path in self.inventory_roots() {
                validate_relative(path)?;
            }
            let mut paths = self.inventory_roots().to_vec();
            paths.sort_unstable();
            paths.dedup();
            if paths.len() != self.inventory_roots().len() {
                bail!("SDK layout contains duplicate paths");
            }
            Ok(())
        }
    }

    #[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
    pub struct SdkManifest {
        pub schema: u32,
        pub kind: String,
        pub host_os: String,
        pub host_arch: String,
        pub host_rid: String,
        pub toolchain: String,
        pub cargo_dotnet_version: String,
        /// Missing only in immutable schema-1 / 0.0.1 bundles.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub layout: Option<SdkLayout>,
        pub files: Vec<SdkFile>,
    }

    impl SdkManifest {
        pub fn new(
            facts: &HostFacts,
            toolchain: String,
            cargo_dotnet_version: String,
            files: Vec<SdkFile>,
        ) -> Self {
            Self {
                schema: CURRENT_SDK_MANIFEST_SCHEMA,
                kind: SDK_MANIFEST_KIND.into(),
                host_os: facts.os.into(),
                host_arch: facts.arch.into(),
                host_rid: facts.host_rid.into(),
                toolchain,
                cargo_dotnet_version,
                layout: Some(SdkLayout::for_host(facts)),
                files,
            }
        }

        pub fn validate_schema_and_layout(&self) -> Result<SdkLayout> {
            if self.kind != SDK_MANIFEST_KIND {
                bail!("unsupported cargo-dotnet SDK kind: {}", self.kind);
            }
            let facts = HostFacts::for_target(
                static_host_component(&self.host_os)?,
                static_arch_component(&self.host_arch)?,
            )
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "SDK names an unsupported host tuple: {}-{}",
                    self.host_os,
                    self.host_arch
                )
            })?;
            if self.host_rid != facts.host_rid {
                bail!(
                    "SDK RID {} does not match host tuple {}-{} (expected {})",
                    self.host_rid,
                    self.host_os,
                    self.host_arch,
                    facts.host_rid
                );
            }
            let expected = SdkLayout::for_host(&facts);
            match self.schema {
                CURRENT_SDK_MANIFEST_SCHEMA => {
                    let layout = self
                        .layout
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("SDK schema 2 is missing its layout"))?;
                    layout.validate()?;
                    if layout != &expected {
                        bail!("SDK schema 2 layout does not match its host contract");
                    }
                    Ok(layout.clone())
                }
                LEGACY_SDK_MANIFEST_SCHEMA
                    if self.cargo_dotnet_version == "0.0.1" && self.layout.is_none() =>
                {
                    Ok(expected)
                }
                LEGACY_SDK_MANIFEST_SCHEMA => {
                    bail!("SDK schema 1 is read-only compatibility for exactly cargo-dotnet 0.0.1")
                }
                schema => bail!(
                    "unsupported cargo-dotnet SDK schema {schema} (expected {CURRENT_SDK_MANIFEST_SCHEMA}, or legacy 1 for 0.0.1)"
                ),
            }
        }
    }

    fn static_host_component(value: &str) -> Result<&'static str> {
        match value {
            "linux" => Ok("linux"),
            "macos" => Ok("macos"),
            "windows" => Ok("windows"),
            other => bail!("unsupported SDK host OS: {other}"),
        }
    }

    fn static_arch_component(value: &str) -> Result<&'static str> {
        match value {
            "x86_64" => Ok("x86_64"),
            "aarch64" => Ok("aarch64"),
            other => bail!("unsupported SDK host architecture: {other}"),
        }
    }

    pub fn validate_relative(path: &str) -> Result<()> {
        if path.is_empty() || path.contains('\\') {
            bail!("invalid SDK path: {path:?}");
        }
        let candidate = Path::new(path);
        if candidate.is_absolute()
            || candidate
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            bail!("unsafe SDK path: {path}");
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn manifest(schema: u32, version: &str, layout: Option<SdkLayout>) -> SdkManifest {
            let facts = HostFacts::for_target("linux", "x86_64").unwrap();
            SdkManifest {
                schema,
                kind: SDK_MANIFEST_KIND.into(),
                host_os: facts.os.into(),
                host_arch: facts.arch.into(),
                host_rid: facts.host_rid.into(),
                toolchain: "nightly-test".into(),
                cargo_dotnet_version: version.into(),
                layout,
                files: vec![],
            }
        }

        #[test]
        fn schema_two_requires_exact_layout() {
            let facts = HostFacts::for_target("linux", "x86_64").unwrap();
            let layout = SdkLayout::for_host(&facts);
            manifest(2, "0.0.2", Some(layout.clone()))
                .validate_schema_and_layout()
                .unwrap();
            assert!(
                manifest(2, "0.0.2", None)
                    .validate_schema_and_layout()
                    .is_err()
            );
            let mut wrong = layout;
            wrong.target_spec = "target/other.json".into();
            assert!(
                manifest(2, "0.0.2", Some(wrong))
                    .validate_schema_and_layout()
                    .is_err()
            );
        }

        #[test]
        fn schema_one_is_read_compatible_only_for_0_0_1() {
            manifest(1, "0.0.1", None)
                .validate_schema_and_layout()
                .unwrap();
            assert!(
                manifest(1, "0.0.2", None)
                    .validate_schema_and_layout()
                    .is_err()
            );
            assert!(
                manifest(
                    1,
                    "0.0.1",
                    Some(SdkLayout::for_host(
                        &HostFacts::for_target("linux", "x86_64").unwrap()
                    ))
                )
                .validate_schema_and_layout()
                .is_err()
            );
            assert!(
                manifest(99, "0.0.2", None)
                    .validate_schema_and_layout()
                    .is_err()
            );
        }

        #[test]
        fn layout_rejects_unsafe_paths() {
            let facts = HostFacts::for_target("linux", "x86_64").unwrap();
            let mut layout = SdkLayout::for_host(&facts);
            layout.pal_root = "../pal".into();
            assert!(layout.validate().is_err());
        }
    }
}

pub mod identity {
    #[derive(Debug, Clone)]
    pub struct ManagedIdentity {
        pub schema: u16,
        pub package_id: String,
        pub assembly_name: String,
        pub root_namespace: String,
        pub module_type: String,
    }
    impl ManagedIdentity {
        pub fn module_full_name(&self) -> String {
            format!("{}.{}", self.root_namespace, self.module_type)
        }
    }

    /// The complete user-authored managed project contract. Identity is compiler/linker-facing;
    /// namespaces and compatibility profile are host/tooling-facing, but they are validated and
    /// versioned together from one Cargo metadata table.
    #[derive(Debug, Clone)]
    pub struct ManagedProjectConfig {
        pub identity: ManagedIdentity,
        pub public_namespaces: Vec<String>,
        pub compatibility_profile: String,
    }
}

pub mod runtime {
    use serde::{Deserialize, Serialize};

    /// Managed runtime/API profile emitted by rustc_codegen_clr.
    ///
    /// Keep the variant order stable: this enum is serialized inside the cilly artifact ABI
    /// envelope. Adding or reordering variants requires an artifact schema bump.
    #[derive(
        Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
    )]
    pub enum DotnetVersion {
        #[default]
        Net10,
        /// Unity's managed `netstandard2.1` profile (not a CoreCLR runtime).
        UnityNetStandard21,
    }
    impl DotnetVersion {
        pub fn tfm(self) -> &'static str {
            match self {
                Self::Net10 => "net10.0",
                Self::UnityNetStandard21 => "netstandard2.1",
            }
        }
        pub fn as_env(self) -> &'static str {
            match self {
                Self::Net10 => "10",
                Self::UnityNetStandard21 => "unity-netstandard2.1",
            }
        }
        /// The `.ver` triplet for a BCL assembly reference.
        pub const fn assembly_ver(self) -> &'static str {
            match self {
                Self::Net10 => "10:0:0:0",
                Self::UnityNetStandard21 => "4:0:0:0",
            }
        }

        /// The parsed `.ver` tuple used by direct PE assembly-reference rows.
        pub const fn assembly_ver_tuple(self) -> (u16, u16, u16, u16) {
            match self {
                Self::Net10 => (10, 0, 0, 0),
                Self::UnityNetStandard21 => (4, 0, 0, 0),
            }
        }

        /// `Microsoft.NETCore.App` framework-version floor for runtime configuration.
        pub const fn framework_version(self) -> &'static str {
            match self {
                Self::Net10 => "10.0.0",
                Self::UnityNetStandard21 => "2.1.0",
            }
        }

        /// Runtime major version, or zero for Unity's netstandard profile.
        pub const fn major(self) -> u32 {
            match self {
                Self::Net10 => 10,
                Self::UnityNetStandard21 => 0,
            }
        }

        /// Whether this runtime exposes native subword `Interlocked` overloads.
        pub const fn supports_subword_interlocked(self) -> bool {
            self.major() >= 9
        }
    }

    impl std::fmt::Display for DotnetVersion {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Net10 => f.write_str(".NET 10"),
                Self::UnityNetStandard21 => f.write_str("Unity netstandard2.1"),
            }
        }
    }
    impl std::str::FromStr for DotnetVersion {
        type Err = String;
        fn from_str(s: &str) -> Result<Self, Self::Err> {
            match s.trim() {
                "10" | "net10" | "net10.0" => Ok(Self::Net10),
                "unity" | "unity-netstandard2.1" | "netstandard2.1" => Ok(Self::UnityNetStandard21),
                other => Err(format!(
                    "--dotnet: unsupported value {other:?}; rust-dotnet 0.0.2 supports .NET 10 or Unity netstandard2.1"
                )),
            }
        }
    }
}
