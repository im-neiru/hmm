use iced::{
    Element, Fill, Padding,
    alignment::Vertical,
    widget::{button, column, container, row, scrollable, text, text_input},
};
use hmm::{VoiceAccent, VoiceEntry, VoiceGender, VoiceModel};

use super::{ACCENTS, App, GENDERS, Load, Message, VoiceBank, accent_label, gender_label, label};
use super::theme;

const FILTER_LABEL_WIDTH: f32 = 62.0;
const FILTER_CHIP_WIDTH: f32 = 78.0;
const FILTER_CHIP_HEIGHT: f32 = 31.0;

pub(super) fn panel(app: &App) -> Element<'_, Message> {
    let query = app.search.trim().to_lowercase();
    let (count, body): (Option<String>, Element<'_, Message>) = match &app.voices {
        Load::Loading => (None, centered("Loading voices…")),
        Load::Failed => (
            None,
            centered("The voice library couldn't load. Press Reload to try again."),
        ),
        Load::Ready(bank) => {
            let visible = visible_voices(app, bank, &query);
            let count = format!("{} / {}", visible.len(), bank.entries.len());
            let body = if visible.is_empty() {
                centered("No voices match. Clear the search or filters.")
            } else {
                voice_list(&visible, app.selected)
            };

            (Some(count), body)
        }
    };

    let mut heading = row![text("Voices").size(22)]
        .spacing(10)
        .align_y(Vertical::Center);

    if let Some(count) = count {
        heading = heading.push(
            container(text(count).size(12).style(theme::muted))
                .padding([3, 8])
                .style(theme::inset),
        );
    }

    container(
        column![
            heading,
            text_input("Search voices by name or ID", &app.search)
                .on_input(Message::Search)
                .size(14)
                .padding(11),
            accent_filter(app.accent),
            gender_filter(app.gender),
            body,
            selected_hint(app.selected_entry()),
        ]
        .spacing(12),
    )
    .padding(15)
    .width(super::VOICES_PANEL_WIDTH)
    .height(Fill)
    .style(theme::panel)
    .into()
}

fn selected_hint(entry: Option<&VoiceEntry>) -> Element<'_, Message> {
    let line = match entry {
        Some(entry) => format!("Selected · {}", entry.name),
        None => "Choose a voice to queue speech".to_owned(),
    };

    container(text(line).size(12).style(theme::muted))
        .padding([9, 10])
        .width(Fill)
        .style(theme::inset)
        .into()
}

fn accent_filter(selected: Option<VoiceAccent>) -> Element<'static, Message> {
    let mut filter = row![
        container(label("ACCENT")).width(FILTER_LABEL_WIDTH),
        chip("All", selected.is_none(), Message::Accent(None)),
    ]
    .spacing(5)
    .align_y(Vertical::Center);

    for accent in ACCENTS {
        filter = filter.push(chip(
            accent_label(accent),
            selected == Some(accent),
            Message::Accent(Some(accent)),
        ));
    }

    filter.into()
}

fn gender_filter(selected: Option<VoiceGender>) -> Element<'static, Message> {
    let mut filter = row![
        container(label("GENDER")).width(FILTER_LABEL_WIDTH),
        chip("All", selected.is_none(), Message::Gender(None)),
    ]
    .spacing(5)
    .align_y(Vertical::Center);

    for gender in GENDERS {
        filter = filter.push(chip(
            gender_label(gender),
            selected == Some(gender),
            Message::Gender(Some(gender)),
        ));
    }

    filter.into()
}

fn chip(value: &'static str, selected: bool, message: Message) -> Element<'static, Message> {
    button(container(text(value).size(12)).width(Fill).center(Fill))
        .width(FILTER_CHIP_WIDTH)
        .height(FILTER_CHIP_HEIGHT)
        .padding([4, 8])
        .style(move |theme, status| theme::selected_button(theme, status, selected))
        .on_press(message)
        .into()
}

fn visible_voices<'a>(
    app: &App,
    bank: &'a VoiceBank,
    query: &str,
) -> Vec<(usize, &'a VoiceEntry)> {
    bank.entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| app.matches(entry, query))
        .collect()
}

fn voice_list<'a>(
    voices: &[(usize, &'a VoiceEntry)],
    selected: Option<usize>,
) -> Element<'a, Message> {
    let rows = voices
        .iter()
        .map(|&(index, entry)| voice_row(index, entry, selected == Some(index)));

    scrollable(
        column(rows)
            .spacing(5)
            .padding(Padding {
                right: 5.0,
                ..Padding::ZERO
            }),
    )
    .width(Fill)
    .height(Fill)
    .into()
}

fn voice_row(index: usize, entry: &VoiceEntry, selected: bool) -> Element<'_, Message> {
    let (accent, gender) = traits(entry);
    let names = column![
        text(entry.name.as_str()).size(14),
        text(entry.id.as_str()).size(11).style(theme::muted),
        row![tag(accent_label(accent)), gender_tag(gender)].spacing(5),
    ]
    .spacing(4)
    .width(Fill);

    button(row![monogram(entry, 38.0), names].spacing(10).align_y(Vertical::Center))
        .padding(9)
        .width(Fill)
        .style(move |theme, status| theme::voice_row(theme, status, selected))
        .on_press(Message::SelectVoice(index))
        .into()
}

fn monogram<'a>(entry: &'a VoiceEntry, size: f32) -> Element<'a, Message> {
    let (_, gender) = traits(entry);
    let initial = entry
        .name
        .chars()
        .next()
        .map(|character| character.to_uppercase().collect::<String>())
        .unwrap_or_else(|| "?".to_owned());

    container(text(initial).size(size * 0.46))
        .center(size)
        .style(theme::avatar_style(gender, entry.id.as_str(), size))
        .into()
}

fn tag(value: &'static str) -> Element<'static, Message> {
    container(text(value).size(10))
        .padding([2, 6])
        .style(theme::accent_tag)
        .into()
}

fn gender_tag(gender: VoiceGender) -> Element<'static, Message> {
    container(text(gender_label(gender)).size(10))
        .padding([2, 6])
        .style(move |_| theme::gender_tag(gender))
        .into()
}

fn centered(value: &'static str) -> Element<'static, Message> {
    container(text(value).size(13).style(theme::muted))
        .center(Fill)
        .padding(12)
        .into()
}

pub(super) fn traits(entry: &VoiceEntry) -> (VoiceAccent, VoiceGender) {
    match &entry.model {
        VoiceModel::Kokoro { accent, gender, .. } => (*accent, *gender),
    }
}
