use anyhow::Result;
use k8s_openapi::api::core::v1::{Event, EventSource, ObjectReference};
use kube::{Api, Client, api::PostParams};

pub async fn emit_pod_event(
    client: &Client,
    namespace: &str,
    pod_name: &str,
    reason: &str,
    event_type: &str,
    message: &str,
) -> Result<()> {
    let events: Api<Event> = Api::namespaced(client.clone(), namespace);

    let involved = ObjectReference {
        api_version: Some("v1".to_string()),
        kind: Some("Pod".to_string()),
        name: Some(pod_name.to_string()),
        namespace: Some(namespace.to_string()),
        ..Default::default()
    };

    let ev = Event {
        metadata: kube::api::ObjectMeta {
            generate_name: Some(format!("{pod_name}-")),
            namespace: Some(namespace.to_string()),
            ..Default::default()
        },
        involved_object: involved,
        reason: Some(reason.to_string()),
        message: Some(message.to_string()),
        type_: Some(event_type.to_string()),
        source: Some(EventSource {
            component: Some("kif-webhook".to_string()),
            ..Default::default()
        }),
        ..Default::default()
    };

    let _ = events.create(&PostParams::default(), &ev).await?;
    Ok(())
}
