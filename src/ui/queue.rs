use iced::{
    Element, Fill,
    alignment::{Horizontal, Vertical},
    widget::{button, column, container, row, scrollable, text},
};

use super::{ActiveJob, App, Message, audio, icons, theme};

pub(super) fn page(app: &App) -> Element<'_, Message> {
    let count = app.queue.len() + usize::from(app.active.is_some());
    let mut content = column![page_header(
        "Queue",
        "Speech jobs are processed one at a time.",
        count,
    )]
    .spacing(16);

    if let Some(active) = &app.active {
        content = content.push(active_card(active));
    }

    if app.queue.is_empty() {
        let (title, detail) = if app.active.is_some() {
            (
                "Queue is clear",
                "The current job is the only one being processed.",
            )
        } else {
            (
                "Nothing queued yet",
                "Compose some text and choose Add to queue.",
            )
        };

        content = content.push(empty_state(title, detail));
    } else {
        let waiting = app.queue.iter().map(queued_card);
        content = content.push(scrollable(column(waiting).spacing(9)).height(Fill));
    }

    container(content)
        .padding(20)
        .width(Fill)
        .height(Fill)
        .style(theme::panel)
        .into()
}

fn page_header<'a>(title: &'a str, subtitle: &'a str, count: usize) -> Element<'a, Message> {
    row![
        column![
            text(title).size(23),
            text(subtitle).size(13).style(theme::muted),
        ]
        .spacing(4),
        container(text(count.to_string()).size(15).color(theme::CYAN))
            .padding([6, 11])
            .style(theme::inset),
    ]
    .spacing(10)
    .align_y(Vertical::Center)
    .into()
}

fn active_card(active: &ActiveJob) -> Element<'_, Message> {
    container(
        row![
            container(icons::icon("waveform", "♪", 23))
                .padding(11)
                .style(theme::avatar_chip),
            column![
                row![
                    text("GENERATING NOW").size(10).color(theme::CYAN),
                    text(active.voice_name.as_str()).size(14),
                ]
                .spacing(9)
                .align_y(Vertical::Center),
                text(
                    active
                        .path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                )
                .size(12)
                .style(theme::muted),
            ]
            .spacing(5)
            .width(Fill),
            container(text("GENERATING").size(10).color(theme::CYAN))
                .padding([5, 8])
                .style(theme::inset),
        ]
        .spacing(12)
        .align_y(Vertical::Center),
    )
    .padding(12)
    .width(Fill)
    .style(theme::active_card)
    .into()
}

fn queued_card(job: &audio::SynthesisJob) -> Element<'_, Message> {
    let mut snippet = job.text.chars().take(130).collect::<String>();
    if job.text.chars().count() > 130 {
        snippet.push('…');
    }

    container(
        row![
            container(icons::icon("clock-countdown", "◷", 22))
                .padding(11)
                .style(theme::inset),
            column![
                text(job.voice_name.as_str()).size(14),
                text(snippet).size(12).style(theme::muted),
                text(
                    job.path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                )
                .size(11)
                .color(theme::CYAN),
            ]
            .spacing(4)
            .width(Fill),
            button(
                container(icons::icon("x", "×", 17))
                    .width(Fill)
                    .center(Fill),
            )
            .width(38.0)
            .height(38.0)
            .padding(0)
            .style(theme::secondary_button)
            .on_press(Message::RemoveQueued(job.id)),
        ]
        .spacing(12)
        .align_y(Vertical::Center),
    )
    .padding(12)
    .width(Fill)
    .style(theme::inset)
    .into()
}

fn empty_state<'a>(title: &'a str, detail: &'a str) -> Element<'a, Message> {
    container(
        column![
            container(icons::icon("waveform", "♪", 28))
                .padding(14)
                .style(theme::empty_icon),
            text(title).size(16),
            text(detail).size(12).style(theme::muted),
        ]
        .spacing(9)
        .align_x(Horizontal::Center),
    )
    .width(Fill)
    .height(Fill)
    .center(Fill)
    .padding(24)
    .style(theme::empty_state)
    .into()
}
