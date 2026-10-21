use serde::{Deserialize, Serialize};

pub const FORMAT_VERSION: u16 = 1;

pub const VOICES_ARCHIVE_FILE: &str = "voices.zip";
pub const VOICES_LOOKUP_FILE: &str = "voices.postcard";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[repr(u8)]
pub enum VoiceAccent {
    EnglishAmerican = 0,
    EnglishBritish = 1,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[repr(u8)]
pub enum VoiceGender {
    Female = 0,
    Male = 1,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum VoiceDataFormat {
    ZstdF32Le,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum VoiceModel {
    Kokoro {
        accent: VoiceAccent,
        gender: VoiceGender,
        frames: u16,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VoiceEntry {
    pub id: String,
    pub name: String,
    pub model: VoiceModel,
    pub archive_path: String,
    pub data_format: VoiceDataFormat,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct VoiceLookup {
    pub format_version: u16,
    pub entries: Vec<VoiceEntry>,
}
