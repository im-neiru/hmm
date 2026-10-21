use std::{
    error::Error,
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::Arc,
};

use heck::ToTitleCase;
use hf_hub::{HFClient, HFResult};
use tokio::{fs, sync::Semaphore, task, task::JoinSet};
use zip::ZipArchive;

use super::super::*;

#[path = "../../schema/kokoro_voice.rs"]
mod schema;

use schema::{FORMAT_VERSION, STYLE_WIDTH, VoiceAccent, VoiceCollection, VoiceEntry, VoiceGender};

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

    if !fs::try_exists(&collection_path).await? {
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

            let Some(id) = path.file_stem().and_then(|name| name.to_str()) else {
                continue;
            };

            if voice_metadata(id).is_some() {
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

        while let Some(result) = tasks.join_next().await {
            voices.push(result??);
        }

        voices.sort_unstable_by(|a, b| {
            a.name
                .cmp(&b.name)
                .then_with(|| (a.accent as u8).cmp(&(b.accent as u8)))
                .then_with(|| (a.gender as u8).cmp(&(b.gender as u8)))
        });

        let collection = VoiceCollection {
            format_version: FORMAT_VERSION,
            voices,
        };

        let serialized = postcard::to_allocvec(&collection)?;
        let compressed = zstd::bulk::compress(&serialized, ZSTD_LEVEL)?;

        fs::create_dir_all(&cache_dir).await?;

        let temporary = collection_path.with_extension("data.tmp");
        fs::write(&temporary, compressed).await?;
        fs::rename(&temporary, &collection_path).await?;
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

fn voice_metadata(id: &str) -> Option<(VoiceAccent, VoiceGender)> {
    let prefix = id.get(..3)?;

    let metadata = match prefix {
        "af_" => (VoiceAccent::EnglishAmerican, VoiceGender::Female),
        "am_" => (VoiceAccent::EnglishAmerican, VoiceGender::Male),
        "bf_" => (VoiceAccent::EnglishBritish, VoiceGender::Female),
        "bm_" => (VoiceAccent::EnglishBritish, VoiceGender::Male),
        _ => return None,
    };

    Some(metadata)
}

fn format_voice_name(id: &str) -> String {
    id.get(3..).unwrap_or(id).to_title_case()
}

fn convert_voice(path: &Path) -> io::Result<VoiceEntry> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file).map_err(io::Error::other)?;

    let prefix = archive
        .file_names()
        .map(|name| name.map_err(io::Error::other))
        .collect::<io::Result<Vec<_>>>()?
        .iter()
        .find_map(|name| {
            let name = name.as_ref();

            if name == "data.pkl" {
                Some(String::new())
            } else {
                name.strip_suffix("/data.pkl").map(str::to_owned)
            }
        })
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing PyTorch data.pkl"))?;

    let member = |name: &str| {
        if prefix.is_empty() {
            name.to_owned()
        } else {
            format!("{prefix}/{name}")
        }
    };

    let byte_order = match archive.by_name(&member("byteorder")) {
        Ok(mut file) => {
            let mut value = String::new();
            file.read_to_string(&mut value)?;
            value
        }
        Err(_) => "little".to_owned(),
    };

    let storage_name = member("data/0");
    let mut bytes = Vec::new();

    archive
        .by_name(&storage_name)
        .map_err(io::Error::other)?
        .read_to_end(&mut bytes)?;

    let frame_bytes = STYLE_WIDTH * size_of::<f32>();

    if bytes.is_empty() || bytes.len() % frame_bytes != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid voice tensor byte length",
        ));
    }

    let frames = bytes.len() / frame_bytes;

    if frames == 0 || frames > MAX_FRAMES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid voice frame count: {frames}"),
        ));
    }

    let mut styles = Vec::with_capacity(frames * STYLE_WIDTH);

    for chunk in bytes.chunks_exact(4) {
        let raw: [u8; 4] = chunk.try_into().map_err(io::Error::other)?;

        let value = match byte_order.as_str() {
            "little" => f32::from_le_bytes(raw),
            "big" => f32::from_be_bytes(raw),
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unsupported PyTorch byte order",
                ));
            }
        };

        if !value.is_finite() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "voice contains non-finite values",
            ));
        }

        styles.push(value);
    }

    let id = path
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid voice filename"))?;

    let (accent, gender) = voice_metadata(id)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "unsupported voice"))?;

    Ok(VoiceEntry {
        name: format_voice_name(id),
        accent,
        gender,
        frames: frames as u16,
        styles,
    })
}
