use anyhow::Result;
use futures::StreamExt;
use k8s_openapi::api::core::v1::Secret;
use kube::{Api, Client};
use kube_runtime::WatchStreamExt;
use kube_runtime::watcher::{Config as WatcherConfig, watcher};
use tracing::{error, info};

use crate::jwks::{JwksStore, parse_jwks};

const JWKS_SECRET_KEY: &str = "jwks.json";

/// Watch the specified Kubernetes Secret for changes and update the JWKS store on change.
pub async fn run_secret_watcher(
    namespace: String,
    secret_name: String,
    store: JwksStore,
) -> Result<()> {
    let client = Client::try_default().await?;
    let secrets: Api<Secret> = Api::namespaced(client, &namespace);
    let wc = WatcherConfig::default().fields(&format!("metadata.name={}", secret_name));
    let stream = watcher(secrets, wc);

    stream
        .applied_objects()
        .for_each(|ev| async {
            let secret = match ev {
                Ok(secret) => secret,
                Err(e) => {
                    error!(error=?e, "error watching secret");
                    return;
                }
            };

            let data = match secret.data {
                Some(d) => d,
                None => {
                    error!("secret {} has no data", secret_name);
                    return;
                }
            };

            let bytes = match data.get(JWKS_SECRET_KEY) {
                Some(v) => v.0.clone(),
                None => {
                    error!("secret missing key {}", JWKS_SECRET_KEY);
                    return;
                }
            };

            match parse_jwks(bytes) {
                Ok(jwks) => {
                    store.set(jwks).await;
                    info!("JWKS updated from secret");
                }
                Err(e) => {
                    error!(error=?e, "invalid jwks.json in secret; keeping last good JWKS");
                }
            }
        })
        .await;

    Ok(())
}
