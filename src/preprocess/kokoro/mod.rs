use std::{error::Error, path::PathBuf};

use burn_onnx::ModelGen;
use hf_hub::{HFClient, HFResult};
use tokio::{fs, task};

const MODEL_FILE: &str = "kokoro_82m_v1.onnx";
const MODEL_REPO_FILE: &str = "onnx/model.onnx";

const HF_OWNER: &str = "onnx-community";
const HF_MODEL: &str = "Kokoro-82M-v1.0-ONNX";

async fn download_kokoro_model() -> HFResult<PathBuf> {
    let target_path = super::resolve_manifest_path(super::MODEL_DIR, MODEL_FILE);

    if fs::try_exists(&target_path).await? {
        println!(
            "Model already exists at {}, skipping download.",
            target_path.display()
        );
        return Ok(target_path);
    }

    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).await?;
    }

    let client = HFClient::new()?;
    let repo = client.model(HF_OWNER, HF_MODEL);

    let downloaded_path = repo
        .download_file()
        .filename(MODEL_REPO_FILE)
        .send()
        .await?;

    fs::copy(&downloaded_path, &target_path).await?;

    println!("Model successfully saved to: {}", target_path.display());

    Ok(target_path)
}

pub(super) async fn process_model() -> Result<(), Box<dyn Error>> {
    let input_path = download_kokoro_model().await?;
    let out_dir = super::resolve_manifest_path(super::MODEL_OUT_DIR, MODEL_FILE);

    println!("cargo:rerun-if-changed={}", input_path.display());

    let input = input_path.to_string_lossy().into_owned();
    let output = out_dir.to_string_lossy().into_owned();

    task::spawn_blocking(move || {
        ModelGen::new()
            .input(&input)
            .out_dir(output.trim_end_matches(".onnx"))
            .run_from_script();
    })
    .await?;

    Ok(())
}
