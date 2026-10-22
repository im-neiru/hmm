use iced::{
    Element, Fill,
    widget::{button, container, row, text},
};

use super::{Message, Page, theme};

pub(super) fn view(page: Page, queued: usize, done: usize) -> Element<'static, Message> {
    container(
        row![
            tab("Compose", page == Page::Compose, Message::SetPage(Page::Compose)),
            tab(
                &format!("Queue · {queued}"),
                page == Page::Queue,
                Message::SetPage(Page::Queue),
            ),
            tab(
                &format!("Done · {done}"),
                page == Page::Done,
                Message::SetPage(Page::Done),
            ),
        ]
        .spacing(4)
        .width(Fill),
    )
    .padding(4)
    .width(Fill)
    .style(theme::tab_strip)
    .into()
}

fn tab(title: &str, selected: bool, message: Message) -> Element<'static, Message> {
    let title = title.to_owned();

    button(
        container(text(title).size(13))
            .width(Fill)
            .center(Fill),
    )
    .width(Fill)
    .height(theme::TAB_HEIGHT)
    .padding([6, 10])
    .style(move |theme, status| theme::tab_button(theme, status, selected))
    .on_press(message)
    .into()
}
