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
    pub error: Option<String>,
}

/// Distinct from a successfully installed/committed Forge instance. Reports refer to staging.
pub fn run_verified_processors(request: &ProcessorRunRequest) -> ProcessorRunReport {
    let mut report = ProcessorRunReport {
        outcome: ProcessorOutcome::Failed,
        completed: 0,
        exit_codes: vec![],
        log_paths: vec![],
        verified_outputs: vec![],
        error: None,
    };
    if let Err(err) = execute(request, &mut report) {
        report.error = Some(err.to_string());
        // A failed private stage is NOT committed. Retain it for processor logs/evidence;
        // the caller may explicitly clean it up after collecting diagnostics.
        return report;
    }
    report.outcome = ProcessorOutcome::Verified;
    report
}

fn execute(
    request: &ProcessorRunRequest,
    report: &mut ProcessorRunReport,
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
    tl_game::probe_java(&request.java, 8)
        .map_err(|e| LoaderError::InvalidMetadata(format!("Java 8 required: {e}")))?;
    fs::create_dir(&request.stage_root)
        .map_err(|e| LoaderError::InvalidMetadata(format!("stage create: {e}")))?;
    let mut marker = File::create(request.stage_root.join(".tl-loader-stage"))
        .map_err(|e| LoaderError::InvalidMetadata(format!("stage marker: {e}")))?;
    marker
        .write_all(b"tl-loader-forge-v1")
        .map_err(|e| LoaderError::InvalidMetadata(format!("stage marker: {e}")))?;
    for (index, processor) in request.processors.iter().enumerate() {
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
            verify_sha1(&target, &expected.sha1)?;
            report.verified_outputs.push(target);
        }
        report.completed += 1;
    }
    Ok(())
}

fn validate_processor(processor: &ApprovedProcessor) -> Result<(), LoaderError> {
    if processor.outputs.is_empty()
        || processor.outputs.len() > 64
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
    ];
    if !trusted.iter().any(|(prefix, main)| {
        processor.executable.coordinate.starts_with(prefix) && processor.main_class == *main
    }) {
        return Err(LoaderError::InvalidMetadata(
            "unapproved processor entrypoint".into(),
        ));
    }
    for file in std::iter::once(&processor.executable).chain(processor.classpath.iter()) {
        maven::artifact_path(&file.coordinate)?;
        let relative = maven::artifact_path(&file.coordinate)?;
        let origin_ok = [
            "https://maven.minecraftforge.net",
            "https://repo.maven.apache.org/maven2",
            "https://libraries.minecraft.net",
        ]
        .iter()
        .any(|base| file.source == format!("{base}/{relative}"));
        if !origin_ok {
            return Err(LoaderError::InvalidMetadata(
                "unapproved processor artifact source".into(),
            ));
        }
        verify_sha1(&file.path, &file.sha1)?;
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
    for item in &processor.outputs {
        tl_core::validate_safe_relative_path(&item.relative_path)
            .map_err(|_| LoaderError::InvalidInput)?;
        valid_sha1(&item.sha1)?;
    }
    Ok(())
}

fn valid_sha1(sha1: &str) -> Result<(), LoaderError> {
    if sha1.len() != 40 || !sha1.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(LoaderError::InvalidMetadata(
            "expected official SHA1".into(),
        ));
    }
    Ok(())
}

fn verify_sha1(path: &Path, sha1: &str) -> Result<(), LoaderError> {
    valid_sha1(sha1)?;
    reject_symlink_ancestors(path)?;
    use sha1::{Digest, Sha1};
    let mut file = File::open(path)
        .map_err(|e| LoaderError::InvalidMetadata(format!("artifact missing: {e}")))?;
    let mut hash = Sha1::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let n = file.read(&mut buffer)
            .map_err(|e| LoaderError::InvalidMetadata(format!("artifact read failed: {e}")))?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    if format!("{:x}", hash.finalize()) != sha1.to_ascii_lowercase() {
        return Err(LoaderError::InvalidMetadata("artifact checksum mismatch".into()));
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
