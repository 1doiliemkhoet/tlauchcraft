//! Download-only audit of official candidates; never starts Minecraft or writes an instance.
use sha2::{Digest, Sha256};
use std::{io::Cursor, sync::atomic::AtomicBool};
use tl_pack::skin_compat::{candidate, download};

fn main() -> Result<(), String> {
    for minecraft in ["1.16.5", "1.20.1"] {
        let release = candidate(minecraft, "forge").ok_or("missing candidate")?;
        let bytes = download(minecraft, "forge", &AtomicBool::new(false))?;
        let mut jar = zip::ZipArchive::new(Cursor::new(&bytes)).map_err(|e| e.to_string())?;
        for name in ["META-INF/MANIFEST.MF", "META-INF/mods.toml", "mcmod.info"] {
            if let Ok(entry) = jar.by_name(name) {
                use std::io::Read;
                let mut metadata = String::new();
                entry
                    .take(32 * 1024)
                    .read_to_string(&mut metadata)
                    .map_err(|e| e.to_string())?;
                println!("{name}: {metadata}");
            }
        }
        println!(
            "{} {} {} size={} sha256={:x}",
            minecraft,
            release.version_id,
            release.filename,
            bytes.len(),
            Sha256::digest(&bytes)
        );
    }
    Ok(())
}
