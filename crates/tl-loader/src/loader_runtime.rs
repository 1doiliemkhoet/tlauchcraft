//! Version-aware loader runtime contracts, independent of a single Forge release.
//! This is an integration interface: it never executes Java or mutates an instance.
use crate::{
    forge_graph::{prepare_forge_1165, ForgeBindings, PrepareReport, PrepareState},
    runtime::{IntermediateEvidence, ProcessorOutcome, ProcessorRunReport, ProcessorRunRequest, VerifiedProcessorJar},
    LoaderError, LoaderKind,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeState {
    NeedsAction,
    Failed,
    /// All processors and declared output hashes checked; no instance commit implied.
    ProcessorsVerified,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoaderRuntimeVerification {
    pub state: RuntimeState,
    pub loader: LoaderKind,
    pub minecraft_version: String,
    pub loader_version: String,
    pub expected_processors: usize,
    pub completed_processors: usize,
    pub verified_outputs: Vec<PathBuf>,
    pub log_paths: Vec<PathBuf>,
    pub missing: Vec<String>,
    pub error: Option<String>,
    /// Always false. Only Chat 4's transactional instance installer may mark Ready.
    pub instance_ready: bool,
}

/// Typed handoff to Chat 4. Only Chat 4 may perform a transactional instance
/// commit after independent inheritance, generated library and world safety checks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoaderInstallationHandoff {
    pub loader: LoaderKind,
    pub minecraft_version: String,
    pub loader_version: String,
    pub required_java_major: u32,
    pub runtime: LoaderRuntimeVerification,
    pub verified_artifacts: Vec<VerifiedProcessorJar>,
    pub intermediate_evidence: Vec<IntermediateEvidence>,
    pub missing_dependencies: Vec<String>,
    pub can_publish_instance: bool,
}

/// Validate a reported library against the official Forge installation inventory.
/// Missing/empty official library URL is a NeedsAction, not an inferred Maven URL.
pub fn installation_handoff(
    runtime: LoaderRuntimeVerification,
    audit: &crate::forge_provenance::ForgeProvenanceAudit,
    local_artifacts: &[VerifiedProcessorJar],
    execution: Option<&ProcessorRunReport>,
) -> LoaderInstallationHandoff {
    let mut missing = audit.needs_action.clone();
    let all = audit.profile_libraries.iter().chain(&audit.derived_libraries);
    let mut verified = Vec::new();
    for library in all {
        let candidate = local_artifacts.iter().find(|x| x.coordinate == library.coordinate);
        match (&library.source, candidate) {
            (
                crate::forge_provenance::SourceRequirement::CanonicalMaven { url },
                Some(artifact),
            ) if artifact.source == *url
                && artifact.sha1.eq_ignore_ascii_case(&library.sha1)
                && crate::runtime::validate_verified_artifact(artifact).is_ok() =>
            {
                verified.push(artifact.clone());
            }
            _ => missing.push(format!("verified-library:{}", library.coordinate)),
        }
    }
    if runtime.state != RuntimeState::ProcessorsVerified {
        missing.push("approved-processor-execution-evidence".into());
    }
    if runtime.minecraft_version != audit.minecraft_version
        || audit.derived_version != format!("{}-forge-{}", runtime.minecraft_version, runtime.loader_version)
    {
        missing.push("derived-version-inheritance-mismatch".into());
    }
    LoaderInstallationHandoff {
        loader: runtime.loader.clone(),
        minecraft_version: runtime.minecraft_version.clone(),
        loader_version: runtime.loader_version.clone(),
        required_java_major: 8, // current audited Forge 1.16.5 processor adapter
        intermediate_evidence: execution.map_or_else(Vec::new, |r| r.intermediate_evidence.clone()),
        runtime,
        verified_artifacts: verified,
        missing_dependencies: missing,
        can_publish_instance: false,
    }
}

pub trait LoaderRuntimeAdapter {
    /// Produce a preflight request, not a runnable permission or an install verdict.
    fn prepare(
        &self,
        profile: &str,
        derived_version: &Value,
        bindings: &ForgeBindings,
    ) -> PrepareReport;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ForgeLegacy1165Adapter;

impl LoaderRuntimeAdapter for ForgeLegacy1165Adapter {
    fn prepare(
        &self,
        profile: &str,
        derived_version: &Value,
        bindings: &ForgeBindings,
    ) -> PrepareReport {
        prepare_forge_1165(profile, derived_version, bindings)
    }
}

/// A conservative version compatibility gate. Unsupported combinations fail closed.
/// New Forge/NeoForge families require a separately audited adapter, not a fallback.
pub fn forge_adapter_for(
    minecraft_version: &str,
    forge_version: &str,
) -> Result<ForgeLegacy1165Adapter, LoaderError> {
    crate::validate_component(minecraft_version)?;
    crate::validate_component(forge_version)?;
    if minecraft_version == "1.16.5" && forge_version.starts_with("36.2.") {
        return Ok(ForgeLegacy1165Adapter);
    }
    Err(LoaderError::UnsupportedVersion(format!(
        "Forge {minecraft_version}-{forge_version}: runtime adapter not implemented"
    )))
}

/// Convert plan and actual process evidence into a typed Chat 4 handoff.
/// It never converts a plan-only Verified status into execution success.
pub fn verification_from_prepare(
    loader: LoaderKind,
    minecraft_version: &str,
    loader_version: &str,
    plan: &PrepareReport,
) -> LoaderRuntimeVerification {
    let mut result = LoaderRuntimeVerification {
        state: match plan.state {
            PrepareState::Verified | PrepareState::NeedsAction => RuntimeState::NeedsAction,
            PrepareState::Failed => RuntimeState::Failed,
        },
        loader,
        minecraft_version: minecraft_version.to_owned(),
        loader_version: loader_version.to_owned(),
        expected_processors: plan.request.as_ref().map_or(0, |r| r.processors.len()),
        completed_processors: 0,
        verified_outputs: Vec::new(),
        log_paths: Vec::new(),
        missing: plan.unresolved.clone(),
        error: plan.error.clone(),
        instance_ready: false,
    };
    if plan.state == PrepareState::Verified {
        result
            .missing
            .push("approved-restricted-processor-execution".to_owned());
    }
    result
}

/// Caller has responsibility for confirming Control's external execution approval.
/// This function only assesses evidence; it NEVER starts Java or commits game files.
pub fn verification_from_execution(
    loader: LoaderKind,
    minecraft_version: &str,
    loader_version: &str,
    request: &ProcessorRunRequest,
    execution: &ProcessorRunReport,
) -> LoaderRuntimeVerification {
    let expected = request.processors.len();
    let expected_outputs: usize = request.processors.iter().map(|p| p.outputs.len()).sum();
    let expected_intermediates: usize = request
        .processors
        .iter()
        .map(|p| p.intermediate_outputs.len())
        .sum();
    let expected_paths: Vec<PathBuf> = request
        .processors
        .iter()
        .flat_map(|p| {
            p.outputs
                .iter()
                .map(|o| request.stage_root.join(&o.relative_path))
        })
        .collect();
    let hashes_valid = execution.verified_outputs == expected_paths
        && request.processors.iter().all(|p| {
            p.outputs.iter().all(|o| {
                let target = request.stage_root.join(&o.relative_path);
                target.is_file() && crate::runtime::verify_sha1(&target, &o.sha1).is_ok()
            })
        });
    let mut observed = std::collections::BTreeSet::new();
    let intermediate_evidence_valid = execution.intermediate_evidence.len()
        == expected_intermediates
        && execution.intermediate_evidence.iter().all(|e| {
            let path = request.stage_root.join(&e.relative_path);
            if e.producer_index >= request.processors.len()
                || !request.processors[e.producer_index]
                    .intermediate_outputs
                    .contains(&e.relative_path)
                || !observed.insert((e.producer_index, e.relative_path.clone()))
                || e.size_bytes == 0
                || e.observed_sha1.len() != 40
                || !e.observed_sha1.bytes().all(|b| b.is_ascii_hexdigit())
                || crate::runtime::verify_sha1(&path, &e.observed_sha1).is_err()
            {
                return false;
            }
            std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() == e.size_bytes)
        });
    let complete = expected > 0
        && execution.outcome == ProcessorOutcome::Verified
        && execution.error.is_none()
        && execution.completed == expected
        && execution.exit_codes.len() == expected
        && execution.exit_codes.iter().all(|x| *x == Some(0))
        && execution.verified_outputs.len() == expected_outputs
        && hashes_valid
        && intermediate_evidence_valid;
    LoaderRuntimeVerification {
        state: if complete {
            RuntimeState::ProcessorsVerified
        } else {
            RuntimeState::Failed
        },
        loader,
        minecraft_version: minecraft_version.to_owned(),
        loader_version: loader_version.to_owned(),
        expected_processors: expected,
        completed_processors: execution.completed,
        verified_outputs: execution.verified_outputs.clone(),
        log_paths: execution.log_paths.clone(),
        missing: Vec::new(),
        error: if complete {
            None
        } else {
            Some(
                execution.error.clone().unwrap_or_else(|| {
                    "processor evidence incomplete; no installation verdict".into()
                }),
            )
        },
        instance_ready: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{ApprovedProcessor, ExpectedOutput, VerifiedProcessorJar};
    use std::path::PathBuf;

    #[test]
    fn only_supported_forge_family_can_prepare() {
        assert!(forge_adapter_for("1.16.5", "36.2.29").is_ok());
        assert!(forge_adapter_for("1.16.5", "36.2.34").is_ok());
        assert!(forge_adapter_for("1.20.1", "47.3.0").is_err());
        assert!(forge_adapter_for("1.21.1", "52.0.0").is_err());
    }

    fn request() -> ProcessorRunRequest {
        ProcessorRunRequest {
            minecraft_version: "1.16.5".into(),
            java: PathBuf::from("java.exe"),
            stage_root: PathBuf::from("stage"),
            processors: vec![ApprovedProcessor {
                executable: VerifiedProcessorJar {
                    coordinate: "net.minecraftforge:installertools:1.2.0".into(),
                    path: PathBuf::from("tool.jar"),
                    sha1: "51cff3c1f0bc59f6187e7040cc12a4e9b1eca7aa".into(),
                    source: String::new(),
                },
                classpath: vec![],
                main_class: "net.minecraftforge.installertools.ConsoleTool".into(),
                arguments: vec![],
                outputs: vec![ExpectedOutput {
                    relative_path: "output.jar".into(),
                    sha1: "51cff3c1f0bc59f6187e7040cc12a4e9b1eca7aa".into(),
                }],
                intermediate_outputs: vec![],
            }],
            timeout_seconds: 15,
        }
    }

    #[test]
    fn planning_never_proves_runtime() {
        let plan = PrepareReport {
            state: PrepareState::Verified,
            request: Some(request()),
            unresolved: vec![],
            error: None,
        };
        let verdict = verification_from_prepare(LoaderKind::Forge, "1.16.5", "36.2.29", &plan);
        assert_eq!(verdict.state, RuntimeState::NeedsAction);
        assert!(!verdict.instance_ready);
    }

    #[test]
    fn incomplete_output_evidence_fails_closed() {
        let execution = ProcessorRunReport {
            outcome: ProcessorOutcome::Verified,
            completed: 1,
            exit_codes: vec![Some(0)],
            log_paths: vec![],
            verified_outputs: vec![],
            intermediate_evidence: vec![],
            error: None,
        };
        let verdict = verification_from_execution(
            LoaderKind::Forge,
            "1.16.5",
            "36.2.29",
            &request(),
            &execution,
        );
        assert_eq!(verdict.state, RuntimeState::Failed);
        assert!(!verdict.instance_ready);
    }

    #[test]
    fn verified_processor_stage_does_not_mark_instance_ready() {
        let root = std::env::temp_dir().join(format!("tl03-stage-{}", std::process::id()));
        if root.exists() {
            std::fs::remove_dir_all(&root).unwrap();
        }
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("output.jar"), b"fixture").unwrap();
        let mut req = request();
        req.stage_root = root.clone();
        let execution = ProcessorRunReport {
            outcome: ProcessorOutcome::Verified,
            completed: 1,
            exit_codes: vec![Some(0)],
            log_paths: vec![root.join("process.log")],
            verified_outputs: vec![root.join("output.jar")],
            intermediate_evidence: vec![],
            error: None,
        };
        let verdict =
            verification_from_execution(LoaderKind::Forge, "1.16.5", "36.2.29", &req, &execution);
        assert_eq!(verdict.state, RuntimeState::ProcessorsVerified);
        assert!(!verdict.instance_ready);
        std::fs::write(root.join("output.jar"), b"tampered").unwrap();
        let altered =
            verification_from_execution(LoaderKind::Forge, "1.16.5", "36.2.29", &req, &execution);
        assert_eq!(altered.state, RuntimeState::Failed);
        std::fs::remove_dir_all(root).unwrap();
    }
}
