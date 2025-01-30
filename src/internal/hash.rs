use std::{fs, io::Read, path::Path};

use anyhow::Context;

const BUFFER_SIZE: usize = 524288;

pub fn hash_crc32<C: FnMut(usize)>(path: &Path, mut callback: C) -> Result<u32, anyhow::Error> {
    let mut file = fs::File::open(path).with_context(|| format!("Opening file for hashing: {}", path.display()))?;

    let mut hasher = crc32fast::Hasher::new();

    let mut buf = [0u8; BUFFER_SIZE];

    while let Ok(bytes) = file.read(&mut buf) {
        if bytes == 0 {
            break;
        }

        hasher.update(&buf[..bytes]);

        callback(bytes);
    }

    let hash: u32 = hasher.finalize();

    Ok(hash)
}
