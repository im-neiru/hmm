use serde::{Deserialize, Serialize};

pub const FORMAT_VERSION: u16 = 1;
pub const STYLE_WIDTH: usize = 256;

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

#[derive(Debug, Deserialize, Serialize)]
pub struct VoiceEntry {
    pub name: String,
    pub accent: VoiceAccent,
    pub gender: VoiceGender,
    pub frames: u16,
    pub styles: Vec<f32>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct VoiceCollection {
    pub format_version: u16,
    pub voices: Vec<VoiceEntry>,
}
