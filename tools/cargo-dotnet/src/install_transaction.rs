//! Lock-coordinated activation of an SDK home and its optional Cargo front-end.
//!
//! Setup and bundle installation stage complete inputs before entering this module. Activation
//! then swaps the old objects aside, promotes the new objects, validates them, and rolls back on
//! an ordinary error. The lock leases below keep normal consumers from observing a mid-activation
//! home; crash recovery is intentionally outside this small activation primitive.

#[cfg(windows)]
use std::fs::OpenOptions;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use fs2::FileExt;

#[derive(Debug)]
pub(crate) struct CliActivation {
    staged: PathBuf,
    destination: PathBuf,
}

impl CliActivation {
    pub(crate) fn new(staged: PathBuf, destination: PathBuf) -> Self {
        Self {
            staged,
            destination,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RollbackDisposition {
    /// Return caller-owned setup inputs when activation fails.
    RestoreInputs,
    /// Let the temporary staging owners discard inputs when activation fails.
    DiscardInputs,
}

/// Shared lifetime lease for a process consuming one installed SDK home. Activations take the
/// exclusive side of the same lock, so they cannot rename the home out from under a consumer.
pub(crate) struct InstalledHomeLease(File);

impl InstalledHomeLease {
    pub(crate) fn acquire(home: &Path) -> Result<Self> {
        let locations = locations(home)?;
        let file = open_lock(&locations.lock, "installed SDK lifetime")?;
        FileExt::lock_shared(&file).with_context(|| {
            format!(
                "locking installed SDK home for use {}",
                locations.home.display()
            )
        })?;
        Ok(Self(file))
    }
}

impl Drop for InstalledHomeLease {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}

/// Shared coordination held while an installed driver resolves and starts its home driver.
/// Activation takes the exclusive side before taking the home lock, closing the pathname-to-lease
/// hand-off without requiring a durable transaction journal.
pub(crate) struct LaunchCoordinationLease(File);

impl LaunchCoordinationLease {
    pub(crate) fn acquire(home: &Path) -> Result<Self> {
        let locations = locations(home)?;
        let file = open_lock(&locations.handoff_lock, "installed SDK launch coordination")?;
        FileExt::lock_shared(&file).with_context(|| {
            format!(
                "locking installed SDK launch coordination for {}",
                locations.home.display()
            )
        })?;
        Ok(Self(file))
    }
}

impl Drop for LaunchCoordinationLease {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}

struct Locations {
    home: PathBuf,
    parent: PathBuf,
    handoff_lock: PathBuf,
    lock: PathBuf,
}

struct TransactionLock(File);

impl TransactionLock {
    fn acquire(path: &Path) -> Result<Self> {
        let file = open_lock(path, "SDK activation")?;
        FileExt::lock_exclusive(&file)
            .with_context(|| format!("locking SDK activation {}", path.display()))?;
        Ok(Self(file))
    }
}

impl Drop for TransactionLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}

/// Hold every resource lock in a stable order. Homes can share one Cargo-bin destination, so
/// activation locks both the home and front-end resources before changing either one.
struct TransactionLocks {
    _locks: Vec<TransactionLock>,
}

impl TransactionLocks {
    fn acquire(paths: impl IntoIterator<Item = PathBuf>) -> Result<Self> {
        let mut paths = paths.into_iter().collect::<Vec<_>>();
        paths.sort();
        paths.dedup();
        Ok(Self {
            _locks: paths
                .iter()
                .map(|path| TransactionLock::acquire(path))
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

fn open_lock(path: &Path, label: &str) -> Result<File> {
    let file = rust_dotnet_sdk_core::safe_fs::create_or_open_regular_nofollow(path)
        .with_context(|| format!("opening {label} lock {}", path.display()))?;
    Ok(file)
}

fn locations(home: &Path) -> Result<Locations> {
    let home = crate::path_safety::planned_absolute(home)?;
    let parent = home
        .parent()
        .context("SDK install home has no parent")?
        .to_path_buf();
    let id = crate::content_cache::digest_parts([
        b"cargo-dotnet-activation-v1".as_slice(),
        home.as_os_str().as_encoded_bytes(),
    ]);
    Ok(Locations {
        handoff_lock: parent.join(".cargo-dotnet-0-launch-handoff.lock"),
        lock: parent.join(format!(".cargo-dotnet-activation-{id}.lock")),
        home,
        parent,
    })
}

fn cli_destination_lock(destination: &Path) -> Result<PathBuf> {
    let destination = planned_regular_file_destination(destination)?;
    let parent = destination
        .parent()
        .context("cargo-dotnet destination has no parent")?;
    let id = crate::content_cache::digest_parts([
        b"cargo-dotnet-cli-destination-lock-v1".as_slice(),
        destination.as_os_str().as_encoded_bytes(),
    ]);
    Ok(parent.join(format!(".cargo-dotnet-cli-destination-{id}.lock")))
}

pub(crate) fn activate<B, C, V>(
    staged_home: &Path,
    home: &Path,
    cli: Option<CliActivation>,
    rollback_disposition: RollbackDisposition,
    before_backup: B,
    before_cli: C,
    validate: V,
) -> Result<()>
where
    B: FnOnce() -> Result<()>,
    C: FnOnce() -> Result<()>,
    V: FnOnce() -> Result<()>,
{
    crate::path_safety::require_owned_or_empty_sdk_home(home)?;
    let locations = locations(home)?;
    let cli = cli.map(prepare_cli).transpose()?;
    let cli_lock = cli
        .as_ref()
        .map(|cli| cli_destination_lock(&cli.destination))
        .transpose()?;
    let _locks = TransactionLocks::acquire(
        [locations.handoff_lock.clone(), locations.lock.clone()]
            .into_iter()
            .chain(cli_lock),
    )?;

    let staged_home = canonical_regular_directory(staged_home, "staged SDK home")?;
    sync_tree(&staged_home)?;
    before_backup()?;

    let home_backup_dir = tempfile::Builder::new()
        .prefix(".cargo-dotnet-home-backup-")
        .tempdir_in(&locations.parent)?;
    let old_home = home_backup_dir.path().join("previous");
    let failed_home = home_backup_dir.path().join("failed-new");
    let cli_backup_dir = cli
        .as_ref()
        .map(|cli| {
            let parent = cli
                .destination
                .parent()
                .context("cargo-dotnet destination has no parent")?;
            tempfile::Builder::new()
                .prefix(".cargo-dotnet-cli-backup-")
                .tempdir_in(parent)
                .context("creating CLI activation backup directory")
        })
        .transpose()?;
    let (old_cli, failed_cli) = cli_backup_dir
        .as_ref()
        .map(|dir| (dir.path().join("previous"), dir.path().join("failed-new")))
        .unzip();

    let had_home = fs::symlink_metadata(&locations.home).is_ok();
    let had_cli = cli
        .as_ref()
        .is_some_and(|cli| fs::symlink_metadata(&cli.destination).is_ok());
    let mut home_backed_up = false;
    let mut cli_backed_up = false;
    let mut home_promoted = false;
    let mut cli_promoted = false;
    let mut before_cli = Some(before_cli);

    let transaction = (|| -> Result<()> {
        if let Some(cli) = &cli {
            if had_cli {
                fs::rename(&cli.destination, old_cli.as_ref().unwrap())
                    .context("backing up previous cargo-dotnet front-end")?;
                cli_backed_up = true;
            }
            before_cli
                .take()
                .context("front-end activation callback was already used")?()?;
            fs::rename(&cli.staged, &cli.destination)
                .context("activating cargo-dotnet front-end")?;
            cli_promoted = true;
            sync_rename(&cli.staged, &cli.destination)?;
        }
        if had_home {
            fs::rename(&locations.home, &old_home).context("backing up previous SDK home")?;
            home_backed_up = true;
            crate::path_safety::require_owned_or_empty_sdk_home(&old_home)?;
        }
        fs::rename(&staged_home, &locations.home).context("activating staged SDK home")?;
        home_promoted = true;
        sync_rename(&staged_home, &locations.home)?;
        if cli.is_none() {
            before_cli
                .take()
                .context("front-end activation callback was already used")?()?;
        }
        validate()
    })();

    match transaction {
        Ok(()) => Ok(()),
        Err(error) => {
            let rollback = rollback(
                &locations,
                &staged_home,
                cli.as_ref(),
                old_home,
                failed_home,
                old_cli.as_deref(),
                failed_cli.as_deref(),
                had_home,
                had_cli,
                home_backed_up,
                cli_backed_up,
                home_promoted,
                cli_promoted,
                rollback_disposition,
            );
            match rollback {
                Ok(()) => Err(error).context("SDK/front-end activation rolled back"),
                Err(rollback) => {
                    let home_backup = home_backup_dir.keep();
                    let cli_backup = cli_backup_dir.map(tempfile::TempDir::keep);
                    bail!(
                        "SDK/front-end activation failed ({error:#}); rollback also failed: {rollback:#}; backups: {}, {}",
                        home_backup.display(),
                        cli_backup
                            .as_ref()
                            .map_or_else(|| "none".to_owned(), |path| path.display().to_string())
                    )
                }
            }
        }
    }
}

fn prepare_cli(cli: CliActivation) -> Result<PreparedCli> {
    Ok(PreparedCli {
        staged: canonical_regular_file(&cli.staged, "staged cargo-dotnet front-end")?,
        destination: planned_regular_file_destination(&cli.destination)?,
    })
}

struct PreparedCli {
    staged: PathBuf,
    destination: PathBuf,
}

#[allow(clippy::too_many_arguments)]
fn rollback(
    locations: &Locations,
    staged_home: &Path,
    cli: Option<&PreparedCli>,
    old_home: PathBuf,
    failed_home: PathBuf,
    old_cli: Option<&Path>,
    failed_cli: Option<&Path>,
    had_home: bool,
    had_cli: bool,
    home_backed_up: bool,
    cli_backed_up: bool,
    home_promoted: bool,
    cli_promoted: bool,
    disposition: RollbackDisposition,
) -> Result<()> {
    let mut errors = Vec::new();
    if cli_promoted {
        if let Some(cli) = cli
            && let Some(failed_cli) = failed_cli
            && let Err(error) = fs::rename(&cli.destination, failed_cli)
        {
            errors.push(format!("move failed front-end aside: {error}"));
        }
    }
    if home_promoted && let Err(error) = fs::rename(&locations.home, &failed_home) {
        errors.push(format!("move failed SDK home aside: {error}"));
    }
    if home_backed_up && let Err(error) = fs::rename(&old_home, &locations.home) {
        errors.push(format!("restore previous SDK home: {error}"));
    }
    if cli_backed_up
        && let (Some(old_cli), Some(cli)) = (old_cli, cli)
        && let Err(error) = fs::rename(old_cli, &cli.destination)
    {
        errors.push(format!("restore previous front-end: {error}"));
    }

    if errors.is_empty() {
        if disposition == RollbackDisposition::RestoreInputs {
            if home_promoted && failed_home.exists() {
                fs::rename(&failed_home, staged_home).context("restoring staged SDK home")?;
            }
            if cli_promoted
                && let (Some(cli), Some(failed_cli)) = (cli, failed_cli)
                && failed_cli.exists()
            {
                fs::rename(failed_cli, &cli.staged)
                    .context("restoring staged cargo-dotnet front-end")?;
            }
        }
        if had_home && !locations.home.exists() {
            errors.push("previous SDK home is not recoverable".into());
        }
        if had_cli
            && let Some(cli) = cli
            && !cli.destination.exists()
        {
            errors.push("previous cargo-dotnet front-end is not recoverable".into());
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        bail!("{}", errors.join("; "))
    }
}

fn canonical_regular_directory(path: &Path, label: &str) -> Result<PathBuf> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("inspecting {label} {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!("{label} is not a regular directory: {}", path.display());
    }
    fs::canonicalize(path).with_context(|| format!("resolving {label} {}", path.display()))
}

fn canonical_regular_file(path: &Path, label: &str) -> Result<PathBuf> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("inspecting {label} {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!("{label} is not a regular file: {}", path.display());
    }
    fs::canonicalize(path).with_context(|| format!("resolving {label} {}", path.display()))
}

fn planned_regular_file_destination(path: &Path) -> Result<PathBuf> {
    if let Ok(metadata) = fs::symlink_metadata(path)
        && (metadata.file_type().is_symlink() || !metadata.is_file())
    {
        bail!(
            "cargo-dotnet front-end destination is not a regular file: {}",
            path.display()
        );
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let parent = fs::canonicalize(parent)?;
    let name = path
        .file_name()
        .context("cargo-dotnet destination has no filename")?;
    Ok(parent.join(name))
}

/// Flush every regular file in a staged tree before it is promoted. Staged SDKs are expected to
/// contain ordinary files and directories only.
pub(crate) fn sync_tree(path: &Path) -> Result<()> {
    let mut entries = fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let entry_path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            bail!(
                "staged SDK home contains a symlink: {}",
                entry_path.display()
            );
        }
        if file_type.is_dir() {
            sync_tree(&entry_path)?;
        } else if file_type.is_file() {
            File::open(&entry_path)?.sync_all()?;
        } else {
            bail!(
                "staged SDK home contains an unsupported entry: {}",
                entry_path.display()
            );
        }
    }
    sync_directory(path)
}

fn sync_rename(source: &Path, destination: &Path) -> Result<()> {
    if let Some(parent) = source.parent() {
        sync_directory(parent)?;
    }
    if destination.parent() != source.parent()
        && let Some(parent) = destination.parent()
    {
        sync_directory(parent)?;
    }
    Ok(())
}

#[cfg(unix)]
pub(crate) fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)
        .with_context(|| format!("opening directory for fsync: {}", path.display()))?
        .sync_all()
        .with_context(|| format!("fsync directory: {}", path.display()))
}

#[cfg(windows)]
pub(crate) fn sync_directory(path: &Path) -> Result<()> {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
        .with_context(|| format!("opening directory for flush: {}", path.display()))?
        .sync_all()
        .with_context(|| format!("flushing directory: {}", path.display()))
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn sync_directory(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn owned_home(path: &Path, marker: &[u8]) {
        fs::create_dir_all(path).unwrap();
        fs::write(path.join("marker"), marker).unwrap();
        fs::write(
            path.join("VERSION"),
            b"schema = 1\nrelease_tag = untagged\nhost_rid = test\ntoolchain = nightly\n",
        )
        .unwrap();
    }

    #[test]
    fn activation_rolls_back_home_and_cli_and_can_restore_inputs() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let staged_home = temp.path().join("staged-home");
        let destination = temp.path().join("bin/cargo-dotnet");
        let staged_cli = temp.path().join("staged-cli");
        owned_home(&home, b"old-home");
        owned_home(&staged_home, b"new-home");
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(&destination, b"old-cli").unwrap();
        fs::write(&staged_cli, b"new-cli").unwrap();

        let error = activate(
            &staged_home,
            &home,
            Some(CliActivation::new(staged_cli.clone(), destination.clone())),
            RollbackDisposition::RestoreInputs,
            || Ok(()),
            || Ok(()),
            || bail!("validation failed"),
        )
        .unwrap_err();
        assert!(error.to_string().contains("rolled back"));
        assert_eq!(fs::read(&home.join("marker")).unwrap(), b"old-home");
        assert_eq!(fs::read(&destination).unwrap(), b"old-cli");
        assert_eq!(fs::read(&staged_home.join("marker")).unwrap(), b"new-home");
        assert_eq!(fs::read(&staged_cli).unwrap(), b"new-cli");
    }

    #[test]
    fn different_sdk_homes_serialize_on_one_cli_destination() {
        use std::sync::mpsc;
        use std::time::Duration;

        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("bin/cargo-dotnet");
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(&destination, b"old-cli").unwrap();
        for name in ["a", "b"] {
            owned_home(&temp.path().join(format!("home-{name}")), b"old-home");
            owned_home(
                &temp.path().join(format!("staged-home-{name}")),
                name.as_bytes(),
            );
            fs::write(temp.path().join(format!("staged-cli-{name}")), name).unwrap();
        }

        let (a_entered_tx, a_entered_rx) = mpsc::channel();
        let (release_a_tx, release_a_rx) = mpsc::channel();
        let root = temp.path().to_path_buf();
        let destination_a = destination.clone();
        let thread_a = std::thread::spawn(move || {
            activate(
                &root.join("staged-home-a"),
                &root.join("home-a"),
                Some(CliActivation::new(root.join("staged-cli-a"), destination_a)),
                RollbackDisposition::DiscardInputs,
                || {
                    a_entered_tx.send(()).unwrap();
                    release_a_rx.recv().unwrap();
                    Ok(())
                },
                || Ok(()),
                || Ok(()),
            )
        });
        a_entered_rx.recv().unwrap();

        let (b_entered_tx, b_entered_rx) = mpsc::channel();
        let root = temp.path().to_path_buf();
        let destination_b = destination.clone();
        let thread_b = std::thread::spawn(move || {
            activate(
                &root.join("staged-home-b"),
                &root.join("home-b"),
                Some(CliActivation::new(root.join("staged-cli-b"), destination_b)),
                RollbackDisposition::DiscardInputs,
                || {
                    b_entered_tx.send(()).unwrap();
                    Ok(())
                },
                || Ok(()),
                || Ok(()),
            )
        });
        assert!(
            b_entered_rx
                .recv_timeout(Duration::from_millis(150))
                .is_err(),
            "second home entered activation while the shared CLI lock was held"
        );
        release_a_tx.send(()).unwrap();
        thread_a.join().unwrap().unwrap();
        b_entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        thread_b.join().unwrap().unwrap();
        assert_eq!(fs::read(destination).unwrap(), b"b");
    }

    #[test]
    fn shared_installed_home_lease_blocks_no_cli_activation_for_full_consumer_lifetime() {
        use std::sync::mpsc;
        use std::time::Duration;

        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let staged_home = temp.path().join("staged-home");
        owned_home(&home, b"old-home");
        owned_home(&staged_home, b"new-home");
        let lease = InstalledHomeLease::acquire(&home).unwrap();
        let (entered_tx, entered_rx) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            activate(
                &staged_home,
                &home,
                None,
                RollbackDisposition::DiscardInputs,
                || {
                    entered_tx.send(()).unwrap();
                    Ok(())
                },
                || Ok(()),
                || Ok(()),
            )
        });

        assert!(
            entered_rx.recv_timeout(Duration::from_millis(150)).is_err(),
            "activation entered while a normal command held the SDK home"
        );
        drop(lease);
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        thread.join().unwrap().unwrap();
        assert_eq!(
            fs::read(temp.path().join("home/marker")).unwrap(),
            b"new-home"
        );
    }

    #[test]
    fn direct_home_driver_nested_under_repo_is_still_installed() {
        use std::process::Stdio;
        use std::time::{Duration, Instant};

        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("rustc_codegen_clr");
        fs::create_dir_all(repo.join("feasibility")).unwrap();
        fs::write(repo.join("feasibility/_cargo_dotnet_core.sh"), b"core").unwrap();
        fs::write(repo.join("x86_64-unknown-dotnet.json"), b"{}").unwrap();
        let home = repo.join("installed-sdk");
        sealed_bootstrap_home(&home);
        let driver = home
            .join("bin")
            .join(format!("cargo-dotnet{}", std::env::consts::EXE_SUFFIX));
        let ready = temp.path().join("nested-home-ready");
        let release = temp.path().join("nested-home-release");
        fs::write(&release, b"release").unwrap();
        let mut child = Command::new(driver)
            .args([
                "--exact",
                "install_transaction::tests::native_bootstrap_lease_helper",
                "--nocapture",
            ])
            .env("CARGO_DOTNET_HOME", &home)
            .env("CARGO_DOTNET_TEST_NATIVE_BOOTSTRAP_HELPER", "1")
            .env("CARGO_DOTNET_TEST_LAUNCHER_LEASE_READY", &ready)
            .env("CARGO_DOTNET_TEST_LAUNCHER_LEASE_RELEASE", &release)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !ready.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        if !ready.exists() {
            let _ = child.kill();
            panic!("driver nested under checkout bypassed installed bootstrap");
        }
        assert!(child.wait().unwrap().success());
    }

    fn sealed_bootstrap_home(path: &Path) {
        let facts = crate::host::HostFacts::detect();
        let layout = rust_dotnet_sdk_core::sdk::SdkLayout::for_host(&facts);
        for required in layout.required_leaves(facts.os) {
            if required.path == layout.cargo_dotnet {
                continue;
            }
            let leaf = path.join(&required.path);
            fs::create_dir_all(leaf.parent().unwrap()).unwrap();
            fs::write(&leaf, format!("bootstrap test {}", required.path)).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                fs::set_permissions(
                    &leaf,
                    fs::Permissions::from_mode(if required.executable { 0o755 } else { 0o644 }),
                )
                .unwrap();
            }
        }
        let build_id = crate::installed_bootstrap::EMBEDDED_DRIVER_BUILD_ID;
        let source_digest = build_id
            .strip_prefix("source-sha256:")
            .expect("test driver build ID is source-bound");
        fs::write(
            path.join("VERSION"),
            format!(
                "schema = 1\ninventory_required = true\ngit_rev = bootstrap-test\nrelease_tag = rust-dotnet-v{}\nsource_tree_sha256 = {source_digest}\ndriver_build_id = {build_id}\ncargo_dotnet_version = {}\nhost_rid = {}\ntoolchain = {}\n",
                env!("CARGO_PKG_VERSION"),
                env!("CARGO_PKG_VERSION"),
                facts.host_rid,
                crate::mode::DEFAULT_TOOLCHAIN,
            ),
        )
        .unwrap();
        crate::bundle::seal_install_home(path, &std::env::current_exe().unwrap()).unwrap();
    }

    #[test]
    fn native_bootstrap_lease_helper() {
        if std::env::var_os("CARGO_DOTNET_TEST_NATIVE_BOOTSTRAP_HELPER").is_none() {
            return;
        }
        match crate::installed_bootstrap::enter().unwrap() {
            crate::installed_bootstrap::Entry::Exit(code) => assert_eq!(code, 0),
            crate::installed_bootstrap::Entry::Continue(lease) => {
                let _lease = lease.expect("installed home driver did not acquire lifetime lease");
                let ready = PathBuf::from(
                    std::env::var_os("CARGO_DOTNET_TEST_LAUNCHER_LEASE_READY").unwrap(),
                );
                let release = PathBuf::from(
                    std::env::var_os("CARGO_DOTNET_TEST_LAUNCHER_LEASE_RELEASE").unwrap(),
                );
                fs::write(ready, b"leased").unwrap();
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                while !release.exists() {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "native bootstrap lease helper timed out"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
        }
    }

    #[test]
    fn compatibility_shell_has_no_perl_or_forgeable_lock_authority() {
        let launcher = fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../feasibility/cargo-dotnet"),
        )
        .unwrap();
        assert!(!launcher.contains("CARGO_DOTNET_LAUNCHER_COORDINATION_HELD"));
        assert!(!launcher.contains("flock("));
        assert!(!launcher.contains("exec perl"));
    }
}
