use anyhow::{Context, Result};
use std::{fs, io::Write, path::Path};

/// Atomically writes the given contents to the specified path. It does this by writing
/// to a temporary file and then renaming it. We don't want the main containers reading
/// a partially-written file.
pub fn atomic_write(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context(format!(
            "failed to create parent dirs for {}",
            path.display()
        ))?;
    }

    let tmp = path.with_extension("tmp");
    let mut f =
        fs::File::create(&tmp).context(format!("failed to create tmp file {}", tmp.display()))?;
    f.write_all(contents.as_bytes()).context(format!(
        "failed to write contents to tmp file {}",
        tmp.display()
    ))?;
    let _ = f.sync_all();

    fs::rename(&tmp, path).context(format!(
        "failed to rename tmp file {} to {}",
        tmp.display(),
        path.display()
    ))?;
    Ok(())
}
