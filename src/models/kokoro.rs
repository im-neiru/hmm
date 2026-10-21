#[path = "base/kokoro_82m_v1/kokoro_82m_v1.rs"]
mod kokoro_82m_v1;

use std::path::Path;

use burn::prelude::*;
use loqui_g2p::{Dialect, G2p, OovFallback};
use serde::Deserialize;
use tokio::fs;

use crate::{errors::InferenceError, utils::FxHashMap};

use kokoro_82m_v1::Model;

const STYLE_WIDTH: usize = 256;
const MAX_TOKENS: usize = 510;

pub(crate) type Vocab = FxHashMap<char, i64>;

#[derive(Deserialize)]
struct Config {
    vocab: FxHashMap<String, usize>,
}

pub(crate) struct Voice {
    styles: Vec<f32>,
}

impl Voice {
    pub(crate) async fn load(path: impl AsRef<Path>) -> Result<Self, InferenceError> {
        let bytes = fs::read(path).await?;

        if bytes.is_empty() || bytes.len() % (STYLE_WIDTH * 4) != 0 {
            return Err(InferenceError::InvalidVoicePack);
        }

        let mut styles = Vec::with_capacity(bytes.len() / 4);

        for chunk in bytes.chunks_exact(4) {
            styles.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
        }

        Ok(Self { styles })
    }

    fn style(&self, token_count: usize) -> Result<&[f32], InferenceError> {
        let start = token_count * STYLE_WIDTH;
        let end = start + STYLE_WIDTH;

        self.styles
            .get(start..end)
            .ok_or(InferenceError::MissingStyle(token_count))
    }
}

pub(crate) fn load_vocab() -> Result<Vocab, InferenceError> {
    let config: Config = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/configs/kokoro.json"
    )))?;

    let mut vocab = Vocab::with_capacity_and_hasher(config.vocab.len(), Default::default());

    for (symbol, id) in config.vocab {
        let mut chars = symbol.chars();

        if let Some(ch) = chars.next() {
            if chars.next().is_none() {
                vocab.insert(ch, id as i64);
            }
        }
    }

    Ok(vocab)
}

pub(crate) fn load_phonemizer() -> Result<G2p, InferenceError> {
    Ok(G2p::new(Dialect::American, OovFallback::Neural)?)
}

pub(crate) fn tokenize(
    phonemizer: &G2p,
    vocab: &Vocab,
    text: &str,
) -> Result<Vec<i64>, InferenceError> {
    let phonemes = phonemizer.phonemize(text);
    let mut tokens = Vec::with_capacity(phonemes.len() + 2);
    tokens.push(0);

    for ch in phonemes.chars() {
        if let Some(&id) = vocab.get(&ch) {
            tokens.push(id);
        }
    }

    let token_count = tokens.len() - 1;

    if token_count == 0 {
        return Err(InferenceError::EmptyTokens);
    }

    if token_count > MAX_TOKENS {
        return Err(InferenceError::SequenceTooLong(token_count));
    }

    tokens.push(0);

    Ok(tokens)
}

pub(crate) fn run(
    model: &Model,
    device: &Device,
    voice: &Voice,
    tokens: &[i64],
    speed: f32,
) -> Result<Vec<f32>, InferenceError> {
    if tokens.len() < 3 || tokens.len() > MAX_TOKENS + 2 {
        return Err(InferenceError::SequenceTooLong(tokens.len()));
    }

    if !speed.is_finite() || speed <= 0.0 {
        return Err(InferenceError::InvalidSpeed);
    }

    let style_values = voice.style(tokens.len() - 2)?;
    let seq_len = tokens.len();

    let tokens = Tensor::<1, Int>::from_ints(tokens, device).reshape([1, seq_len]);

    let style = Tensor::<1>::from_floats(style_values, device).reshape([1, STYLE_WIDTH]);

    let speed = Tensor::<1>::from_floats([speed].as_slice(), device);

    let audio: Tensor<2> = model.forward(tokens, style, speed);
    let [batch, samples] = audio.dims();

    debug_assert_eq!(batch, 1);

    audio
        .reshape([samples])
        .to_data()
        .try_to_vec()
        .map_err(|error| InferenceError::AudioData(error.to_string()))
}
