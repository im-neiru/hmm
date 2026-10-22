use core::f32::consts::TAU;

use iced::{
    Color, Element, Fill, Padding,
    alignment::{Horizontal, Vertical},
    widget::{button, column, container, row, slider, text, text_editor, text_input},
};

use super::{App, Message, icons, label, theme};

pub(super) fn panel(app: &App) -> Element<'_, Message> {
    let characters = app.text.text().trim().chars().count();

    let mut form = column![
        row![
            column![
                text("Compose speech").size(23),
                text("Write once, queue many jobs, collect the results.")
                    .size(13)
                    .style(theme::muted),
            ]
            .spacing(4),
            container(row![
                metric("WAITING", app.queue.len().to_string(), theme::CYAN),
                metric("DONE", app.done.len().to_string(), theme::GREEN),
            ]
            .spacing(8))
            .width(Fill)
            .align_x(Horizontal::Right),
        ]
        .spacing(10)
        .align_y(Vertical::Center),
        label("TEXT TO SPEAK"),
        script_editor(&app.text),
        row![
            text(format!("{characters} characters"))
                .size(12)
                .style(theme::muted),
            container(
                text("Jobs run sequentially")
                    .size(12)
                    .color(theme::CYAN),
            )
            .width(Fill)
            .align_x(Horizontal::Right),
        ],
        label("SPEED"),
        speed_control(app.speed),
        label("OUTPUT FOLDER"),
        output_field(&app.output_dir),
        row![
            container(column![
                text("Timestamped WAV files").size(13),
                text(app.output_dir.as_str())
                    .size(11)
                    .style(theme::muted),
            ]
            .spacing(3))
            .width(Fill),
            button(
                row![
                    icons::icon("plus-circle", "+", 18),
                    text("Add to queue").size(15),
                ]
                .spacing(8)
                .align_y(Vertical::Center),
            )
            .height(44.0)
            .padding([9, 16])
            .style(theme::primary_button)
            .on_press(Message::Enqueue),
        ]
        .spacing(12)
        .align_y(Vertical::Center),
    ]
    .spacing(10)
    .height(Fill);

    if let Some(status) = &app.status {
        form = form.push(
            container(text(status).size(12).style(theme::muted))
                .padding([9, 12])
                .width(Fill)
                .style(theme::inset),
        );
    }

    container(form)
        .padding(20)
        .width(Fill)
        .height(Fill)
        .style(theme::panel)
        .into()
}

fn metric<'a>(title: &'a str, value: String, color: Color) -> Element<'a, Message> {
    container(
        column![
            text(title).size(10).style(theme::muted),
            text(value).size(17).color(color),
        ]
        .spacing(3),
    )
    .padding([7, 10])
    .style(theme::inset)
    .into()
}

fn script_editor(content: &text_editor::Content) -> Element<'_, Message> {
    text_editor(content)
        .placeholder("Type or paste what the voice should say…")
        .on_action(Message::Editor)
        .size(17.0)
        .padding(14)
        .height(Fill)
        .into()
}

fn speed_control(speed: f32) -> Element<'static, Message> {
    column![
        row![
            text("Slower").size(11).style(theme::muted),
            container(text(format!("{speed:.2}×")).size(14).color(theme::CYAN))
                .width(Fill)
                .align_x(Horizontal::Center),
            text("Faster").size(11).style(theme::muted),
        ]
        .align_y(Vertical::Center),
        slider(0.5..=2.0, speed, Message::Speed).step(0.05_f32),
    ]
    .spacing(5)
    .into()
}

fn output_field(path: &str) -> Element<'_, Message> {
    text_input("Choose an output folder", path)
        .on_input(Message::OutputDir)
        .size(14)
        .padding(11)
        .into()
}

pub(super) fn thinking_dots(elapsed: f32) -> Element<'static, Message> {
    let dots = (0..super::DOT_COUNT).map(|index| -> Element<'static, Message> {
        let phase = (elapsed / super::DOT_PERIOD
            - index as f32 * super::DOT_STAGGER)
            .rem_euclid(1.0);
        let lift = (phase * TAU).sin().max(0.0) * super::DOT_LIFT;

        container(
            text(".").size(super::TITLE_SIZE).style(|_| text::Style {
                color: Some(theme::CYAN),
            }),
        )
        .height(super::TITLE_SIZE * 1.3)
        .padding(Padding {
            top: super::DOT_LIFT - lift,
            ..Padding::ZERO
        })
        .into()
    });

    row(dots).into()
}
