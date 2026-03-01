use std::env;

use anyhow::{Context, Result, bail};
use kube::CustomResourceExt;

use kif_api::{CloudRoleBinding, ResolvedCloudRoleBinding};

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        bail!("No CRD specified. Usage: crdgen <crb|rcrb>");
    }

    let crd_type = &args[1];
    let yaml = match crd_type.to_lowercase().as_str() {
        "crb" => {
            let crd = CloudRoleBinding::crd();
            serde_yaml::to_string(&crd)
                .context("Failed to serialize CloudRoleBinding CRD to YAML")?
        }
        "rcrb" => {
            let crd = ResolvedCloudRoleBinding::crd();
            serde_yaml::to_string(&crd)
                .context("Failed to serialize ResolvedCloudRoleBinding CRD to YAML")?
        }
        _ => bail!("Invalid CRD type: {crd_type}. Use 'crb' or 'rcrb'."),
    };

    print!("{}", add_helm_wrapper(&yaml));
    Ok(())
}

fn add_helm_wrapper(yaml: &str) -> String {
    // Validate structure: panic early if metadata already has annotations,
    // which would cause the string injection below to produce a duplicate key.
    let doc: serde_yaml::Value =
        serde_yaml::from_str(yaml).expect("CRD serialization produced invalid YAML");
    assert!(
        doc.get("metadata")
            .and_then(|m| m.get("annotations"))
            .is_none(),
        "CRD metadata already contains annotations; add_helm_wrapper must be updated to merge them"
    );

    let mut output = String::from("{{- if .Values.crds.install }}\n");
    let mut in_metadata = false;
    for line in yaml.lines() {
        if line == "metadata:" {
            in_metadata = true;
        }
        if in_metadata && line == "spec:" {
            output.push_str("  {{- if .Values.crds.keep }}\n");
            output.push_str("  annotations:\n");
            output.push_str("    \"helm.sh/resource-policy\": keep\n");
            output.push_str("  {{- end }}\n");
            in_metadata = false;
        }
        output.push_str(line);
        output.push('\n');
    }
    output.push_str("{{- end }}\n");
    output
}
