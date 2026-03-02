use anyhow::Result;
use std::{env, path::PathBuf};

#[derive(Clone, Debug)]
pub enum AdmissionFailureMode {
    Fail,
    Ignore,
}

#[derive(Clone, Debug)]
pub struct WebhookConfig {
    pub https_port: u16,
    pub health_port: u16,

    pub tls_cert_path: PathBuf,
    pub tls_key_path: PathBuf,

    pub admission_failure_mode: AdmissionFailureMode,

    pub agent_image: String,
    pub agent_image_pull_policy: String,
    pub agent_port: u16,
    pub federation_url: String,

    pub reconcile_workers: usize,
}

impl WebhookConfig {
    pub fn from_env() -> Result<Self> {
        let https_port = env::var("HTTPS_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(9443);
        let health_port = env::var("HEALTH_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5003);

        let tls_cert_path = env::var("TLS_CERT_PATH")
            .map(PathBuf::from)
            .unwrap_or(PathBuf::from("/tls/tls.crt"));

        let tls_key_path = env::var("TLS_KEY_PATH")
            .map(PathBuf::from)
            .unwrap_or(PathBuf::from("/tls/tls.key"));

        let admission_failure_mode = match env::var("ADMISSION_FAILURE_MODE")
            .unwrap_or("fail".to_string())
            .to_lowercase()
            .as_str()
        {
            "fail" => AdmissionFailureMode::Fail,
            "ignore" => AdmissionFailureMode::Ignore,
            v => anyhow::bail!("invalid ADMISSION_FAILURE_MODE: {v} (expected Fail|Ignore)"),
        };

        let agent_image = env::var("AGENT_IMAGE").unwrap_or_else(|_| {
            let tag = env::var("AGENT_IMAGE_TAG").unwrap_or("latest".to_string());
            format!("kif-agent:{tag}")
        });
        let agent_image_pull_policy =
            env::var("AGENT_IMAGE_PULL_POLICY").unwrap_or_else(|_| "IfNotPresent".to_string());
        let agent_port = env::var("AGENT_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5004);
        let federation_host =
            env::var("FEDERATION_SERVICE_HOST").unwrap_or("http://kif-federation".to_string());
        let federation_port = env::var("FEDERATION_SERVICE_PORT").unwrap_or("5001".to_string());
        let federation_url = format!("{}:{}", federation_host, federation_port);

        let reconcile_workers = env::var("RECONCILE_WORKERS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4);

        Ok(Self {
            https_port,
            health_port,
            tls_cert_path,
            tls_key_path,
            admission_failure_mode,
            agent_image,
            agent_image_pull_policy,
            agent_port,
            federation_url,
            reconcile_workers,
        })
    }
}
