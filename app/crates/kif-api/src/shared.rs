use std::fs;

use anyhow::{Context, Result, anyhow};

pub fn pod_namespace() -> Result<String> {
    let path = "/var/run/secrets/kubernetes.io/serviceaccount/namespace";
    let ns =
        fs::read_to_string(path).context(anyhow!("failed to read pod namespace file {}", path))?;
    let ns = ns.trim().to_string();
    if ns.is_empty() {
        return Err(anyhow!("pod namespace file {} was empty", path));
    }
    Ok(ns)
}
