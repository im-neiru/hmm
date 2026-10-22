use iced::{
    Element, Fill, Theme,
    alignment::{Horizontal, Vertical},
    widget::{button, column, container, row, scrollable, text},
};

use super::{App, DoneItem, Message, icons, theme};

pub(super) fn page(app: &App) -> Element<'_, Message> {
    let mut content = column![page_header(
        "Done",
        "Completed audio stays here until you delete it.",
        app.done.len(),
    )]
    .spacing(16);

    if app.done.is_empty() {
        content = content.push(empty_state(
            "No completed audio yet",
            "Finished WAV files appear here, with options to open their folder or delete them.",
        ));
    } else {
        let items = app
            .done
            .iter()
            .map(|item| done_card(item, app.busy_files.contains(&item.id)));

        content = content.push(scrollable(column(items).spacing(9)).height(Fill));
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

fn done_card(item: &DoneItem, deleting: bool) -> Element<'_, Message> {
    let name = item
        .path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let path = item.path.to_string_lossy().into_owned();
    let delete_text = if deleting { "Deleting…" } else { "Delete" };

    container(
        row![
            container(icons::icon("file-audio", "♫", 23))
                .padding(11)
                .style(theme::done_icon),
            column![
                text(name).size(14),
                text(path).size(11).style(theme::muted),
                text("WAV audio · ready").size(11).color(theme::GREEN),
            ]
            .spacing(4)
            .width(Fill),
            column![
                action_button("play", "▶", "Play later", theme::secondary_button),
                button(action_content("folder-open", "↗", "Open folder"))
                    .width(theme::ACTION_WIDTH)
                    .height(theme::ACTION_HEIGHT)
                    .padding([4, 8])
                    .style(theme::secondary_button)
                    .on_press(Message::OpenDoneFolder(item.id)),
                button(action_content("trash", "×", delete_text))
                    .width(theme::ACTION_WIDTH)
                    .height(theme::ACTION_HEIGHT)
                    .padding([4, 8])
                    .style(theme::danger_button)
                    .on_press_maybe((!deleting).then_some(Message::DeleteDone(item.id))),
            ]
            .spacing(6),
        ]
        .spacing(12)
        .align_y(Vertical::Center),
    )
    .padding(12)
    .width(Fill)
    .style(theme::inset)
    .into()
}

fn action_button(
    icon_name: &str,
    fallback: &'static str,
    title: &'static str,
    style: fn(&Theme, button::Status) -> button::Style,
) -> Element<'static, Message> {
    button(action_content(icon_name, fallback, title))
        .width(theme::ACTION_WIDTH)
        .height(theme::ACTION_HEIGHT)
        .padding([4, 8])
        .style(style)
        .into()
}

fn action_content(
    icon_name: &str,
    fallback: &'static str,
    title: &'static str,
) -> Element<'static, Message> {
    container(
        row![
            icons::icon(icon_name, fallback, 15),
            text(title).size(12),
        ]
        .spacing(6)
        .align_y(Vertical::Center),
    )
    .width(Fill)
    .center(Fill)
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
