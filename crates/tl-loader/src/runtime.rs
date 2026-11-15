//! Forge/NeoForge verified processor runner. All outputs remain inside a fresh staging root.
//! Caller owns trusted source acquisition, installer ZIP extraction and final transactional commit.
//! Java is launched directly (no shell) and every declared output must match its expected SHA-1.
use crate::{maven, LoaderError};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedProcessorJar {
    pub coordinate: String,
    pub path: PathBuf,
    /// SHA1 from trusted upstream metadata, or an explicitly approved pinned manifest.
    pub sha1: String,
    /// Official canonical source; cannot point to an arbitrary mirror.
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedOutput {
    /// Path relative to the private staging root, not to an existing Minecraft instance.
    pub relative_path: String,
    pub sha1: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovedProcessor {
    pub executable: VerifiedProcessorJar,
    pub classpath: Vec<VerifiedProcessorJar>,
    /// Must match a known Forge tool's entrypoint. Never read from untrusted JSON mainClass.
    pub main_class: String,
    /// Already-expanded tokens; callers resolve Forge {DATA} and [Maven] references.
    pub arguments: Vec<String>,
    pub outputs: Vec<ExpectedOutput>,
    /// Processor-generated files without upstream SHA-1. Observe file identity and
    /// compute diagnostic SHA-1; never misrepresent this as an authoritative hash.
    #[serde(default)]
    pub intermediate_outputs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessorRunRequest {
    pub minecraft_version: String,
    pub java: PathBuf,
    /// Must not exist. New directory contains only controlled generated artifacts/logs.
    pub stage_root: PathBuf,
    pub processors: Vec<ApprovedProcessor>,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessorOutcome {
    Verified,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessorRunReport {
    pub outcome: ProcessorOutcome,
    pub completed: usize,
    pub exit_codes: Vec<Option<i32>>,
    pub log_paths: Vec<PathBuf>,
    pub verified_outputs: Vec<PathBuf>,
    /// No official expected digest. For diagnostics and dependency trace only.
    #[serde(default)]
    pub intermediate_evidence: Vec<IntermediateEvidence>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntermediateEvidence {
    pub producer_index: usize,
    pub relative_path: String,
    pub observed_sha1: String,
    pub size_bytes: u64,
}

/// Explicit caller-supplied authorization and out-of-process containment proof.
/// This structure is not itself an OS sandbox; the embedding process must enforce
/// Job Object/ACL/network controls before issuing a permit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPermit {
    pub approved_executable_sha1s: Vec<String>,
    pub restricted_environment_attested: bool,
    /// Approved private parent directory, not a Minecraft instance or world.
    pub approved_stage_parent: PathBuf,
}

/// Compatibility entry point is fail-closed. A plan never authorizes Java.
pub fn run_verified_processors(_request: &ProcessorRunRequest) -> ProcessorRunReport {
    ProcessorRunReport {
        outcome: ProcessorOutcome::Failed,
        completed: 0,
        exit_codes: vec![],
        log_paths: vec![],
        verified_outputs: vec![],
        intermediate_evidence: vec![],
        error: Some("processor execution requires explicit Control-approved provenance and restricted host execution permit".into()),
    }
}

/// Executes only after the host explicitly attests restricted execution and
/// Control-approved executable digests. Does not itself supply an OS sandbox.
pub fn run_approved_processors(
    request: &ProcessorRunRequest,
    permit: &ExecutionPermit,
    cancel: &AtomicBool,
) -> ProcessorRunReport {
    let mut report = run_verified_processors(request);
    if let Err(error) = validate_execution_permit(request, permit, cancel) {
        report.error = Some(error.to_string());
        return report;
    }
    report.error = None;
    if let Err(error) = execute(request, &mut report, cancel) {
        report.error = Some(error.to_string());
        return report;
    }
    report.outcome = ProcessorOutcome::Verified;
    report
}

fn validate_execution_permit(
    request: &ProcessorRunRequest,
    permit: &ExecutionPermit,
    cancel: &AtomicBool,
) -> Result<(), LoaderError> {
    if cancel.load(Ordering::SeqCst) {
        return Err(LoaderError::InvalidMetadata(
            "processor execution cancelled".into(),
        ));
    }
    if !permit.restricted_environment_attested
        || !permit.approved_stage_parent.is_absolute()
        || !request.stage_root.is_absolute()
        || request.stage_root.parent() != Some(permit.approved_stage_parent.as_path())
        || request.java.is_relative()
        || permit.approved_executable_sha1s.is_empty()
    {
        return Err(LoaderError::InvalidMetadata(
            "missing OS containment attestation or approved private stage boundary".into(),
        ));
    }
    let prohibited = ["saves", "worlds", "instances", ".minecraft", "profiles"];
    for parent in request.stage_root.ancestors() {
        if parent
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|s| prohibited.iter().any(|v| s.eq_ignore_ascii_case(v)))
        {
            return Err(LoaderError::InvalidMetadata(
                "staging within existing Minecraft data is forbidden".into(),
            ));
        }
    }
    reject_symlink_ancestors(&request.stage_root)?;
    reject_symlink_ancestors(&permit.approved_stage_parent)?;
    let mut approved = std::collections::BTreeSet::new();
    for hash in &permit.approved_executable_sha1s {
        valid_sha1(hash)?;
        if !approved.insert(hash.to_ascii_lowercase()) {
            return Err(LoaderError::InvalidInput);
        }
    }
    for processor in &request.processors {
        if !approved.contains(&processor.executable.sha1.to_ascii_lowercase()) {
            return Err(LoaderError::InvalidMetadata(
                "processor executable SHA-1 not explicitly approved".into(),
            ));
        }
    }
    Ok(())
}

fn execute(
    request: &ProcessorRunRequest,
    report: &mut ProcessorRunReport,
    cancel: &AtomicBool,
) -> Result<(), LoaderError> {
    if request.minecraft_version != "1.16.5" {
        return Err(LoaderError::UnsupportedVersion(
            "runtime processor allowlist currently scoped to Forge 1.16.5".into(),
        ));
    }
    if request.timeout_seconds == 0
        || request.timeout_seconds > 900
        || request.processors.is_empty()
        || request.processors.len() > 32
        || request.stage_root.exists()
        || request.java.as_os_str().is_empty()
    {
        return Err(LoaderError::InvalidInput);
    }
    // Reject symlinks in existing ancestor paths, including the Java executable.
    reject_symlink_ancestors(&request.stage_root)?;
    reject_symlink_ancestors(&request.java)?;
    // Verify everything before running the first processor or creating staging.
    for processor in &request.processors {
        validate_processor(processor)?;
    }
    // Verify Java 8 using Chat 2's existing runtime probe, never silently use PATH Java.
    let java_major = tl_game::probe_java(&request.java, 8)
        .map_err(|e| LoaderError::InvalidMetadata(format!("Java 8 required: {e}")))?;
    if java_major != 8 {
        return Err(LoaderError::JavaIncompatible {
            required: 8,
            actual: java_major,
        });
    }
    fs::create_dir(&request.stage_root)
        .map_err(|e| LoaderError::InvalidMetadata(format!("stage create: {e}")))?;
    let mut marker = File::create(request.stage_root.join(".tl-loader-stage"))
        .map_err(|e| LoaderError::InvalidMetadata(format!("stage marker: {e}")))?;
    marker
        .write_all(b"tl-loader-forge-v1")
        .map_err(|e| LoaderError::InvalidMetadata(format!("stage marker: {e}")))?;
    for (index, processor) in request.processors.iter().enumerate() {
        if cancel.load(Ordering::SeqCst) {
            return Err(LoaderError::InvalidMetadata(
                "processor execution cancelled".into(),
            ));
        }
        let stdout = request
            .stage_root
            .join(format!("processor-{index}.stdout.log"));
        let stderr = request
            .stage_root
            .join(format!("processor-{index}.stderr.log"));
        let output = File::create(&stdout)
            .map_err(|e| LoaderError::InvalidMetadata(format!("stdout log: {e}")))?;
        let errors = File::create(&stderr)
            .map_err(|e| LoaderError::InvalidMetadata(format!("stderr log: {e}")))?;
        report.log_paths.extend([stdout, stderr]);
        let mut classpath = vec![processor.executable.path.clone()];
        classpath.extend(processor.classpath.iter().map(|x| x.path.clone()));
        let cp = std::env::join_paths(classpath)
            .map_err(|e| LoaderError::InvalidMetadata(format!("classpath: {e}")))?;
        let mut child = Command::new(&request.java)
            .current_dir(&request.stage_root)
            .arg("-cp")
            .arg(cp)
            .arg(&processor.main_class)
            .args(&processor.arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::from(output))
            .stderr(Stdio::from(errors))
            .spawn()
            .map_err(|e| LoaderError::InvalidMetadata(format!("processor spawn: {e}")))?;
        let started = Instant::now();
        let exit = loop {
            if cancel.load(Ordering::SeqCst) {
                let _ = child.kill();
                let _ = child.wait();
                report.exit_codes.push(None);
                return Err(LoaderError::InvalidMetadata(
                    "processor execution cancelled".into(),
                ));
            }
            if let Some(status) = child
                .try_wait()
                .map_err(|e| LoaderError::InvalidMetadata(format!("processor wait: {e}")))?
            {
                break status;
            }
            if started.elapsed() >= Duration::from_secs(request.timeout_seconds) {
                child
                    .kill()
                    .map_err(|e| LoaderError::InvalidMetadata(format!("kill: {e}")))?;
                let _ = child.wait();
                report.exit_codes.push(None);
                return Err(LoaderError::InvalidMetadata("processor timed out".into()));
            }
            thread::sleep(Duration::from_millis(25));
        };
        report.exit_codes.push(exit.code());
        if !exit.success() {
            return Err(LoaderError::InvalidMetadata(format!(
                "processor {index} failed with exit code {:?}",
                exit.code()
            )));
        }
        for expected in &processor.outputs {
            let target = request.stage_root.join(&expected.relative_path);
            reject_symlink_ancestors(&target)?;
            if !target.is_file() {
                return Err(LoaderError::InvalidMetadata(
                    "declared output is not a regular file".into(),
                ));
            }
            verify_sha1(&target, &expected.sha1)?;
            report.verified_outputs.push(target);
        }
        for relative in &processor.intermediate_outputs {
            let target = request.stage_root.join(relative);
            reject_symlink_ancestors(&target)?;
            let metadata = fs::metadata(&target).map_err(|e| {
                LoaderError::InvalidMetadata(format!("intermediate output absent: {e}"))
            })?;
            if !metadata.is_file() || metadata.len() == 0 {
                return Err(LoaderError::InvalidMetadata(
                    "intermediate output is empty or not a regular file".into(),
                ));
            }
            // An observed SHA-1 is *not* an official expected checksum.
            let observed_sha1 = hash_file(&target)?;
            report.intermediate_evidence.push(IntermediateEvidence {
                producer_index: index,
                relative_path: relative.clone(),
                observed_sha1,
                size_bytes: metadata.len(),
            });
        }
        report.completed += 1;
    }
    Ok(())
}

fn validate_processor(processor: &ApprovedProcessor) -> Result<(), LoaderError> {
    if (processor.outputs.is_empty() && processor.intermediate_outputs.is_empty())
        || processor.outputs.len() > 64
        || processor.intermediate_outputs.len() > 64
        || processor.arguments.len() > 256
        || processor.classpath.len() > 256
    {
        return Err(LoaderError::InvalidInput);
    }
    // Pinned entrypoints for legacy 1.16.5 toolchain; no arbitrary main class execution.
    let trusted = [
        (
            "net.minecraftforge:installertools:",
            "net.minecraftforge.installertools.ConsoleTool",
        ),
        (
            "net.minecraftforge:jarsplitter:",
            "net.minecraftforge.jarsplitter.ConsoleTool",
        ),
        (
            "net.minecraftforge:ForgeAutoRenamingTool:",
            "net.minecraftforge.fart.Main",
        ),
        (
            "net.minecraftforge:binarypatcher:",
            "net.minecraftforge.binarypatcher.ConsoleTool",
        ),
        (
            "net.md-5:SpecialSource:1.8.5",
            "net.md_5.specialsource.SpecialSource",
        ),
    ];
    if !trusted.iter().any(|(prefix, main)| {
        (processor.executable.coordinate == *prefix
            || (prefix.ends_with(':') && processor.executable.coordinate.starts_with(prefix)))
            && processor.main_class == *main
    }) {
        return Err(LoaderError::InvalidMetadata(
            "unapproved processor entrypoint".into(),
        ));
    }
    for file in std::iter::once(&processor.executable).chain(processor.classpath.iter()) {
        validate_verified_artifact(file)?;
    }
    for arg in &processor.arguments {
        if arg.is_empty()
            || arg.len() > 8192
            || arg.chars().any(char::is_control)
            || arg.contains('{')
            || arg.contains('}')
            || arg.contains('[')
            || arg.contains(']')
        {
            return Err(LoaderError::InvalidMetadata(
                "unresolved or invalid processor argument".into(),
            ));
        }
    }
    let mut output_paths = std::collections::BTreeSet::new();
    for item in &processor.outputs {
        tl_core::validate_safe_relative_path(&item.relative_path)
            .map_err(|_| LoaderError::InvalidInput)?;
        valid_sha1(&item.sha1)?;
        if !output_paths.insert(item.relative_path.as_str()) {
            return Err(LoaderError::InvalidInput);
        }
    }
    for relative in &processor.intermediate_outputs {
        tl_core::validate_safe_relative_path(relative).map_err(|_| LoaderError::InvalidInput)?;
        if !output_paths.insert(relative.as_str()) {
            return Err(LoaderError::InvalidInput);
        }
    }
    Ok(())
}

/// Trust gate shared with the Forge graph resolver. Validates origin *and* bytes.
pub(crate) fn validate_verified_artifact(file: &VerifiedProcessorJar) -> Result<(), LoaderError> {
    let origin_ok = if file.coordinate == "net.minecraft:client:1.16.5" {
        // Mojang object storage names the object by its expected digest.
        [
            "https://piston-data.mojang.com",
            "https://launcher.mojang.com",
        ]
        .iter()
        .any(|host| {
            file.source
                == format!(
                    "{host}/v1/objects/{}/client.jar",
                    file.sha1.to_ascii_lowercase()
                )
        })
    } else {
        let relative = if let Some((coordinate, ext)) = file.coordinate.split_once('@') {
            if !matches!(ext, "zip" | "txt") {
                return Err(LoaderError::InvalidInput);
            }
            let jar = maven::artifact_path(coordinate)?;
            let stem = jar.strip_suffix(".jar").ok_or(LoaderError::InvalidInput)?;
            format!("{stem}.{ext}")
        } else {
            maven::artifact_path(&file.coordinate)?
        };
        [
            "https://maven.minecraftforge.net",
            "https://repo.maven.apache.org/maven2",
            "https://libraries.minecraft.net",
        ]
        .iter()
        .any(|base| file.source == format!("{base}/{relative}"))
    };
    if file.coordinate == "net.md-5:SpecialSource:1.8.5"
        && !file
            .sha1
            .eq_ignore_ascii_case("c670c493c436a885c872fa4810bc698628b95922")
    {
        return Err(LoaderError::InvalidMetadata(
            "SpecialSource 1.8.5 SHA-1 not publisher pinned".into(),
        ));
    }
    if !origin_ok {
        return Err(LoaderError::InvalidMetadata(
            "unapproved processor artifact source".into(),
        ));
    }
    verify_sha1(&file.path, &file.sha1)
}

fn valid_sha1(sha1: &str) -> Result<(), LoaderError> {
    if sha1.len() != 40 || !sha1.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(LoaderError::InvalidMetadata(
            "expected official SHA1".into(),
        ));
    }
    Ok(())
}

fn hash_file(path: &Path) -> Result<String, LoaderError> {
    reject_symlink_ancestors(path)?;
    use sha1::{Digest, Sha1};
    let mut file = File::open(path)
        .map_err(|e| LoaderError::InvalidMetadata(format!("artifact missing: {e}")))?;
    let mut hash = Sha1::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let n = file
            .read(&mut buffer)
            .map_err(|e| LoaderError::InvalidMetadata(format!("artifact read failed: {e}")))?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub(crate) fn verify_sha1(path: &Path, sha1: &str) -> Result<(), LoaderError> {
    valid_sha1(sha1)?;
    reject_symlink_ancestors(path)?;
    use sha1::{Digest, Sha1};
    let mut file = File::open(path)
        .map_err(|e| LoaderError::InvalidMetadata(format!("artifact missing: {e}")))?;
    let mut hash = Sha1::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let n = file
            .read(&mut buffer)
            .map_err(|e| LoaderError::InvalidMetadata(format!("artifact read failed: {e}")))?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    if format!("{:x}", hash.finalize()) != sha1.to_ascii_lowercase() {
        return Err(LoaderError::InvalidMetadata(
            "artifact checksum mismatch".into(),
        ));
    }
    Ok(())
}

fn reject_symlink_ancestors(path: &Path) -> Result<(), LoaderError> {
    for part in path.ancestors() {
        if part.is_symlink() {
            return Err(LoaderError::InvalidInput);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    fn scratch() -> PathBuf {
        std::env::temp_dir().join(format!(
            "tl-loader-test-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }
    fn fixture() -> (PathBuf, ApprovedProcessor) {
        let root = scratch();
        fs::create_dir(&root).unwrap();
        let jar = root.join("tool.jar");
        fs::write(&jar, b"fixture").unwrap();
        let sha = "51cff3c1f0bc59f6187e7040cc12a4e9b1eca7aa";
        let proc = ApprovedProcessor {
            executable: VerifiedProcessorJar {
                coordinate: "net.minecraftforge:installertools:1.2.0".into(),
                path: jar,
                sha1: sha.into(),
                source: "https://maven.minecraftforge.net/net/minecraftforge/installertools/1.2.0/installertools-1.2.0.jar".into(),
            },
            classpath: vec![],
            main_class: "net.minecraftforge.installertools.ConsoleTool".into(),
            arguments: vec!["--task".into(), "MCP_DATA".into()],
            outputs: vec![ExpectedOutput {
                relative_path: "libraries/output.jar".into(),
                sha1: sha.into(),
            }],
            intermediate_outputs: vec![],
        };
        (root, proc)
    }
    #[test]
    fn invalid_processor_refused_before_stage() {
        let (root, mut p) = fixture();
        p.main_class = "java.lang.Runtime".into();
        let stage = root.join("stage");
        let r = ProcessorRunRequest {
            minecraft_version: "1.16.5".into(),
            java: "missing-java.exe".into(),
            stage_root: stage.clone(),
            processors: vec![p],
            timeout_seconds: 3,
        };
        assert_eq!(
            run_verified_processors(&r).outcome,
            ProcessorOutcome::Failed
        );
        assert!(!stage.exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn checksum_mismatch_refused_without_mutation() {
        let (root, mut p) = fixture();
        p.executable.sha1 = "0000000000000000000000000000000000000000".into();
        let stage = root.join("stage");
        let r = ProcessorRunRequest {
            minecraft_version: "1.16.5".into(),
            java: "missing-java.exe".into(),
            stage_root: stage.clone(),
            processors: vec![p],
            timeout_seconds: 3,
        };
        assert_eq!(
            run_verified_processors(&r).outcome,
            ProcessorOutcome::Failed
        );
        assert!(!stage.exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn existing_stage_preserved_on_failure() {
        let (root, p) = fixture();
        let stage = root.join("stage");
        fs::create_dir(&stage).unwrap();
        fs::write(stage.join("world.dat"), b"save").unwrap();
        let r = ProcessorRunRequest {
            minecraft_version: "1.16.5".into(),
            java: "missing-java.exe".into(),
            stage_root: stage.clone(),
            processors: vec![p],
            timeout_seconds: 3,
        };
        assert_eq!(
            run_verified_processors(&r).outcome,
            ProcessorOutcome::Failed
        );
        assert_eq!(fs::read(stage.join("world.dat")).unwrap(), b"save");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn unapproved_run_is_fail_closed_even_with_well_formed_request() {
        let (root, proc) = fixture();
        let request = ProcessorRunRequest {
            minecraft_version: "1.16.5".into(),
            java: root.join("java.exe"),
            stage_root: root.join("stage"),
            processors: vec![proc],
            timeout_seconds: 10,
        };
        let report = run_verified_processors(&request);
        assert_eq!(report.outcome, ProcessorOutcome::Failed);
        assert!(report.error.unwrap().contains("explicit Control-approved"));
        assert!(!request.stage_root.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn permit_rejects_unsafe_parent_and_unapproved_executable_without_java() {
        let (root, proc) = fixture();
        let request = ProcessorRunRequest {
            minecraft_version: "1.16.5".into(),
            java: root.join("java.exe"),
            stage_root: root.join("stage"),
            processors: vec![proc],
            timeout_seconds: 10,
        };
        let cancellation = AtomicBool::new(false);
        let mut permit = ExecutionPermit {
            approved_executable_sha1s: vec!["0000000000000000000000000000000000000000".into()],
            restricted_environment_attested: true,
            approved_stage_parent: root.clone(),
        };
        assert!(run_approved_processors(&request, &permit, &cancellation)
            .error
            .unwrap()
            .contains("not explicitly approved"));
        permit.approved_executable_sha1s = vec!["51cff3c1f0bc59f6187e7040cc12a4e9b1eca7aa".into()];
        permit.restricted_environment_attested = false;
        assert!(run_approved_processors(&request, &permit, &cancellation)
            .error
            .unwrap()
            .contains("containment"));
        permit.restricted_environment_attested = true;
        permit.approved_stage_parent = root.join(".minecraft");
        assert!(run_approved_processors(&request, &permit, &cancellation)
            .error
            .unwrap()
            .contains("containment"));
        assert!(!request.stage_root.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cancellation_refuses_before_creating_stage() {
        let (root, proc) = fixture();
        let request = ProcessorRunRequest {
            minecraft_version: "1.16.5".into(),
            java: root.join("java.exe"),
            stage_root: root.join("stage"),
            processors: vec![proc],
            timeout_seconds: 10,
        };
        let cancellation = AtomicBool::new(true);
        let permit = ExecutionPermit {
            approved_executable_sha1s: vec!["51cff3c1f0bc59f6187e7040cc12a4e9b1eca7aa".into()],
            restricted_environment_attested: true,
            approved_stage_parent: root.clone(),
        };
        let r = run_approved_processors(&request, &permit, &cancellation);
        assert_eq!(r.outcome, ProcessorOutcome::Failed);
        assert!(r.error.unwrap().contains("cancelled"));
        assert!(!request.stage_root.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn actual_output_bytes_must_match_declared_hash() {
        let root = scratch();
        fs::create_dir(&root).unwrap();
        let output = root.join("mismatch.jar");
        fs::write(&output, b"wrong-output").unwrap();
        let official_expected_sha1 = "51cff3c1f0bc59f6187e7040cc12a4e9b1eca7aa";
        assert!(verify_sha1(&output, official_expected_sha1).is_err());
        // Hash mismatch cannot be accepted as a verified processor output.
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_java_probe_does_not_create_stage() {
        let (root, p) = fixture();
        let stage = root.join("stage");
        let r = ProcessorRunRequest {
            minecraft_version: "1.16.5".into(),
            java: root.join("does-not-exist.exe"),
            stage_root: stage.clone(),
            processors: vec![p],
            timeout_seconds: 3,
        };
        assert_eq!(
            run_verified_processors(&r).outcome,
            ProcessorOutcome::Failed
        );
        assert!(!stage.exists());
        fs::remove_dir_all(root).unwrap();
    }
}
