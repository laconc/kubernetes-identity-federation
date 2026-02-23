use std::collections::BTreeMap;

use k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// CloudRoleBinding binds a Kubernetes ServiceAccount (in the same namespace)
/// to one or more cloud providers.
///
/// This CRD supports either:
/// - a single CloudRoleBinding configuring multiple providers for the ServiceAccount, or
/// - multiple CloudRoleBindings for the same ServiceAccount configuring different providers.
#[derive(CustomResource, Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[kube(
    group = "64f.dev",
    version = "v1alpha1",
    kind = "CloudRoleBinding",
    plural = "cloudrolebindings",
    category = "all",
    namespaced,
    status = "CloudRoleBindingStatus",
    shortname = "crb"
)]
#[kube(
    printcolumn = r#"{"name":"ServiceAccount","type":"string","jsonPath":".spec.subject.serviceAccountName"}"#,
    printcolumn = r#"{"name":"Providers","type":"string","jsonPath":".status.providers"}"#,
    printcolumn = r#"{"name":"Ready","type":"string","jsonPath":".status.conditions[?(@.type=='Ready')].status"}"#
)]
pub struct CloudRoleBindingSpec {
    /// The subject in this namespace this binding applies to.
    pub subject: SubjectRef,

    /// Optional attributes to include in minted provider tokens for ABAC decisions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attributes: Option<AttributesSpec>,

    /// Provider-specific configuration. Any subset may be provided.
    #[serde(default)]
    pub providers: ProvidersSpec,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubjectRef {
    /// ServiceAccount in the same namespace.
    #[schemars(length(min = 1))]
    pub service_account_name: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AttributesSpec {
    /// Include standard Kubernetes provenance attributes by default.
    ///
    /// If unset, treated as true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_provenance: Option<bool>,

    /// Additional attributes to include in minted tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<BTreeMap<String, String>>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProvidersSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aws: Option<AwsProviderSpec>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub azure: Option<AzureProviderSpec>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gcp: Option<GcpProviderSpec>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AwsProviderSpec {
    /// IAM Role ARN to assume via STS AssumeRoleWithWebIdentity.
    #[schemars(length(min = 1), regex(pattern = r"^arn:aws:iam::\d{12}:role\/.+$"))]
    pub role_arn: String,

    /// Default AWS region.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1))]
    pub region: Option<String>,

    /// If true, inject AWS_STS_REGIONAL_ENDPOINTS=regional.
    #[serde(default = "default_true")]
    pub sts_regional_endpoints: bool,

    /// Audience to use when minting the AWS token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1))]
    pub audience: Option<String>,

    /// Max session duration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_session_duration_seconds: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AzureProviderSpec {
    /// Entra tenant UUID.
    #[schemars(length(min = 1), regex(pattern = r"^[0-9a-fA-F-]{36}$"))]
    pub tenant_id: String,

    /// Client/app UUID (or managed identity client ID.)
    #[schemars(length(min = 1), regex(pattern = r"^[0-9a-fA-F-]{36}$"))]
    pub client_id: String,

    /// Audience to use when minting the Azure token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1))]
    pub audience: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GcpProviderSpec {
    /// Full resource name of the Workload Identity Provider.
    #[schemars(length(min = 1))]
    pub workload_identity_provider: String,

    /// GCP service account email.
    #[schemars(
        length(min = 1),
        regex(pattern = r"^[^@\s]+@[^@\s]+\.iam\.gserviceaccount\.com$")
    )]
    pub service_account_email: String,

    /// Audience to use when minting the GCP token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1))]
    pub audience: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1))]
    pub project_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CloudRoleBindingStatus {
    /// Summary string for quick display (e.g., "aws,azure" or "aws").
    #[serde(default)]
    pub providers: String,

    /// Conditions set by your controller (validation status, conflicts, etc.)
    #[serde(default)]
    pub conditions: Vec<Condition>,

    /// Last error for operator visibility, if there was one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Condition {
    pub r#type: ConditionType,
    pub status: ConditionStatus,

    /// Generation observed when the condition was set.
    pub observed_generation: i64,

    /// Machine-readable reason.
    pub reason: ConditionReason,

    /// Human-readable message with details about the transition.
    #[schemars(length(min = 1))]
    pub message: String,

    /// RFC3339 timestamp of last transition.
    pub last_transition_time: Time,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub enum ConditionStatus {
    True,
    False,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub enum ConditionType {
    Ready,
    Conflict,
    InvalidSpec,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub enum ConditionReason {
    /// The resource was validated successfully.
    Validated,
    /// Multiple bindings defined the same provider block for the same ServiceAccount.
    ProviderConflict,
    /// Attribute key conflicts across bindings for the same ServiceAccount.
    AttributeConflict,
    /// Required fields missing or invalid.
    ValidationFailed,
    /// Generic error.
    Error,
}

/// ResolvedCloudRoleBinding is the webhook-produced, merged, validated
/// effective configuration for a single ServiceAccount in a namespace.
#[derive(CustomResource, Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "64f.dev",
    version = "v1alpha1",
    kind = "ResolvedCloudRoleBinding",
    plural = "resolvedcloudrolebindings",
    namespaced,
    status = "ResolvedCloudRoleBindingStatus",
    shortname = "rcrb"
)]
#[kube(
    printcolumn = r#"{"name":"ServiceAccount","type":"string","jsonPath":".spec.subject.serviceAccountName"}"#,
    printcolumn = r#"{"name":"Providers","type":"string","jsonPath":".status.providers"}"#,
    printcolumn = r#"{"name":"Ready","type":"string","jsonPath":".status.conditions[?(@.type=='Ready')].status"}"#
)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedCloudRoleBindingSpec {
    /// The ServiceAccount this resolved binding applies to (same namespace as this resource).
    pub subject: SubjectRef,

    /// Resolved attributes that will be included in minted provider tokens.
    ///
    /// - includeProvenance defaults to true if unset (policy behavior)
    /// - extra may be merged from multiple CloudRoleBindings
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attributes: Option<AttributesSpec>,

    /// Resolved provider configuration for this ServiceAccount.
    ///
    /// This is the merged output of all applicable CloudRoleBindings
    /// after conflict policy is applied.
    #[serde(default)]
    pub providers: ProvidersSpec,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedCloudRoleBindingStatus {
    /// A short, human-friendly summary of configured providers (e.g. "aws,azure").
    #[serde(default)]
    pub providers: String,

    /// Conditions reflect merge/validation state computed by the controller.
    #[serde(default)]
    pub conditions: Vec<Condition>,

    /// Source CloudRoleBindings that contributed to the resolved config.
    /// Useful for debugging and traceability.
    #[serde(default)]
    pub sources: Vec<CloudRoleBindingRef>,

    /// Hash of the effective resolved config (stable canonicalization recommended).
    /// Lets you cheaply detect when the effective config changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_hash: Option<String>,

    /// Optional last error for operator visibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CloudRoleBindingRef {
    /// Name of the CloudRoleBinding (same namespace).
    #[schemars(length(min = 1))]
    pub name: String,

    /// Resource generation observed when it contributed to the resolved config.
    /// Helps debugging “why didn’t my change apply?”
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<i64>,
}

fn default_true() -> bool {
    true
}
