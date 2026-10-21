use hf_hub::{HFClient, HFResult};
use tokio::fs;

use super::super::*;

const REPO_OWNER: &str = "hexgrad";
const REPO_NAME: &str = "Kokoro-82M";
const REPO_DIR: &str = "voices";
const VOICES_DIR: &str = "voices/kokoro";

async fn download_kokoro_voices() -> HFResult<PathBuf> {
    let output_dir = resolve_manifest_path(VOICES_DIR, "");

    if fs::try_exists(&output_dir).await? {
        println!(
            "Voices already exist at {}, skipping download.",
            output_dir.display()
        );
        return Ok(output_dir);
    }

    fs::create_dir_all(&output_dir).await?;

    let client = HFClient::new()?;

    client
        .model(REPO_OWNER, REPO_NAME)
        .snapshot_download()
        .allow_patterns(vec![format!("{REPO_DIR}/*.pt").into()])
        .local_dir(output_dir)
        .max_workers(4)
        .send()
        .await
}
