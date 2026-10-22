use std::{
    io::{self, Read, Seek, SeekFrom},
    sync::{Arc, Mutex},
};

use mmap_io::MemoryMappedFile;
use tokio::{fs, task};
use zip::ZipArchive;

#[path = "../schema/voice_lookup.rs"]
mod lookup;

use crate::errors::InferenceError;

use super::{kokoro, resolve_data_path};

use lookup::{
    FORMAT_VERSION, VOICES_ARCHIVE_FILE, VOICES_LOOKUP_FILE, VoiceDataFormat, VoiceLookup,
};

pub use lookup::{VoiceAccent, VoiceEntry, VoiceGender, VoiceModel};

pub struct VoiceEnumerator {
    lookup: VoiceLookup,
    archive: Arc<Mutex<ZipArchive<MappedZipReader>>>,
}

impl VoiceEnumerator {
    pub async fn open() -> Result<Self, InferenceError> {
        let lookup_path = resolve_data_path(VOICES_LOOKUP_FILE)?;
        let archive_path = resolve_data_path(VOICES_ARCHIVE_FILE)?;

        let bytes = fs::read(lookup_path).await?;

        let lookup: VoiceLookup = postcard::from_bytes(&bytes).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid voice lookup: {error}"),
            )
        })?;

        if lookup.format_version != FORMAT_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "unsupported voice lookup version: {}",
                    lookup.format_version
                ),
            )
            .into());
        }

        let archive = task::spawn_blocking(move || -> io::Result<_> {
            let mapping = MemoryMappedFile::open_ro(archive_path).map_err(io::Error::other)?;

            ZipArchive::new(MappedZipReader::new(mapping)).map_err(io::Error::other)
        })
        .await??;

        Ok(Self {
            lookup,
            archive: Arc::new(Mutex::new(archive)),
        })
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = &VoiceEntry> + '_ {
        self.lookup.entries.iter()
    }

    pub fn find(&self, model: &str, id: &str) -> Option<&VoiceEntry> {
        self.lookup.entries.iter().find(|entry| {
            entry.id.eq_ignore_ascii_case(id)
                && entry
                    .archive_path
                    .split_once('/')
                    .is_some_and(|(slug, _)| slug.eq_ignore_ascii_case(model))
        })
    }

    pub fn find_by_name(&self, model: &str, name: &str) -> Option<&VoiceEntry> {
        self.lookup.entries.iter().find(|entry| {
            entry.name.eq_ignore_ascii_case(name)
                && entry
                    .archive_path
                    .split_once('/')
                    .is_some_and(|(slug, _)| slug.eq_ignore_ascii_case(model))
        })
    }

    pub async fn load(&self, entry: &VoiceEntry) -> Result<Voice, InferenceError> {
        if entry.data_format != VoiceDataFormat::ZstdF32Le {
            return Err(InferenceError::InvalidVoicePack);
        }

        let archive = Arc::clone(&self.archive);
        let path = entry.archive_path.clone();

        let bytes = task::spawn_blocking(move || -> io::Result<Vec<u8>> {
            let mut archive = archive
                .lock()
                .map_err(|_| io::Error::other("voice archive lock poisoned"))?;

            let mut file = archive.by_name(&path).map_err(io::Error::other)?;

            let capacity = usize::try_from(file.size()).map_err(io::Error::other)?;

            if capacity > MAX_VOICE_BYTES {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "compressed voice exceeds size limit",
                ));
            }

            let mut bytes = Vec::with_capacity(capacity);
            file.read_to_end(&mut bytes)?;

            Ok(bytes)
        })
        .await??;

        let voice = match &entry.model {
            VoiceModel::Kokoro { frames, .. } => {
                let voice = kokoro::Voice::load_owned(bytes).await?;

                let expected = usize::from(*frames)
                    .checked_mul(KOKORO_STYLE_WIDTH)
                    .ok_or(InferenceError::InvalidVoicePack)?;

                if voice.styles().len() != expected {
                    return Err(InferenceError::InvalidVoicePack);
                }

                Voice(VoiceInner::Kokoro(voice))
            }
        };

        Ok(voice)
    }

    pub async fn load_by_id(&self, model: &str, id: &str) -> Result<Voice, InferenceError> {
        let entry = self.find(model, id).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("voice not found: {model}/{id}"),
            )
        })?;

        self.load(entry).await
    }
}

const MAX_VOICE_BYTES: usize = 64 * 1024 * 1024;
const KOKORO_STYLE_WIDTH: usize = 256;

pub struct Voice(pub(crate) VoiceInner);

pub(crate) enum VoiceInner {
    Kokoro(kokoro::Voice),
}

impl Voice {
    pub(crate) fn as_kokoro(&self) -> &kokoro::Voice {
        match &self.0 {
            VoiceInner::Kokoro(voice) => voice,
        }
    }
}

struct MappedZipReader {
    mapping: MemoryMappedFile,
    position: u64,
}

impl MappedZipReader {
    fn new(mapping: MemoryMappedFile) -> Self {
        Self {
            mapping,
            position: 0,
        }
    }
}

impl Read for MappedZipReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() || self.position >= self.mapping.len() {
            return Ok(0);
        }

        let remaining = self.mapping.len() - self.position;
        let available = usize::try_from(remaining).unwrap_or(usize::MAX);
        let count = buffer.len().min(available);

        self.mapping
            .read_into(self.position, &mut buffer[..count])
            .map_err(io::Error::other)?;

        self.position += count as u64;

        Ok(count)
    }
}

impl Seek for MappedZipReader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let position = match position {
            SeekFrom::Start(position) => i128::from(position),
            SeekFrom::Current(offset) => i128::from(self.position) + i128::from(offset),
            SeekFrom::End(offset) => i128::from(self.mapping.len()) + i128::from(offset),
        };

        if !(0..=i128::from(u64::MAX)).contains(&position) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid ZIP archive seek position",
            ));
        }

        self.position = position as u64;

        Ok(self.position)
    }
}
