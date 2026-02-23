use std::{collections::BTreeMap, time::SystemTime};

use anyhow::{Context, Result, anyhow};
use k8s_openapi::{
    ByteString,
    api::{
        authentication::v1::{TokenReview, TokenReviewSpec},
        core::v1::Secret,
    },
};
use kube::{Api, Client, api::PostParams};
use rsa::{RsaPrivateKey, pkcs8::EncodePrivateKey};

use crate::jwt::SigningMaterial;

const SIGNING_SECRET_KEY_PEM: &str = "private_key_pkcs8.pem";
const SIGNING_SECRET_KEY_KID: &str = "kid";
const JWKS_SECRET_KEY: &str = "jwks.json";

use kif_api::ResolvedCloudRoleBinding;

#[derive(Clone, Debug)]
pub struct TokenSubject {
    pub username: String,
}

pub async fn token_review(client: &Client, token: &str) -> Result<TokenSubject> {
    let api: Api<TokenReview> = Api::all(client.clone());
    let tr = TokenReview {
        spec: TokenReviewSpec {
            token: Some(token.to_string()),
            audiences: None,
        },
        ..Default::default()
    };

    let created = api.create(&Default::default(), &tr).await?;
    let status = created.status.context("TokenReview missing status")?;
    if status.authenticated != Some(true) {
        return Err(anyhow!("token not authenticated"));
    }
    let user = status
        .user
        .ok_or_else(|| anyhow!("TokenReview missing user"))?;
    if user.username.is_none() {
        return Err(anyhow!("TokenReview user missing username"));
    }

    Ok(TokenSubject {
        username: user.username.unwrap(),
    })
}

pub fn parse_service_account_username(u: &str) -> Option<(String, String)> {
    // system:serviceaccount:<ns>:<sa>
    let prefix = "system:serviceaccount:";
    if !u.starts_with(prefix) {
        return None;
    }
    let rest = &u[prefix.len()..];
    let mut parts = rest.split(':');
    let ns = parts.next()?.to_string();
    let sa = parts.next()?.to_string();
    if ns.is_empty() || sa.is_empty() {
        return None;
    }
    Some((ns, sa))
}

pub async fn get_resolved_binding(
    client: &Client,
    namespace: &str,
    sa_name: &str,
) -> Result<ResolvedCloudRoleBinding> {
    let api: Api<ResolvedCloudRoleBinding> = Api::namespaced(client.clone(), namespace);
    Ok(api.get(sa_name).await?)
}

/// Ensure signing Secret + JWKS Secret exist.
pub async fn ensure_signing_and_jwks(
    client: &Client,
    namespace: &str,
    signing_secret_name: &str,
    jwks_secret_name: &str,
    rsa_bits: usize,
) -> Result<SigningMaterial> {
    let secrets: Api<Secret> = Api::namespaced(client.clone(), namespace);

    // Check if signing secret exists and is valid
    if let Some(existing) = secrets.get_opt(signing_secret_name).await? {
        let data = existing
            .data
            .context("existing signing secret missing data")?;
        let pem = data
            .get(SIGNING_SECRET_KEY_PEM)
            .context(format!("signing secret missing {SIGNING_SECRET_KEY_PEM}"))?
            .0
            .clone();
        let kid = data
            .get(SIGNING_SECRET_KEY_KID)
            .context(format!("signing secret missing {SIGNING_SECRET_KEY_KID}"))?
            .0
            .clone();

        let pem = String::from_utf8(pem)?;
        let kid = String::from_utf8(kid)?;

        let material = SigningMaterial {
            kid,
            private_key_pem: pem,
        };

        // Re-publish the JWKS secret in case it's missing or outdated
        let jwks_json = crate::jwt::jwks_json_from_private_pem(&material)?;
        upsert_jwks_secret(&secrets, jwks_secret_name, jwks_json).await?;

        return Ok(material);
    }

    //  Create signing secret
    let kid = format!(
        "kid-{}",
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs()
    );
    let mut rng = rand::rng();
    let key = RsaPrivateKey::new(&mut rng, rsa_bits)?;
    let pem = key.to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)?;
    let pem_str = pem.to_string();

    let material = SigningMaterial {
        kid: kid.clone(),
        private_key_pem: pem_str.clone(),
    };

    let mut data: BTreeMap<String, ByteString> = BTreeMap::new();
    data.insert(
        SIGNING_SECRET_KEY_PEM.to_string(),
        ByteString(pem_str.into_bytes()),
    );
    data.insert(
        SIGNING_SECRET_KEY_KID.to_string(),
        ByteString(kid.into_bytes()),
    );

    let signing = Secret {
        metadata: kube::api::ObjectMeta {
            name: Some(signing_secret_name.to_string()),
            ..Default::default()
        },
        type_: Some("Opaque".to_string()),
        data: Some(data),
        ..Default::default()
    };

    secrets.create(&PostParams::default(), &signing).await?;

    //  Publish JWKS secret
    let jwks_json = crate::jwt::jwks_json_from_private_pem(&material)?;
    upsert_jwks_secret(&secrets, jwks_secret_name, jwks_json).await?;

    Ok(material)
}

async fn upsert_jwks_secret(secrets: &Api<Secret>, name: &str, jwks_json: String) -> Result<()> {
    let mut data: BTreeMap<String, ByteString> = BTreeMap::new();
    data.insert(
        JWKS_SECRET_KEY.to_string(),
        ByteString(jwks_json.into_bytes()),
    );

    let desired = Secret {
        metadata: kube::api::ObjectMeta {
            name: Some(name.to_string()),
            ..Default::default()
        },
        type_: Some("Opaque".to_string()),
        data: Some(data),
        ..Default::default()
    };

    if secrets.get_opt(name).await?.is_some() {
        secrets
            .replace(name, &PostParams::default(), &desired)
            .await?;
    } else {
        secrets.create(&PostParams::default(), &desired).await?;
    }
    Ok(())
}
