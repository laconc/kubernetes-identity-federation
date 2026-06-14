use crate::{
    config::{AdmissionFailureMode, WebhookConfig},
    k8s, mutate,
};

use anyhow::{Context, Result};
use k8s_openapi::api::core::v1::Pod;
use kube::{
    Api, Client,
    core::{
        DynamicObject,
        admission::{AdmissionRequest, AdmissionResponse, AdmissionReview, Operation},
    },
};

use kif_api::{ConditionStatus, ConditionType, ResolvedCloudRoleBinding};

#[derive(Clone)]
pub struct AppState {
    pub cfg: WebhookConfig,
    pub client: Client,
}

pub async fn handle(
    review: AdmissionReview<Pod>,
    state: AppState,
) -> Result<AdmissionReview<DynamicObject>> {
    let req: AdmissionRequest<Pod> = review
        .try_into()
        .context("invalid AdmissionReview payload")?;
    let mut resp = AdmissionResponse::from(&req);

    if req.operation != Operation::Create {
        resp.allowed = true;
        return Ok(resp.into_review());
    }

    let pod = req.object.clone().context("missing pod object")?;
    let ns = req.namespace.as_deref().unwrap_or("default").to_string();
    let pod_name = pod.metadata.name.clone().unwrap_or("unknown".to_string());

    // Idempotency: if agent already exists, allow
    if mutate::pod_has_container(&pod, "kif-agent") {
        resp.allowed = true;
        return Ok(resp.into_review());
    }

    let sa_name = pod
        .spec
        .as_ref()
        .and_then(|s| s.service_account_name.clone())
        .unwrap_or("default".to_string());

    let resolved_api: Api<ResolvedCloudRoleBinding> = Api::namespaced(state.client.clone(), &ns);
    let resolved = match resolved_api.get_opt(&sa_name).await? {
        Some(r) => r,
        None => {
            // No binding for this ServiceAccount: the pod is not managed by kif.
            // The webhook fires cluster-wide, so the overwhelming majority of
            // pods have no CloudRoleBinding and must be admitted unchanged. This
            // is distinct from a binding that exists but isn't usable yet (below),
            // which honours the configured admission failure mode.
            resp.allowed = true;
            return Ok(resp.into_review());
        }
    };

    if !is_ready(&resolved) {
        return fail_or_skip(
            state,
            &req,
            pod_name,
            format!("ResolvedCloudRoleBinding not Ready for {ns}/{sa_name}"),
        )
        .await;
    }

    let config_hash = resolved
        .status
        .as_ref()
        .and_then(|s| s.config_hash.clone())
        .context("ResolvedCloudRoleBinding.status.configHash is required")?;

    // AWS-only currently
    let aws = resolved
        .spec
        .providers
        .aws
        .clone()
        .context("AWS provider is required (current support is AWS only)")?;

    let patch = mutate::build_pod_patch(
        pod,
        &state.cfg.agent_image,
        &state.cfg.agent_image_pull_policy,
        state.cfg.agent_port,
        &state.cfg.federation_url,
        &sa_name,
        &config_hash,
        &aws,
    )?;

    resp.allowed = true;
    resp = resp.with_patch(patch).context("failed to set patch")?;
    Ok(resp.into_review())
}

fn is_ready(resolved: &ResolvedCloudRoleBinding) -> bool {
    let Some(status) = &resolved.status else {
        return false;
    };
    status.conditions.iter().any(|c| {
        matches!(c.r#type, ConditionType::Ready) && matches!(c.status, ConditionStatus::True)
    })
}

async fn fail_or_skip(
    state: AppState,
    req: &AdmissionRequest<Pod>,
    pod_name: String,
    message: String,
) -> Result<AdmissionReview<DynamicObject>> {
    let mut resp = AdmissionResponse::from(req);

    match state.cfg.admission_failure_mode {
        AdmissionFailureMode::Fail => {
            resp.allowed = false;
            resp = resp.deny(message);
            Ok(resp.into_review())
        }
        AdmissionFailureMode::Ignore => {
            resp.allowed = true;

            let ns = req.namespace.as_deref().unwrap_or("default");
            let msg = format!("Injection skipped (ADMISSION_FAILURE_MODE=Ignore): {message}");
            let _ = k8s::emit_pod_event(
                &state.client,
                ns,
                &pod_name,
                "InjectionSkipped",
                "Warning",
                &msg,
            )
            .await;

            Ok(resp.into_review())
        }
    }
}
