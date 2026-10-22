use miette::Diagnostic;
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
pub enum InferenceError {
    #[error(transparent)]
    #[diagnostic(
        code(inference::io),
        help("check that the file exists and that you have permission to read it")
    )]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    #[diagnostic(
        code(inference::config),
        help("the model config.json is malformed; re-download it from the model repository")
    )]
    Config(#[from] serde_json::Error),

    #[error(transparent)]
    #[diagnostic(
        code(inference::phonemizer),
        help("phonemization failed; check the dialect and that the g2p resources are available")
    )]
    Phonemizer(#[from] loqui_g2p::Error),

    #[error("invalid voice pack")]
    #[diagnostic(
        code(inference::invalid_voice_pack),
        help(
            "a voice pack is a raw f32 file of [510, 1, 256] style vectors; the file may be truncated or from another model"
        )
    )]
    InvalidVoicePack,

    #[error("text produced no supported tokens")]
    #[diagnostic(
        code(inference::empty_tokens),
        help("the input is empty or only contains characters the model does not know")
    )]
    EmptyTokens,

    #[error("token sequence too long: {0} tokens")]
    #[diagnostic(
        code(inference::sequence_too_long),
        help(
            "split the text into shorter chunks (for example by sentence) and synthesize them separately"
        )
    )]
    SequenceTooLong(usize),

    #[error("voice has no style vector for {0} phonemes")]
    #[diagnostic(
        code(inference::missing_style),
        help(
            "the input is longer than this voice pack covers; use shorter chunks or another voice"
        )
    )]
    MissingStyle(usize),

    #[error("speed must be finite and greater than zero")]
    #[diagnostic(
        code(inference::invalid_speed),
        help("use a value such as 1.0 (normal), 0.8 (slower) or 1.2 (faster)")
    )]
    InvalidSpeed,

    #[error("audio conversion failed: {0}")]
    #[diagnostic(
        code(inference::audio_data),
        help(
            "the model output could not be read back as f32 samples; this is likely a backend or device issue"
        )
    )]
    AudioData(String),

    #[error("inference task failed to complete")]
    #[diagnostic(
        code(inference::task),
        help("the blocking inference task panicked or was cancelled")
    )]
    Task(#[from] tokio::task::JoinError),

    #[error("model returned an invalid audio batch size: {0}")]
    #[diagnostic(
        code(inference::audio_shape),
        help("this API expects a single-utterance result with batch size 1")
    )]
    InvalidAudioShape(usize),
}

#[derive(Debug, Error, Diagnostic)]
pub enum DeviceSelectError {
    #[error("unknown backend `{0}` in HMM_BACKEND")]
    #[diagnostic(
        code(device::unknown_backend),
        help("valid values are: cuda, rocm, vulkan, metal")
    )]
    UnknownBackend(String),

    #[error("backend `{0}` is not compiled into this build")]
    #[diagnostic(
        code(device::backend_not_compiled),
        help(
            "cuda and rocm need `--features cuda` / `--features rocm`; vulkan is for Windows, Linux and Android, metal for macOS and iOS"
        )
    )]
    BackendNotCompiled(String),

    #[error("no working compute device found{}", format_attempts(.attempts))]
    #[diagnostic(
        code(device::no_working_device),
        help(
            "update the GPU driver, check the attempts listed above, or force a different backend with HMM_BACKEND"
        )
    )]
    NoWorkingDevice {
        forced: Option<String>,
        attempts: Vec<String>,
    },
}

fn format_attempts(attempts: &[String]) -> String {
    if attempts.is_empty() {
        return "\n  (no devices were reported by any enabled backend)".into();
    }
    attempts.iter().map(|a| format!("\n  - {a}")).collect()
}
