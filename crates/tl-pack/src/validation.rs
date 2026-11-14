//! Typed dependency audit and fail-closed playable-instance gate.
//! The actual Forge processor runner belongs to tl-loader (Chat 3). This
//! module accepts independent verification evidence; it NEVER runs processors.
use crate::{curseforge::{CurseForgeFile, CurseForgeResolveReport, FileAvailability}, install::InstalledMod, PackMetadata, PackSource};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::{collections::{HashMap, HashSet}, fs::File, io::Read, path::{Path, PathBuf}};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyIssue {
    DuplicateFileId,
    DuplicateDestination,
    MissingRequired,
    MissingOptional,
    DependencyCycle,
    UnsupportedMinecraft,
    LoaderMismatch,
    InvalidChecksum,
    InvalidFileSize,
    UnverifiedLoader,
    UnverifiedJava,
    UnverifiedGame,
    MissingManifest,
    UnsafePath,
    UnverifiedFile,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationFinding {
    pub issue: DependencyIssue,
    pub project_id: Option<u64>,
    pub file_id: Option<u64>,
    pub required: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DependencyRef {
    pub project_id: u64,
    pub file_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyNode {
    pub project_id: u64,
    pub file_id: u64,
    pub required: bool,
    pub minecraft_version: String,
    pub target_path: String,
    pub dependencies: Vec<DependencyRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyAudit {
    pub nodes: usize,
    pub required_nodes: usize,
    pub optional_nodes: usize,
    pub findings: Vec<ValidationFinding>,
    pub complete: bool,
}

fn issue(issue: DependencyIssue, id: Option<(u64, u64)>, required: bool, detail: impl Into<String>) -> ValidationFinding {
    ValidationFinding { issue, project_id: id.map(|x| x.0), file_id: id.map(|x| x.1), required, detail: detail.into() }
}

/// Checks project+file identities, Windows-insensitive destination collisions,
/// game versions, missing edges and graph cycles before any READY decision.
/// Optional dependency absence is reported but does not block completeness.
pub fn audit_dependencies(nodes: &[DependencyNode], minecraft: &str) -> DependencyAudit {
    let mut findings = Vec::new();
    let mut ids = HashSet::new();
    let mut destinations = HashMap::<String, (u64, u64)>::new();
    let mut references = HashMap::<(u64, u64), &DependencyNode>::new();
    for node in nodes {
        let id = (node.project_id, node.file_id);
        if id.0 == 0 || id.1 == 0 || !ids.insert(id) {
            findings.push(issue(DependencyIssue::DuplicateFileId, Some(id), true, "invalid or duplicate projectID/fileID"));
        }
        if node.minecraft_version != minecraft {
            findings.push(issue(DependencyIssue::UnsupportedMinecraft, Some(id), node.required, "declared Minecraft version mismatch"));
        }
        if crate::safe_path(&node.target_path).is_err() || !node.target_path.starts_with("mods/") {
            findings.push(issue(DependencyIssue::UnsafePath, Some(id), node.required, "unsafe or unsupported mod destination"));
        }
        let destination = node.target_path.to_lowercase();
        if destinations.insert(destination, id).is_some() {
            findings.push(issue(DependencyIssue::DuplicateDestination, Some(id), true, "multiple mods target the same Windows filename"));
        }
        references.insert(id, node);
    }
    for node in nodes {
        for dep in &node.dependencies {
            let target = (dep.project_id, dep.file_id);
            if !references.contains_key(&target) {
                findings.push(issue(if node.required { DependencyIssue::MissingRequired } else { DependencyIssue::MissingOptional },
                    Some(target), node.required, "dependency has no resolved, approved file ID"));
            }
        }
    }
    fn dfs(id: (u64,u64), nodes: &HashMap<(u64,u64), &DependencyNode>,
        state: &mut HashMap<(u64,u64),u8>, findings: &mut Vec<ValidationFinding>) {
        if state.get(&id) == Some(&2) { return; }
        if state.get(&id) == Some(&1) {
            findings.push(issue(DependencyIssue::DependencyCycle, Some(id), true, "dependency cycle detected"));
            return;
        }
        state.insert(id, 1);
        if let Some(node) = nodes.get(&id) {
            for dep in &node.dependencies {
                let key = (dep.project_id, dep.file_id);
                if nodes.contains_key(&key) { dfs(key, nodes, state, findings); }
            }
        }
        state.insert(id, 2);
    }
    let mut states = HashMap::new();
    for id in &ids { dfs(*id, &references, &mut states, &mut findings); }
    let complete = !findings.iter().any(|f| f.required);
    DependencyAudit {
        nodes: nodes.len(), required_nodes: nodes.iter().filter(|n| n.required).count(),
        optional_nodes: nodes.iter().filter(|n| !n.required).count(), findings, complete,
    }
}

/// Receipt to be constructed ONLY from a successfully verified tl_loader::runtime
/// installation, never from Modpack ZIP metadata or a preliminary loader plan.
/// The adapter lives in Chat 3 once its runtime API is merged into develop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgeRuntimeReceipt {
    pub minecraft_version: String,
    pub loader_kind: String,
    pub loader_version: String,
    pub required_java_major: u32,
    pub processor_execution_verified: bool,
    pub libraries_verified: bool,
    pub outputs_verified: bool,
    pub java_major_verified: bool,
    pub minecraft_files_verified: bool,
    pub instance_manifest_verified: bool,
}

/// Typed hand-off for Chat 3's `tl_loader::runtime` executor.
/// Pack only consumes a verified receipt; no processor runner duplication.
pub trait ForgeRuntimeVerification {
    fn verified_receipt(&self, manifest: &PackMetadata) -> Result<ForgeRuntimeReceipt, String>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalValidation {
    pub ready: bool,
    pub expected_required: usize,
    pub verified_required: usize,
    pub verified_optional: usize,
    pub blocked: usize,
    pub failed: usize,
    pub findings: Vec<ValidationFinding>,
}
fn hash_file(path: &Path, size: u64) -> Result<(String, String), String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| "file missing".to_owned())?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err("not a regular file".into());
    }
    if metadata.len() != size || size == 0 || size > crate::MAX_SINGLE_FILE {
        return Err("file size mismatch".into());
    }
    let mut sha1 = Sha1::new();
    let mut sha256 = Sha256::new();
    let mut f = File::open(path).map_err(|_| "cannot open file".to_owned())?;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let count = f.read(&mut buf).map_err(|_| "file read failed".to_owned())?;
        if count == 0 { break; }
        sha1.update(&buf[..count]);
        sha256.update(&buf[..count]);
    }
    Ok((format!("{:x}", sha1.finalize()), format!("{:x}", sha256.finalize())))
}

pub fn validate_staged_instance(
    root: &Path,
    manifest: &PackMetadata,
    resolved: &CurseForgeResolveReport,
    installed: &[InstalledMod],
    audit: &DependencyAudit,
    forge: Option<&ForgeRuntimeReceipt>,
) -> FinalValidation {
    let mut findings = audit.findings.clone();
    let mut verified_required = 0;
    let mut verified_optional = 0;
    let mut blocked = 0;
    let mut failed = 0;
    if manifest.source != PackSource::CurseforgeZip || manifest.archive_sha256.len() != 64 {
        findings.push(issue(DependencyIssue::MissingManifest, None, true, "verified pack manifest/archive hash required"));
    }
    let mut installed_by_id = HashMap::new();
    for file in installed {
        if installed_by_id.insert((file.project_id, file.file_id), file).is_some() {
            findings.push(issue(DependencyIssue::DuplicateFileId, Some((file.project_id,file.file_id)), true, "duplicate installed identity"));
        }
    }
    let mut path_names = HashSet::new();
    for file in &resolved.files {
        let id = (file.project_id, file.file_id);
        if file.availability != FileAvailability::Available {
            blocked += 1;
            findings.push(issue(if file.required {DependencyIssue::MissingRequired} else {DependencyIssue::MissingOptional},
                Some(id), file.required, format!("not legally available: {}", file.reason)));
            continue;
        }
        let Some(saved) = installed_by_id.get(&id) else {
            if file.required { failed += 1; }
            findings.push(issue(DependencyIssue::MissingRequired, Some(id), file.required, "resolved mod not present on disk"));
            continue;
        };
        let Some(name) = &file.file_name else {
            failed += 1;
            findings.push(issue(DependencyIssue::UnsafePath, Some(id), true, "mod filename missing"));
            continue;
        };
        let relative = format!("mods/{name}");
        if saved.relative_path != relative || crate::safe_path(&relative).is_err() || !path_names.insert(relative.to_lowercase()) {
            failed += 1;
            findings.push(issue(DependencyIssue::DuplicateDestination, Some(id), true, "unsafe or duplicate mod path"));
            continue;
        }
        let Some(expected_sha1) = file.sha1.as_ref().filter(|v| v.len()==40 && v.bytes().all(|x| x.is_ascii_hexdigit())) else {
            blocked += 1;
            findings.push(issue(DependencyIssue::InvalidChecksum, Some(id), file.required, "trusted SHA-1 missing"));
            continue;
        };
        let Some(expected_size) = file.expected_size else {
            blocked += 1;
            findings.push(issue(DependencyIssue::InvalidFileSize, Some(id), file.required, "trusted file size missing"));
            continue;
        };
        let path: PathBuf = root.join(&relative);
        // Forbid symlinks on ancestors, not just the target.
        if path.ancestors().take_while(|p| *p != root).any(|p|
            std::fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink())) {
            failed += 1;
            findings.push(issue(DependencyIssue::UnsafePath, Some(id), true, "symlink in staged mod path"));
            continue;
        }
        match hash_file(&path, expected_size) {
            Ok((actual_sha1, actual_sha256))
                if actual_sha1.eq_ignore_ascii_case(expected_sha1)
                    && actual_sha256 == saved.actual_sha256 && saved.bytes == expected_size => {
                    if file.required {verified_required += 1;} else {verified_optional += 1;}
                }
            _ => {
                failed += 1;
                findings.push(issue(DependencyIssue::UnverifiedFile, Some(id), file.required, "mod bytes, size or hash do not match"));
            }
        }
    }
    let expected_required = resolved.files.iter().filter(|f| f.required).count();
    if resolved.files.len() < manifest.files.len() || manifest.files.iter().any(|f|
        !resolved.files.iter().any(|r| f.project_id==Some(r.project_id) && f.file_id==Some(r.file_id) && f.required==r.required)) {
        findings.push(issue(DependencyIssue::MissingManifest, None, true, "resolved file set does not cover pack manifest"));
    }
    match forge {
        Some(receipt) if receipt.minecraft_version == manifest.game_version
            && receipt.loader_kind == manifest.loader.kind
            && manifest.loader.version.as_deref() == Some(receipt.loader_version.as_str())
            && receipt.required_java_major != 0
            && receipt.processor_execution_verified && receipt.libraries_verified
            && receipt.outputs_verified && receipt.java_major_verified
            && receipt.minecraft_files_verified && receipt.instance_manifest_verified => {}
        _ => findings.push(issue(DependencyIssue::UnverifiedLoader, None, true,
            "Forge processors, libraries, Java, Minecraft and instance manifest must be verified")),
    }
    let ready = audit.complete && findings.iter().all(|f| !f.required)
        && verified_required == expected_required && !manifest.files.is_empty()
        && forge.is_some();
    FinalValidation { ready, expected_required, verified_required, verified_optional,
        blocked, failed, findings }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn node(id:u64, path:&str, deps:Vec<DependencyRef>, required:bool)->DependencyNode {
        DependencyNode{project_id:id,file_id:id+100,required,minecraft_version:"1.16.5".into(),
            target_path:path.into(),dependencies:deps}
    }
    #[test]
    fn graph_fixture_reports_missing_optional_and_required_and_cycles() {
        let a = node(1,"mods/a.jar",vec![DependencyRef{project_id:2,file_id:102}],true);
        let b = node(2,"mods/b.jar",vec![DependencyRef{project_id:1,file_id:101},
            DependencyRef{project_id:4,file_id:104}],true);
        let c = node(3,"mods/c.jar",vec![DependencyRef{project_id:5,file_id:105}],false);
        let report = audit_dependencies(&[a,b,c], "1.16.5");
        assert_eq!(report.nodes,3);
        assert_eq!(report.required_nodes,2);
        assert_eq!(report.optional_nodes,1);
        assert!(report.findings.iter().any(|f| f.issue==DependencyIssue::DependencyCycle));
        assert!(report.findings.iter().any(|f| f.issue==DependencyIssue::MissingRequired));
        assert!(report.findings.iter().any(|f| f.issue==DependencyIssue::MissingOptional));
        assert!(!report.complete);
    }
    #[test]
    fn graph_fixture_accepts_resolved_dependencies() {
        let report = audit_dependencies(&[
            node(1,"mods/a.jar",vec![DependencyRef{project_id:2,file_id:102}],true),
            node(2,"mods/b.jar",vec![],true),
            node(3,"mods/optional.jar",vec![],false)
        ], "1.16.5");
        assert_eq!(report.nodes,3);
        assert!(report.complete);
        assert!(report.findings.is_empty());
    }
    #[test]
    fn duplicate_case_and_game_version_cannot_be_accepted() {
        let mut b=node(2,"mods/A.jar",vec![],true);
        b.minecraft_version="1.20.1".into();
        let report=audit_dependencies(&[node(1,"mods/a.jar",vec![],true),b],"1.16.5");
        assert!(report.findings.iter().any(|f| f.issue==DependencyIssue::DuplicateDestination));
        assert!(report.findings.iter().any(|f| f.issue==DependencyIssue::UnsupportedMinecraft));
        assert!(!report.complete);
    }
    #[test]
    fn forge_proof_is_mandatory_even_with_all_mods() {
        let pack=PackMetadata{name:"fixture".into(),source:PackSource::CurseforgeZip,
            game_version:"1.16.5".into(),loader:crate::Loader{kind:"forge".into(),version:Some("36.2.34".into())},
            archive_sha256:"a".repeat(64),archive_entries:1,uncompressed_bytes:1,
            files:vec![crate::PlannedFile{path:"mods/fixture.jar".into(),provider:"curseforge".into(),
                project_id:Some(1),file_id:Some(2),urls:vec![],sha1:None,sha512:None,required:true,side:"both".into()}],
            overrides:vec![],warnings:vec![]};
        let report=CurseForgeResolveReport{files:vec![],resolved_count:0,needs_action_count:0,ready:false};
        let audit=audit_dependencies(&[],"1.16.5");
        let result=validate_staged_instance(Path::new("nonexistent"),&pack,&report,&[],&audit,None);
        assert!(!result.ready);
        assert!(result.findings.iter().any(|f| f.issue==DependencyIssue::UnverifiedLoader));
        assert!(result.findings.iter().any(|f| f.issue==DependencyIssue::MissingManifest));
    }
}
