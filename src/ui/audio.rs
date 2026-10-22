use std::{
    env,
    fs::{self, File},
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use hmm::{Audio, Inference, KokoroOptions, Options, VoiceEntry, VoiceEnumerator, VoiceModel};
use tokio::task;

use super::{Scope, Shared, UiError, describe, join_error};

pub(super) struct SynthesisJob {
    pub id: u64,
    pub engine: Shared<Inference>,
    pub voices: Shared<VoiceEnumerator>,
    pub entry: VoiceEntry,
    pub voice_name: String,
    pub text: String,
    pub speed: f32,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub(super) struct Generated {
    pub path: PathBuf,
    pub seconds: f32,
    pub elapsed: Duration,
}

pub(super) fn default_done_dir() -> PathBuf {
    dirs::cache_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(env::temp_dir)
        .join("HMM")
        .join("done")
}

pub(super) fn timestamped_wav_path(directory: &Path) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    directory.join(format!(
        "speech-{}-{:09}.wav",
        timestamp.as_secs(),
        timestamp.subsec_nanos(),
    ))
}

pub(super) fn list_wav_files(directory: &Path) -> io::Result<Vec<PathBuf>> {
    fs::create_dir_all(directory)?;

    let mut paths = fs::read_dir(directory)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("wav"))
        })
        .collect::<Vec<_>>();

    paths.sort_by(|left, right| right.file_name().cmp(&left.file_name()));

    Ok(paths)
}

pub(super) fn delete_file(path: &Path) -> io::Result<()> {
    fs::remove_file(path)
}

pub(super) fn play_audio(_path: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "audio playback is not implemented yet",
    ))
}

pub(super) fn open_parent_folder(path: &Path) -> io::Result<()> {
    let folder = path.parent().unwrap_or(path);

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("explorer.exe");
        command.arg(folder);
        command
    };

    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(folder);
        command
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(folder);
        command
    };

    command.spawn().map(|_| ())
}

pub(super) async fn run_job(job: SynthesisJob) -> Result<Generated, UiError> {
    let started = Instant::now();
    let parent = job.path.parent().map(Path::to_path_buf).unwrap_or_default();

    fs::create_dir_all(&parent).map_err(|error| {
        UiError::new(
            Scope::Output,
            "Couldn't create the output folder",
            format!("{}: {}", parent.display(), describe(&error)),
        )
    })?;

    let voice = job.voices.load(&job.entry).await.map_err(|error| {
        UiError::from_error(Scope::Synthesis, "Couldn't load the voice", &error)
    })?;
    let options = match &job.entry.model {
        VoiceModel::Kokoro { .. } => Options::Kokoro(KokoroOptions { speed: job.speed }),
    };

    let audio = job
        .engine
        .synthesize(job.text.as_str(), voice, options)
        .await
        .map_err(|error| {
            UiError::from_error(Scope::Synthesis, "Speech synthesis failed", &error)
        })?;

    let seconds = audio.samples.len() as f32
        / (f32::from(audio.channels.max(1)) * audio.sample_rate.max(1) as f32);
    let path = job.path.clone();

    match task::spawn_blocking(move || write_wav(&path, &audio)).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            return Err(UiError::new(
                Scope::Output,
                "Couldn't save the audio",
                format!(
                    "{}: {}\nCheck that the folder exists and that you can write to it.",
                    job.path.display(),
                    describe(&error),
                ),
            ));
        }
        Err(error) => return Err(join_error(Scope::Output, error)),
    }

    Ok(Generated {
        path: job.path,
        seconds,
        elapsed: started.elapsed(),
    })
}

fn write_wav(path: &Path, audio: &Audio) -> io::Result<()> {
    const BYTES_PER_SAMPLE: u16 = 2;

    let too_large = || {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "audio is too large for a WAV file",
        )
    };
    let block_align = audio
        .channels
        .checked_mul(BYTES_PER_SAMPLE)
        .ok_or_else(too_large)?;
    let byte_rate = audio
        .sample_rate
        .checked_mul(u32::from(block_align))
        .ok_or_else(too_large)?;
    let data_len = audio
        .samples
        .len()
        .checked_mul(usize::from(BYTES_PER_SAMPLE))
        .ok_or_else(too_large)?;
    let data_len = u32::try_from(data_len).map_err(|_| too_large())?;
    let riff_len = 36u32.checked_add(data_len).ok_or_else(too_large)?;
    let mut file = BufWriter::new(File::create(path)?);

    file.write_all(b"RIFF")?;
    file.write_all(&riff_len.to_le_bytes())?;
    file.write_all(b"WAVE")?;
    file.write_all(b"fmt ")?;
    file.write_all(&16u32.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&audio.channels.to_le_bytes())?;
    file.write_all(&audio.sample_rate.to_le_bytes())?;
    file.write_all(&byte_rate.to_le_bytes())?;
    file.write_all(&block_align.to_le_bytes())?;
    file.write_all(&16u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&data_len.to_le_bytes())?;

    for sample in &audio.samples {
        let sample = if sample.is_finite() {
            sample.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        let pcm = if sample <= -1.0 {
            i16::MIN
        } else {
            (sample * f32::from(i16::MAX)) as i16
        };

        file.write_all(&pcm.to_le_bytes())?;
    }

    file.flush()
}
