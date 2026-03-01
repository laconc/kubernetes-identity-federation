use anyhow::{Result, bail};
use json_patch::Patch;
use k8s_openapi::api::core::v1::{
    ConfigMapProjection, Container, DownwardAPIProjection, DownwardAPIVolumeFile, EnvVar,
    EnvVarSource, HTTPGetAction, KeyToPath, ObjectFieldSelector, Pod, PodSpec, Probe,
    ProjectedVolumeSource, ServiceAccountTokenProjection, Volume, VolumeMount, VolumeProjection,
};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use serde_json::Value;

use kif_api::AwsProviderSpec;

pub fn pod_has_container(pod: &Pod, name: &str) -> bool {
    let Some(spec) = pod.spec.as_ref() else {
        return false;
    };
    spec.containers.iter().any(|c| c.name == name)
        || spec
            .init_containers
            .iter()
            .flatten()
            .any(|c| c.name == name)
}

pub fn build_pod_patch(
    mut pod: Pod,
    agent_image: &str,
    agent_port: u16,
    federation_url: &str,
    service_account_name: &str,
    config_hash: &str,
    aws: &AwsProviderSpec,
) -> Result<Patch> {
    let original: Value = serde_json::to_value(&pod)?;

    let spec: &mut PodSpec = pod
        .spec
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("pod.spec missing"))?;

    spec.automount_service_account_token = Some(false);

    if spec.volumes.is_none() {
        spec.volumes = Some(vec![]);
    }
    let volumes = spec.volumes.as_mut().unwrap();

    ensure_sa_projected_token_volume(volumes)?;
    ensure_aws_emptydir_volume(volumes)?;

    // Inject the agent as a sidecar init container so it starts and writes the token
    // before any of the other containers come up
    let agent = build_agent_container(
        agent_image,
        agent_port,
        federation_url,
        service_account_name,
        config_hash,
    )?;
    spec.init_containers
        .get_or_insert_with(Vec::new)
        .insert(0, agent);

    // Inject the AWS volume and env vars into init containers (except the agent)
    for c in spec.init_containers.iter_mut().flatten() {
        if c.name == "kif-agent" {
            continue;
        }
        inject_aws_into_app_container(c, aws)?;
    }

    // Inject the AWS volume and env vars into the main containers
    for c in &mut spec.containers {
        inject_aws_into_app_container(c, aws)?;
    }

    let modified: Value = serde_json::to_value(&pod)?;
    Ok(json_patch::diff(&original, &modified))
}

fn ensure_sa_projected_token_volume(volumes: &mut Vec<Volume>) -> Result<()> {
    if volumes.iter().any(|v| v.name == "kif-sa-token") {
        return Ok(());
    }

    let sat = ServiceAccountTokenProjection {
        path: "token".to_string(),
        audience: None,
        expiration_seconds: None,
    };

    let proj = ProjectedVolumeSource {
        sources: Some(vec![
            VolumeProjection {
                service_account_token: Some(sat),
                ..Default::default()
            },
            VolumeProjection {
                downward_api: Some(DownwardAPIProjection {
                    items: Some(vec![DownwardAPIVolumeFile {
                        path: "namespace".to_string(),
                        field_ref: Some(ObjectFieldSelector {
                            api_version: Some("v1".to_string()),
                            field_path: "metadata.namespace".to_string(),
                        }),
                        ..Default::default()
                    }]),
                }),
                ..Default::default()
            },
            VolumeProjection {
                config_map: Some(ConfigMapProjection {
                    name: "kube-root-ca.crt".to_string(),
                    items: Some(vec![KeyToPath {
                        key: "ca.crt".to_string(),
                        path: "ca.crt".to_string(),
                        ..Default::default()
                    }]),
                    optional: Some(false),
                }),
                ..Default::default()
            },
        ]),
        ..Default::default()
    };

    volumes.push(Volume {
        name: "kif-sa-token".to_string(),
        projected: Some(proj),
        ..Default::default()
    });

    Ok(())
}

fn ensure_aws_emptydir_volume(volumes: &mut Vec<Volume>) -> Result<()> {
    if volumes.iter().any(|v| v.name == "kif-aws") {
        return Ok(());
    }
    volumes.push(Volume {
        name: "kif-aws".to_string(),
        empty_dir: Some(Default::default()),
        ..Default::default()
    });
    Ok(())
}

fn build_agent_container(
    agent_image: &str,
    agent_port: u16,
    federation_url: &str,
    service_account_name: &str,
    config_hash: &str,
) -> Result<Container> {
    let mut c = Container {
        name: "kif-agent".to_string(),
        image: Some(agent_image.to_string()),
        image_pull_policy: Some("IfNotPresent".to_string()),
        restart_policy: Some("Always".to_string()),
        startup_probe: Some(Probe {
            http_get: Some(HTTPGetAction {
                path: Some("/readyz".to_string()),
                port: IntOrString::Int(agent_port.into()),
                ..Default::default()
            }),
            failure_threshold: Some(30),
            period_seconds: Some(10),
            ..Default::default()
        }),
        ..Default::default()
    };

    c.env = Some(vec![
        EnvVar {
            name: "SERVICE_ACCOUNT_NAME".to_string(),
            value: Some(service_account_name.to_string()),
            ..Default::default()
        },
        EnvVar {
            name: "CONFIG_HASH".to_string(),
            value: Some(config_hash.to_string()),
            ..Default::default()
        },
        EnvVar {
            name: "FEDERATION_URL".to_string(),
            value: Some(federation_url.to_string()),
            ..Default::default()
        },
        EnvVar {
            name: "PORT".to_string(),
            value: Some(agent_port.to_string()),
            ..Default::default()
        },
        EnvVar {
            name: "POD_NAME".to_string(),
            value_from: Some(EnvVarSource {
                field_ref: Some(ObjectFieldSelector {
                    field_path: "metadata.name".to_string(),
                    api_version: Some("v1".to_string()),
                }),
                ..Default::default()
            }),
            ..Default::default()
        },
    ]);

    c.volume_mounts = Some(vec![
        VolumeMount {
            name: "kif-sa-token".to_string(),
            mount_path: "/var/run/secrets/kubernetes.io/serviceaccount".to_string(),
            read_only: Some(true),
            ..Default::default()
        },
        VolumeMount {
            name: "kif-aws".to_string(),
            mount_path: "/var/run/kif/aws".to_string(),
            read_only: Some(false),
            ..Default::default()
        },
    ]);

    Ok(c)
}

fn inject_aws_into_app_container(c: &mut Container, aws: &AwsProviderSpec) -> Result<()> {
    if aws.role_arn.trim().is_empty() {
        bail!("aws.role_arn is empty in resolved binding");
    }

    if c.env.is_none() {
        c.env = Some(vec![]);
    }
    if c.volume_mounts.is_none() {
        c.volume_mounts = Some(vec![]);
    }

    let env = c.env.as_mut().unwrap();
    let vms = c.volume_mounts.as_mut().unwrap();

    if !vms.iter().any(|m| m.name == "kif-aws") {
        vms.push(VolumeMount {
            name: "kif-aws".to_string(),
            mount_path: "/var/run/kif/aws".to_string(),
            read_only: Some(true),
            ..Default::default()
        });
    }

    upsert_env(env, "AWS_ROLE_ARN", &aws.role_arn);
    upsert_env(env, "AWS_WEB_IDENTITY_TOKEN_FILE", "/var/run/kif/aws/token");

    if let Some(region) = aws.region.as_deref() {
        upsert_env(env, "AWS_REGION", region);
        upsert_env(env, "AWS_DEFAULT_REGION", region);
    }

    if aws.sts_regional_endpoints {
        upsert_env(env, "AWS_STS_REGIONAL_ENDPOINTS", "regional");
    }

    Ok(())
}

fn upsert_env(env: &mut Vec<EnvVar>, name: &str, value: &str) {
    if let Some(e) = env.iter_mut().find(|e| e.name == name) {
        e.value = Some(value.to_string());
        e.value_from = None;
        return;
    }
    env.push(EnvVar {
        name: name.to_string(),
        value: Some(value.to_string()),
        ..Default::default()
    });
}
