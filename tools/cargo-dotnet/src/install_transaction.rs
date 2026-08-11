//! Durable activation of an SDK home and its optional Cargo front-end.
//!
//! Every destructive rename is preceded by a synced journal and uses deterministic backup
//! locations. A later setup or bundle-install invocation can therefore roll an interrupted
//! transaction back, or finish cleaning a transaction whose validation was durably committed.

use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const JOURNAL_SCHEMA: u32 = 2;
const OWNER_FILE: &str = "OWNER";
const CLI_BOOTSTRAP_FILE: &str = "bootstrap.json";

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

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Prepared,
    CliBackedUp,
    CliPromoted,
    HomeBackedUp,
    HomePromoted,
    RolledBack,
    Committed,
}

impl Phase {
    fn sequence(self) -> u8 {
        match self {
            Self::Prepared => 0,
            Self::CliBackedUp => 1,
            Self::CliPromoted => 2,
            Self::HomeBackedUp => 3,
            Self::HomePromoted => 4,
            Self::RolledBack => 5,
            Self::Committed => 6,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CliJournal {
    staged: PathBuf,
    destination: PathBuf,
    replacement: PathBuf,
    backup: PathBuf,
    failed_new: PathBuf,
    had_previous: bool,
    previous_sha256: Option<String>,
    staged_sha256: String,
}

struct PreparedCli {
    staged: PathBuf,
    destination: PathBuf,
    transaction: PathBuf,
    had_previous: bool,
    previous_sha256: Option<String>,
    staged_sha256: String,
}

struct RollbackInputs {
    staged_home: PathBuf,
    staged_cli: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RollbackDisposition {
    /// Return caller-owned setup inputs so the caller can inspect or retry them synchronously.
    RestoreInputs,
    /// Consume transaction-owned extraction inputs; recovery must leave no staged payload behind.
    DiscardInputs,
}

struct InitialBootstrapGuard {
    parent: PathBuf,
    directory: PathBuf,
    id: String,
    armed: bool,
}

impl InitialBootstrapGuard {
    fn new(locations: &Locations, cli: &PreparedCli) -> Result<Self> {
        publish_cli_bootstrap(locations, cli)?;
        Ok(Self {
            parent: cli
                .transaction
                .parent()
                .context("CLI activation directory has no parent")?
                .to_path_buf(),
            directory: cli.transaction.clone(),
            id: locations.id.clone(),
            armed: true,
        })
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for InitialBootstrapGuard {
    fn drop(&mut self) {
        if self.armed && self.directory.exists() {
            let _ = remove_owned_transaction_dir(&self.parent, &self.directory, &self.id);
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CliBootstrap {
    schema: u32,
    transaction_id: String,
    home: PathBuf,
    destination: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Journal {
    schema: u32,
    transaction_id: String,
    phase: Phase,
    home: PathBuf,
    staged_home: PathBuf,
    home_backup: PathBuf,
    failed_home: PathBuf,
    had_home: bool,
    staged_home_sha256: String,
    cli: Option<CliJournal>,
}

struct Locations {
    id: String,
    home: PathBuf,
    parent: PathBuf,
    transaction: PathBuf,
    handoff_lock: PathBuf,
    lock: PathBuf,
}

struct TransactionLock(File);

impl TransactionLock {
    fn acquire(path: &Path) -> Result<Self> {
        let file = rust_dotnet_sdk_core::safe_fs::create_or_open_regular_nofollow(path)
            .with_context(|| format!("opening SDK activation lock {}", path.display()))?;
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

/// Shared lifetime lease for a process consuming one installed SDK home. Activations and recovery
/// take the exclusive side of this exact lock, so they cannot rename the home out from under a
/// normal command or one of its rustc-wrapper children.
pub(crate) struct InstalledHomeLease(File);

impl InstalledHomeLease {
    pub(crate) fn acquire(home: &Path) -> Result<Self> {
        let locations = locations(home)?;
        let file = rust_dotnet_sdk_core::safe_fs::create_or_open_regular_nofollow(&locations.lock)
            .with_context(|| {
                format!(
                    "opening installed SDK lifetime lock {}",
                    locations.lock.display()
                )
            })?;
        FileExt::lock_shared(&file).with_context(|| {
            format!(
                "locking installed SDK home for use {}",
                locations.home.display()
            )
        })?;
        Ok(Self(file))
    }
}

/// Shared coordination held from before an installed driver pathname is opened until the child
/// has acquired its per-home lifetime lease. Activation and recovery take the exclusive side of
/// this same fixed lock before they can replace either the home or Cargo-bin bootstrap.
pub(crate) struct LaunchCoordinationLease(File);

impl LaunchCoordinationLease {
    pub(crate) fn acquire(home: &Path) -> Result<Self> {
        let locations = locations(home)?;
        let file =
            rust_dotnet_sdk_core::safe_fs::create_or_open_regular_nofollow(&locations.handoff_lock)
                .with_context(|| {
                    format!(
                        "opening installed SDK launch coordination lock {}",
                        locations.handoff_lock.display()
                    )
                })?;
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

pub(crate) fn has_pending(home: &Path) -> Result<bool> {
    let locations = locations(home)?;
    Ok(locations.transaction.exists())
}

impl Drop for InstalledHomeLease {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}

/// Hold every filesystem resource lock in canonical lexical order. A front-end destination is
/// independent of the SDK home, so two homes that share one Cargo bin directory must serialize
/// on both resources to avoid mixed rollback.
struct TransactionLocks {
    _locks: Vec<TransactionLock>,
}

impl TransactionLocks {
    fn acquire(paths: impl IntoIterator<Item = PathBuf>) -> Result<Self> {
        let mut paths = paths.into_iter().collect::<Vec<_>>();
        paths.sort();
        paths.dedup();
        let mut locks = Vec::with_capacity(paths.len());
        for path in paths {
            locks.push(TransactionLock::acquire(&path)?);
        }
        Ok(Self { _locks: locks })
    }
}

pub(crate) fn recover(home: &Path) -> Result<bool> {
    let locations = locations(home)?;
    let _locks = acquire_recovery_locks(&locations)?;
    cleanup_home_recovery_artifacts(&locations)?;
    recover_locked(&locations)
}

/// Recover an activation associated with the currently-installed cargo-dotnet executable.
///
/// The bootstrap is published beside the destination before that destination is atomically
/// replaced. Consequently every post-promotion process can find the SDK-home journal without
/// relying on the SDK home itself. A held activation lock means the installer that launched this
/// process is still alive; normal commands wait for it and fail closed rather than running
/// against a mixed home/front-end. Rustc-wrapper children dispatch before this function.
#[cfg(test)]
pub(crate) fn recover_pending_for_current_cli() -> Result<bool> {
    let executable = std::env::current_exe().context("locating cargo-dotnet for recovery")?;
    let Some(parent) = executable.parent() else {
        return Ok(false);
    };
    let parent = fs::canonicalize(parent)?;
    let destination = planned_regular_file_destination(&executable)?;
    let mut candidates = fs::read_dir(&parent)?.collect::<std::io::Result<Vec<_>>>()?;
    candidates.sort_by_key(|entry| entry.file_name());
    let mut recovered = false;
    for entry in candidates {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with(".cargo-dotnet-cli-activation-")
            || name.contains(".initializing-")
            || name.contains(".tombstone-")
        {
            continue;
        }
        let file_type = entry.file_type()?;
        if !file_type.is_dir() || file_type.is_symlink() {
            continue;
        }
        let bootstrap_path = entry.path().join(CLI_BOOTSTRAP_FILE);
        let bootstrap: CliBootstrap = match read_json_regular(&bootstrap_path) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if bootstrap.schema != JOURNAL_SCHEMA || bootstrap.destination != destination {
            continue;
        }
        let locations = locations(&bootstrap.home)?;
        if bootstrap.transaction_id != locations.id || entry.path() != parent.join(name) {
            continue;
        }
        let _locks = TransactionLocks::acquire([
            locations.handoff_lock.clone(),
            locations.lock.clone(),
            cli_destination_lock(&destination)?,
        ])?;
        cleanup_home_recovery_artifacts(&locations)?;
        let did_recover = recover_locked(&locations)?;
        if !locations.transaction.exists() && entry.path().exists() {
            remove_owned_transaction_dir(&parent, &entry.path(), &locations.id)?;
        }
        recovered |= did_recover;
    }
    Ok(recovered)
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
    let cli_lock = cli
        .as_ref()
        .map(|cli| planned_regular_file_destination(&cli.destination))
        .transpose()?
        .map(|destination| cli_destination_lock(&destination))
        .transpose()?;
    let _locks = TransactionLocks::acquire(
        [locations.handoff_lock.clone(), locations.lock.clone()]
            .into_iter()
            .chain(cli_lock),
    )?;
    if recover_locked(&locations)? {
        eprintln!(
            "==> recovered interrupted SDK activation for {}",
            locations.home.display()
        );
    }

    before_backup()?;
    let staged_home = canonical_regular_directory(staged_home, "staged SDK home")?;
    sync_tree(&staged_home)?;
    let staged_home_sha256 = crate::content_cache::tree_digest(&staged_home)?;
    let had_home = fs::symlink_metadata(&locations.home).is_ok();
    let cli = cli.map(|cli| prepare_cli(&locations, cli)).transpose()?;
    let rollback_inputs = RollbackInputs {
        staged_home: staged_home.clone(),
        staged_cli: cli.as_ref().map(|cli| cli.staged.clone()),
    };
    let mut journal =
        publish_initial_journal(&locations, &staged_home, staged_home_sha256, had_home, cli)?;

    let mut before_cli = Some(before_cli);
    let transaction = (|| -> Result<()> {
        if let Some(cli) = &journal.cli {
            if cli.had_previous {
                copy_regular_file_create_new(
                    &cli.destination,
                    &cli.backup,
                    "previous cargo-dotnet front-end",
                )?;
            }
            test_crash_point("after_cli_backup_rename");
            test_crash_point("after_cli_backup_copy");
            if cli.had_previous {
                validate_regular_file_hash(
                    &cli.backup,
                    cli.previous_sha256
                        .as_deref()
                        .context("previous CLI hash is missing")?,
                    "previous cargo-dotnet front-end",
                )?;
            }
            journal.phase = Phase::CliBackedUp;
            write_state(&locations, &journal)?;

            before_cli
                .take()
                .context("front-end activation callback was already used")?()?;
            copy_regular_file_create_new(
                &cli.staged,
                &cli.replacement,
                "staged cargo-dotnet replacement",
            )?;
            validate_regular_file_hash(
                &cli.replacement,
                &cli.staged_sha256,
                "staged cargo-dotnet replacement",
            )?;
            atomic_replace_file(&cli.replacement, &cli.destination)
                .context("atomically activating cargo-dotnet front-end")?;
            sync_rename(&cli.replacement, &cli.destination)?;
            test_crash_point("after_cli_promote_rename");
            validate_regular_file_hash(
                &cli.destination,
                &cli.staged_sha256,
                "activated cargo-dotnet front-end",
            )?;
            journal.phase = Phase::CliPromoted;
            write_state(&locations, &journal)?;
        }
        if journal.had_home {
            fs::rename(&journal.home, &journal.home_backup)
                .context("backing up previous SDK home")?;
            sync_rename(&journal.home, &journal.home_backup)?;
            test_crash_point("after_home_backup_rename");
            crate::path_safety::require_owned_or_empty_sdk_home(&journal.home_backup)?;
            journal.phase = Phase::HomeBackedUp;
            write_state(&locations, &journal)?;
        }
        fs::rename(&journal.staged_home, &journal.home).context("activating staged SDK home")?;
        sync_rename(&journal.staged_home, &journal.home)?;
        test_crash_point("after_home_promote_rename");
        journal.phase = Phase::HomePromoted;
        write_state(&locations, &journal)?;
        if journal.cli.is_none() {
            before_cli
                .take()
                .context("front-end activation callback was already used")?()?;
        }

        validate()?;
        journal.phase = Phase::Committed;
        write_state(&locations, &journal)?;
        test_crash_point("after_commit_journal");
        Ok(())
    })();

    match transaction {
        Ok(()) => finish_committed(&locations, &journal)
            .context("finishing committed SDK/front-end activation"),
        Err(error) => match rollback(
            &locations,
            &mut journal,
            (rollback_disposition == RollbackDisposition::RestoreInputs)
                .then_some(&rollback_inputs),
        ) {
            Ok(()) => Err(error).context("SDK/front-end activation rolled back"),
            Err(rollback) => bail!(
                "SDK/front-end activation failed ({error:#}); rollback also failed: {rollback:#}; recoverable journal: {}",
                locations.transaction.display()
            ),
        },
    }
}

fn acquire_recovery_locks(locations: &Locations) -> Result<TransactionLocks> {
    loop {
        let mut expected = vec![locations.handoff_lock.clone(), locations.lock.clone()];
        if locations.transaction.exists()
            && let Ok(journal) = load_latest_state(locations)
            && let Some(cli) = journal.cli
        {
            expected.push(cli_destination_lock(&cli.destination)?);
        }
        let locks = TransactionLocks::acquire(expected.clone())?;

        // The home lock now freezes journal publication. If a CLI resource appeared while we
        // waited, release and reacquire the complete sorted set instead of violating lock order.
        let mut actual = vec![locations.handoff_lock.clone(), locations.lock.clone()];
        if locations.transaction.exists() {
            let journal = load_latest_state(locations)?;
            if let Some(cli) = journal.cli {
                actual.push(cli_destination_lock(&cli.destination)?);
            }
        }
        expected.sort();
        expected.dedup();
        actual.sort();
        actual.dedup();
        if actual == expected {
            return Ok(locks);
        }
        drop(locks);
    }
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
        transaction: parent.join(format!(".cargo-dotnet-activation-{id}")),
        // The compatibility shell can compute this name without duplicating Rust's path hashing.
        // It holds the shared side before opening the replaceable home driver. Activations take
        // the exclusive side before the per-home hash lock, closing the exec-to-lease gap.
        handoff_lock: parent.join(".cargo-dotnet-0-launch-handoff.lock"),
        lock: parent.join(format!(".cargo-dotnet-activation-{id}.lock")),
        id,
        home,
        parent,
    })
}

fn prepare_cli(locations: &Locations, cli: CliActivation) -> Result<PreparedCli> {
    let staged = canonical_regular_file(&cli.staged, "staged cargo-dotnet front-end")?;
    open_regular_file_within_parent(&staged, "staged cargo-dotnet front-end")?.sync_all()?;
    let destination = planned_regular_file_destination(&cli.destination)?;
    let had_previous = fs::symlink_metadata(&destination).is_ok();
    let previous_sha256 = had_previous
        .then(|| regular_file_sha256(&destination, "previous cargo-dotnet front-end"))
        .transpose()?;
    let staged_sha256 = regular_file_sha256(&staged, "staged cargo-dotnet front-end")?;
    let parent = destination
        .parent()
        .context("cargo-dotnet destination has no parent")?;
    let transaction = parent.join(format!(".cargo-dotnet-cli-activation-{}", locations.id));
    Ok(PreparedCli {
        staged,
        destination,
        transaction,
        had_previous,
        previous_sha256,
        staged_sha256,
    })
}

fn publish_initial_journal(
    locations: &Locations,
    staged_home: &Path,
    staged_home_sha256: String,
    had_home: bool,
    cli: Option<PreparedCli>,
) -> Result<Journal> {
    if locations.transaction.exists() {
        bail!(
            "SDK activation journal already exists: {}",
            locations.transaction.display()
        );
    }
    let initializing = tempfile::Builder::new()
        .prefix(&format!(
            ".cargo-dotnet-activation-{}.initializing-",
            locations.id
        ))
        .tempdir_in(&locations.parent)?;
    write_owner(initializing.path(), &locations.id)?;
    let mut bootstrap_guard = cli
        .as_ref()
        .map(|prepared| InitialBootstrapGuard::new(locations, prepared))
        .transpose()?;

    let cli = cli
        .map(|prepared| {
            let staged = initializing.path().join("staged-cli");
            move_regular_file(&prepared.staged, &staged, "staged cargo-dotnet front-end")?;
            Ok::<_, anyhow::Error>(CliJournal {
                staged: locations.transaction.join("staged-cli"),
                destination: prepared.destination,
                replacement: prepared.transaction.join("replacement-cli"),
                backup: prepared.transaction.join("previous-cli"),
                failed_new: prepared.transaction.join("failed-cli"),
                had_previous: prepared.had_previous,
                previous_sha256: prepared.previous_sha256,
                staged_sha256: prepared.staged_sha256,
            })
        })
        .transpose()?;

    let adopted_home = initializing.path().join("staged-home");
    fs::rename(staged_home, &adopted_home).with_context(|| {
        format!(
            "adopting staged SDK home {} into transaction",
            staged_home.display()
        )
    })?;
    sync_rename(staged_home, &adopted_home)?;
    test_crash_point("after_initial_journal_directory");
    let journal = Journal {
        schema: JOURNAL_SCHEMA,
        transaction_id: locations.id.clone(),
        phase: Phase::Prepared,
        home: locations.home.clone(),
        staged_home: locations.transaction.join("staged-home"),
        home_backup: locations.transaction.join("previous-home"),
        failed_home: locations.transaction.join("failed-home"),
        had_home,
        staged_home_sha256,
        cli,
    };
    write_state_at(initializing.path(), &journal)?;
    test_crash_point("during_initial_state");
    File::open(initializing.path())?.sync_all()?;
    let initializing = initializing.keep();
    fs::rename(&initializing, &locations.transaction)
        .context("publishing initial SDK activation journal")?;
    sync_rename(&initializing, &locations.transaction)?;
    test_crash_point("after_initial_journal_publish");
    if let Some(guard) = &mut bootstrap_guard {
        guard.disarm();
    }
    Ok(journal)
}

fn publish_cli_bootstrap(locations: &Locations, cli: &PreparedCli) -> Result<()> {
    let parent = cli
        .transaction
        .parent()
        .context("CLI activation directory has no parent")?;
    let canonical_parent = fs::canonicalize(parent)?;
    if canonical_parent != parent {
        bail!("cargo-dotnet destination parent is not canonical");
    }
    cleanup_cli_recovery_artifacts(parent, &locations.id)?;
    if cli.transaction.exists() {
        bail!(
            "cargo-dotnet activation bootstrap already exists: {}",
            cli.transaction.display()
        );
    }
    let temporary = tempfile::Builder::new()
        .prefix(&format!(
            ".cargo-dotnet-cli-activation-{}.initializing-",
            locations.id
        ))
        .tempdir_in(parent)?;
    write_owner(temporary.path(), &locations.id)?;
    let bootstrap = CliBootstrap {
        schema: JOURNAL_SCHEMA,
        transaction_id: locations.id.clone(),
        home: locations.home.clone(),
        destination: cli.destination.clone(),
    };
    write_json_create_new(
        &temporary.path().join(CLI_BOOTSTRAP_FILE),
        &bootstrap,
        "CLI recovery bootstrap",
    )?;
    File::open(temporary.path())?.sync_all()?;
    let temporary = temporary.keep();
    fs::rename(&temporary, &cli.transaction)
        .context("publishing cargo-dotnet recovery bootstrap")?;
    sync_rename(&temporary, &cli.transaction)
}

fn write_owner(directory: &Path, id: &str) -> Result<()> {
    let path = directory.join(OWNER_FILE);
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)?;
    use std::io::Write as _;
    file.write_all(format!("cargo-dotnet-activation-v2 {id}\n").as_bytes())?;
    file.sync_all()?;
    Ok(())
}

fn write_json_create_new(path: &Path, value: &impl Serialize, label: &str) -> Result<()> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .with_context(|| format!("creating {label} {}", path.display()))?;
    serde_json::to_writer_pretty(&mut file, value)?;
    use std::io::Write as _;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

fn recover_locked(locations: &Locations) -> Result<bool> {
    let metadata = match fs::symlink_metadata(&locations.transaction) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!(
            "SDK activation journal is not a regular directory: {}",
            locations.transaction.display()
        );
    }
    let canonical_transaction = fs::canonicalize(&locations.transaction)?;
    if canonical_transaction != locations.transaction
        || canonical_transaction.parent() != Some(&locations.parent)
    {
        bail!("SDK activation journal escapes its canonical parent");
    }
    if !owner_matches(&locations.transaction, &locations.id) {
        bail!("SDK activation journal ownership marker is invalid");
    }
    let mut journal = load_latest_state(locations)?;
    validate_journal_locations(locations, &journal)?;
    if matches!(journal.phase, Phase::Committed | Phase::RolledBack) {
        finish_committed(locations, &journal)?;
    } else {
        rollback(locations, &mut journal, None)?;
    }
    Ok(true)
}

fn validate_journal_locations(locations: &Locations, journal: &Journal) -> Result<()> {
    if journal.schema != JOURNAL_SCHEMA
        || journal.transaction_id != locations.id
        || journal.home != locations.home
        || journal.staged_home != locations.transaction.join("staged-home")
        || journal.home_backup != locations.transaction.join("previous-home")
        || journal.failed_home != locations.transaction.join("failed-home")
    {
        bail!("SDK activation journal paths or identity are invalid");
    }
    if let Some(cli) = &journal.cli {
        let destination = planned_regular_file_destination(&cli.destination)?;
        let parent = destination
            .parent()
            .context("journal CLI destination has no parent")?;
        let transaction = parent.join(format!(".cargo-dotnet-cli-activation-{}", locations.id));
        if transaction.exists() {
            validate_fixed_directory(parent, &transaction, "CLI activation backup")?;
        } else if !matches!(journal.phase, Phase::Committed | Phase::RolledBack) {
            bail!("CLI activation backup disappeared before transaction completion");
        }
        if cli.destination != destination
            || cli.staged != locations.transaction.join("staged-cli")
            || cli.replacement != transaction.join("replacement-cli")
            || cli.backup != transaction.join("previous-cli")
            || cli.failed_new != transaction.join("failed-cli")
            || cli.staged_sha256.len() != 64
            || cli
                .previous_sha256
                .as_ref()
                .is_some_and(|hash| hash.len() != 64)
        {
            bail!("SDK activation journal CLI paths or hashes are invalid");
        }
    }
    Ok(())
}

fn rollback(
    locations: &Locations,
    journal: &mut Journal,
    restore_inputs: Option<&RollbackInputs>,
) -> Result<()> {
    let mut errors = Vec::new();
    if let Some(cli) = &journal.cli {
        if cli.had_previous {
            let previous = cli.previous_sha256.as_deref().unwrap_or_default();
            let destination_hash =
                regular_file_sha256(&cli.destination, "cargo-dotnet front-end during rollback")
                    .ok();
            if cli.backup.exists() {
                if destination_hash.as_deref() == Some(cli.staged_sha256.as_str())
                    && !cli.failed_new.exists()
                    && let Err(error) = copy_regular_file_create_new(
                        &cli.destination,
                        &cli.failed_new,
                        "failed cargo-dotnet front-end",
                    )
                {
                    errors.push(format!("preserve failed front-end: {error:#}"));
                }
                if destination_hash.as_deref() != Some(previous)
                    && destination_hash.as_deref() != Some(cli.staged_sha256.as_str())
                    && destination_hash.is_some()
                {
                    errors.push("cargo-dotnet destination changed during rollback".into());
                } else if destination_hash.as_deref() != Some(previous)
                    && let Err(error) = atomic_replace_file(&cli.backup, &cli.destination)
                        .context("restoring previous cargo-dotnet front-end atomically")
                        .and_then(|()| sync_rename(&cli.backup, &cli.destination))
                {
                    errors.push(format!("restore previous front-end: {error:#}"));
                }
            }
            if !regular_file_sha256(&cli.destination, "restored cargo-dotnet front-end")
                .is_ok_and(|hash| hash == previous)
            {
                errors.push("previous cargo-dotnet front-end is not recoverable".into());
            }
        } else if cli.destination.exists() {
            match regular_file_sha256(&cli.destination, "new cargo-dotnet front-end") {
                Ok(hash) if hash == cli.staged_sha256 => {
                    if let Err(error) = fs::remove_file(&cli.destination) {
                        errors.push(format!("remove newly activated front-end: {error:#}"));
                    }
                }
                Ok(_) => errors.push("new cargo-dotnet destination changed during rollback".into()),
                Err(error) => errors.push(format!("inspect new front-end: {error:#}")),
            }
        }
    }

    if journal.home_backup.exists() {
        if journal.home.exists()
            && let Err(error) = move_home_aside(journal)
        {
            errors.push(format!("move failed SDK home aside: {error:#}"));
        }
        if !journal.home.exists()
            && let Err(error) = fs::rename(&journal.home_backup, &journal.home)
                .context("restoring previous SDK home")
                .and_then(|()| sync_rename(&journal.home_backup, &journal.home))
        {
            errors.push(format!("restore previous SDK home: {error:#}"));
        }
    } else if !journal.had_home
        && journal.home.exists()
        && (journal.phase >= Phase::HomePromoted || !journal.staged_home.exists())
        && let Err(error) = move_home_aside(journal)
    {
        errors.push(format!("remove newly activated SDK home: {error:#}"));
    }

    if journal.had_home && !journal.home.exists() {
        errors.push("previous SDK home is not recoverable".into());
    }
    if let Some(cli) = &journal.cli
        && cli.had_previous
        && !cli.destination.exists()
    {
        errors.push("previous cargo-dotnet front-end is not recoverable".into());
    }

    if errors.is_empty() {
        if let Some(inputs) = restore_inputs {
            restore_staged_inputs(journal, inputs)?;
        }
        journal.phase = Phase::RolledBack;
        write_state(locations, journal)?;
        cleanup_failed_and_staged(locations, journal)?;
        cleanup_transaction_directories(locations, journal)?;
        Ok(())
    } else {
        bail!("{}", errors.join("; "))
    }
}

fn restore_staged_inputs(journal: &Journal, inputs: &RollbackInputs) -> Result<()> {
    if inputs.staged_home.exists() {
        bail!(
            "refusing to overwrite staged SDK input restored by another process: {}",
            inputs.staged_home.display()
        );
    }
    let home_source = if journal.staged_home.exists() {
        Some(&journal.staged_home)
    } else if journal.failed_home.exists() {
        Some(&journal.failed_home)
    } else {
        None
    };
    if let Some(source) = home_source {
        if crate::content_cache::tree_digest(source)? != journal.staged_home_sha256 {
            bail!("staged SDK input changed during rollback");
        }
        fs::rename(source, &inputs.staged_home).context("restoring staged SDK input")?;
        sync_rename(source, &inputs.staged_home)?;
    }

    if let (Some(original), Some(cli)) = (&inputs.staged_cli, &journal.cli) {
        if original.exists() {
            bail!(
                "refusing to overwrite staged CLI input restored by another process: {}",
                original.display()
            );
        }
        if cli.staged.exists() {
            validate_regular_file_hash(&cli.staged, &cli.staged_sha256, "staged CLI input")?;
            move_regular_file(&cli.staged, original, "staged CLI input")?;
        }
    }
    Ok(())
}

fn move_home_aside(journal: &Journal) -> Result<()> {
    if journal.failed_home.exists() {
        bail!("failed-home recovery location is already occupied");
    }
    fs::rename(&journal.home, &journal.failed_home).context("moving failed SDK home aside")?;
    sync_rename(&journal.home, &journal.failed_home)
}

fn finish_committed(locations: &Locations, journal: &Journal) -> Result<()> {
    if journal.home_backup.exists() {
        crate::path_safety::require_owned_or_empty_sdk_home(&journal.home_backup)?;
        crate::path_safety::remove_dir_all_within(&locations.transaction, &journal.home_backup)?;
    }
    if let Some(cli) = &journal.cli
        && cli.backup.exists()
    {
        validate_regular_file_hash(
            &cli.backup,
            cli.previous_sha256
                .as_deref()
                .context("committed transaction lacks its previous CLI hash")?,
            "previous cargo-dotnet front-end backup",
        )?;
        fs::remove_file(&cli.backup)?;
        sync_directory(cli.backup.parent().context("CLI backup has no parent")?)?;
    }
    cleanup_failed_and_staged(locations, journal)?;
    cleanup_transaction_directories(locations, journal)
}

fn cleanup_failed_and_staged(locations: &Locations, journal: &Journal) -> Result<()> {
    if journal.staged_home.exists() {
        safe_discard_home(
            &locations.transaction,
            &journal.staged_home,
            &journal.staged_home_sha256,
        )?;
    }
    if journal.failed_home.exists() {
        safe_discard_home(
            &locations.transaction,
            &journal.failed_home,
            &journal.staged_home_sha256,
        )?;
    }
    if let Some(cli) = &journal.cli {
        for (path, label) in [
            (&cli.staged, "staged cargo-dotnet front-end"),
            (&cli.replacement, "cargo-dotnet replacement"),
        ] {
            if path.exists() {
                validate_regular_file_hash(path, &cli.staged_sha256, label)?;
                fs::remove_file(path)?;
            }
        }
        if cli.failed_new.exists() {
            validate_regular_file_hash(
                &cli.failed_new,
                &cli.staged_sha256,
                "failed staged cargo-dotnet front-end",
            )?;
            fs::remove_file(&cli.failed_new)?;
        }
    }
    Ok(())
}

fn safe_discard_home(boundary: &Path, path: &Path, expected_sha256: &str) -> Result<()> {
    let digest_matches =
        crate::content_cache::tree_digest(path).is_ok_and(|digest| digest == expected_sha256);
    if !digest_matches {
        crate::path_safety::require_owned_or_empty_sdk_home(path).with_context(|| {
            format!(
                "refusing to delete changed, unowned recovery home {}",
                path.display()
            )
        })?;
    }
    crate::path_safety::remove_dir_all_within(boundary, path)
}

fn cleanup_transaction_directories(locations: &Locations, journal: &Journal) -> Result<()> {
    // Retire the CLI bootstrap first. After Committed (or a completed rollback) both public
    // objects are already coherent, while the home journal remains an authoritative recovery
    // record if this cleanup is interrupted or invoked through an alternate executable.
    if let Some(cli) = &journal.cli {
        let directory = cli
            .backup
            .parent()
            .context("CLI transaction has no parent")?;
        let parent = directory
            .parent()
            .context("CLI transaction parent is missing")?;
        cleanup_cli_recovery_artifacts(parent, &locations.id)?;
        if directory.exists() {
            remove_owned_transaction_dir(parent, directory, &locations.id)?;
        }
        test_crash_point("after_cli_bootstrap_retired");
    }

    let tombstone = locations.parent.join(format!(
        ".cargo-dotnet-activation-{}.tombstone-{}",
        locations.id,
        unique_transaction_suffix()
    ));
    fs::rename(&locations.transaction, &tombstone)
        .context("retiring completed SDK activation journal")?;
    sync_rename(&locations.transaction, &tombstone)?;
    test_crash_point("after_journal_tombstone_rename");

    remove_owned_transaction_dir(&locations.parent, &tombstone, &locations.id)?;
    sync_directory(&locations.parent)
}

fn unique_transaction_suffix() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{}-{nanos}", std::process::id())
}

fn cleanup_home_recovery_artifacts(locations: &Locations) -> Result<()> {
    let prefixes = [
        format!(".cargo-dotnet-activation-{}.initializing-", locations.id),
        format!(".cargo-dotnet-activation-{}.tombstone-", locations.id),
        format!(".cargo-dotnet-activation-{}.deleting-", locations.id),
    ];
    cleanup_owned_siblings(&locations.parent, &locations.id, &prefixes)
}

fn cleanup_cli_recovery_artifacts(parent: &Path, id: &str) -> Result<()> {
    let prefixes = [
        format!(".cargo-dotnet-cli-activation-{id}.initializing-"),
        format!(".cargo-dotnet-cli-activation-{id}.tombstone-"),
        format!(".cargo-dotnet-cli-activation-{id}.deleting-"),
    ];
    cleanup_owned_siblings(parent, id, &prefixes)
}

fn cleanup_owned_siblings(parent: &Path, id: &str, prefixes: &[String]) -> Result<()> {
    let mut entries = fs::read_dir(parent)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !prefixes.iter().any(|prefix| name.starts_with(prefix)) {
            continue;
        }
        let file_type = entry.file_type()?;
        if file_type.is_symlink() || !file_type.is_dir() {
            continue;
        }
        if owner_matches(&entry.path(), id) {
            remove_owned_transaction_dir(parent, &entry.path(), id)?;
        }
    }
    Ok(())
}

fn owner_matches(directory: &Path, id: &str) -> bool {
    canonical_regular_file(&directory.join(OWNER_FILE), "activation ownership marker")
        .and_then(|path| fs::read_to_string(path).map_err(Into::into))
        .is_ok_and(|owner| owner == format!("cargo-dotnet-activation-v2 {id}\n"))
}

fn remove_owned_transaction_dir(parent: &Path, directory: &Path, id: &str) -> Result<()> {
    let parent = fs::canonicalize(parent)?;
    let metadata = fs::symlink_metadata(directory)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!("activation cleanup target is not a regular directory");
    }
    let canonical = fs::canonicalize(directory)?;
    if canonical.parent() != Some(parent.as_path()) || !owner_matches(&canonical, id) {
        bail!("activation cleanup target is not owned by this transaction");
    }
    let prefix = if directory
        .file_name()
        .is_some_and(|name| name.to_string_lossy().contains("cli-activation"))
    {
        format!(".cargo-dotnet-cli-activation-{id}.deleting-")
    } else {
        format!(".cargo-dotnet-activation-{id}.deleting-")
    };
    let quarantined = parent.join(format!("{prefix}{}", unique_transaction_suffix()));
    fs::rename(&canonical, &quarantined).context("quarantining activation cleanup target")?;
    sync_rename(&canonical, &quarantined)?;
    test_cleanup_swap_point(&quarantined);

    // Revalidate the object after the namespace move. If a writable parent swapped the source
    // between validation and rename, or swaps the quarantined leaf at the test barrier, never
    // follow it into an outside tree.
    let metadata = fs::symlink_metadata(&quarantined)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!("quarantined activation cleanup target is not a regular directory");
    }
    let directory = fs::canonicalize(&quarantined)?;
    if directory != quarantined
        || directory.parent() != Some(parent.as_path())
        || !owner_matches(&directory, id)
    {
        bail!("quarantined activation cleanup target is not owned by this transaction");
    }
    let mut entries = fs::read_dir(&directory)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if entry.file_name() == std::ffi::OsStr::new(OWNER_FILE)
            || entry.file_name() == std::ffi::OsStr::new(CLI_BOOTSTRAP_FILE)
        {
            continue;
        }
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            bail!("activation journal cleanup encountered a symlink");
        }
        if file_type.is_dir() {
            crate::path_safety::remove_dir_all_within(&directory, &path)?;
        } else if file_type.is_file() {
            fs::remove_file(&path)?;
        } else {
            bail!("activation journal cleanup encountered an unsupported entry");
        }
        test_crash_point("during_journal_tombstone_cleanup");
    }
    let bootstrap = directory.join(CLI_BOOTSTRAP_FILE);
    if bootstrap.exists() {
        let bootstrap = canonical_regular_file(&bootstrap, "CLI recovery bootstrap")?;
        if bootstrap.parent() != Some(directory.as_path()) {
            bail!("CLI recovery bootstrap escapes its transaction directory");
        }
        fs::remove_file(bootstrap)?;
    }
    fs::remove_file(directory.join(OWNER_FILE))?;
    fs::remove_dir(&directory)?;
    sync_directory(&parent)
}

#[cfg(all(test, unix))]
fn test_cleanup_swap_point(directory: &Path) {
    use std::os::unix::fs::symlink;

    let Some(outside) = std::env::var_os("CARGO_DOTNET_TEST_CLEANUP_SWAP_OUTSIDE") else {
        return;
    };
    let parked = directory.with_file_name(format!(
        ".cargo-dotnet-test-parked-{}",
        unique_transaction_suffix()
    ));
    fs::rename(directory, parked).unwrap();
    symlink(outside, directory).unwrap();
}

#[cfg(not(all(test, unix)))]
fn test_cleanup_swap_point(_directory: &Path) {}

fn validate_fixed_directory(parent: &Path, path: &Path, label: &str) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("inspecting {label} {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!("{label} is not a regular directory: {}", path.display());
    }
    let canonical =
        fs::canonicalize(path).with_context(|| format!("resolving {label} {}", path.display()))?;
    if canonical != path || canonical.parent() != Some(parent) {
        bail!(
            "{label} escapes its canonical parent: {}",
            canonical.display()
        );
    }
    Ok(())
}

fn write_state(locations: &Locations, journal: &Journal) -> Result<()> {
    write_state_at(&locations.transaction, journal)
}

fn write_state_at(directory: &Path, journal: &Journal) -> Result<()> {
    let sequence = journal.phase.sequence();
    let final_path = directory.join(format!("state-{sequence:02}.json"));
    let mut bytes = serde_json::to_vec_pretty(journal)?;
    bytes.push(b'\n');
    if final_path.exists() {
        if rust_dotnet_sdk_core::safe_fs::read_regular_nofollow(&final_path)? == bytes {
            return Ok(());
        }
        bail!("activation journal state {sequence} already exists with different bytes");
    }
    let mut temporary = tempfile::Builder::new()
        .prefix(&format!(".state-{sequence:02}."))
        .suffix(".tmp")
        .tempfile_in(directory)
        .with_context(|| format!("creating activation journal state {sequence}"))?;
    use std::io::Write as _;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist_noclobber(&final_path)
        .map_err(|error| error.error)
        .with_context(|| format!("publishing activation journal state {sequence}"))?;
    sync_directory(directory)
}

fn load_latest_state(locations: &Locations) -> Result<Journal> {
    let mut states = Vec::new();
    for entry in fs::read_dir(&locations.transaction)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(sequence) = name
            .strip_prefix("state-")
            .and_then(|name| name.strip_suffix(".json"))
            .and_then(|value| value.parse::<u8>().ok())
        else {
            continue;
        };
        let metadata = entry.file_type()?;
        if !metadata.is_file() || metadata.is_symlink() {
            bail!("activation journal contains a non-regular state file");
        }
        let journal: Journal = serde_json::from_slice(&fs::read(entry.path())?)
            .with_context(|| format!("parsing activation journal state {sequence}"))?;
        if journal.phase.sequence() != sequence {
            bail!("activation journal state sequence does not match its phase");
        }
        states.push((sequence, journal));
    }
    states
        .into_iter()
        .max_by_key(|(sequence, _)| *sequence)
        .map(|(_, journal)| journal)
        .context("activation journal has no durable state")
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

fn regular_file_sha256(path: &Path, label: &str) -> Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(snapshot_regular_file(path, label)?)
    ))
}

#[cfg(test)]
fn read_json_regular<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    serde_json::from_slice(&snapshot_regular_file(path, "transaction JSON")?)
        .context("parsing transaction JSON")
}

fn copy_regular_file_create_new(source: &Path, destination: &Path, label: &str) -> Result<()> {
    copy_regular_file_create_new_with_hook(source, destination, label, || {})
}

fn copy_regular_file_create_new_with_hook(
    source: &Path,
    destination: &Path,
    label: &str,
    before_source_open: impl FnOnce(),
) -> Result<()> {
    before_source_open();
    let mut input = open_regular_file_within_parent(source, label)?;
    let permissions = input.metadata()?.permissions();
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)
        .with_context(|| format!("creating {label} {}", destination.display()))?;
    std::io::copy(&mut input, &mut output)
        .with_context(|| format!("copying {label} from {}", source.display()))?;
    output.set_permissions(permissions)?;
    output.sync_all()?;
    sync_directory(
        destination
            .parent()
            .context("copied transaction file has no parent")?,
    )
}

/// Open an arbitrary transaction leaf through a stable parent capability.
///
/// Transaction sources are deliberately not reopened after a pathname validation: callers can
/// stage a file in a concurrently-writable directory, and a validation-then-`File::open` pair
/// would permit a regular source to be replaced with a symlink. `safe_fs` binds both the parent
/// and leaf without following that replacement.
fn open_regular_file_within_parent(path: &Path, label: &str) -> Result<File> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .context("transaction regular file has no filename")?;
    let (_, file) = rust_dotnet_sdk_core::safe_fs::open_regular_within(parent, Path::new(name))
        .with_context(|| {
            format!(
                "opening {label} without following links: {}",
                path.display()
            )
        })?;
    Ok(file)
}

fn snapshot_regular_file(path: &Path, label: &str) -> Result<Vec<u8>> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .context("transaction regular file has no filename")?;
    let (_, bytes) =
        rust_dotnet_sdk_core::safe_fs::snapshot_regular_within(parent, Path::new(name))
            .with_context(|| {
                format!(
                    "reading {label} without following links: {}",
                    path.display()
                )
            })?;
    Ok(bytes)
}

fn move_regular_file(source: &Path, destination: &Path, label: &str) -> Result<()> {
    canonical_regular_file(source, label)?;
    match fs::rename(source, destination) {
        Ok(()) => sync_rename(source, destination),
        Err(error) if is_cross_device_error(&error) => {
            // EXDEV: preserve the no-follow/create-new contract across filesystem boundaries.
            copy_regular_file_create_new(source, destination, label)?;
            if let Err(error) = fs::remove_file(source) {
                let _ = fs::remove_file(destination);
                return Err(error).with_context(|| format!("removing moved {label} source"));
            }
            sync_rename(source, destination)
        }
        Err(error) => Err(error).with_context(|| format!("moving {label}")),
    }
}

fn is_cross_device_error(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::CrossesDevices
        || matches!(error.raw_os_error(), Some(17) | Some(18))
}

#[cfg(unix)]
fn atomic_replace_file(replacement: &Path, destination: &Path) -> Result<()> {
    fs::rename(replacement, destination).with_context(|| {
        format!(
            "replacing {} with {}",
            destination.display(),
            replacement.display()
        )
    })
}

#[cfg(windows)]
fn atomic_replace_file(replacement: &Path, destination: &Path) -> Result<()> {
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
    let replacement_wide = wide(replacement);
    let destination_wide = wide(destination);
    let success = unsafe {
        if destination.exists() {
            let replaced = ReplaceFileW(
                destination_wide.as_ptr(),
                replacement_wide.as_ptr(),
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            if replaced != 0 {
                replaced
            } else {
                // ReplaceFileW is unavailable for some otherwise-supported filesystems. The
                // replace-existing MoveFileExW fallback retains one atomic namespace operation.
                MoveFileExW(
                    replacement_wide.as_ptr(),
                    destination_wide.as_ptr(),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            }
        } else {
            MoveFileExW(
                replacement_wide.as_ptr(),
                destination_wide.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        }
    };
    if success == 0 {
        return Err(std::io::Error::last_os_error()).context("atomic Windows CLI replacement");
    }
    Ok(())
}

fn validate_regular_file_hash(path: &Path, expected: &str, label: &str) -> Result<()> {
    let actual = regular_file_sha256(path, label)?;
    if actual != expected {
        bail!("{label} bytes changed during activation");
    }
    Ok(())
}

pub(crate) fn sync_tree(path: &Path) -> Result<()> {
    sync_tree_with_hook(path, path, &mut |_| {})
}

fn sync_tree_with_hook(
    root: &Path,
    path: &Path,
    before_file_open: &mut dyn FnMut(&Path),
) -> Result<()> {
    let mut entries = fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let file_type = entry.file_type()?;
        let entry_path = entry.path();
        let metadata = fs::symlink_metadata(&entry_path)?;
        if file_type.is_symlink()
            || rust_dotnet_sdk_core::safe_fs::metadata_is_link_or_reparse(&metadata)
        {
            bail!(
                "staged SDK home contains a symlink: {}",
                entry_path.display()
            );
        } else if file_type.is_dir() {
            sync_tree_with_hook(root, &entry_path, before_file_open)?;
        } else if file_type.is_file() {
            let relative = entry_path
                .strip_prefix(root)
                .context("staged SDK entry escaped the sync root")?;
            before_file_open(&entry_path);
            let (_, file) = rust_dotnet_sdk_core::safe_fs::open_regular_within(root, relative)?;
            file.sync_all()?;
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
fn test_crash_point(point: &str) {
    if std::env::var("CARGO_DOTNET_TEST_CRASH_POINT").as_deref() == Ok(point) {
        #[cfg(unix)]
        unsafe {
            unsafe extern "C" {
                fn kill(pid: i32, signal: i32) -> i32;
            }
            if kill(std::process::id() as i32, 9) != 0 {
                std::process::abort();
            }
            loop {
                std::hint::spin_loop();
            }
        }
        #[cfg(not(unix))]
        std::process::abort();
    }
}

#[cfg(not(test))]
fn test_crash_point(_point: &str) {}

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
    fn transaction_kill_helper() {
        let Ok(mode) = std::env::var("CARGO_DOTNET_TRANSACTION_HELPER") else {
            return;
        };
        let root = PathBuf::from(std::env::var_os("CARGO_DOTNET_TRANSACTION_ROOT").unwrap());
        let home = root.join("home");
        let staged_home = root.join("staged-home");
        let cli = (mode == "setup")
            .then(|| CliActivation::new(root.join("staged-cli"), root.join("bin/cargo-dotnet")));
        activate(
            &staged_home,
            &home,
            cli,
            RollbackDisposition::DiscardInputs,
            || Ok(()),
            || Ok(()),
            || Ok(()),
        )
        .unwrap();
    }

    #[test]
    fn installed_cli_recovery_helper() {
        if std::env::var_os("CARGO_DOTNET_INSTALLED_RECOVERY_HELPER").is_none() {
            return;
        }
        recover_pending_for_current_cli().unwrap();
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
    fn cleanup_swap_helper() {
        if std::env::var_os("CARGO_DOTNET_CLEANUP_SWAP_HELPER").is_none() {
            return;
        }
        let root = PathBuf::from(std::env::var_os("CARGO_DOTNET_TRANSACTION_ROOT").unwrap());
        let locations = locations(&root.join("home")).unwrap();
        fs::create_dir(&locations.transaction).unwrap();
        write_owner(&locations.transaction, &locations.id).unwrap();
        assert!(
            remove_owned_transaction_dir(&locations.parent, &locations.transaction, &locations.id,)
                .is_err()
        );
    }

    #[cfg(unix)]
    fn assert_no_transaction_residue(root: &Path) {
        for directory in [root, &root.join("bin")] {
            for entry in fs::read_dir(directory).unwrap() {
                let name = entry.unwrap().file_name().to_string_lossy().into_owned();
                assert!(
                    !name.starts_with(".cargo-dotnet-activation-") || name.ends_with(".lock"),
                    "left SDK transaction artifact {name}"
                );
                assert!(
                    !name.starts_with(".cargo-dotnet-cli-activation-"),
                    "left CLI transaction artifact {name}"
                );
            }
        }
        assert!(!root.join("staged-home").exists());
        assert!(!root.join("staged-cli").exists());
    }

    #[cfg(unix)]
    fn run_installed_destination_recovery(destination: &Path) {
        let status = Command::new(destination)
            .args([
                "--exact",
                "install_transaction::tests::installed_cli_recovery_helper",
                "--nocapture",
            ])
            .env("CARGO_DOTNET_INSTALLED_RECOVERY_HELPER", "1")
            .status()
            .unwrap();
        assert!(
            status.success(),
            "installed destination could not bootstrap transaction recovery"
        );
    }

    #[cfg(unix)]
    #[test]
    fn installed_destination_recovers_every_durable_crash_phase_without_residue() {
        let cases = [
            ("after_initial_journal_directory", b"old-home".as_slice()),
            ("during_initial_state", b"old-home".as_slice()),
            ("after_initial_journal_publish", b"old-home".as_slice()),
            ("after_cli_backup_rename", b"old-home".as_slice()),
            ("after_cli_backup_copy", b"old-home".as_slice()),
            ("after_cli_promote_rename", b"old-home".as_slice()),
            ("after_home_backup_rename", b"old-home".as_slice()),
            ("after_home_promote_rename", b"old-home".as_slice()),
            ("after_commit_journal", b"new-home".as_slice()),
            ("after_cli_bootstrap_retired", b"new-home".as_slice()),
            ("after_journal_tombstone_rename", b"new-home".as_slice()),
            ("during_journal_tombstone_cleanup", b"new-home".as_slice()),
        ];
        for (crash_point, expected_home) in cases {
            let temp = tempfile::tempdir().unwrap();
            owned_home(&temp.path().join("home"), b"old-home");
            owned_home(&temp.path().join("staged-home"), b"new-home");
            fs::create_dir_all(temp.path().join("bin")).unwrap();
            let executable = std::env::current_exe().unwrap();
            let destination = temp.path().join("bin/cargo-dotnet");
            fs::copy(&executable, &destination).unwrap();
            fs::copy(&executable, temp.path().join("staged-cli")).unwrap();

            let status = Command::new(&executable)
                .args([
                    "--exact",
                    "install_transaction::tests::transaction_kill_helper",
                    "--nocapture",
                ])
                .env("CARGO_DOTNET_TRANSACTION_HELPER", "setup")
                .env("CARGO_DOTNET_TRANSACTION_ROOT", temp.path())
                .env("CARGO_DOTNET_TEST_CRASH_POINT", crash_point)
                .status()
                .unwrap();
            assert!(!status.success(), "crash point {crash_point} did not fire");
            assert!(
                destination.exists(),
                "{crash_point} removed the installed CLI"
            );

            run_installed_destination_recovery(&destination);
            // Once a committed cleanup retires the CLI bootstrap first, an alternate setup/home
            // invocation owns residual journal cleanup. The destination must remain runnable in
            // that window, but it intentionally has no stale home pointer to follow.
            let _ = recover(&temp.path().join("home")).unwrap();
            assert_eq!(
                fs::read(temp.path().join("home/marker")).unwrap(),
                expected_home,
                "wrong recovery direction after {crash_point}"
            );
            assert_no_transaction_residue(temp.path());
        }
    }

    #[cfg(unix)]
    #[test]
    fn setup_kill_between_home_and_cli_promotion_recovers_both_old_objects() {
        let temp = tempfile::tempdir().unwrap();
        owned_home(&temp.path().join("home"), b"old-home");
        owned_home(&temp.path().join("staged-home"), b"new-home");
        fs::create_dir_all(temp.path().join("bin")).unwrap();
        fs::write(temp.path().join("bin/cargo-dotnet"), b"old-cli").unwrap();
        fs::write(temp.path().join("staged-cli"), b"new-cli").unwrap();

        let status = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "install_transaction::tests::transaction_kill_helper",
                "--nocapture",
            ])
            .env("CARGO_DOTNET_TRANSACTION_HELPER", "setup")
            .env("CARGO_DOTNET_TRANSACTION_ROOT", temp.path())
            .env("CARGO_DOTNET_TEST_CRASH_POINT", "after_home_promote_rename")
            .status()
            .unwrap();
        assert!(!status.success());

        assert!(recover(&temp.path().join("home")).unwrap());
        assert_eq!(
            fs::read(temp.path().join("home/marker")).unwrap(),
            b"old-home"
        );
        assert_eq!(
            fs::read(temp.path().join("bin/cargo-dotnet")).unwrap(),
            b"old-cli"
        );
    }

    #[cfg(unix)]
    #[test]
    fn bundle_kill_after_commit_finishes_forward_without_restoring_backup() {
        let temp = tempfile::tempdir().unwrap();
        owned_home(&temp.path().join("home"), b"old-home");
        owned_home(&temp.path().join("staged-home"), b"new-home");

        let status = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "install_transaction::tests::transaction_kill_helper",
                "--nocapture",
            ])
            .env("CARGO_DOTNET_TRANSACTION_HELPER", "bundle")
            .env("CARGO_DOTNET_TRANSACTION_ROOT", temp.path())
            .env("CARGO_DOTNET_TEST_CRASH_POINT", "after_commit_journal")
            .status()
            .unwrap();
        assert!(!status.success());

        assert!(recover(&temp.path().join("home")).unwrap());
        assert_eq!(
            fs::read(temp.path().join("home/marker")).unwrap(),
            b"new-home"
        );
    }

    #[cfg(unix)]
    #[test]
    fn recovery_rejects_symlinked_journal_without_touching_outside_state() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let locations = locations(&home).unwrap();
        let outside = temp.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("sentinel"), b"keep").unwrap();
        symlink(&outside, &locations.transaction).unwrap();

        let error = recover(&home).unwrap_err();
        assert!(error.to_string().contains("journal"));
        assert_eq!(fs::read(outside.join("sentinel")).unwrap(), b"keep");
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_revalidates_quarantined_directory_after_parent_swap() {
        let temp = tempfile::tempdir().unwrap();
        let outside = temp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("sentinel"), b"keep").unwrap();
        let status = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "install_transaction::tests::cleanup_swap_helper",
                "--nocapture",
            ])
            .env("CARGO_DOTNET_CLEANUP_SWAP_HELPER", "1")
            .env("CARGO_DOTNET_TRANSACTION_ROOT", temp.path())
            .env("CARGO_DOTNET_TEST_CLEANUP_SWAP_OUTSIDE", &outside)
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(fs::read(outside.join("sentinel")).unwrap(), b"keep");
    }

    #[test]
    fn windows_and_unix_cross_device_codes_are_recognized() {
        assert!(is_cross_device_error(&std::io::Error::from_raw_os_error(
            17
        )));
        assert!(is_cross_device_error(&std::io::Error::from_raw_os_error(
            18
        )));
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
            "--no-install-cli activation entered while a normal command held the SDK home"
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
    fn native_cargo_bin_locks_before_driver_open_and_no_cli_activation() {
        use std::process::Stdio;
        use std::sync::mpsc;
        use std::time::{Duration, Instant};

        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let staged_home = temp.path().join("staged-home");
        let cargo_bin = temp.path().join("cargo-home/bin");
        let launcher = cargo_bin.join(format!("cargo-dotnet{}", std::env::consts::EXE_SUFFIX));
        let pre_main_ready = temp.path().join("pre-main-ready");
        let pre_main_release = temp.path().join("pre-main-release");
        let ready = temp.path().join("lease-ready");
        let release = temp.path().join("lease-release");
        sealed_bootstrap_home(&home);
        owned_home(&staged_home, b"new-home");
        fs::create_dir_all(&cargo_bin).unwrap();
        fs::copy(std::env::current_exe().unwrap(), &launcher).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&launcher, fs::Permissions::from_mode(0o755)).unwrap();
        }

        let mut consumer = Command::new(&launcher)
            .args([
                "--exact",
                "install_transaction::tests::native_bootstrap_lease_helper",
                "--nocapture",
            ])
            .env("CARGO_DOTNET_HOME", &home)
            .env("CARGO_DOTNET_TEST_NATIVE_BOOTSTRAP_HELPER", "1")
            .env("CARGO_DOTNET_TEST_PRE_MAIN_READY", &pre_main_ready)
            .env("CARGO_DOTNET_TEST_PRE_MAIN_RELEASE", &pre_main_release)
            .env("CARGO_DOTNET_TEST_LAUNCHER_LEASE_READY", &ready)
            .env("CARGO_DOTNET_TEST_LAUNCHER_LEASE_RELEASE", &release)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !pre_main_ready.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        if !pre_main_ready.exists() {
            let _ = consumer.kill();
            panic!("native Cargo-bin bootstrap did not acquire its pre-main lock");
        }

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
            "--no-install-cli activation entered during the launcher's pre-main pause"
        );
        fs::write(&pre_main_release, b"release").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !ready.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        if !ready.exists() {
            let _ = consumer.kill();
            panic!("installed native driver did not adopt its per-home lifetime lease");
        }
        assert!(
            entered_rx.recv_timeout(Duration::from_millis(150)).is_err(),
            "--no-install-cli activation bypassed the native driver's per-home lease"
        );
        fs::write(&release, b"release").unwrap();
        assert!(consumer.wait().unwrap().success());
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        thread.join().unwrap().unwrap();
        assert_eq!(
            fs::read(temp.path().join("home/marker")).unwrap(),
            b"new-home"
        );
    }

    #[test]
    fn direct_home_driver_locks_and_validates_before_no_cli_activation() {
        use std::process::Stdio;
        use std::sync::mpsc;
        use std::time::{Duration, Instant};

        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let staged_home = temp.path().join("staged-home");
        sealed_bootstrap_home(&home);
        owned_home(&staged_home, b"new-home");
        let driver = home
            .join("bin")
            .join(format!("cargo-dotnet{}", std::env::consts::EXE_SUFFIX));
        let pre_main_ready = temp.path().join("direct-pre-main-ready");
        let pre_main_release = temp.path().join("direct-pre-main-release");
        let ready = temp.path().join("direct-lease-ready");
        let release = temp.path().join("direct-lease-release");
        let mut consumer = Command::new(&driver)
            .args([
                "--exact",
                "install_transaction::tests::native_bootstrap_lease_helper",
                "--nocapture",
            ])
            .env("CARGO_DOTNET_HOME", &home)
            .env("CARGO_DOTNET_TEST_NATIVE_BOOTSTRAP_HELPER", "1")
            .env("CARGO_DOTNET_TEST_PRE_MAIN_READY", &pre_main_ready)
            .env("CARGO_DOTNET_TEST_PRE_MAIN_RELEASE", &pre_main_release)
            .env("CARGO_DOTNET_TEST_LAUNCHER_LEASE_READY", &ready)
            .env("CARGO_DOTNET_TEST_LAUNCHER_LEASE_RELEASE", &release)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !pre_main_ready.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            pre_main_ready.exists(),
            "direct driver did not acquire coordination"
        );

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
        assert!(entered_rx.recv_timeout(Duration::from_millis(150)).is_err());
        fs::write(&pre_main_release, b"release").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !ready.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            ready.exists(),
            "direct driver did not acquire its home lease"
        );
        assert!(entered_rx.recv_timeout(Duration::from_millis(150)).is_err());
        fs::write(&release, b"release").unwrap();
        assert!(consumer.wait().unwrap().success());
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        thread.join().unwrap().unwrap();
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

    #[cfg(unix)]
    #[test]
    fn installed_recovery_waits_for_an_in_progress_activation_lock() {
        use std::time::Duration;

        let temp = tempfile::tempdir().unwrap();
        owned_home(&temp.path().join("home"), b"old-home");
        owned_home(&temp.path().join("staged-home"), b"new-home");
        fs::create_dir_all(temp.path().join("bin")).unwrap();
        let executable = std::env::current_exe().unwrap();
        let destination = temp.path().join("bin/cargo-dotnet");
        fs::copy(&executable, &destination).unwrap();
        fs::copy(&executable, temp.path().join("staged-cli")).unwrap();

        let killed = Command::new(&executable)
            .args([
                "--exact",
                "install_transaction::tests::transaction_kill_helper",
                "--nocapture",
            ])
            .env("CARGO_DOTNET_TRANSACTION_HELPER", "setup")
            .env("CARGO_DOTNET_TRANSACTION_ROOT", temp.path())
            .env(
                "CARGO_DOTNET_TEST_CRASH_POINT",
                "after_initial_journal_publish",
            )
            .status()
            .unwrap();
        assert!(!killed.success());

        let locations = locations(&temp.path().join("home")).unwrap();
        let locks = TransactionLocks::acquire([
            locations.lock.clone(),
            cli_destination_lock(&destination).unwrap(),
        ])
        .unwrap();
        let mut recovery = Command::new(&destination)
            .args([
                "--exact",
                "install_transaction::tests::installed_cli_recovery_helper",
                "--nocapture",
            ])
            .env("CARGO_DOTNET_INSTALLED_RECOVERY_HELPER", "1")
            .spawn()
            .unwrap();
        std::thread::sleep(Duration::from_millis(150));
        assert!(
            recovery.try_wait().unwrap().is_none(),
            "recovery skipped a held activation lock instead of waiting"
        );
        drop(locks);
        assert!(recovery.wait().unwrap().success());
        assert_no_transaction_residue(temp.path());
    }

    #[cfg(unix)]
    #[test]
    fn copying_transaction_source_rejects_a_leaf_swapped_to_an_outside_symlink() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("staged-cli");
        let destination = temp.path().join("replacement");
        let outside = temp.path().join("outside-cli");
        fs::write(&source, b"trusted staged CLI").unwrap();
        fs::write(&outside, b"outside CLI must not be copied").unwrap();

        let error = copy_regular_file_create_new_with_hook(
            &source,
            &destination,
            "staged cargo-dotnet front-end",
            || {
                fs::remove_file(&source).unwrap();
                symlink(&outside, &source).unwrap();
            },
        )
        .unwrap_err();

        assert!(error.to_string().contains("without following links"));
        assert!(!destination.exists());
        assert_eq!(
            fs::read(&outside).unwrap(),
            b"outside CLI must not be copied"
        );
    }

    #[cfg(unix)]
    #[test]
    fn syncing_staged_tree_rejects_a_leaf_swapped_to_an_outside_symlink() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let staged = temp.path().join("staged-home");
        let leaf = staged.join("marker");
        let outside = temp.path().join("outside-marker");
        fs::create_dir(&staged).unwrap();
        fs::write(&leaf, b"trusted staged home").unwrap();
        fs::write(&outside, b"outside marker must not be synced").unwrap();

        let mut swapped = false;
        let error = sync_tree_with_hook(&staged, &staged, &mut |candidate| {
            if candidate == leaf && !swapped {
                swapped = true;
                fs::remove_file(&leaf).unwrap();
                symlink(&outside, &leaf).unwrap();
            }
        })
        .unwrap_err();

        assert!(swapped);
        assert!(error.to_string().contains("contained file"));
        assert_eq!(
            fs::read(&outside).unwrap(),
            b"outside marker must not be synced"
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_atomic_replace_supports_an_existing_destination() {
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("cargo-dotnet.exe");
        let replacement = temp.path().join("replacement.exe");
        fs::write(&destination, b"old").unwrap();
        fs::write(&replacement, b"new").unwrap();
        atomic_replace_file(&replacement, &destination).unwrap();
        assert_eq!(fs::read(destination).unwrap(), b"new");
        assert!(!replacement.exists());
    }
}
