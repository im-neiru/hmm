use core::{
    fmt::Display,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::{
    io::{self, Read, Seek, SeekFrom},
    path::Path,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use mmap_io::MemoryMappedFile;
use symphonia::{
    core::{
        codecs::audio::AudioDecoderOptions,
        errors::Error as SymphoniaError,
        formats::{FormatOptions, TrackType, probe::Hint},
        io::{MediaSource, MediaSourceStream},
        meta::MetadataOptions,
    },
    default::{get_codecs, get_probe},
};

const CALLBACKS_AFTER_END: usize = 3;
const POLL_INTERVAL: Duration = Duration::from_millis(8);

struct DecodedAudio {
    samples: Vec<f32>,
    sample_rate: u32,
    channels: usize,
}

struct MappedSource<R> {
    reader: R,
    len: u64,
}

impl<R: Read> Read for MappedSource<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        self.reader.read(output)
    }
}

impl<R: Seek> Seek for MappedSource<R> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.reader.seek(position)
    }
}

impl<R> MediaSource for MappedSource<R>
where
    R: Read + Seek + Send + Sync,
{
    fn is_seekable(&self) -> bool {
        true
    }

    fn byte_len(&self) -> Option<u64> {
        Some(self.len)
    }
}

pub(super) fn play(path: &Path, stop: Arc<AtomicBool>) -> io::Result<()> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| io_error("No default audio output device is available"))?;
    let supported = device.default_output_config().map_err(io_error)?;
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();

    let decoded = decode_wav(path)?;
    let samples = Arc::new(resample(
        &decoded.samples,
        decoded.channels,
        decoded.sample_rate,
        usize::from(config.channels),
        config.sample_rate,
    )?);

    let finished = Arc::new(AtomicBool::new(false));
    let trailing_callbacks = Arc::new(AtomicUsize::new(0));
    let callback_error = Arc::new(Mutex::new(None));

    let stream = match sample_format {
        SampleFormat::F32 => build_stream::<f32>(
            &device,
            config.clone(),
            Arc::clone(&samples),
            Arc::clone(&stop),
            Arc::clone(&finished),
            Arc::clone(&trailing_callbacks),
            Arc::clone(&callback_error),
        ),
        SampleFormat::F64 => build_stream::<f64>(
            &device,
            config.clone(),
            Arc::clone(&samples),
            Arc::clone(&stop),
            Arc::clone(&finished),
            Arc::clone(&trailing_callbacks),
            Arc::clone(&callback_error),
        ),
        SampleFormat::I8 => build_stream::<i8>(
            &device,
            config.clone(),
            Arc::clone(&samples),
            Arc::clone(&stop),
            Arc::clone(&finished),
            Arc::clone(&trailing_callbacks),
            Arc::clone(&callback_error),
        ),
        SampleFormat::I16 => build_stream::<i16>(
            &device,
            config.clone(),
            Arc::clone(&samples),
            Arc::clone(&stop),
            Arc::clone(&finished),
            Arc::clone(&trailing_callbacks),
            Arc::clone(&callback_error),
        ),
        SampleFormat::I24 => build_stream::<cpal::I24>(
            &device,
            config.clone(),
            Arc::clone(&samples),
            Arc::clone(&stop),
            Arc::clone(&finished),
            Arc::clone(&trailing_callbacks),
            Arc::clone(&callback_error),
        ),
        SampleFormat::I32 => build_stream::<i32>(
            &device,
            config.clone(),
            Arc::clone(&samples),
            Arc::clone(&stop),
            Arc::clone(&finished),
            Arc::clone(&trailing_callbacks),
            Arc::clone(&callback_error),
        ),
        SampleFormat::I64 => build_stream::<i64>(
            &device,
            config.clone(),
            Arc::clone(&samples),
            Arc::clone(&stop),
            Arc::clone(&finished),
            Arc::clone(&trailing_callbacks),
            Arc::clone(&callback_error),
        ),
        SampleFormat::U8 => build_stream::<u8>(
            &device,
            config.clone(),
            Arc::clone(&samples),
            Arc::clone(&stop),
            Arc::clone(&finished),
            Arc::clone(&trailing_callbacks),
            Arc::clone(&callback_error),
        ),
        SampleFormat::U16 => build_stream::<u16>(
            &device,
            config.clone(),
            Arc::clone(&samples),
            Arc::clone(&stop),
            Arc::clone(&finished),
            Arc::clone(&trailing_callbacks),
            Arc::clone(&callback_error),
        ),
        SampleFormat::U24 => build_stream::<cpal::U24>(
            &device,
            config.clone(),
            Arc::clone(&samples),
            Arc::clone(&stop),
            Arc::clone(&finished),
            Arc::clone(&trailing_callbacks),
            Arc::clone(&callback_error),
        ),
        SampleFormat::U32 => build_stream::<u32>(
            &device,
            config.clone(),
            Arc::clone(&samples),
            Arc::clone(&stop),
            Arc::clone(&finished),
            Arc::clone(&trailing_callbacks),
            Arc::clone(&callback_error),
        ),
        SampleFormat::U64 => build_stream::<u64>(
            &device,
            config.clone(),
            Arc::clone(&samples),
            Arc::clone(&stop),
            Arc::clone(&finished),
            Arc::clone(&trailing_callbacks),
            Arc::clone(&callback_error),
        ),
        unsupported => {
            return Err(io_error(format!(
                "The default output device uses an unsupported sample format: {unsupported}"
            )));
        }
    }
    .map_err(io_error)?;

    stream.play().map_err(io_error)?;

    loop {
        let stream_error = match callback_error.lock() {
            Ok(mut error) => error.take(),
            Err(_) => None,
        };

        if let Some(error) = stream_error {
            return Err(io_error(error));
        }

        if finished.load(Ordering::Acquire)
            && trailing_callbacks.load(Ordering::Acquire) >= CALLBACKS_AFTER_END
        {
            break;
        }

        thread::sleep(POLL_INTERVAL);
    }

    Ok(())
}

fn build_stream<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    samples: Arc<Vec<f32>>,
    stop: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
    trailing_callbacks: Arc<AtomicUsize>,
    callback_error: Arc<Mutex<Option<String>>>,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample + FromSample<f32>,
{
    let sample_count = samples.len();
    let mut cursor = 0_usize;

    device.build_output_stream::<T, _, _>(
        config,
        move |output: &mut [T], _| {
            if stop.load(Ordering::Relaxed) || cursor >= sample_count {
                for sample in output {
                    *sample = T::from_sample(0.0);
                }

                finished.store(true, Ordering::Release);
                trailing_callbacks.fetch_add(1, Ordering::Release);
                return;
            }

            let end = cursor.saturating_add(output.len()).min(sample_count);
            let consumed = end - cursor;

            for (target, source) in output.iter_mut().zip(samples[cursor..end].iter().copied()) {
                *target = T::from_sample(source);
            }

            for target in output.iter_mut().skip(consumed) {
                *target = T::from_sample(0.0);
            }

            cursor = end;

            if cursor == sample_count {
                finished.store(true, Ordering::Release);
            }
        },
        move |error| {
            if let Ok(mut slot) = callback_error.lock() {
                *slot = Some(error.to_string());
            }
        },
        None,
    )
}

fn decode_wav(path: &Path) -> io::Result<DecodedAudio> {
    let mapping = MemoryMappedFile::open_ro(path).map_err(io_error)?;
    let source = MappedSource {
        reader: mapping.reader(),
        len: mapping.len(),
    };
    let stream = MediaSourceStream::new(Box::new(source), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("wav");

    let mut format = get_probe()
        .probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(io_error)?;

    let (track_id, mut decoder) = {
        let track = format
            .default_track(TrackType::Audio)
            .ok_or_else(|| invalid_data("The WAV file does not contain an audio track"))?;
        let parameters = track
            .codec_params
            .as_ref()
            .ok_or_else(|| invalid_data("The WAV file is missing codec parameters"))?
            .audio()
            .ok_or_else(|| invalid_data("The WAV track has no audio codec parameters"))?;
        let decoder = get_codecs()
            .make_audio_decoder(parameters, &AudioDecoderOptions::default())
            .map_err(io_error)?;

        (track.id, decoder)
    };

    let mut samples = Vec::new();
    let mut sample_rate = None;
    let mut channels = None;

    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            Err(SymphoniaError::ResetRequired) => {
                return Err(invalid_data("The WAV stream requires a decoder reset"));
            }
            Err(error) => return Err(io_error(error)),
        };

        if packet.track_id != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(error) => return Err(io_error(error)),
        };

        let spec = decoded.spec();
        let current_channels = spec.channels().count();

        match (sample_rate, channels) {
            (Some(rate), Some(count)) if rate != spec.rate() || count != current_channels => {
                return Err(invalid_data(
                    "The WAV stream changes audio format during playback",
                ));
            }
            _ => {
                sample_rate = Some(spec.rate());
                channels = Some(current_channels);
            }
        }

        let sample_count = decoded.samples_interleaved();
        let sample_start = samples.len();
        let sample_end = sample_start
            .checked_add(sample_count)
            .ok_or_else(|| invalid_data("The decoded audio is too large"))?;

        samples.resize(sample_end, 0.0);
        decoded.copy_to_slice_interleaved(&mut samples[sample_start..sample_end]);
    }

    let sample_rate = sample_rate.ok_or_else(|| invalid_data("The WAV file contains no audio"))?;
    let channels = channels.ok_or_else(|| invalid_data("The WAV file contains no channels"))?;

    if samples.is_empty() {
        return Err(invalid_data("The WAV file contains no decodable samples"));
    }

    for sample in &mut samples {
        let value: f32 = *sample;
        *sample = if value.is_finite() {
            value.clamp(-1.0, 1.0)
        } else {
            0.0
        };
    }

    Ok(DecodedAudio {
        samples,
        sample_rate,
        channels,
    })
}

fn resample(
    source: &[f32],
    source_channels: usize,
    source_rate: u32,
    output_channels: usize,
    output_rate: u32,
) -> io::Result<Vec<f32>> {
    if source_channels == 0 || output_channels == 0 || source_rate == 0 || output_rate == 0 {
        return Err(invalid_data("The WAV file has invalid audio parameters"));
    }

    let source_frames = source.len() / source_channels;

    if source_frames == 0 {
        return Err(invalid_data(
            "The WAV file contains no complete audio frames",
        ));
    }

    if source_channels == output_channels && source_rate == output_rate {
        return Ok(source.to_vec());
    }

    let scaled_frames = (source_frames as u128) * u128::from(output_rate);
    let output_frames = (scaled_frames + u128::from(source_rate) / 2) / u128::from(source_rate);

    let output_frames = usize::try_from(output_frames)
        .map_err(|_| invalid_data("The resampled audio is too large"))?
        .max(1);
    let output_len = output_frames
        .checked_mul(output_channels)
        .ok_or_else(|| invalid_data("The resampled audio is too large"))?;

    let mut output = Vec::with_capacity(output_len);
    let scale = f64::from(source_rate) / f64::from(output_rate);

    for output_frame in 0..output_frames {
        let position = output_frame as f64 * scale;
        let left = (position as usize).min(source_frames - 1);
        let right = (left + 1).min(source_frames - 1);
        let fraction = (position - left as f64).clamp(0.0, 1.0) as f32;

        for output_channel in 0..output_channels {
            let left_sample = channel_sample(
                source,
                left,
                source_channels,
                output_channel,
                output_channels,
            );
            let right_sample = channel_sample(
                source,
                right,
                source_channels,
                output_channel,
                output_channels,
            );

            output.push(left_sample + (right_sample - left_sample) * fraction);
        }
    }

    Ok(output)
}

fn channel_sample(
    samples: &[f32],
    frame: usize,
    source_channels: usize,
    output_channel: usize,
    output_channels: usize,
) -> f32 {
    let offset = frame * source_channels;

    if output_channels == 1 {
        let mut sum = 0.0_f32;

        for sample in &samples[offset..offset + source_channels] {
            sum += *sample;
        }

        return sum / source_channels as f32;
    }

    let source_channel = if source_channels == 1 {
        0
    } else {
        output_channel.min(source_channels - 1)
    };

    samples[offset + source_channel]
}

fn io_error(error: impl Display) -> io::Error {
    io::Error::new(io::ErrorKind::Other, error.to_string())
}

fn invalid_data(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
