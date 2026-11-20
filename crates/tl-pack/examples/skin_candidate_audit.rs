//! Download-only audit of official candidates; never starts Minecraft or writes an instance.
use sha2::{Digest, Sha256};
use std::sync::atomic::AtomicBool;
use tl_pack::skin_compat::{candidate, download};

fn main() -> Result<(), String> {
    for minecraft in ["1.16.5", "1.20.1"] {
        let release = candidate(minecraft, "forge").ok_or("missing candidate")?;
        let bytes = download(minecraft, "forge", &AtomicBool::new(false))?;
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
