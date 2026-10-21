use std::sync::Arc;

use burn::prelude::*;

use crate::{
    KokoroOptions,
    errors::InferenceError,
    models::{
        kokoro::Kokoro,
        voice_enumerator::{Voice, VoiceEnumerator},
    },
};

#[derive(Debug)]
pub struct Audio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

impl Audio {
    pub fn mono(samples: Vec<f32>, sample_rate: u32) -> Self {
        Self {
            samples,
            sample_rate,
            channels: 1,
        }
    }

    pub fn into_samples(self) -> Vec<f32> {
        self.samples
    }
}

pub(crate) trait InferenceModel: Send + Sync + 'static {
    type Options: Send + 'static;

    fn synthesize(
        &self,
        text: &str,
        voice: &Voice,
        options: &Self::Options,
    ) -> Result<Audio, InferenceError>;
}

pub enum Model {
    Kokoro(Kokoro),
}

pub enum Options {
    Kokoro(KokoroOptions),
}

pub struct Inference {
    model: Arc<Model>,
    voices: Arc<VoiceEnumerator>,
}

impl Inference {
    pub async fn new(device: Device) -> Result<Self, InferenceError> {
        let voices = VoiceEnumerator::open().await?;
        let model = Kokoro::load(device).await?;

        Ok(Self {
            model: Arc::new(Model::Kokoro(model)),
            voices: Arc::new(voices),
        })
    }

    pub fn voices(&self) -> &VoiceEnumerator {
        &self.voices
    }

    pub async fn load_voice(&self, model: &str, id: &str) -> Result<Voice, InferenceError> {
        self.voices.load_by_id(model, id).await
    }

    pub async fn synthesize(
        &self,
        text: impl Into<String>,
        voice: Voice,
        options: Options,
    ) -> Result<Audio, InferenceError> {
        let model = Arc::clone(&self.model);
        let text = text.into();

        match (&*model, options) {
            (Model::Kokoro(model), Options::Kokoro(options)) => {
                model.synthesize(&text, &voice, &options)
            }
        }
    }
}
