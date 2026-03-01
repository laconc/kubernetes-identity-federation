use anyhow::{Context, anyhow};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub enum MintError {
    ConfigHashMismatch,
    Other(anyhow::Error),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MintRequest {
    pub namespace: String,
    pub service_account_name: String,
    pub config_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pod_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MintResponse {
    pub aws: Option<MintedToken>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MintedToken {
    pub token: String,
    pub expires_in_seconds: u64,
}

pub async fn mint(
    client: &reqwest::Client,
    federation_url: String,
    bearer_sa_token: String,
    req: MintRequest,
) -> Result<MintResponse, MintError> {
    let url = format!("{}/v1/mint", federation_url.trim_end_matches('/'));

    let resp = client
        .post(url)
        .bearer_auth(bearer_sa_token)
        .json(&req)
        .send()
        .await
        .context("send mint request")
        .map_err(MintError::Other)?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();

    match status {
        StatusCode::OK => {
            let resp = serde_json::from_str(&body)
                .context("failed to parse mint response body")
                .map_err(MintError::Other)?;
            Ok(resp)
        }
        StatusCode::CONFLICT => Err(MintError::ConfigHashMismatch),
        _ => Err(MintError::Other(anyhow!(
            "failed to mint request, status {}: {}",
            status,
            body
        ))),
    }
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, anyhow, bail};
    use axum::{
        Router,
        extract::{Json, State},
        http::{HeaderMap, StatusCode},
        routing::post,
    };
    use serde_json::Value;
    use std::{net::SocketAddr, path::PathBuf, sync::Arc};
    use tokio::net::TcpListener;

    use crate::{federation, writer};

    #[derive(Clone, Default)]
    struct Seen {
        auth: Arc<tokio::sync::Mutex<Option<String>>>,
        body: Arc<tokio::sync::Mutex<Option<Value>>>,
    }

    async fn mock_mint(
        State(seen): State<Seen>,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> (StatusCode, Json<Value>) {
        let auth = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        *seen.auth.lock().await = auth;
        *seen.body.lock().await = Some(body.clone());

        (
            StatusCode::OK,
            Json(serde_json::json!({
                "aws": { "token": "MOCK_AWS_TOKEN", "expiresInSeconds": 3600 }
            })),
        )
    }

    async fn mint_once_and_write(
        federation_url: String,
        namespace: String,
        service_account_name: String,
        config_hash: String,
        sa_token_path: PathBuf,
        aws_token_path: PathBuf,
    ) -> Result<()> {
        let client = reqwest::Client::builder().build()?;

        let sa_token = tokio::fs::read_to_string(&sa_token_path).await?;
        let sa_token = sa_token.trim().to_string();
        if sa_token.is_empty() {
            bail!("ServiceAccount token file was empty");
        }

        let req = federation::MintRequest {
            namespace,
            service_account_name,
            config_hash,
            pod_name: None,
        };
        let resp = federation::mint(&client, federation_url, sa_token, req)
            .await
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;

        let aws = resp.aws.ok_or_else(|| anyhow!("no AWS token returned"))?;
        writer::atomic_write(&aws_token_path, &aws.token)?;
        Ok(())
    }

    #[tokio::test]
    async fn mint_once_writes_token_and_sends_expected_request() {
        let seen = Seen::default();

        // Mock federation
        let app = Router::new()
            .route("/v1/mint", post(mock_mint))
            .with_state(seen.clone());

        let listener = TcpListener::bind("0.0.0.0:0").await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        // Temp files
        let tmp = tempfile::tempdir().unwrap();
        let sa_token_path = tmp.path().join("sa.token");
        tokio::fs::write(&sa_token_path, "DUMMY_SA_TOKEN")
            .await
            .unwrap();

        let aws_token_path = tmp.path().join("aws.token");

        // Call helper
        mint_once_and_write(
            format!("http://{}", addr),
            "default".to_string(),
            "app".to_string(),
            "abc123".to_string(),
            sa_token_path.clone(),
            aws_token_path.clone(),
        )
        .await
        .unwrap();

        // Verify token written
        let written = tokio::fs::read_to_string(&aws_token_path).await.unwrap();
        assert_eq!(written, "MOCK_AWS_TOKEN");

        // Verify request contents
        let auth = seen.auth.lock().await.clone().unwrap();
        assert_eq!(auth, "Bearer DUMMY_SA_TOKEN");

        let body = seen.body.lock().await.clone().unwrap();
        assert_eq!(body["namespace"].as_str().unwrap(), "default");
        assert_eq!(body["serviceAccountName"].as_str().unwrap(), "app");
        assert_eq!(body["configHash"].as_str().unwrap(), "abc123");
    }
}
