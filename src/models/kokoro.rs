#[path = "base/kokoro_82m_v1/kokoro_82m_v1.rs"]
mod kokoro_82m_v1;

use std::path::{Path, PathBuf};

use burn::prelude::*;
use loqui_g2p::{Dialect, G2p, OovFallback};
use serde::Deserialize;
use tokio::task;

use crate::{
    errors::InferenceError,
    inference::{Audio, InferenceModel},
    utils::FxHashMap,
};

use kokoro_82m_v1::Model;

const STYLE_WIDTH: usize = 256;
const MAX_PHONEMES: usize = 510;
const SAMPLE_RATE: u32 = 24_000;

const CONFIG_JSON: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/configs/kokoro.json"));

pub type Vocab = FxHashMap<char, i64>;

#[derive(Deserialize)]
struct Config {
    vocab: FxHashMap<String, usize>,
}

pub struct Voice {
    styles: Vec<f32>,
}

impl Voice {
    pub async fn load(path: impl AsRef<Path>) -> Result<Self, InferenceError> {
        let path: PathBuf = path.as_ref().to_path_buf();
        let bytes = tokio::fs::read(path).await?;

        task::spawn_blocking(move || {
            let stride = STYLE_WIDTH * size_of::<f32>();

            if bytes.is_empty() || bytes.len() % stride != 0 || bytes.len() / stride > MAX_PHONEMES
            {
                return Err(InferenceError::InvalidVoicePack);
            }

            let mut styles = Vec::with_capacity(bytes.len() / 4);

            for chunk in bytes.chunks_exact(4) {
                styles.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
            }

            Ok(Self { styles })
        })
        .await?
    }

    fn style(&self, phoneme_count: usize) -> Result<&[f32], InferenceError> {
        let index = phoneme_count
            .checked_sub(1)
            .ok_or(InferenceError::EmptyTokens)?;

        let start = index * STYLE_WIDTH;
        let end = start + STYLE_WIDTH;

        self.styles
            .get(start..end)
            .ok_or(InferenceError::MissingStyle(phoneme_count))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub speed: f32,
}

impl Default for Options {
    fn default() -> Self {
        Self { speed: 1.0 }
    }
}

pub struct Kokoro {
    model: Model,
    device: Device,
    phonemizer: G2p,
    vocab: Vocab,
}

impl Kokoro {
    pub async fn load() -> Result<Self, InferenceError> {
        let (model, device, phonemizer, vocab) = task::spawn_blocking(|| {
            let device = Device::default();
            let model: Model =
                Model::from_file("src/models/base/kokoro_82m_v1/kokoro_82m_v1.bpk", &device);

            let config: Config = serde_json::from_str(CONFIG_JSON)?;

            let mut vocab = Vocab::with_capacity_and_hasher(config.vocab.len(), Default::default());

            for (symbol, id) in config.vocab {
                let mut chars = symbol.chars();

                if let Some(ch) = chars.next() {
                    if chars.next().is_none() {
                        vocab.insert(ch, id as i64);
                    }
                }
            }

            let phonemizer = G2p::new(Dialect::American, OovFallback::Neural)?;

            Ok::<_, InferenceError>((model, device, phonemizer, vocab))
        })
        .await??;

        Ok(Self {
            model,
            device,
            phonemizer,
            vocab,
        })
    }

    fn phoneme_chunks(&self, text: &str) -> Result<Vec<String>, InferenceError> {
        let mut chunks = Vec::new();

        for line in text.lines().map(str::trim) {
            if line.is_empty() {
                continue;
            }

            let mut current = String::new();

            for token in self.phonemizer.tokens(line) {
                let phoneme_count = token.phonemes.chars().count();
                let current_count = current.trim_end().chars().count();

                let separator = usize::from(
                    phoneme_count != 0
                        && !current.trim().is_empty()
                        && current.chars().last().is_some_and(char::is_whitespace),
                );

                if current_count + separator + phoneme_count > MAX_PHONEMES {
                    let chunk = current.trim().to_owned();

                    if !chunk.is_empty() {
                        chunks.push(chunk);
                    }

                    current.clear();

                    if phoneme_count > MAX_PHONEMES {
                        return Err(InferenceError::SequenceTooLong(phoneme_count));
                    }
                }

                current.push_str(&token.phonemes);

                if token.whitespace {
                    current.push(' ');
                }
            }

            let chunk = current.trim().to_owned();

            if !chunk.is_empty() {
                chunks.push(chunk);
            }
        }

        if chunks.is_empty() {
            return Err(InferenceError::EmptyTokens);
        }

        Ok(chunks)
    }

    fn synthesize_chunk(
        &self,
        phonemes: &str,
        voice: &Voice,
        speed: f32,
    ) -> Result<Vec<f32>, InferenceError> {
        let phoneme_count = phonemes.chars().count();

        if phoneme_count == 0 {
            return Err(InferenceError::EmptyTokens);
        }

        if phoneme_count > MAX_PHONEMES {
            return Err(InferenceError::SequenceTooLong(phoneme_count));
        }

        let style_values = voice.style(phoneme_count)?;

        let mut tokens = Vec::with_capacity(phoneme_count + 2);
        tokens.push(0);

        tokens.extend(
            phonemes
                .chars()
                .filter_map(|ch| self.vocab.get(&ch).copied()),
        );

        if tokens.len() == 1 {
            return Err(InferenceError::EmptyTokens);
        }

        tokens.push(0);

        let seq_len = tokens.len();

        let tokens =
            Tensor::<1, Int>::from_ints(tokens.as_slice(), &self.device).reshape([1, seq_len]);

        let style = Tensor::<1>::from_floats(style_values, &self.device).reshape([1, STYLE_WIDTH]);

        let speed = Tensor::<1>::from_floats([speed].as_slice(), &self.device);

        let audio: Tensor<2> = self.model.forward(tokens, style, speed);

        let [batch, sample_count] = audio.dims();

        if batch != 1 {
            return Err(InferenceError::InvalidAudioShape(batch));
        }

        let samples: Vec<f32> = audio
            .reshape([sample_count])
            .to_data()
            .try_to_vec()
            .map_err(|error| InferenceError::AudioData(error.to_string()))?;

        Ok(samples)
    }
}

impl InferenceModel for Kokoro {
    type Voice = Voice;
    type Options = Options;

    fn synthesize(
        &self,
        text: &str,
        voice: &Self::Voice,
        options: &Self::Options,
    ) -> Result<Audio, InferenceError> {
        if !(0.25..=4.0).contains(&options.speed) {
            return Err(InferenceError::InvalidSpeed);
        }

        let chunks = self.phoneme_chunks(text)?;
        let mut samples = Vec::new();

        for phonemes in chunks {
            samples.extend(self.synthesize_chunk(&phonemes, voice, options.speed)?);
        }

        Ok(Audio::mono(samples, SAMPLE_RATE))
    }
}
