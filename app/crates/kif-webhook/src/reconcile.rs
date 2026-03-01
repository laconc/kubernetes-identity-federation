use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use anyhow::Result;
use futures::StreamExt;
use kube::{Api, Client, api::PostParams};
use kube_runtime::watcher::{Event, watcher};
use tokio::sync::mpsc;
use tracing::warn;

use crate::{
    config::WebhookConfig,
    merge,
    queue::{Key, Queue},
};

use kif_api::{CloudRoleBinding, CloudRoleBindingRef, ResolvedCloudRoleBinding};

pub async fn watch_cloud_role_bindings(
    client: Client,
    q: Queue,
    ready: Arc<AtomicBool>,
) -> Result<()> {
    let crb_api: Api<CloudRoleBinding> = Api::all(client);
    let mut w = watcher(crb_api, Default::default()).boxed();

    while let Some(ev) = w.next().await {
        match ev? {
            Event::InitDone => {
                ready.store(true, Ordering::Relaxed);
            }
            Event::Apply(crb) | Event::Delete(crb) | Event::InitApply(crb) => {
                let sa = crb.spec.subject.service_account_name;
                if let Some(namespace) = crb.metadata.namespace {
                    q.enqueue(Key {
                        namespace,
                        service_account_name: sa,
                    })
                    .await;
                }
            }
            _ => {}
        }
    }

    Ok(())
}

pub async fn run_workers(
    client: Client,
    cfg: WebhookConfig,
    rx: mpsc::Receiver<Key>,
    q: Queue,
) -> Result<()> {
    let rx = std::sync::Arc::new(tokio::sync::Mutex::new(rx));
    let workers = cfg.reconcile_workers.max(1);

    let mut joins = vec![];
    for i in 0..workers {
        let client = client.clone();
        let rx = rx.clone();
        let q = q.clone();
        joins.push(tokio::spawn(async move {
            loop {
                let key = {
                    let mut guard = rx.lock().await;
                    guard.recv().await
                };
                let Some(key) = key else {
                    break;
                };

                if let Err(e) = reconcile(&client, &key).await {
                    warn!(
                        worker = i,
                        error=?e,
                        namespace=%key.namespace,
                        sa=%key.service_account_name,
                        "reconcile failed"
                    );
                }
                q.done(&key).await;
            }
        }));
    }

    for j in joins {
        let _ = j.await;
    }
    Ok(())
}

/// We search for CloudRoleBindings that reference the specified service account in its
/// namespace and merge them to create a ResolvedCloudRoleBinding resource.
async fn reconcile(client: &Client, key: &Key) -> Result<()> {
    let ns = &key.namespace;
    let sa = &key.service_account_name;

    let crb_api: Api<CloudRoleBinding> = Api::namespaced(client.clone(), ns);

    let all = crb_api.list(&Default::default()).await?;
    let relevant_crbs: Vec<CloudRoleBinding> = all
        .into_iter()
        .filter(|crb| crb.spec.subject.service_account_name == *sa)
        .collect();

    let mut inputs = Vec::with_capacity(relevant_crbs.len());
    let mut sources = Vec::with_capacity(relevant_crbs.len());
    for crb in &relevant_crbs {
        inputs.push(crb);
        sources.push(CloudRoleBindingRef {
            name: crb.metadata.name.clone().unwrap_or_default(),
            generation: crb.metadata.generation,
        });
    }

    let resolved = match merge::merge_inputs(sa, inputs) {
        Ok(merged) => {
            // Success: mark all relevant CRBs Ready and clear last_error
            for crb in &relevant_crbs {
                let name = crb.metadata.name.as_deref().unwrap_or_default();
                if name.is_empty() {
                    continue;
                }

                let mut updated = crb.clone();
                updated.status = Some(merge::build_crb_status(&updated, true, None));

                crb_api
                    .replace_status(name, &PostParams::default(), &updated)
                    .await?;
            }

            merge::build_resolved_from_spec(ns, sa, sources, merged)?
        }
        Err(issue) => {
            // Failure: mark affected CRBs with the appropriate condition and update last_error
            for crb in &relevant_crbs {
                let name = crb.metadata.name.as_deref().unwrap_or_default();
                if name.is_empty() {
                    continue;
                }

                if issue.affected_crb_names.iter().any(|n| n == name) {
                    let mut updated = crb.clone();
                    updated.status = Some(merge::build_crb_status(&updated, false, Some(&issue)));

                    crb_api
                        .replace_status(name, &PostParams::default(), &updated)
                        .await?;
                }
            }

            merge::build_failed_resolved(ns, sa, sources, &issue)
        }
    };

    let resolved_api: Api<ResolvedCloudRoleBinding> = Api::namespaced(client.clone(), ns);
    merge::upsert_resolved(&resolved_api, resolved).await?;

    Ok(())
}
