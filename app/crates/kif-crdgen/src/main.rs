use anyhow::Result;
use kube::CustomResourceExt;

use kif_api::CloudRoleBinding;

fn main() -> Result<()> {
    let crd = CloudRoleBinding::crd();
    let yaml = serde_yaml::to_string(&crd).expect("Failed to serialize CRD to YAML");
    println!("{yaml}");
    Ok(())
}
