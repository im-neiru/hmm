use core::{error::Error, mem::size_of};

use std::{
    collections::BTreeSet,
    fs::File,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

use heck::ToTitleCase;
use hf_hub::HFClient;
use tokio::{fs, sync::Semaphore, task, task::JoinSet};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

use super::super::*;

#[path = "../../schema/voice_lookup.rs"]
mod lookup;

use lookup::{
    FORMAT_VERSION, VOICES_ARCHIVE_FILE, VOICES_LOOKUP_FILE, VoiceAccent, VoiceDataFormat,
    VoiceEntry, VoiceGender, VoiceLookup, VoiceModel,
};

const REPO_OWNER: &str = "hexgrad";
const REPO_NAME: &str = "Kokoro-82M";
const REPO_DIR: &str = "voices";

const VOICES_CACHE_DIR: &str = "voices";
const KOKORO_SOURCE_DIR: &str = "voices/kokoro";
const DOWNLOAD_DIR_NAME: &str = "voice-download";

const STYLE_WIDTH: usize = 256;
const MAX_FRAMES: usize = 510;
const MAX_WORKERS: usize = 4;
const ZSTD_LEVEL: i32 = 12;

struct ConvertedVoice {
    entry: VoiceEntry,
    compressed_data: Vec<u8>,
}

async fn download_kokoro_voices(staging_dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
    if fs::try_exists(staging_dir).await? {
        fs::remove_dir_all(staging_dir).await?;
    }

    fs::create_dir_all(staging_dir).await?;

    let client = HFClient::new()?;

    let downloaded_dir = client
        .model(REPO_OWNER, REPO_NAME)
        .snapshot_download()
        .allow_patterns(vec![
            format!("{REPO_DIR}/af_*.pt").into(),
            format!("{REPO_DIR}/am_*.pt").into(),
            format!("{REPO_DIR}/bf_*.pt").into(),
            format!("{REPO_DIR}/bm_*.pt").into(),
        ])
        .local_dir(staging_dir.to_path_buf())
        .max_workers(MAX_WORKERS)
        .send()
        .await?;

    Ok(downloaded_dir)
}

pub(crate) async fn process_voices() -> Result<(), Box<dyn Error>> {
    let out_dir = PathBuf::from(
        std::env::var_os("OUT_DIR")
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "OUT_DIR is not set"))?,
    );

    let voices_cache_dir = resolve_manifest_path(VOICES_CACHE_DIR, "");
    let source_dir = resolve_manifest_path(KOKORO_SOURCE_DIR, "");

    let archive_cache = voices_cache_dir.join(VOICES_ARCHIVE_FILE);
    let lookup_cache = voices_cache_dir.join(VOICES_LOOKUP_FILE);

    fs::create_dir_all(&voices_cache_dir).await?;
    fs::create_dir_all(&source_dir).await?;

    let staging_dir = out_dir.join(DOWNLOAD_DIR_NAME);
    let downloaded_dir = download_kokoro_voices(&staging_dir).await?;

    merge_downloaded_voices(&downloaded_dir, &source_dir).await?;

    if fs::try_exists(&downloaded_dir).await? {
        fs::remove_dir_all(&downloaded_dir).await?;
    }

    println!("cargo:rerun-if-changed={}", source_dir.display());
    println!(
        "cargo:rerun-if-changed={}",
        resolve_manifest_path("schema/voice_lookup.rs", "").display()
    );

    let sources = collect_voice_sources(&source_dir).await?;

    if sources.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "no American or British Kokoro voices found",
        )
        .into());
    }

    for source in &sources {
        println!("cargo:rerun-if-changed={}", source.display());
    }

    if voice_assets_are_stale(&sources, &archive_cache, &lookup_cache).await? {
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

        let mut converted = Vec::new();

        while let Some(result) = tasks.join_next().await {
            converted.push(result??);
        }

        converted.sort_unstable_by(|a, b| a.entry.id.cmp(&b.entry.id));

        let archive_output = archive_cache.clone();
        let lookup_output = lookup_cache.clone();

        task::spawn_blocking(move || {
            write_voice_assets(converted, &archive_output, &lookup_output)
        })
        .await??;

        println!("Updated voice archive cache: {}", archive_cache.display());
        println!("Updated voice lookup cache: {}", lookup_cache.display());
    }

    let target_dir = get_data_dir(&out_dir)?;

    fs::create_dir_all(&target_dir).await?;

    copy_if_updated(&archive_cache, &target_dir.join(VOICES_ARCHIVE_FILE)).await?;

    copy_if_updated(&lookup_cache, &target_dir.join(VOICES_LOOKUP_FILE)).await?;

    println!(
        "Voice archive ready: {}",
        target_dir.join(VOICES_ARCHIVE_FILE).display()
    );
    println!(
        "Voice lookup ready: {}",
        target_dir.join(VOICES_LOOKUP_FILE).display()
    );

    Ok(())
}

async fn merge_downloaded_voices(downloaded_dir: &Path, source_dir: &Path) -> io::Result<()> {
    fs::create_dir_all(source_dir).await?;

    let downloaded_voices_dir = downloaded_dir.join(REPO_DIR);
    let mut directory = fs::read_dir(&downloaded_voices_dir).await?;

    while let Some(entry) = directory.next_entry().await? {
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

        let Some(id) = source.file_stem().and_then(|name| name.to_str()) else {
            continue;
        };

        if voice_metadata(id).is_none() {
            continue;
        }

        let file_name = source.file_name().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid downloaded voice filename",
            )
        })?;

        let destination = source_dir.join(file_name);

        let unchanged = if fs::try_exists(&destination).await? {
            files_equal(&source, &destination).await?
        } else {
            false
        };

        if !unchanged {
            fs::copy(&source, &destination).await?;
        }
    }

    Ok(())
}

async fn files_equal(a: &Path, b: &Path) -> io::Result<bool> {
    let a_metadata = fs::metadata(a).await?;
    let b_metadata = fs::metadata(b).await?;

    if a_metadata.len() != b_metadata.len() {
        return Ok(false);
    }

    Ok(fs::read(a).await? == fs::read(b).await?)
}

async fn collect_voice_sources(source_dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut directory = fs::read_dir(source_dir).await?;
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

    Ok(sources)
}

async fn voice_assets_are_stale(
    sources: &[PathBuf],
    archive_path: &Path,
    lookup_path: &Path,
) -> io::Result<bool> {
    if !fs::try_exists(archive_path).await? || !fs::try_exists(lookup_path).await? {
        return Ok(true);
    }

    let lookup_bytes = fs::read(lookup_path).await?;

    let cached_lookup: VoiceLookup = match postcard::from_bytes(&lookup_bytes) {
        Ok(lookup) => lookup,
        Err(_) => return Ok(true),
    };

    if cached_lookup.format_version != FORMAT_VERSION {
        return Ok(true);
    }

    let cached_ids = cached_lookup
        .entries
        .iter()
        .filter_map(|entry| match &entry.model {
            VoiceModel::Kokoro { .. } => Some(entry.id.to_ascii_lowercase()),
        })
        .collect::<BTreeSet<_>>();

    let source_ids = sources
        .iter()
        .filter_map(|path| {
            path.file_stem()
                .and_then(|name| name.to_str())
                .map(str::to_ascii_lowercase)
        })
        .collect::<BTreeSet<_>>();

    if cached_ids != source_ids {
        return Ok(true);
    }

    let archive_modified = fs::metadata(archive_path).await?.modified()?;
    let lookup_modified = fs::metadata(lookup_path).await?.modified()?;
    let cache_modified = archive_modified.min(lookup_modified);

    for source in sources {
        let modified = fs::metadata(source).await?.modified()?;

        if modified > cache_modified {
            return Ok(true);
        }
    }

    Ok(false)
}

fn voice_metadata(id: &str) -> Option<(VoiceAccent, VoiceGender)> {
    match id.get(..3)? {
        "af_" => Some((VoiceAccent::EnglishAmerican, VoiceGender::Female)),
        "am_" => Some((VoiceAccent::EnglishAmerican, VoiceGender::Male)),
        "bf_" => Some((VoiceAccent::EnglishBritish, VoiceGender::Female)),
        "bm_" => Some((VoiceAccent::EnglishBritish, VoiceGender::Male)),
        _ => None,
    }
}

fn format_voice_name(id: &str) -> String {
    id.get(3..).unwrap_or(id).to_title_case()
}

fn convert_voice(path: &Path) -> io::Result<ConvertedVoice> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file).map_err(io::Error::other)?;

    let names = (0..archive.len())
        .map(|index| {
            let file = archive.by_index(index).map_err(io::Error::other)?;

            file.name()
                .map(|name| name.into_owned())
                .map_err(io::Error::other)
        })
        .collect::<io::Result<Vec<String>>>()?;

    let prefix = names
        .iter()
        .find_map(|name| {
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
            value.trim().to_owned()
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

    let mut normalized = Vec::with_capacity(bytes.len());

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

        normalized.extend_from_slice(&value.to_le_bytes());
    }

    let id = path
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid voice filename"))?;

    let (accent, gender) = voice_metadata(id)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "unsupported voice"))?;

    let compressed_data = zstd::bulk::compress(&normalized, ZSTD_LEVEL)?;

    Ok(ConvertedVoice {
        entry: VoiceEntry {
            id: id.to_owned(),
            name: format_voice_name(id),
            model: VoiceModel::Kokoro {
                accent,
                gender,
                frames: frames as u16,
            },
            archive_path: format!("kokoro/{id}.bin"),
            data_format: VoiceDataFormat::ZstdF32Le,
        },
        compressed_data,
    })
}

fn write_voice_assets(
    converted: Vec<ConvertedVoice>,
    archive_path: &Path,
    lookup_path: &Path,
) -> io::Result<()> {
    if let Some(parent) = archive_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    if let Some(parent) = lookup_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let archive_temp = archive_path.with_extension("zip.tmp");
    let lookup_temp = lookup_path.with_extension("postcard.tmp");

    let file = File::create(&archive_temp)?;
    let mut archive = ZipWriter::new(file);
    let mut entries = Vec::with_capacity(converted.len());

    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

    for converted_voice in converted {
        archive
            .start_file(converted_voice.entry.archive_path.clone(), options)
            .map_err(io::Error::other)?;

        archive.write_all(&converted_voice.compressed_data)?;
        entries.push(converted_voice.entry);
    }

    drop(archive.finish().map_err(io::Error::other)?);

    let lookup = VoiceLookup {
        format_version: FORMAT_VERSION,
        entries,
    };

    let serialized = postcard::to_allocvec(&lookup).map_err(io::Error::other)?;

    std::fs::write(&lookup_temp, serialized)?;

    replace_file(&archive_temp, archive_path)?;
    replace_file(&lookup_temp, lookup_path)?;

    Ok(())
}

fn replace_file(source: &Path, target: &Path) -> io::Result<()> {
    if target.exists() {
        std::fs::remove_file(target)?;
    }

    std::fs::rename(source, target)
}

async fn copy_if_updated(source: &Path, target: &Path) -> io::Result<()> {
    let source_metadata = fs::metadata(source).await?;

    let should_copy = match fs::metadata(target).await {
        Ok(target_metadata) => {
            let source_modified = source_metadata.modified()?;
            let target_modified = target_metadata.modified()?;

            source_metadata.len() != target_metadata.len() || source_modified > target_modified
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => true,
        Err(error) => return Err(error),
    };

    if !should_copy {
        return Ok(());
    }

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).await?;
    }

    fs::copy(source, target).await?;

    Ok(())
}
