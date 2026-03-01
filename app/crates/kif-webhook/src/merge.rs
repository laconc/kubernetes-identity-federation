use std::collections::BTreeMap;

use anyhow::{Result, anyhow, bail};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;
use k8s_openapi::jiff::Timestamp;
use kube::{Api, api::PostParams};
use sha2::{Digest, Sha256};

use kif_api::{
    AttributesSpec, AwsProviderSpec, CloudRoleBinding, CloudRoleBindingRef, CloudRoleBindingStatus,
    Condition, ConditionReason, ConditionStatus, ConditionType, ProvidersSpec,
    ResolvedCloudRoleBinding, ResolvedCloudRoleBindingSpec, ResolvedCloudRoleBindingStatus,
    SubjectRef,
};

#[derive(Clone, Debug)]
pub struct MergeIssue {
    pub condition_type: ConditionType,
    pub reason: ConditionReason,
    pub message: String,
    /// Names of CloudRoleBindings that should be marked with this condition.
    pub affected_crb_names: Vec<String>,
}

pub fn build_resolved_from_spec(
    namespace: &str,
    sa_name: &str,
    sources: Vec<CloudRoleBindingRef>,
    merged: ResolvedCloudRoleBindingSpec,
) -> Result<ResolvedCloudRoleBinding> {
    let mut providers = vec![];
    if merged.providers.aws.is_some() {
        providers.push("aws");
    }
    if merged.providers.azure.is_some() {
        providers.push("azure");
    }
    if merged.providers.gcp.is_some() {
        providers.push("gcp");
    }

    let spec_bytes = serde_json::to_vec(&merged)?;
    let mut h = Sha256::new();
    h.update(spec_bytes);
    let config_hash = hex::encode(h.finalize());

    let observed_generation = sources
        .iter()
        .filter_map(|s| s.generation)
        .max()
        .unwrap_or_default();

    let status = ResolvedCloudRoleBindingStatus {
        config_hash: Some(config_hash),
        providers: providers.join(","),
        sources,
        last_error: None,
        conditions: vec![Condition {
            r#type: ConditionType::Ready,
            status: ConditionStatus::True,
            reason: ConditionReason::Validated,
            message: "resolved and validated".to_string(),
            observed_generation,
            last_transition_time: Time(Timestamp::now()),
        }],
    };

    Ok(ResolvedCloudRoleBinding {
        metadata: kube::api::ObjectMeta {
            name: Some(sa_name.to_string()),
            namespace: Some(namespace.to_string()),
            ..Default::default()
        },
        spec: merged,
        status: Some(status),
    })
}

pub fn build_failed_resolved(
    namespace: &str,
    sa_name: &str,
    sources: Vec<CloudRoleBindingRef>,
    issue: &MergeIssue,
) -> ResolvedCloudRoleBinding {
    let observed_generation = sources
        .iter()
        .filter_map(|s| s.generation)
        .max()
        .unwrap_or_default();

    let status = ResolvedCloudRoleBindingStatus {
        config_hash: None,
        providers: String::new(),
        sources,
        last_error: Some(issue.message.clone()),
        conditions: vec![Condition {
            r#type: ConditionType::Ready,
            status: ConditionStatus::False,
            reason: issue.reason.clone(),
            message: issue.message.clone(),
            observed_generation,
            last_transition_time: Time(Timestamp::now()),
        }],
    };

    ResolvedCloudRoleBinding {
        metadata: kube::api::ObjectMeta {
            name: Some(sa_name.to_string()),
            namespace: Some(namespace.to_string()),
            ..Default::default()
        },
        spec: ResolvedCloudRoleBindingSpec {
            subject: SubjectRef {
                service_account_name: sa_name.to_string(),
            },
            attributes: None,
            providers: ProvidersSpec::default(),
        },
        status: Some(status),
    }
}

pub fn build_crb_status(
    crb: &CloudRoleBinding,
    ready: bool,
    issue: Option<&MergeIssue>,
) -> CloudRoleBindingStatus {
    let providers = providers_summary(&crb.spec.providers);
    let observed_generation = crb.metadata.generation.unwrap_or_default();

    let now = Time(Timestamp::now());

    let mut conditions = Vec::new();
    conditions.push(Condition {
        r#type: ConditionType::Ready,
        status: if ready {
            ConditionStatus::True
        } else {
            ConditionStatus::False
        },
        reason: if ready {
            ConditionReason::Validated
        } else {
            issue
                .map(|i| i.reason.clone())
                .unwrap_or(ConditionReason::Error)
        },
        message: if ready {
            "validated".to_string()
        } else {
            issue
                .map(|i| i.message.clone())
                .unwrap_or_else(|| "not ready".to_string())
        },
        observed_generation,
        last_transition_time: now.clone(),
    });

    if let Some(i) = issue {
        match i.condition_type {
            ConditionType::InvalidSpec => {
                conditions.push(Condition {
                    r#type: ConditionType::InvalidSpec,
                    status: ConditionStatus::True,
                    reason: i.reason.clone(),
                    message: i.message.clone(),
                    observed_generation,
                    last_transition_time: now.clone(),
                });
            }
            ConditionType::Conflict => {
                conditions.push(Condition {
                    r#type: ConditionType::Conflict,
                    status: ConditionStatus::True,
                    reason: i.reason.clone(),
                    message: i.message.clone(),
                    observed_generation,
                    last_transition_time: now.clone(),
                });
            }
            ConditionType::Ready => {}
        }
    } else {
        // Explicitly clear other conditions when healthy.
        conditions.push(Condition {
            r#type: ConditionType::Conflict,
            status: ConditionStatus::False,
            reason: ConditionReason::Validated,
            message: "no conflicts".to_string(),
            observed_generation,
            last_transition_time: now.clone(),
        });
        conditions.push(Condition {
            r#type: ConditionType::InvalidSpec,
            status: ConditionStatus::False,
            reason: ConditionReason::Validated,
            message: "spec is valid".to_string(),
            observed_generation,
            last_transition_time: now,
        });
    }

    CloudRoleBindingStatus {
        providers,
        conditions,
        last_error: if ready {
            None
        } else {
            issue.map(|i| i.message.clone())
        },
    }
}

fn providers_summary(p: &ProvidersSpec) -> String {
    let mut providers = vec![];
    if p.aws.is_some() {
        providers.push("aws");
    }
    if p.azure.is_some() {
        providers.push("azure");
    }
    if p.gcp.is_some() {
        providers.push("gcp");
    }
    providers.join(",")
}

/// Merge and validate the CloudRoleBindings for a given service account.
pub fn merge_inputs(
    sa_name: &str,
    inputs: Vec<&CloudRoleBinding>,
) -> std::result::Result<ResolvedCloudRoleBindingSpec, MergeIssue> {
    // There's a conflict if two CRDs define the same provider
    let mut aws: Option<AwsProviderSpec> = None;
    let mut aws_set_by: Option<String> = None;

    // Check for conflicts in the attributes
    let mut include_prov: Option<bool> = None;
    let mut extra: BTreeMap<String, String> = BTreeMap::new();

    for crb in &inputs {
        let crb_name = crb.metadata.name.clone().unwrap_or_default();

        // All the CRDs must have at least one provider set
        let providers = &crb.spec.providers;
        let provider_present =
            providers.aws.is_some() || providers.azure.is_some() || providers.gcp.is_some();
        if !provider_present {
            return Err(MergeIssue {
                condition_type: ConditionType::InvalidSpec,
                reason: ConditionReason::ValidationFailed,
                message: "CloudRoleBinding must set at least one provider block".to_string(),
                affected_crb_names: vec![crb_name],
            });
        }

        if let Some(p) = crb.spec.providers.aws.clone() {
            if aws.is_some() {
                return Err(MergeIssue {
                    condition_type: ConditionType::Conflict,
                    reason: ConditionReason::ProviderConflict,
                    message: "conflict: multiple CloudRoleBindings set providers.aws".to_string(),
                    affected_crb_names: vec![aws_set_by.clone().unwrap_or_default(), crb_name]
                        .into_iter()
                        .filter(|n| !n.is_empty())
                        .collect(),
                });
            }
            aws = Some(p);
            aws_set_by = Some(crb_name);
        }

        if let Some(a) = crb.spec.attributes.clone() {
            if let Some(v) = a.include_provenance {
                if let Some(prev) = include_prov {
                    if prev != v {
                        return Err(MergeIssue {
                            condition_type: ConditionType::Conflict,
                            reason: ConditionReason::AttributeConflict,
                            message: "conflict: includeProvenance differs across CloudRoleBindings"
                                .to_string(),
                            affected_crb_names: inputs
                                .iter()
                                .filter_map(|c| c.metadata.name.clone())
                                .collect(),
                        });
                    }
                } else {
                    include_prov = Some(v);
                }
            }

            if let Some(m) = a.extra {
                for (k, v) in m {
                    if extra.contains_key(&k) {
                        return Err(MergeIssue {
                            condition_type: ConditionType::Conflict,
                            reason: ConditionReason::AttributeConflict,
                            message: format!("conflict: duplicate attributes.extra key: {k}"),
                            affected_crb_names: inputs
                                .iter()
                                .filter_map(|c| c.metadata.name.clone())
                                .collect(),
                        });
                    }
                    extra.insert(k, v);
                }
            }
        }
    }

    // At least one provider must be set after merging. (AWS-only for now.)
    let aws = aws.ok_or_else(|| MergeIssue {
        condition_type: ConditionType::InvalidSpec,
        reason: ConditionReason::ValidationFailed,
        message: anyhow!("no providers configured after merge").to_string(),
        affected_crb_names: inputs
            .iter()
            .filter_map(|c| c.metadata.name.clone())
            .collect(),
    })?;

    Ok(ResolvedCloudRoleBindingSpec {
        subject: SubjectRef {
            service_account_name: sa_name.to_string(),
        },
        attributes: Some(AttributesSpec {
            include_provenance: include_prov.or(Some(true)),
            extra: if extra.is_empty() { None } else { Some(extra) },
        }),
        providers: ProvidersSpec {
            aws: Some(aws),
            azure: None,
            gcp: None,
        },
    })
}

pub async fn upsert_resolved(
    api: &Api<ResolvedCloudRoleBinding>,
    resolved: ResolvedCloudRoleBinding,
) -> Result<()> {
    let name = resolved.metadata.name.as_deref().unwrap_or_default();
    if name.is_empty() {
        bail!("ResolvedCloudRoleBinding.metadata.name is required");
    }

    if api.get_opt(name).await?.is_some() {
        api.replace(name, &PostParams::default(), &resolved).await?;
    } else {
        api.create(&PostParams::default(), &resolved).await?;
    }

    Ok(())
}
