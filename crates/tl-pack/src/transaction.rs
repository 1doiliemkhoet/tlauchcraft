//! Crash-recoverable private CurseForge staging transaction.
//!
//! This module never publishes an instance, modifies an existing profile/world,
//! or trusts client-provided Forge booleans to claim readiness. Publishing will
//! require the merged, independently verified tl-loader runtime adapter.
use crate::{
    curseforge::CurseForgeResolveReport,
    install::{stage_curseforge_downloads, InstallStatus, StageReport},
    PackMetadata, PackSource,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

const JOURNAL_LIMIT: u64 = 256 * 1024;
const JOURNAL_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TransactionState {
    Installing,
    NeedsAction,
    Failed,
    Cancelled,
    Interrupted,
    RolledBack,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionJournal {
    pub version: u32,
    pub operation_id: String,
    pub archive_sha256: String,
    pub minecraft_version: String,
    pub forge_version: String,
    pub state: TransactionState,
    pub required_total: usize,
    pub optional_total: usize,
    pub downloaded: usize,
    pub blocked: usize,
    pub failures: usize,
    /// Reserved for the trusted loader adapter. Never true in this module.
    pub ready: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionResult {
    pub journal: TransactionJournal,
    pub staging_path: PathBuf,
    pub installed: Vec<crate::install::InstalledMod>,
    pub blocked: Vec<String>,
    pub failures: Vec<String>,
    pub ready: bool,
}

fn reject_existing_symlinks(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("transaction parent must be absolute".into());
    }
    for ancestor in path.ancestors() {
        if fs::symlink_metadata(ancestor).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("symlinked transaction path is not allowed".into());
        }
    }
    Ok(())
}

fn validate_id(id: &str) -> Result<(), String> {
    if id.len() < 8 || id.len() > 64 || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err("invalid transaction operation ID".into());
    }
    Ok(())
}

fn paths(parent: &Path, operation_id: &str) -> Result<(PathBuf, PathBuf), String> {
    validate_id(operation_id)?;
    reject_existing_symlinks(parent)?;
    if !parent.is_dir() {
        return Err("transaction parent must exist".into());
    }
    Ok((
        parent.join(format!("{operation_id}.staging")),
        parent.join(format!("{operation_id}.journal.json")),
    ))
}

fn write_initial(path: &Path, journal: &TransactionJournal) -> Result<(), String> {
    let contents = serde_json::to_vec_pretty(journal).map_err(|_| "journal encoding failed")?;
    let mut handle = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| "transaction ID already exists or journal cannot be created")?;
    handle
        .write_all(&contents)
        .and_then(|_| handle.sync_all())
        .map_err(|_| "initial journal write failed".into())
}

/// Atomic replace, with a unique temporary file in the same directory.
/// On failure the initial journal remains, so recovery reports INTERRUPTED.
fn update_journal(path: &Path, journal: &TransactionJournal) -> Result<(), String> {
    let next = path.with_extension("journal.pending");
    let bytes = serde_json::to_vec_pretty(journal).map_err(|_| "journal encoding failed")?;
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&next)
        .map_err(|_| "journal update collision")?;
    let outcome = output.write_all(&bytes).and_then(|_| output.sync_all());
    drop(output);
    if outcome.is_err() {
        return Err("journal update failed".into());
    }
    fs::rename(&next, path).map_err(|_| "journal atomic replace failed".into())
}

fn read_journal(path: &Path) -> Result<TransactionJournal, String> {
    reject_existing_symlinks(path)?;
    let file = File::open(path).map_err(|_| "transaction journal missing")?;
    let metadata = file.metadata().map_err(|_| "journal metadata unreadable")?;
    if !metadata.is_file() || metadata.len() > JOURNAL_LIMIT || metadata.len() == 0 {
        return Err("journal is not a bounded regular file".into());
    }
    let mut buffer = Vec::new();
    file.take(JOURNAL_LIMIT + 1)
        .read_to_end(&mut buffer)
        .map_err(|_| "cannot read transaction journal")?;
    if buffer.len() as u64 > JOURNAL_LIMIT {
        return Err("oversized transaction journal".into());
    }
    let journal: TransactionJournal =
        serde_json::from_slice(&buffer).map_err(|_| "corrupt transaction journal")?;
    if journal.version != JOURNAL_VERSION || journal.ready {
        return Err("unrecognized or unsafe transaction journal".into());
    }
    validate_id(&journal.operation_id)?;
    Ok(journal)
}

/// Resume never infers success from leftover bytes. An INSTALLING journal
/// means the process may have exited at any point; audit again from scratch.
pub fn recover_transaction(
    parent: &Path,
    operation_id: &str,
) -> Result<TransactionJournal, String> {
    let (stage, journal_path) = paths(parent, operation_id)?;
    let mut journal = read_journal(&journal_path)?;
    if journal.operation_id != operation_id {
        return Err("transaction journal operation mismatch".into());
    }
    reject_existing_symlinks(&stage)?;
    if journal.state == TransactionState::Installing {
        journal.state = TransactionState::Interrupted;
        update_journal(&journal_path, &journal)?;
    }
    Ok(journal)
}

/// Request rollback only for an owned private stage. Preserve bytes for
/// diagnostics rather than deleting worlds or other unrecognized directories.
/// An interrupted job can never publish itself after this transition.
pub fn rollback_transaction(
    parent: &Path,
    operation_id: &str,
) -> Result<TransactionJournal, String> {
    let (stage, journal_path) = paths(parent, operation_id)?;
    let mut journal = read_journal(&journal_path)?;
    if journal.operation_id != operation_id {
        return Err("transaction journal operation mismatch".into());
    }
    reject_existing_symlinks(&stage)?;
    if journal.state == TransactionState::RolledBack {
        return Ok(journal);
    }
    journal.state = TransactionState::RolledBack;
    journal.ready = false;
    update_journal(&journal_path, &journal)?;
    Ok(journal)
}

/// Stages only new files, never a preexisting destination. The durable journal
/// is created *before* the first extraction/download, allowing crash detection.
/// This does not execute Forge, install Minecraft or publish READY.
pub fn stage_transaction(
    parent: &Path,
    operation_id: &str,
    archive: &Path,
    metadata: &PackMetadata,
    resolved: &CurseForgeResolveReport,
    cancel: &AtomicBool,
) -> Result<TransactionResult, String> {
    let (stage, journal_path) = paths(parent, operation_id)?;
    if metadata.source != PackSource::CurseforgeZip {
        return Err("only CurseForge packs supported by this transaction".into());
    }
    if stage.exists() || journal_path.exists() {
        return Err("transaction already exists; use recovery, never overwrite".into());
    }
    if metadata.archive_sha256.len() != 64
        || !metadata
            .archive_sha256
            .bytes()
            .all(|x| x.is_ascii_hexdigit())
        || metadata.loader.kind != "forge"
        || metadata.loader.version.is_none()
    {
        return Err("verified CurseForge manifest and Forge version required".into());
    }
    let mut journal = TransactionJournal {
        version: JOURNAL_VERSION,
        operation_id: operation_id.to_owned(),
        archive_sha256: metadata.archive_sha256.clone(),
        minecraft_version: metadata.game_version.clone(),
        forge_version: metadata.loader.version.clone().unwrap_or_default(),
        state: TransactionState::Installing,
        required_total: metadata.files.iter().filter(|f| f.required).count(),
        optional_total: metadata.files.iter().filter(|f| !f.required).count(),
        downloaded: 0,
        blocked: 0,
        failures: 0,
        ready: false,
    };
    write_initial(&journal_path, &journal)?;
    let outcome = stage_curseforge_downloads(archive, metadata, resolved, &stage, cancel);
    let mut installed = Vec::new();
    let mut blocked = Vec::new();
    let mut failures = Vec::new();
    match outcome {
        Ok(StageReport {
            status,
            installed: done,
            blocked: unavailable,
            failures: errors,
            ..
        }) => {
            journal.state = match status {
                InstallStatus::Cancelled => TransactionState::Cancelled,
                InstallStatus::Failed => TransactionState::Failed,
                InstallStatus::NeedsAction => TransactionState::NeedsAction,
            };
            installed = done;
            blocked = unavailable;
            failures = errors;
        }
        Err(_) => {
            journal.state = TransactionState::Failed;
            failures
                .push("stage extraction/download failed; inspect private journal and stage".into());
        }
    }
    journal.downloaded = installed.len();
    journal.blocked = blocked.len();
    journal.failures = failures.len();
    update_journal(&journal_path, &journal)?;
    Ok(TransactionResult {
        journal,
        staging_path: stage,
        installed,
        blocked,
        failures,
        ready: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Loader, PackSource};

    fn metadata() -> PackMetadata {
        PackMetadata {
            name: "Rustic Waters fixture".into(),
            source: PackSource::CurseforgeZip,
            game_version: "1.16.5".into(),
            loader: Loader {
                kind: "forge".into(),
                version: Some("36.2.34".into()),
            },
            archive_sha256: "a".repeat(64),
            archive_entries: 1,
            uncompressed_bytes: 1024,
            files: Vec::new(),
            overrides: Vec::new(),
            warnings: Vec::new(),
        }
    }

    #[test]
    fn interrupted_journal_does_not_publish_or_delete_existing_world() {
        let dir = tempfile::tempdir().unwrap();
        let parent = dir.path().canonicalize().unwrap();
        let stage = parent.join("test-job-1.staging");
        fs::create_dir(&stage).unwrap();
        fs::write(stage.join("diagnostic.txt"), b"keep").unwrap();
        let world = parent.join("existing-world");
        fs::create_dir(&world).unwrap();
        fs::write(world.join("level.dat"), b"existing save").unwrap();
        let mut journal = TransactionJournal {
            version: JOURNAL_VERSION,
            operation_id: "test-job-1".into(),
            archive_sha256: "a".repeat(64),
            minecraft_version: "1.16.5".into(),
            forge_version: "36.2.34".into(),
            state: TransactionState::Installing,
            required_total: 7,
            optional_total: 2,
            downloaded: 3,
            blocked: 1,
            failures: 0,
            ready: false,
        };
        write_initial(&parent.join("test-job-1.journal.json"), &journal).unwrap();
        journal = recover_transaction(&parent, "test-job-1").unwrap();
        assert_eq!(journal.state, TransactionState::Interrupted);
        assert!(!journal.ready);
        assert_eq!(fs::read(world.join("level.dat")).unwrap(), b"existing save");
        journal = rollback_transaction(&parent, "test-job-1").unwrap();
        assert_eq!(journal.state, TransactionState::RolledBack);
        assert!(!journal.ready);
        assert_eq!(fs::read(stage.join("diagnostic.txt")).unwrap(), b"keep");
        assert_eq!(fs::read(world.join("level.dat")).unwrap(), b"existing save");
    }

    #[test]
    fn cancel_before_staging_preserves_nonready_journal() {
        let dir = tempfile::tempdir().unwrap();
        let parent = dir.path().canonicalize().unwrap();
        let cancelled = AtomicBool::new(true);
        let pack = metadata();
        let resolved = CurseForgeResolveReport {
            files: Vec::new(),
            resolved_count: 0,
            needs_action_count: 0,
            dependency_actions: Vec::new(),
            ready: false,
        };
        let result = stage_transaction(
            &parent,
            "cancel-job-1",
            &parent.join("never-created.zip"),
            &pack,
            &resolved,
            &cancelled,
        )
        .unwrap();
        assert_eq!(result.journal.state, TransactionState::Cancelled);
        assert!(!result.ready);
        assert_eq!(result.journal.downloaded, 0);
        assert!(!parent.join("cancel-job-1.staging").exists());
        assert!(stage_transaction(
            &parent,
            "cancel-job-1",
            &parent.join("never-created.zip"),
            &pack,
            &resolved,
            &cancelled,
        )
        .is_err());
    }

    #[test]
    fn fail_closed_on_stage_error_and_preserve_existing_profiles() {
        let dir = tempfile::tempdir().unwrap();
        let parent = dir.path().canonicalize().unwrap();
        fs::write(parent.join("profiles.json"), b"user profile").unwrap();
        let pack = metadata();
        let resolved = CurseForgeResolveReport {
            files: Vec::new(),
            resolved_count: 0,
            needs_action_count: 0,
            dependency_actions: Vec::new(),
            ready: false,
        };
        let result = stage_transaction(
            &parent,
            "broken-job-1",
            &parent.join("missing.zip"),
            &pack,
            &resolved,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(result.journal.state, TransactionState::Failed);
        assert_eq!(result.journal.failures, 1);
        assert!(!result.ready);
        assert_eq!(
            fs::read(parent.join("profiles.json")).unwrap(),
            b"user profile"
        );
    }

    #[test]
    fn rollback_is_idempotent_without_deleting_staging() {
        let dir = tempfile::tempdir().unwrap();
        let parent = dir.path().canonicalize().unwrap();
        let journal = TransactionJournal {
            version: JOURNAL_VERSION,
            operation_id: "rollback-1".into(),
            archive_sha256: "a".repeat(64),
            minecraft_version: "1.16.5".into(),
            forge_version: "36.2.34".into(),
            state: TransactionState::Failed,
            required_total: 2,
            optional_total: 1,
            downloaded: 1,
            blocked: 0,
            failures: 1,
            ready: false,
        };
        write_initial(&parent.join("rollback-1.journal.json"), &journal).unwrap();
        assert_eq!(
            rollback_transaction(&parent, "rollback-1").unwrap().state,
            TransactionState::RolledBack
        );
        assert_eq!(
            rollback_transaction(&parent, "rollback-1").unwrap().state,
            TransactionState::RolledBack
        );
    }
}
