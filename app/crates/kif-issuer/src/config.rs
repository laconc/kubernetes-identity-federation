use anyhow::{Result, anyhow};
use std::{env, fs};

#[derive(Clone, Debug, Default)]
pub struct IssuerConfig {
    /// Public issuer URL
    pub issuer_url: String,

    /// If true, use an informer to watch for changes to the jwks Secret
    pub running_in_cluster: bool,

    /// HTTP port
    pub port: u16,

    /// Secret name containing jwks.json (when running in cluster)
    pub jwks_secret_name: String,

    /// File path to jwks.json
    pub jwks_file_path: String,
}

impl IssuerConfig {
    pub fn from_env() -> Result<Self> {
        let issuer_url = env::var("ISSUER_URL").map_err(|_| anyhow!("ISSUER_URL is required"))?;

        let running_in_cluster = env::var("RUNNING_IN_CLUSTER")
            .ok()
            .and_then(|v| v.parse::<bool>().ok())
            .unwrap_or(false);

        let port = env::var("PORT")
            .ok()
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(5000);

        let jwks_secret_name = env::var("JWKS_SECRET_NAME").unwrap_or("kif-jwks".to_string());

        let jwks_file_path =
            env::var("JWKS_FILE_PATH").map_err(|_| anyhow!("JWKS_FILE_PATH is required"))?;

        Ok(Self {
            issuer_url: issuer_url.trim_end_matches('/').to_string(),
            running_in_cluster,
            port,
            jwks_secret_name,
            jwks_file_path,
        })
    }

    /// Pod namespace is only required when running in cluster.
    pub fn pod_namespace() -> Result<String> {
        let path = "/var/run/secrets/kubernetes.io/serviceaccount/namespace";
        let ns = fs::read_to_string(path)
            .map_err(|e| anyhow!("failed to read pod namespace file {}: {}", path, e))?;
        let ns = ns.trim().to_string();
        if ns.is_empty() {
            return Err(anyhow!("pod namespace file {} was empty", path));
        }
        Ok(ns)
    }
}
