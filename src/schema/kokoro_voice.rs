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

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VoiceEntry {
    pub name: String,
    pub accent: VoiceAccent,
    pub gender: VoiceGender,
    pub frames: u16,
    pub styles: Vec<f32>,
}

impl VoiceEntry {
    pub fn styles_for_tokens(&self, token_count: usize) -> Option<&[f32]> {
        if token_count == 0 || token_count > usize::from(self.frames) {
            return None;
        }

        let end = token_count.checked_mul(STYLE_WIDTH)?;

        self.styles.get(..end)
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct VoiceCollection {
    pub format_version: u16,
    pub voices: Vec<VoiceEntry>,
}

impl VoiceCollection {
    pub fn find(
        &self,
        name: &str,
        accent: VoiceAccent,
        gender: VoiceGender,
    ) -> Option<&VoiceEntry> {
        self.voices.iter().find(|voice| {
            voice.name.eq_ignore_ascii_case(name)
                && voice.accent == accent
                && voice.gender == gender
        })
    }
}
