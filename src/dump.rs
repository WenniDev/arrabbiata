//! Writing captured payloads to disk.
//!
//! This is what worked the protocol out, and it stays in for two reasons: `dump.all` turns
//! discovery back on when a future game version changes something, and `dump.on_refusal`
//! means a payload the score handler could not account for is preserved rather than lost.

use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Result;

use crate::CONFIGURATION;

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub fn written() -> u64 {
    SEQUENCE.load(Ordering::Relaxed)
}

/// Writes a payload out, returning the filenames produced.
pub fn write(label: &str, json: Option<&[u8]>, kbin: Option<&[u8]>) -> Result<Vec<String>> {
    let directory = &CONFIGURATION.dump.directory;
    std::fs::create_dir_all(directory)
        .map_err(|err| anyhow::anyhow!("Could not create '{}': {err}", directory.display()))?;

    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let stem = format!(
        "{sequence:05}_{}_{}",
        chrono::Local::now().format("%H%M%S%.3f"),
        sanitize(label)
    );

    let mut files = Vec::new();
    for (contents, extension) in [(json, "json"), (kbin, "kbin")] {
        if let Some(contents) = contents {
            let name = format!("{stem}.{extension}");
            std::fs::write(directory.join(&name), contents)?;
            files.push(name);
        }
    }

    Ok(files)
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::sanitize;

    #[test]
    fn filenames_keep_only_safe_characters() {
        assert_eq!(sanitize("write-eacnet-usersave"), "write-eacnet-usersave");
        assert_eq!(sanitize("a/b\\c:d"), "a_b_c_d");
    }
}
