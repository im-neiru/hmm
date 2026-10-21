use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

pub const FORMAT_VERSION: u16 = 1;
pub const STYLE_WIDTH: usize = 256;

#[derive(Debug, Serialize, Deserialize)]
pub struct VoiceCollection {
    pub format_version: u16,
    pub voices: Vec<VoiceEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VoiceEntry {
    pub name: String,
    pub accent: VoiceAccent,
    pub frames: u16,
    pub styles: Vec<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum VoiceAccent {
    EnglishAmerican = 1,
    EnglishBritish = 2,
}

impl Serialize for VoiceAccent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u8(*self as u8)
    }
}

impl<'de> Deserialize<'de> for VoiceAccent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match u8::deserialize(deserializer)? {
            1 => Ok(Self::EnglishAmerican),
            2 => Ok(Self::EnglishBritish),
            id => Err(D::Error::custom(format!("unknown voice accent ID: {id}"))),
        }
    }
}
