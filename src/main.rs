use std::{convert::TryFrom, fs::File, io::Write, path::Path};

use tokio::runtime::Builder;

use hmm::{Audio, Inference, KokoroOptions, Options, VoiceEnumerator};

fn main() {
    let rt = Builder::new_multi_thread()
        .worker_threads(4)
        .thread_stack_size(1024 * 1024 * 32)
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        rt.spawn(run()).await.unwrap();
    });
}

async fn run() {
    let voices = VoiceEnumerator::open().await.unwrap();

    let entry = voices.find("kokoro", "af_jessica").unwrap();
    let voice = voices.load(entry).await.unwrap();


    let device = burn::tensor::Device::flex();

    let inference = Inference::new(device).await.unwrap();

    let audio = inference
        .synthesize(
            "Testing text to speech with Burn!",
            voice,
            Options::Kokoro(KokoroOptions { speed: 1.0}),
        )
        .await
        .unwrap();

    write_wav(Path::new("sample.wav"), &audio);
}

fn write_wav(path: &Path, audio: &Audio) {
    let bytes_per_sample = 2u16;
    let block_align = audio.channels.checked_mul(bytes_per_sample).unwrap();
    let byte_rate = audio
        .sample_rate
        .checked_mul(u32::from(block_align))
        .unwrap();

    let data_len = audio
        .samples
        .len()
        .checked_mul(usize::from(bytes_per_sample))
        .unwrap();

    let data_len = u32::try_from(data_len).unwrap();
    let riff_len = 36u32.checked_add(data_len).unwrap();

    let mut file = File::create(path).unwrap();

    file.write_all(b"RIFF").unwrap();
    file.write_all(&riff_len.to_le_bytes()).unwrap();
    file.write_all(b"WAVE").unwrap();

    file.write_all(b"fmt ").unwrap();
    file.write_all(&16u32.to_le_bytes()).unwrap();
    file.write_all(&1u16.to_le_bytes()).unwrap();
    file.write_all(&audio.channels.to_le_bytes()).unwrap();
    file.write_all(&audio.sample_rate.to_le_bytes()).unwrap();
    file.write_all(&byte_rate.to_le_bytes()).unwrap();
    file.write_all(&block_align.to_le_bytes()).unwrap();
    file.write_all(&16u16.to_le_bytes()).unwrap();

    file.write_all(b"data").unwrap();
    file.write_all(&data_len.to_le_bytes()).unwrap();

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

        file.write_all(&pcm.to_le_bytes()).unwrap();
    }
}
