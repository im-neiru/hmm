use iced::{Element, Font, widget::text};
use iconflow::{Pack, Size, Style, try_icon};

use super::Message;

pub(super) fn icon(name: &str, fallback: &'static str, size: u16) -> Element<'static, Message> {
    match try_icon(Pack::Phosphor, name, Style::Regular, Size::Regular) {
        Ok(icon) => {
            let glyph = char::from_u32(icon.codepoint)
                .unwrap_or('?')
                .to_string();

            text(glyph)
                .size(f32::from(size))
                .font(Font::with_name(icon.family))
                .into()
        }
        Err(_) => text(fallback).size(f32::from(size)).into(),
    }
}
