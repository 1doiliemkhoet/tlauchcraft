//! Trusted LAN primitives. Standalone until tl-core freezes its shared API.
//! Does not download Minecraft, install a server loader, or bypass authentication.
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LanError {
    InvalidInput(String),
    UnsafeBind(String),
    PortInUse(u16),
    CompatibilityConflict(Vec<String>),
    NotVerified(String),
    LaunchFailed(String),
    Io(String),
    NetworkFailure(String),
}
impl From<io::Error> for LanError {
    fn from(e: io::Error) -> Self {
        Self::Io(e.to_string())
    }
}
pub type Result<T> = std::result::Result<T, LanError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Side {
    Client,
    Server,
    Both,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackFile {
    pub relative_path: String,
    pub sha256: String,
    pub side: Side,
    pub gameplay: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackFingerprint {
    pub minecraft: String,
    pub loader_kind: String,
    pub loader_version: String,
    pub files: Vec<PackFile>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatibilityReport {
    pub compatible: bool,
    pub blockers: Vec<String>,
    pub ignored_client_files: Vec<String>,
}
fn normalized_files(
    pack: &PackFingerprint,
    server_view: bool,
) -> std::result::Result<BTreeMap<String, String>, String> {
    let mut files = BTreeMap::new();
    for file in &pack.files {
        if file.relative_path.is_empty()
            || file.relative_path.starts_with('/')
            || file.relative_path.contains('\\')
            || file.relative_path.contains(':')
            || file
                .relative_path
                .split('/')
                .any(|s| s.is_empty() || s == "." || s == "..")
        {
            return Err(format!("invalid pack path: {}", file.relative_path));
        }
        if !file.sha256.chars().all(|c| c.is_ascii_hexdigit()) || file.sha256.len() != 64 {
            return Err(format!("missing SHA256 for {}", file.relative_path));
        }
        if server_view && file.side == Side::Client {
            continue;
        }
        if !server_view && file.side == Side::Server {
            continue;
        }
        if !file.gameplay {
            continue;
        }
        if files
            .insert(
                file.relative_path.to_ascii_lowercase(),
                file.sha256.to_ascii_lowercase(),
            )
            .is_some()
        {
            return Err(format!(
                "duplicate case-insensitive file: {}",
                file.relative_path
            ));
        }
    }
    Ok(files)
}
/// Checks identical gameplay-relevant content and loader. Cosmetic client-only files
/// are ignored. This is a preflight, NOT proof that the game handshake will succeed.
pub fn compare_packs(server: &PackFingerprint, client: &PackFingerprint) -> CompatibilityReport {
    let mut blockers = Vec::new();
    if server.minecraft != client.minecraft {
        blockers.push("minecraft version differs".into());
    }
    if server.loader_kind != client.loader_kind || server.loader_version != client.loader_version {
        blockers.push("loader kind/version differs".into());
    }
    let server_files = normalized_files(server, true);
    let client_files = normalized_files(client, false);
    match (server_files, client_files) {
        (Ok(server_files), Ok(client_files)) => {
            let names: BTreeSet<_> = server_files
                .keys()
                .chain(client_files.keys())
                .cloned()
                .collect();
            for name in names {
                // Server-only files need not exist on client, and vice versa.
                let sf = server
                    .files
                    .iter()
                    .find(|f| f.relative_path.eq_ignore_ascii_case(&name));
                let cf = client
                    .files
                    .iter()
                    .find(|f| f.relative_path.eq_ignore_ascii_case(&name));
                if matches!(sf.map(|f| &f.side), Some(Side::Server))
                    || matches!(cf.map(|f| &f.side), Some(Side::Client))
                {
                    continue;
                }
                if server_files.get(&name) != client_files.get(&name) {
                    blockers.push(format!("gameplay file missing or hash differs: {name}"));
                }
            }
        }
        (Err(e), _) | (_, Err(e)) => blockers.push(e),
    }
    let ignored_client_files = client
        .files
        .iter()
        .filter(|f| f.side == Side::Client || !f.gameplay)
        .map(|f| f.relative_path.clone())
        .collect();
    CompatibilityReport {
        compatible: blockers.is_empty(),
        blockers,
        ignored_client_files,
    }
}

#[derive(Debug, Clone)]
pub struct LanServerSpec {
    pub server_root: PathBuf,
    pub bind: Ipv4Addr,
    pub port: u16,
    pub max_players: u16,
    pub whitelist: bool,
    pub acknowledged_offline_risk: bool,
    pub confirmed_private_network: bool,
}
/// Reject unspecified, publicly routable, multicast and loopback interfaces.
/// Site-local 10/8, 172.16/12 and 192.168/16 are accepted, never 0.0.0.0.
pub fn validate_server_spec(spec: &LanServerSpec) -> Result<()> {
    if spec.port == 0 || spec.max_players == 0 || spec.max_players > 100 {
        return Err(LanError::InvalidInput(
            "invalid port or player limit".into(),
        ));
    }
    if !spec.bind.is_private() {
        return Err(LanError::UnsafeBind(format!(
            "{} is not RFC1918 private",
            spec.bind
        )));
    }
    if !spec.acknowledged_offline_risk || !spec.confirmed_private_network {
        return Err(LanError::UnsafeBind(
            "offline identity spoofing risk and Private firewall profile must be acknowledged"
                .into(),
        ));
    }
    if spec.server_root.as_os_str().is_empty() {
        return Err(LanError::InvalidInput("empty server root".into()));
    }
    Ok(())
}
/// Performs exactly one bounded TCP probe to a private LAN endpoint.
/// This only diagnoses reachability: it does NOT verify Minecraft protocol/identity.
pub fn probe_lan_endpoint(host: Ipv4Addr, port: u16, timeout: Duration) -> Result<()> {
    if !(host.is_private() || host.is_loopback())
        || port == 0
        || timeout.is_zero()
        || timeout > Duration::from_secs(10)
    {
        return Err(LanError::InvalidInput(
            "unsafe address, port or probe timeout".into(),
        ));
    }
    TcpStream::connect_timeout(&SocketAddr::new(IpAddr::V4(host), port), timeout)
        .map(|_| ())
        .map_err(|e| LanError::NetworkFailure(e.to_string()))
}

/// Preflight bind only; OS firewall configuration and WAN reachability require on-device tests.
pub fn check_port_available(bind: Ipv4Addr, port: u16) -> Result<()> {
    match TcpListener::bind(SocketAddr::new(IpAddr::V4(bind), port)) {
        Ok(listener) => {
            drop(listener);
            Ok(())
        }
        Err(e) if e.kind() == io::ErrorKind::AddrInUse => Err(LanError::PortInUse(port)),
        Err(e) => Err(LanError::Io(e.to_string())),
    }
}
/// Configuration is generated only for a separate, empty server instance directory.
pub fn initialize_server_properties(spec: &LanServerSpec) -> Result<PathBuf> {
    validate_server_spec(spec)?;
    fs::create_dir_all(&spec.server_root)?;
    let target = spec.server_root.join("server.properties");
    // Do not overwrite an existing server or follow an attacker-created link.
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)?;
    writeln!(output, "# Generated for trusted LAN ONLY")?;
    writeln!(output, "server-ip={}", spec.bind)?;
    writeln!(output, "server-port={}", spec.port)?;
    writeln!(output, "online-mode=false")?;
    writeln!(output, "white-list={}", spec.whitelist)?;
    writeln!(output, "max-players={}", spec.max_players)?;
    writeln!(output, "enable-rcon=false")?;
    writeln!(output, "enable-query=false")?;
    writeln!(output, "enable-command-block=false")?;
    output.sync_all()?;
    Ok(target)
}
/// A safe local address for Direct Connect. No scanning or public discovery.
pub fn direct_connect_address(bind: Ipv4Addr, port: u16) -> Result<String> {
    if !bind.is_private() || port == 0 {
        return Err(LanError::UnsafeBind(
            "non-private address or invalid port".into(),
        ));
    }
    Ok(format!("{bind}:{port}"))
}
/// Preserve offline identity expectations: nickname changes change the standard
/// offline-mode server player UUID; a client-local UUID does not override it.
pub fn validate_offline_username(name: &str) -> Result<()> {
    if !(3..=16).contains(&name.len())
        || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(LanError::InvalidInput(
            "offline username must be 3-16 ASCII letters/digits/underscores".into(),
        ));
    }
    Ok(())
}
pub fn require_stable_identity(previous: &str, next: &str) -> Result<()> {
    validate_offline_username(previous)?;
    validate_offline_username(next)?;
    if previous != next {
        return Err(LanError::CompatibilityConflict(vec![
            "username changed; offline server may assign a different UUID and inventory. Backup and migrate with explicit operator action".into()
        ]));
    }
    Ok(())
}

/// Starts an existing, locally supplied server JAR; does NOT fetch/provision one.
/// The caller must verify server binary, Java runtime, loader and EULA separately.
pub struct ServerProcess {
    child: Child,
}
impl ServerProcess {
    pub fn start(
        java_path: &Path,
        server_jar: &Path,
        root: &Path,
        max_ram_mb: u32,
    ) -> Result<Self> {
        if !(512..=65536).contains(&max_ram_mb) {
            return Err(LanError::InvalidInput("RAM out of supported bounds".into()));
        }
        let root = fs::canonicalize(root)?;
        let jar = fs::canonicalize(server_jar)?;
        if !jar.starts_with(&root) || jar.extension().and_then(|s| s.to_str()) != Some("jar") {
            return Err(LanError::InvalidInput(
                "server JAR must be within the isolated server root".into(),
            ));
        }
        // A standalone process start must not bypass the LAN-only configuration gate.
        let properties = fs::read_to_string(root.join("server.properties"))
            .map_err(|_| LanError::NotVerified("missing trusted-LAN server.properties".into()))?;
        let setting = |key: &str| -> Option<String> {
            properties
                .lines()
                .filter_map(|line| line.split_once('='))
                .find(|(k, _)| k.trim() == key)
                .map(|(_, v)| v.trim().to_string())
        };
        let bind = setting("server-ip").and_then(|v| v.parse::<Ipv4Addr>().ok());
        if !bind.map(|ip| ip.is_private()).unwrap_or(false)
            || setting("online-mode").as_deref() != Some("false")
            || setting("enable-rcon").as_deref() != Some("false")
        {
            return Err(LanError::UnsafeBind(
                "server configuration is not restricted to trusted LAN".into(),
            ));
        }
        if !root.join("eula.txt").is_file()
            || !fs::read_to_string(root.join("eula.txt"))?
                .lines()
                .any(|l| l.trim() == "eula=true")
        {
            return Err(LanError::NotVerified(
                "server EULA must be explicitly accepted".into(),
            ));
        }
        let log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(root.join("console.log"))?;
        let err = log.try_clone()?;
        let child = Command::new(java_path)
            .current_dir(&root)
            .args([
                format!("-Xmx{max_ram_mb}M"),
                "-jar".into(),
                jar.to_string_lossy().into_owned(),
                "nogui".into(),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(err))
            .spawn()
            .map_err(|e| LanError::LaunchFailed(e.to_string()))?;
        Ok(Self { child })
    }
    pub fn pid(&self) -> u32 {
        self.child.id()
    }
    pub fn try_exit_code(&mut self) -> Result<Option<i32>> {
        Ok(self.child.try_wait()?.map(|s| s.code().unwrap_or(-1)))
    }
    /// Graceful Minecraft 'stop'; wait for the process to exit before taking backups.
    /// Timeout returns an error without killing the server or claiming the save completed.
    pub fn stop(&mut self, attempts: u32, poll: Duration) -> Result<i32> {
        if let Some(mut input) = self.child.stdin.take() {
            input.write_all(b"stop\n")?;
            input.flush()?;
        }
        for _ in 0..attempts {
            if let Some(status) = self.child.try_wait()? {
                return Ok(status.code().unwrap_or(-1));
            }
            std::thread::sleep(poll);
        }
        Err(LanError::LaunchFailed(
            "server did not exit after stop; world may still be in use".into(),
        ))
    }
}

/// Explicit cold backup only; callers must first observe a clean server exit.
/// Refuses symlinks and never overwrites an earlier backup.
pub fn backup_stopped_world(
    world: &Path,
    backups: &Path,
    backup_name: &str,
    server_is_stopped: bool,
) -> Result<PathBuf> {
    if !server_is_stopped {
        return Err(LanError::NotVerified("server is running".into()));
    }
    if backup_name.is_empty()
        || !backup_name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(LanError::InvalidInput("unsafe backup name".into()));
    }
    if !world.is_dir() || fs::symlink_metadata(world)?.file_type().is_symlink() {
        return Err(LanError::InvalidInput(
            "world must be a real directory".into(),
        ));
    }
    // Never recurse into a backup root nested in the world itself.
    // Reject symlinked backup roots so writes cannot escape the intended location.
    if backups.exists() && fs::symlink_metadata(backups)?.file_type().is_symlink() {
        return Err(LanError::InvalidInput(
            "backup root cannot be a symlink".into(),
        ));
    }
    fs::create_dir_all(backups)?;
    let canonical_world = fs::canonicalize(world)?;
    let canonical_backups = fs::canonicalize(backups)?;
    if canonical_backups.starts_with(&canonical_world) {
        return Err(LanError::InvalidInput(
            "backup directory must be outside source world".into(),
        ));
    }
    let destination = canonical_backups.join(backup_name);
    fs::create_dir(&destination)?;
    fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let ty = entry.file_type()?;
            let target = dst.join(entry.file_name());
            if ty.is_symlink() {
                return Err(LanError::InvalidInput("symlink in world backup".into()));
            }
            if ty.is_dir() {
                fs::create_dir(&target)?;
                copy_dir(&entry.path(), &target)?;
            } else if ty.is_file() {
                fs::copy(entry.path(), target)?;
            } else {
                return Err(LanError::InvalidInput(
                    "special file in world backup".into(),
                ));
            }
        }
        Ok(())
    }
    // An incomplete backup remains available for diagnosis; source world is untouched.
    copy_dir(world, &destination)?;
    Ok(destination)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pack(side: Side, hash: &str) -> PackFingerprint {
        PackFingerprint {
            minecraft: "1.21.1".into(),
            loader_kind: "fabric".into(),
            loader_version: "0.16.9".into(),
            files: vec![PackFile {
                relative_path: "mods/gameplay.jar".into(),
                sha256: hash.into(),
                side,
                gameplay: true,
            }],
        }
    }
    fn hash(c: char) -> String {
        c.to_string().repeat(64)
    }
    #[test]
    fn matching_pack_passes() {
        assert!(
            compare_packs(&pack(Side::Both, &hash('a')), &pack(Side::Both, &hash('a'))).compatible
        );
    }
    #[test]
    fn hash_mismatch_is_blocked() {
        assert!(
            !compare_packs(&pack(Side::Both, &hash('a')), &pack(Side::Both, &hash('b'))).compatible
        );
    }
    #[test]
    fn cosmetic_client_files_do_not_block() {
        let a = pack(Side::Both, &hash('a'));
        let mut b = a.clone();
        b.files.push(PackFile {
            relative_path: "shaderpacks/pretty.zip".into(),
            sha256: hash('b'),
            side: Side::Client,
            gameplay: false,
        });
        assert!(compare_packs(&a, &b).compatible);
    }
    #[test]
    fn server_only_mod_does_not_require_client_copy() {
        let a = pack(Side::Server, &hash('a'));
        let b = PackFingerprint {
            files: vec![],
            ..a.clone()
        };
        assert!(compare_packs(&a, &b).compatible);
    }
    #[test]
    fn loader_mismatch_fails() {
        let a = pack(Side::Both, &hash('a'));
        let mut b = a.clone();
        b.loader_version = "0.15.0".into();
        assert!(!compare_packs(&a, &b).compatible);
    }
    #[test]
    fn traversal_is_rejected() {
        let mut a = pack(Side::Both, &hash('a'));
        a.files[0].relative_path = "../mods/a.jar".into();
        assert!(!compare_packs(&a, &a).compatible);
    }
    #[test]
    fn public_bind_denied() {
        let s = LanServerSpec {
            server_root: "server".into(),
            bind: Ipv4Addr::new(0, 0, 0, 0),
            port: 25565,
            max_players: 4,
            whitelist: true,
            acknowledged_offline_risk: true,
            confirmed_private_network: true,
        };
        assert!(matches!(
            validate_server_spec(&s),
            Err(LanError::UnsafeBind(_))
        ));
    }
    #[test]
    fn explicit_consent_required() {
        let s = LanServerSpec {
            server_root: "server".into(),
            bind: Ipv4Addr::new(192, 168, 1, 2),
            port: 25565,
            max_players: 4,
            whitelist: true,
            acknowledged_offline_risk: false,
            confirmed_private_network: true,
        };
        assert!(validate_server_spec(&s).is_err());
    }
    #[test]
    fn rejects_backup_inside_source_world() {
        let unique = format!(
            "tl-lan-world-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        );
        let world = std::env::temp_dir().join(unique);
        fs::create_dir(&world).unwrap();
        let inside = world.join("backups");
        assert!(matches!(
            backup_stopped_world(&world, &inside, "snapshot", true),
            Err(LanError::InvalidInput(_))
        ));
        fs::remove_dir_all(&world).unwrap();
    }
    #[test]
    fn refuses_live_backup() {
        assert!(matches!(
            backup_stopped_world(Path::new("no-world"), Path::new("no-backup"), "snap", false),
            Err(LanError::NotVerified(_))
        ));
    }
    #[test]
    fn rejects_bad_hash_and_duplicate_paths() {
        let mut a = pack(Side::Both, &hash('a'));
        a.files[0].sha256 = "not-sha256".into();
        assert!(!compare_packs(&a, &a).compatible);
        a.files[0].sha256 = hash('a');
        let mut duplicate = a.files[0].clone();
        duplicate.relative_path = "MODS/GAMEPLAY.JAR".into();
        a.files.push(duplicate);
        assert!(!compare_packs(&a, &a).compatible);
    }
    #[test]
    fn rejects_invalid_direct_connect_address() {
        assert!(direct_connect_address(Ipv4Addr::new(8, 8, 8, 8), 25565).is_err());
        assert!(direct_connect_address(Ipv4Addr::new(192, 168, 1, 5), 25565).is_ok());
    }
    #[test]
    fn creates_lan_properties_only_once() {
        let unique = format!(
            "tl-lan-config-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        );
        let root = std::env::temp_dir().join(unique);
        let spec = LanServerSpec {
            server_root: root.clone(),
            bind: Ipv4Addr::new(192, 168, 2, 15),
            port: 25565,
            max_players: 3,
            whitelist: true,
            acknowledged_offline_risk: true,
            confirmed_private_network: true,
        };
        let path = initialize_server_properties(&spec).unwrap();
        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.contains("online-mode=false"));
        assert!(contents.contains("enable-rcon=false"));
        assert!(contents.contains("white-list=true"));
        assert!(initialize_server_properties(&spec).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn occupied_port_is_detected() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(matches!(
            check_port_available(Ipv4Addr::LOCALHOST, port),
            Err(LanError::PortInUse(_))
        ));
    }
    #[test]
    fn probe_success_and_connection_refused() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(probe_lan_endpoint(Ipv4Addr::LOCALHOST, port, Duration::from_millis(250)).is_ok());
        drop(listener);
        assert!(matches!(
            probe_lan_endpoint(Ipv4Addr::LOCALHOST, port, Duration::from_millis(250)),
            Err(LanError::NetworkFailure(_))
        ));
    }
    #[test]
    fn probe_rejects_public_targets() {
        assert!(matches!(
            probe_lan_endpoint(Ipv4Addr::new(8, 8, 8, 8), 25565, Duration::from_millis(100)),
            Err(LanError::InvalidInput(_))
        ));
    }
    #[test]
    fn server_start_fails_without_verified_config() {
        let root = std::env::temp_dir().join(format!("tl-lan-start-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let jar = root.join("test.jar");
        fs::write(&jar, b"fixture only").unwrap();
        let error = ServerProcess::start(Path::new("java"), &jar, &root, 1024);
        assert!(matches!(error, Err(LanError::NotVerified(_))));
        fs::remove_dir_all(&root).unwrap();
    }
    #[test]
    fn server_start_rejects_unsafe_config_even_with_eula() {
        let root = std::env::temp_dir().join(format!("tl-lan-unsafe-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let jar = root.join("test.jar");
        fs::write(&jar, b"fixture only").unwrap();
        fs::write(
            root.join("server.properties"),
            b"server-ip=0.0.0.0\nonline-mode=false\nenable-rcon=false\n",
        )
        .unwrap();
        fs::write(root.join("eula.txt"), b"eula=true\n").unwrap();
        assert!(matches!(
            ServerProcess::start(Path::new("java"), &jar, &root, 1024),
            Err(LanError::UnsafeBind(_))
        ));
        fs::remove_dir_all(&root).unwrap();
    }
    #[test]
    fn cold_backup_preserves_world_bytes() {
        let root = std::env::temp_dir().join(format!("tl-lan-cold-{}", std::process::id()));
        let world = root.join("world");
        fs::create_dir_all(world.join("playerdata")).unwrap();
        fs::write(world.join("level.dat"), b"NBT-fixture").unwrap();
        fs::write(
            world.join("playerdata").join("123.dat"),
            b"inventory-fixture",
        )
        .unwrap();
        let backups = root.join("backups");
        let dest = backup_stopped_world(&world, &backups, "snapshot_1", true).unwrap();
        assert_eq!(fs::read(dest.join("level.dat")).unwrap(), b"NBT-fixture");
        assert_eq!(
            fs::read(dest.join("playerdata").join("123.dat")).unwrap(),
            b"inventory-fixture"
        );
        assert!(backup_stopped_world(&world, &backups, "snapshot_1", true).is_err());
        assert_eq!(fs::read(world.join("level.dat")).unwrap(), b"NBT-fixture");
        fs::remove_dir_all(&root).unwrap();
    }
    #[test]
    fn rename_blocks_inventory_risk() {
        assert!(require_stable_identity("Player_1", "Player_1").is_ok());
        assert!(require_stable_identity("Player_1", "Player_2").is_err());
    }
}
