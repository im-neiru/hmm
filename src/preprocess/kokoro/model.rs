use std::{error::Error, io, path::PathBuf};

use burn_onnx::ModelGen;
use hf_hub::{HFClient, HFResult};
use tokio::{fs, task};

use super::super::*;

const MODEL_FILE: &str = "kokoro_82m_v1.onnx";
const MODEL_REPO_FILE: &str = "onnx/model.onnx";

const HF_OWNER: &str = "onnx-community";
const HF_MODEL: &str = "Kokoro-82M-v1.0-ONNX";

async fn download_kokoro_model() -> HFResult<PathBuf> {
    let target_path = resolve_manifest_path(MODEL_DIR, MODEL_FILE);

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

pub(crate) async fn process_model() -> Result<(), Box<dyn Error>> {
    let input_path = download_kokoro_model().await?;
    let out_dir = resolve_manifest_path(MODEL_OUT_DIR, MODEL_FILE);

    println!("cargo:rerun-if-changed={}", input_path.display());

    let input = input_path.to_string_lossy().into_owned();
    let output = out_dir.with_extension("").to_string_lossy().into_owned();

    task::spawn_blocking(move || {
        ModelGen::new()
            .input(&input)
            .out_dir(&output)
            .run_from_script();
    })
    .await?;

    let model_name = out_dir
        .file_stem()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid model output path"))?;

    let source_bpk = out_dir
        .with_extension("")
        .join(model_name)
        .with_extension("bpk");

    if !fs::try_exists(&source_bpk).await? {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "generated model weights not found: {}",
                source_bpk.display()
            ),
        )
        .into());
    }

    let cargo_out_dir = PathBuf::from(
        std::env::var_os("OUT_DIR")
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "OUT_DIR is not set"))?,
    );

    let target_dir = get_data_dir(&cargo_out_dir)?;
    fs::create_dir_all(&target_dir).await?;

    let target_bpk = target_dir.join(source_bpk.file_name().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "invalid model weights filename")
    })?);

    fs::copy(&source_bpk, &target_bpk).await?;

    println!("Model weights ready: {}", target_bpk.display());

    Ok(())
}
