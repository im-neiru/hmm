use std::{error::Error, io, path::Path, sync::Arc};

use hf_hub::{HFClient, HFResult};
use pytorch_reader::{DType, PytorchReader};
use tokio::{fs, sync::Semaphore, task, task::JoinSet};

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

const STYLE_WIDTH: usize = 256;
const MAX_FRAMES: usize = 510;
const MAX_WORKERS: usize = 4;

pub(super) async fn process_voices() -> Result<(), Box<dyn Error>> {
    let output_dir = download_kokoro_voices().await?;
    let voices_dir = output_dir.join(REPO_DIR);

    let mut entries = fs::read_dir(&voices_dir).await?;
    let semaphore = Arc::new(Semaphore::new(MAX_WORKERS));
    let mut tasks = JoinSet::new();

    while let Some(entry) = entries.next_entry().await? {
        if !entry.file_type().await?.is_file() {
            continue;
        }

        let source = entry.path();

        if !source
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("pt"))
        {
            continue;
        }

        let is_english = source
            .file_stem()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with('a') || name.starts_with('b'));

        if !is_english {
            continue;
        }

        let target = source.with_extension("bin");

        if fs::try_exists(&target).await? {
            continue;
        }

        let semaphore = Arc::clone(&semaphore);

        tasks.spawn(async move {
            let _permit = semaphore.acquire_owned().await.map_err(io::Error::other)?;

            let bytes = task::spawn_blocking(move || convert_voice(&source))
                .await
                .map_err(io::Error::other)??;

            let temporary = target.with_extension("bin.tmp");
            fs::write(&temporary, bytes).await?;
            fs::rename(&temporary, &target).await?;

            println!("Converted voice: {}", target.display());
            Ok::<(), io::Error>(())
        });
    }

    while let Some(result) = tasks.join_next().await {
        result??;
    }

    Ok(())
}

fn convert_voice(path: &Path) -> io::Result<Vec<u8>> {
    let reader = PytorchReader::new(path).map_err(io::Error::other)?;

    if reader.len() != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("expected one voice tensor in {}", path.display()),
        ));
    }

    let tensor = reader
        .tensors()
        .values()
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing voice tensor"))?;

    if tensor.dtype() != DType::F32 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("expected f32 voice tensor in {}", path.display()),
        ));
    }

    let frames = match tensor.shape() {
        [frames, 1, STYLE_WIDTH] | [frames, STYLE_WIDTH] => *frames,
        shape => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid voice shape: {shape:?}"),
            ));
        }
    };

    if frames == 0 || frames > MAX_FRAMES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid voice frame count: {frames}"),
        ));
    }

    let bytes = tensor.read().map_err(io::Error::other)?;
    let expected = frames * STYLE_WIDTH * size_of::<f32>();

    if bytes.len() != expected {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid voice tensor byte length",
        ));
    }

    let mut output = Vec::with_capacity(expected);

    for chunk in bytes.chunks_exact(4) {
        let value = f32::from_ne_bytes(chunk.try_into().unwrap());

        if !value.is_finite() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "voice contains non-finite values",
            ));
        }

        output.extend_from_slice(&value.to_le_bytes());
    }

    Ok(output)
}
