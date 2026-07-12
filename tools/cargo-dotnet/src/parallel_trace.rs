//! Opt-in acceptance instrumentation for proving that distinct consumer builds overlap.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result, bail};
use serde_json::json;

use crate::context::{Context, crate_cache_key};

pub struct StageGuard {
    trace: Option<PathBuf>,
    crate_key: String,
    stage: &'static str,
}

impl StageGuard {
    pub fn enter(ctx: &Context, stage: &'static str) -> Result<Self> {
        let trace = std::env::var_os("CARGO_DOTNET_PARALLEL_TRACE").map(PathBuf::from);
        let crate_key = crate_cache_key(&ctx.crate_dir)?;
        if let Some(path) = &trace {
            append(path, &crate_key, stage, "enter")?;
            wait_at_barrier(&crate_key, stage)?;
        }
        Ok(Self {
            trace,
            crate_key,
            stage,
        })
    }
}

impl Drop for StageGuard {
    fn drop(&mut self) {
        if let Some(path) = &self.trace {
            let _ = append(path, &self.crate_key, self.stage, "exit");
        }
    }
}

fn append(path: &Path, crate_key: &str, stage: &str, event: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let line = serde_json::to_string(&json!({
        "schema": 1,
        "pid": std::process::id(),
        "crate_key": crate_key,
        "stage": stage,
        "event": event,
        "unix_nanos": SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos().to_string()
    }))?;
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{line}").with_context(|| format!("append parallel trace {}", path.display()))
}

fn wait_at_barrier(crate_key: &str, stage: &str) -> Result<()> {
    let Some(root) = std::env::var_os("CARGO_DOTNET_PARALLEL_BARRIER").map(PathBuf::from) else {
        return Ok(());
    };
    fs::create_dir_all(&root)?;
    fs::write(root.join(format!("{stage}-{crate_key}.ready")), b"ready\n")?;
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        let count = fs::read_dir(&root)?
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(&format!("{stage}-"))
            })
            .count();
        if count >= 2 {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            bail!("parallel acceptance barrier timed out waiting for a second {stage} build");
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}
