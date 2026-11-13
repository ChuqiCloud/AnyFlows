//! Export or verify the language-neutral connector contract artifacts.

use std::error::Error;
use std::fs;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn Error>> {
    let action = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "check".to_owned());
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let artifacts = [
        (
            root.join("contracts/plugins/v1/schema.json"),
            af_plugin_contract::schema_document(),
        ),
        (
            root.join("contracts/plugins/v1/openapi.json"),
            af_plugin_contract::openapi_document(),
        ),
    ];

    match action.as_str() {
        "export" => {
            for (path, value) in artifacts {
                fs::create_dir_all(
                    path.parent()
                        .expect("plugin contract artifact has a parent"),
                )?;
                fs::write(&path, render(&value)?)?;
                println!("wrote {}", path.display());
            }
            Ok(())
        }
        "check" => {
            for (path, value) in artifacts {
                let expected = render(&value)?;
                let actual = fs::read_to_string(&path)?;
                if actual != expected {
                    return Err(format!("{} is stale; run with `export`", path.display()).into());
                }
            }
            println!("plugin contract artifacts are current");
            Ok(())
        }
        _ => Err(
            "usage: cargo run -p af-plugin-contract --example export_contract -- <export|check>"
                .into(),
        ),
    }
}

fn render(value: &serde_json::Value) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(value).map(|document| format!("{document}\n"))
}
