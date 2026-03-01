use anyhow::{Result, anyhow};
use base64::Engine;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use openidconnect::core::{CoreJsonWebKey, CoreJsonWebKeySet};
use rsa::traits::PublicKeyParts;
use rsa::{RsaPrivateKey, pkcs8::DecodePrivateKey};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// In-memory signing material loaded from the signing Secret.
#[derive(Clone, Debug)]
pub struct SigningMaterial {
    pub kid: String,
    pub private_key_pem: String,
}

#[derive(Serialize)]
pub struct AwsClaims {
    iss: String,
    sub: String,
    aud: String,
    iat: u64,
    nbf: u64,
    exp: u64,
    jti: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    kif: Option<KifClaims>,
}

#[derive(Serialize)]
struct KifClaims {
    #[serde(skip_serializing_if = "Option::is_none")]
    k8s: Option<KifK8s>,

    #[serde(skip_serializing_if = "Option::is_none")]
    attributes: Option<BTreeMap<String, String>>,
}

#[derive(Serialize)]
struct KifK8s {
    namespace: String,
    service_account: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pod: Option<String>,
}

impl AwsClaims {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        issuer: &str,
        subject: &str,  // system:serviceaccount:<ns>:<sa>
        audience: &str, // sts.amazonaws.com
        ttl_seconds: u64,
        include_provenance: bool,
        namespace: &str,
        service_account: &str,
        pod_name: Option<String>,
        extra_attributes: Option<BTreeMap<String, String>>,
    ) -> Self {
        let now = now_unix_seconds();
        let exp = now + ttl_seconds;

        let kif = if include_provenance || extra_attributes.is_some() {
            Some(KifClaims {
                k8s: include_provenance.then(|| KifK8s {
                    namespace: namespace.to_string(),
                    service_account: service_account.to_string(),
                    pod: pod_name,
                }),
                attributes: extra_attributes,
            })
        } else {
            None
        };

        AwsClaims {
            iss: issuer.to_string(),
            sub: subject.to_string(),
            aud: audience.to_string(),
            iat: now,
            nbf: now,
            exp,
            jti: format!("kif-{}", now_nanos()),
            kif,
        }
    }
}

pub fn sign_rs256<T: Serialize>(signing: &SigningMaterial, claims: &T) -> Result<String> {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(signing.kid.clone());

    let key = EncodingKey::from_rsa_pem(signing.private_key_pem.as_bytes())
        .map_err(|e| anyhow!("invalid rsa pem: {e}"))?;

    Ok(encode(&header, claims, &key)?)
}

/// Build a JWKS JSON document from the private key.
/// This output is written to the JWKS Secret and served by the issuer.
pub fn jwks_json_from_private_pem(signing: &SigningMaterial) -> Result<String> {
    let rsa = RsaPrivateKey::from_pkcs8_pem(&signing.private_key_pem)
        .map_err(|e| anyhow!("parse pkcs8 pem: {e}"))?;
    let pubkey = rsa.to_public_key();

    let n = base64_url(pubkey.n_bytes());
    let e = base64_url(pubkey.e_bytes());

    // Build a CoreJwk via serde to avoid version-specific constructors.
    let jwk_json = serde_json::json!({
        "kty": "RSA",
        "kid": signing.kid,
        "use": "sig",
        "alg": "RS256",
        "n": n,
        "e": e
    });

    let jwk: CoreJsonWebKey = serde_json::from_value(jwk_json)?;
    let jwks = CoreJsonWebKeySet::new(vec![jwk]);

    Ok(serde_json::to_string(&jwks)?)
}

fn now_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs()
}

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_nanos()
}

fn base64_url(bytes: Box<[u8]>) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aws_claims_with_provenance_includes_kif_k8s() {
        let before = now_unix_seconds();
        let claims = AwsClaims::new(
            "https://issuer.example.com",
            "system:serviceaccount:default:my-sa",
            "sts.amazonaws.com",
            3600,
            true,
            "default",
            "my-sa",
            Some("my-pod".to_string()),
            None,
        );
        let after = now_unix_seconds();

        let v = serde_json::to_value(&claims).unwrap();
        assert_eq!(v["iss"], "https://issuer.example.com");
        assert_eq!(v["sub"], "system:serviceaccount:default:my-sa");
        assert_eq!(v["aud"], "sts.amazonaws.com");

        let iat = v["iat"].as_u64().unwrap();
        assert!(iat >= before && iat <= after);

        let exp = v["exp"].as_u64().unwrap();
        assert!(exp >= before + 3600 && exp <= after + 3600);

        // include_provenance=true → kif.k8s present
        assert!(v["kif"]["k8s"].is_object());
        assert_eq!(v["kif"]["k8s"]["namespace"], "default");
        assert_eq!(v["kif"]["k8s"]["service_account"], "my-sa");
        assert_eq!(v["kif"]["k8s"]["pod"], "my-pod");
    }

    #[test]
    fn aws_claims_without_provenance_omits_kif() {
        let claims = AwsClaims::new(
            "https://issuer.example.com",
            "system:serviceaccount:default:my-sa",
            "sts.amazonaws.com",
            3600,
            false,
            "default",
            "my-sa",
            None,
            None,
        );
        let v = serde_json::to_value(&claims).unwrap();
        // include_provenance=false, no extra_attributes → kif field absent
        assert!(v.get("kif").is_none());
    }

    #[test]
    fn aws_claims_without_provenance_with_extra_attrs_includes_kif_but_no_k8s() {
        let mut attrs = BTreeMap::new();
        attrs.insert("team".to_string(), "platform".to_string());

        let claims = AwsClaims::new(
            "https://issuer.example.com",
            "system:serviceaccount:default:my-sa",
            "sts.amazonaws.com",
            3600,
            false,
            "default",
            "my-sa",
            None,
            Some(attrs),
        );
        let v = serde_json::to_value(&claims).unwrap();
        // kif present (due to extra_attrs), but k8s absent (include_provenance=false)
        assert!(v["kif"].is_object());
        assert!(v["kif"].get("k8s").is_none());
        assert_eq!(v["kif"]["attributes"]["team"], "platform");
    }
}
