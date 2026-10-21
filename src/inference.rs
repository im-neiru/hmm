use std::sync::Arc;

use crate::errors::InferenceError;

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

pub trait InferenceModel: Send + Sync + 'static {
    type Voice: Send + Sync + 'static;
    type Options: Send + 'static;

    fn synthesize(
        &self,
        text: &str,
        voice: &Self::Voice,
        options: &Self::Options,
    ) -> Result<Audio, InferenceError>;
}

pub struct Inference<M: InferenceModel> {
    model: Arc<M>,
}

impl<M: InferenceModel> Inference<M> {
    pub fn new(model: M) -> Self {
        Self {
            model: Arc::new(model),
        }
    }

    pub async fn synthesize(
        &self,
        text: impl Into<String>,
        voice: Arc<M::Voice>,
        options: M::Options,
    ) -> Result<Audio, InferenceError> {
        let model = Arc::clone(&self.model);
        let text = text.into();

        tokio::task::spawn_blocking(move || model.synthesize(&text, &voice, &options)).await?
    }
}
