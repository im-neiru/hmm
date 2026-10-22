use iced::{
    Background, Border, Color, Element, Fill, Shadow, Theme, Vector,
    alignment::{Horizontal, Vertical},
    widget::{Column, button, column, container, row, text},
};

use super::{Message, Toast, theme};

pub(super) fn layer(toasts: &[Toast]) -> Element<'_, Message> {
    let mut list = Column::new().spacing(8).align_x(Horizontal::Right);

    if toasts.len() > 1 {
        list = list.push(
            button(text("Clear all").size(12))
                .padding([5, 11])
                .style(theme::secondary_button)
                .on_press(Message::ClearToasts),
        );
    }

    list = list.extend(toasts.iter().map(toast_view));

    container(list)
        .width(Fill)
        .height(Fill)
        .align_x(Horizontal::Right)
        .align_y(Vertical::Bottom)
        .padding(20)
        .into()
}

fn toast_view(toast: &Toast) -> Element<'_, Message> {
    let content = column![
        text(toast.error.title.as_str()).size(14).color(theme::PINK),
        text(toast.error.detail.as_str()).size(12),
    ]
    .spacing(4)
    .width(Fill);

    let close = button(text("×").size(16))
        .padding([0, 8])
        .style(button::text)
        .on_press(Message::DismissToast(toast.id));

    container(row![content, close].spacing(8))
        .width(super::TOAST_WIDTH)
        .padding(12)
        .style(toast_style)
        .into()
}

fn toast_style(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb8(53, 36, 57))),
        text_color: Some(theme::TEXT),
        border: Border {
            radius: 13.0.into(),
            width: 1.0,
            color: theme::PINK,
        },
        shadow: Shadow {
            color: Color::BLACK.scale_alpha(0.48),
            offset: Vector::new(0.0, 5.0),
            blur_radius: 15.0,
        },
        ..container::Style::default()
    }
}
