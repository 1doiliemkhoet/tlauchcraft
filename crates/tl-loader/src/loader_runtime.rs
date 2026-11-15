//! Version-aware loader runtime contracts, independent of a single Forge release.
//! This is an integration interface: it never executes Java or mutates an instance.
use crate::{
    forge_graph::{prepare_forge_1165, ForgeBindings, PrepareReport, PrepareState},
    runtime::{ProcessorOutcome, ProcessorRunReport, ProcessorRunRequest},
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
    let complete = expected > 0
        && execution.outcome == ProcessorOutcome::Verified
        && execution.error.is_none()
        && execution.completed == expected
        && execution.exit_codes.len() == expected
        && execution.exit_codes.iter().all(|x| *x == Some(0))
        && execution.verified_outputs.len() == expected_outputs;
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
        let execution = ProcessorRunReport {
            outcome: ProcessorOutcome::Verified,
            completed: 1,
            exit_codes: vec![Some(0)],
            log_paths: vec![PathBuf::from("process.log")],
            verified_outputs: vec![PathBuf::from("output.jar")],
            error: None,
        };
        let verdict = verification_from_execution(
            LoaderKind::Forge,
            "1.16.5",
            "36.2.29",
            &request(),
            &execution,
        );
        assert_eq!(verdict.state, RuntimeState::ProcessorsVerified);
        assert!(!verdict.instance_ready);
    }
}
