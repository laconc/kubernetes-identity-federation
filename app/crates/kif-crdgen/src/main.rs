use std::env;

use anyhow::{Context, Result};
use kube::CustomResourceExt;

use kif_api::{CloudRoleBinding, ResolvedCloudRoleBinding};

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        panic!("No CRD specified. Usage: crdgen <crb|rcrb>");
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
        _ => panic!(
            "Invalid CRD type: {crd_type}. Use 'crb' or 'rcrb'."
        ),
    };

    println!("{yaml}");
    Ok(())
}
