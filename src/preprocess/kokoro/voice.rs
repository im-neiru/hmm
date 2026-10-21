use std::{
    error::Error,
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use hf_hub::{HFClient, HFResult};
use pytorch_reader::{DType, PytorchReader};
use tokio::{fs, sync::Semaphore, task, task::JoinSet};

use super::super::*;

#[path = "../../schema/kokoro_voice.rs"]
mod schema;

use schema::{FORMAT_VERSION, STYLE_WIDTH, VoiceAccent, VoiceCollection, VoiceEntry};

const REPO_OWNER: &str = "hexgrad";
const REPO_NAME: &str = "Kokoro-82M";
const REPO_DIR: &str = "voices";
const VOICES_DIR: &str = "voices/kokoro";

const COLLECTION_FILE: &str = "collection.data";
const BUNDLE_RELATIVE_PATH: &str = "voices/kokoro.bin";

const MAX_FRAMES: usize = 510;
const MAX_WORKERS: usize = 4;
const ZSTD_LEVEL: i32 = 12;

async fn download_kokoro_voices() -> HFResult<PathBuf> {
    let output_dir = resolve_manifest_path(VOICES_DIR, "");

    fs::create_dir_all(&output_dir).await?;

    let client = HFClient::new()?;

    client
        .model(REPO_OWNER, REPO_NAME)
        .snapshot_download()
        .allow_patterns(vec![
            format!("{REPO_DIR}/af_*.pt").into(),
            format!("{REPO_DIR}/am_*.pt").into(),
            format!("{REPO_DIR}/bf_*.pt").into(),
            format!("{REPO_DIR}/bm_*.pt").into(),
        ])
        .local_dir(output_dir)
        .max_workers(MAX_WORKERS)
        .send()
        .await
}

pub(crate) async fn process_voices() -> Result<(), Box<dyn Error>> {
    let cache_dir = resolve_manifest_path(VOICES_DIR, "");
    let collection_path = cache_dir.join(COLLECTION_FILE);

    if fs::try_exists(&collection_path).await? {
        println!(
            "Using cached voice collection: {}",
            collection_path.display()
        );
    } else {
        let repo_path = download_kokoro_voices().await?;
        let source_dir = repo_path.join(REPO_DIR);

        let mut directory = fs::read_dir(&source_dir).await?;
        let mut sources = Vec::new();

        while let Some(entry) = directory.next_entry().await? {
            if !entry.file_type().await?.is_file() {
                continue;
            }

            let path = entry.path();

            if !path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("pt"))
            {
                continue;
            }

            let Some(name) = path.file_stem().and_then(|name| name.to_str()) else {
                continue;
            };

            if voice_accent(name).is_some() {
                sources.push(path);
            }
        }

        sources.sort_unstable();

        if sources.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "no American or British Kokoro voices found",
            )
            .into());
        }

        let semaphore = Arc::new(Semaphore::new(MAX_WORKERS));
        let mut tasks = JoinSet::new();

        for source in sources {
            let semaphore = Arc::clone(&semaphore);

            tasks.spawn(async move {
                let _permit = semaphore.acquire_owned().await.map_err(io::Error::other)?;

                task::spawn_blocking(move || convert_voice(&source))
                    .await
                    .map_err(io::Error::other)?
            });
        }

        let mut voices = Vec::new();
        let mut first_error = None;

        while let Some(result) = tasks.join_next().await {
            match result {
                Ok(Ok(voice)) => voices.push(voice),
                Ok(Err(error)) => {
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
                Err(error) => {
                    if first_error.is_none() {
                        first_error = Some(io::Error::other(error));
                    }
                }
            }
        }

        if let Some(error) = first_error {
            return Err(Box::new(error));
        }

        voices.sort_unstable_by(|a, b| a.name.cmp(&b.name));

        let collection = VoiceCollection {
            format_version: FORMAT_VERSION,
            voices,
        };

        let serialized = postcard::to_allocvec(&collection).map_err(io::Error::other)?;

        let compressed = zstd::bulk::compress(&serialized, ZSTD_LEVEL)?;

        fs::create_dir_all(&cache_dir).await?;

        let temporary = collection_path.with_extension("data.tmp");

        fs::write(&temporary, compressed).await?;
        fs::rename(&temporary, &collection_path).await?;

        println!("Created voice collection: {}", collection_path.display());
    }

    let out_dir = PathBuf::from(
        std::env::var_os("OUT_DIR")
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "OUT_DIR is not set"))?,
    );

    let bundle_path = out_dir.join(BUNDLE_RELATIVE_PATH);

    if let Some(parent) = bundle_path.parent() {
        fs::create_dir_all(parent).await?;
    }

    fs::copy(&collection_path, &bundle_path).await?;

    println!("cargo:rerun-if-changed={}", collection_path.display());
    println!("Voice bundle ready: {}", bundle_path.display());

    Ok(())
}

fn voice_accent(name: &str) -> Option<VoiceAccent> {
    if name.starts_with("af_") || name.starts_with("am_") {
        Some(VoiceAccent::EnglishAmerican)
    } else if name.starts_with("bf_") || name.starts_with("bm_") {
        Some(VoiceAccent::EnglishBritish)
    } else {
        None
    }
}

fn convert_voice(path: &Path) -> io::Result<VoiceEntry> {
    let reader = PytorchReader::new(path).map_err(io::Error::other)?;

    if reader.len() != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("expected one tensor in {}", path.display()),
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
            format!("expected f32 tensor in {}", path.display()),
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
            format!("invalid frame count: {frames}"),
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

    let mut styles = Vec::with_capacity(frames * STYLE_WIDTH);

    for chunk in bytes.chunks_exact(4) {
        let value = f32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);

        if !value.is_finite() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "voice contains non-finite values",
            ));
        }

        styles.push(value);
    }

    let name = path
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid voice name"))?
        .to_owned();

    let accent = voice_accent(&name)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "unsupported voice accent"))?;

    Ok(VoiceEntry {
        name,
        accent,
        frames: frames as u16,
        styles,
    })
}
