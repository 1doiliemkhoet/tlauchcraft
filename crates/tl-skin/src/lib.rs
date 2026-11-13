//! Local-only skin assets. This crate does NOT install a Minecraft skin provider.
//! In-game and peer visibility require independent, explicit real-world verification.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, io::{Cursor, Write}, path::{Path, PathBuf}};
use uuid::Uuid;

const MAX_PNG_BYTES: usize = 2 * 1024 * 1024;
const MAX_PIXELS: usize = 64 * 64 * 4;
const MAX_HISTORY: usize = 64;

#[derive(Debug)]
pub enum SkinError {
    InvalidInput(String),
    Io(std::io::Error),
    Json(serde_json::Error),
    Png(String),
    NotFound,
}
impl std::fmt::Display for SkinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(m) | Self::Png(m) => write!(f, "{m}"),
            Self::Io(e) => write!(f, "{e}"),
            Self::Json(e) => write!(f, "{e}"),
            Self::NotFound => write!(f, "skin not found"),
        }
    }
}
impl std::error::Error for SkinError {}
impl From<std::io::Error> for SkinError { fn from(e: std::io::Error) -> Self { Self::Io(e) } }
impl From<serde_json::Error> for SkinError { fn from(e: serde_json::Error) -> Self { Self::Json(e) } }
pub type Result<T> = std::result::Result<T, SkinError>;

// Share the canonical Foundation model and selection contract (no duplicate UUID types).
pub use tl_core::{SkinModel as Model, SkinSelection};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Visibility { PreviewOnly, ProviderReadyUnverified, LocalVerified, PeerVerified }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkinRecord {
    pub id: Uuid,
    pub name: String,
    pub sha256: String,
    pub model: Model,
    pub legacy: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Catalog {
    skins: BTreeMap<Uuid, SkinRecord>,
    profiles: BTreeMap<Uuid, SkinSelection>,
    instance_overrides: BTreeMap<Uuid, SkinSelection>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkinImage { pub height: u32, pub rgba: Vec<u8> }

fn digest(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }
fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() || name.len() > 80 || name.trim() != name
        || name.chars().any(|c| c.is_control() || c == '/' || c == '\\')
    { return Err(SkinError::InvalidInput("unsafe cosmetic name".into())); }
    Ok(())
}
/// Decode a real PNG, always normalizing to RGBA8. Reject invalid CRC and unsafe output sizes.
pub fn decode_skin(bytes: &[u8]) -> Result<SkinImage> {
    if bytes.len() > MAX_PNG_BYTES || bytes.len() < 33 || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(SkinError::InvalidInput("invalid/oversized PNG".into()));
    }
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| SkinError::Png(e.to_string()))?;
    let (w, h) = (reader.info().width, reader.info().height);
    if w != 64 || (h != 64 && h != 32) {
        return Err(SkinError::InvalidInput("skin must be 64x64 or 64x32".into()));
    }
    if reader.output_buffer_size() > MAX_PIXELS {
        return Err(SkinError::InvalidInput("expanded PNG exceeds limit".into()));
    }
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| SkinError::Png(e.to_string()))?;
    let pixels = &buf[..info.buffer_size()];
    let mut rgba = Vec::with_capacity((w*h*4) as usize);
    match info.color_type {
        png::ColorType::Rgba => rgba.extend_from_slice(pixels),
        png::ColorType::Rgb => {
            for p in pixels.chunks_exact(3) { rgba.extend_from_slice(&[p[0],p[1],p[2],255]); }
        },
        png::ColorType::GrayscaleAlpha => {
            for p in pixels.chunks_exact(2) { rgba.extend_from_slice(&[p[0],p[0],p[0],p[1]]); }
        },
        png::ColorType::Grayscale => {
            for &p in pixels { rgba.extend_from_slice(&[p,p,p,255]); }
        },
        _ => return Err(SkinError::Png("unsupported decoded color format".into())),
    }
    if rgba.len() != (w*h*4) as usize { return Err(SkinError::Png("invalid decoded pixel count".into())); }
    // Force opaque base layer; overlay layer can retain alpha.
    // Legacy skins have no separate outer-body layers.
    for y in 0..h {
        for x in 0..w {
            let is_base = if h == 32 { !(x >= 32 && y < 16) } else {
                (y < 16 && x < 32) || (y >= 16 && y < 32) || (y >= 48 && x < 16)
            };
            if is_base { rgba[((y*w+x)*4+3) as usize] = 255; }
        }
    }
    Ok(SkinImage { height: h, rgba })
}
pub fn encode_png(image: &SkinImage) -> Result<Vec<u8>> {
    if image.height != 32 && image.height != 64 || image.rgba.len() != (64*image.height*4) as usize {
        return Err(SkinError::InvalidInput("invalid skin pixel buffer".into()));
    }
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, 64, image.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| SkinError::Png(e.to_string()))?;
        writer.write_image_data(&image.rgba).map_err(|e| SkinError::Png(e.to_string()))?;
    }
    Ok(out)
}
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or_else(|| SkinError::InvalidInput("missing parent".into()))?;
    fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(".{}.tmp", Uuid::new_v4()));
    {
        let mut f = fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(SkinError::Io(e));
    }
    Ok(())
}
pub struct SkinLibrary { root: PathBuf, catalog: Catalog }
impl SkinLibrary {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(root.join("images"))?;
        let catalog = if root.join("catalog.json").exists() {
            serde_json::from_slice(&fs::read(root.join("catalog.json"))?)?
        } else { Catalog::default() };
        Ok(Self { root, catalog })
    }
    fn save(&self) -> Result<()> {
        atomic_write(&self.root.join("catalog.json"), &serde_json::to_vec_pretty(&self.catalog)?)
    }
    fn image_path(&self, id: Uuid) -> PathBuf { self.root.join("images").join(format!("{id}.png")) }
    pub fn list(&self) -> Vec<SkinRecord> { self.catalog.skins.values().cloned().collect() }
    pub fn import_png(&mut self, name: &str, bytes: &[u8], model: Model) -> Result<SkinRecord> {
        validate_name(name)?;
        let img = decode_skin(bytes)?;
        let encoded = encode_png(&img)?;
        let id = Uuid::new_v4();
        let record = SkinRecord { id, name: name.into(), sha256: digest(&encoded), model, legacy: img.height == 32 };
        atomic_write(&self.image_path(id), &encoded)?;
        self.catalog.skins.insert(id, record.clone());
        self.save()?;
        Ok(record)
    }
    pub fn export_png(&self, id: Uuid) -> Result<Vec<u8>> {
        let record = self.catalog.skins.get(&id).ok_or(SkinError::NotFound)?;
        let bytes = fs::read(self.image_path(id))?;
        if digest(&bytes) != record.sha256 { return Err(SkinError::InvalidInput("skin integrity mismatch".into())); }
        Ok(bytes)
    }
    pub fn rename(&mut self, id: Uuid, name: &str) -> Result<()> {
        validate_name(name)?;
        self.catalog.skins.get_mut(&id).ok_or(SkinError::NotFound)?.name = name.into();
        self.save()
    }
    pub fn duplicate(&mut self, id: Uuid, name: &str) -> Result<SkinRecord> {
        let model = self.catalog.skins.get(&id).ok_or(SkinError::NotFound)?.model;
        let bytes = self.export_png(id)?;
        self.import_png(name, &bytes, model)
    }
    pub fn delete_confirmed(&mut self, id: Uuid, confirmed: bool) -> Result<()> {
        if !confirmed { return Err(SkinError::InvalidInput("explicit confirmation required".into())); }
        if !self.catalog.skins.contains_key(&id) { return Err(SkinError::NotFound); }
        // Remove references before deleting image. A failed delete keeps catalog recoverable.
        self.catalog.profiles.retain(|_,s| s.skin_id != id);
        self.catalog.instance_overrides.retain(|_,s| s.skin_id != id);
        self.catalog.skins.remove(&id);
        self.save()?;
        fs::remove_file(self.image_path(id))?;
        Ok(())
    }
    pub fn assign_profile(&mut self, selection: SkinSelection) -> Result<()> {
        if !self.catalog.skins.contains_key(&selection.skin_id) { return Err(SkinError::NotFound); }
        self.catalog.profiles.insert(selection.profile_id, selection);
        self.save()
    }
    pub fn assign_instance_override(&mut self, instance: Uuid, selection: SkinSelection) -> Result<()> {
        if !self.catalog.skins.contains_key(&selection.skin_id) { return Err(SkinError::NotFound); }
        self.catalog.instance_overrides.insert(instance, selection);
        self.save()
    }
    pub fn selection(&self, profile: Uuid, instance: Option<Uuid>) -> Option<&SkinSelection> {
        instance.and_then(|i| self.catalog.instance_overrides.get(&i))
            .filter(|s| s.profile_id == profile)
            .or_else(|| self.catalog.profiles.get(&profile))
    }
}
/// Pure pixel editing model for UI consumers. No renderer is bundled in this crate.
pub struct SkinEditor {
    image: SkinImage,
    undo: Vec<Vec<u8>>,
    redo: Vec<Vec<u8>>,
}
impl SkinEditor {
    pub fn new(image: SkinImage) -> Self { Self { image, undo: Vec::new(), redo: Vec::new() } }
    pub fn image(&self) -> &SkinImage { &self.image }
    fn checkpoint(&mut self) {
        if self.undo.len() == MAX_HISTORY { self.undo.remove(0); }
        self.undo.push(self.image.rgba.clone());
        self.redo.clear();
    }
    pub fn paint(&mut self, x: u32, y: u32, rgba: [u8;4]) -> Result<()> {
        if x >= 64 || y >= self.image.height { return Err(SkinError::InvalidInput("pixel out of bounds".into())); }
        self.checkpoint();
        let offset = ((y*64+x)*4) as usize;
        self.image.rgba[offset..offset+4].copy_from_slice(&rgba);
        Ok(())
    }
    pub fn sample(&self, x: u32, y: u32) -> Result<[u8;4]> {
        if x >= 64 || y >= self.image.height { return Err(SkinError::InvalidInput("pixel out of bounds".into())); }
        let n = ((y*64+x)*4) as usize;
        Ok(self.image.rgba[n..n+4].try_into().unwrap())
    }
    pub fn erase(&mut self, x: u32, y: u32) -> Result<()> { self.paint(x,y,[0,0,0,0]) }
    pub fn undo(&mut self) -> bool {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.image.rgba, prev)); true
        } else { false }
    }
    pub fn redo(&mut self) -> bool {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.image.rgba, next)); true
        } else { false }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub provider_id: String,
    pub minecraft_version: String,
    pub loader: String,
    pub local_skin: bool,
    pub peer_skin: bool,
    pub custom_cape: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppearanceStatus {
    pub preview: bool,
    pub local: Visibility,
    pub peer: Visibility,
    pub evidence_local: Option<String>,
    pub evidence_peer: Option<String>,
}
impl Default for AppearanceStatus {
    fn default() -> Self {
        Self { preview: true, local: Visibility::PreviewOnly, peer: Visibility::PreviewOnly, evidence_local: None, evidence_peer: None }
    }
}
impl AppearanceStatus {
    pub fn provider_prepared(&mut self, provider: &ProviderCapabilities) {
        if provider.local_skin { self.local = Visibility::ProviderReadyUnverified; }
        if provider.peer_skin { self.peer = Visibility::ProviderReadyUnverified; }
    }
    pub fn verify_local(&mut self, evidence: String) -> Result<()> {
        if self.local != Visibility::ProviderReadyUnverified || evidence.trim().is_empty() {
            return Err(SkinError::InvalidInput("real local evidence required".into()));
        }
        self.evidence_local = Some(evidence);
        self.local = Visibility::LocalVerified;
        Ok(())
    }
    pub fn verify_peer(&mut self, local_evidence: String, peer_evidence: String) -> Result<()> {
        if self.peer != Visibility::ProviderReadyUnverified || self.local != Visibility::LocalVerified
            || local_evidence.trim().is_empty() || peer_evidence.trim().is_empty() {
            return Err(SkinError::InvalidInput("independent evidence from both clients required".into()));
        }
        self.evidence_peer = Some(format!("local={local_evidence}; peer={peer_evidence}"));
        self.peer = Visibility::PeerVerified;
        Ok(())
    }
}
/// Only transfer cosmetic bytes after user opt-in; never imply secure identity on offline LAN.
pub fn lan_share_payload(consent: bool, image: &SkinImage) -> Result<Option<Vec<u8>>> {
    if !consent { return Ok(None); }
    let encoded = encode_png(image)?;
    if encoded.len() > MAX_PNG_BYTES { return Err(SkinError::InvalidInput("skin exceeds LAN limit".into())); }
    Ok(Some(encoded))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UvRegion { pub name: &'static str, pub x: u32, pub y: u32, pub width: u32, pub height: u32 }
/// Atlas rectangles for the main 64x64 Minecraft skin regions. UI can use these
/// for layer toggles and texture inspection; a 3D mesh renderer is still required.
pub fn atlas_regions() -> Vec<UvRegion> {
    vec![
        UvRegion { name:"head", x:0, y:0, width:32, height:16 },
        UvRegion { name:"hat", x:32, y:0, width:32, height:16 },
        UvRegion { name:"right_leg", x:0, y:16, width:16, height:16 },
        UvRegion { name:"body", x:16, y:16, width:24, height:16 },
        UvRegion { name:"right_arm", x:40, y:16, width:16, height:16 },
        UvRegion { name:"left_leg", x:16, y:48, width:16, height:16 },
        UvRegion { name:"left_arm", x:32, y:48, width:16, height:16 },
        UvRegion { name:"body_jacket", x:16, y:32, width:24, height:16 },
        UvRegion { name:"right_sleeve", x:40, y:32, width:16, height:16 },
        UvRegion { name:"left_sleeve", x:48, y:48, width:16, height:16 },
        UvRegion { name:"right_pants", x:0, y:32, width:16, height:16 },
        UvRegion { name:"left_pants", x:0, y:48, width:16, height:16 },
    ]
}
impl SkinEditor {
    /// Fill a four-connected contiguous region, bounded by the fixed 64x64 atlas.
    pub fn flood_fill(&mut self, x: u32, y: u32, color: [u8;4]) -> Result<usize> {
        let original = self.sample(x,y)?;
        if original == color { return Ok(0); }
        self.checkpoint();
        let w = 64usize;
        let h = self.image.height as usize;
        let mut stack = vec![(x as usize,y as usize)];
        let mut visited = vec![false; w*h];
        let mut changed = 0usize;
        while let Some((px,py)) = stack.pop() {
            let idx = py*w+px;
            if visited[idx] { continue; }
            visited[idx] = true;
            if self.image.rgba[idx*4..idx*4+4] != original { continue; }
            self.image.rgba[idx*4..idx*4+4].copy_from_slice(&color);
            changed += 1;
            if px > 0 { stack.push((px-1,py)); }
            if px+1 < w { stack.push((px+1,py)); }
            if py > 0 { stack.push((px,py-1)); }
            if py+1 < h { stack.push((px,py+1)); }
        }
        Ok(changed)
    }
    /// Reflect horizontally within a selected atlas rectangle (does not swap limbs).
    pub fn mirror_region(&mut self, x:u32, y:u32, width:u32, height:u32) -> Result<()> {
        if width == 0 || height == 0 || x.checked_add(width).map_or(true, |n| n>64)
            || y.checked_add(height).map_or(true, |n| n>self.image.height) {
            return Err(SkinError::InvalidInput("invalid mirror rectangle".into()));
        }
        self.checkpoint();
        for row in y..y+height {
            for col in 0..width/2 {
                let a = ((row*64 + x+col)*4) as usize;
                let b = ((row*64 + x+width-1-col)*4) as usize;
                for k in 0..4 { self.image.rgba.swap(a+k,b+k); }
            }
        }
        Ok(())
    }
    /// Select unique colors in deterministic scan order, suitable for UI palette controls.
    pub fn palette(&self, max_colors: usize) -> Vec<[u8;4]> {
        if max_colors == 0 { return Vec::new(); }
        let mut out = Vec::new();
        for rgba in self.image.rgba.chunks_exact(4) {
            let c = [rgba[0],rgba[1],rgba[2],rgba[3]];
            if !out.contains(&c) { out.push(c); }
            if out.len() >= max_colors { break; }
        }
        out
    }
}
