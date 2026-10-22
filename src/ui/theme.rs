use iced::{
    Background, Border, Color, Gradient, Radians, Shadow, Theme, Vector,
    gradient::Linear,
    widget::{button, container, text},
};
use hmm::VoiceGender;

pub(super) const BG: Color = Color::from_rgb8(26, 27, 46);
pub(super) const SURFACE: Color = Color::from_rgb8(36, 40, 59);
pub(super) const SURFACE_ALT: Color = Color::from_rgb8(45, 49, 74);
pub(super) const BORDER: Color = Color::from_rgb8(65, 72, 104);
pub(super) const TEXT: Color = Color::from_rgb8(192, 202, 245);
pub(super) const MUTED: Color = Color::from_rgb8(169, 177, 214);
pub(super) const BLUE: Color = Color::from_rgb8(122, 162, 247);
pub(super) const CYAN: Color = Color::from_rgb8(125, 207, 255);
pub(super) const PINK: Color = Color::from_rgb8(247, 118, 142);
pub(super) const PURPLE: Color = Color::from_rgb8(187, 154, 247);
pub(super) const GREEN: Color = Color::from_rgb8(158, 206, 106);
pub(super) const RED: Color = Color::from_rgb8(255, 99, 132);

pub(super) const TAB_HEIGHT: f32 = 42.0;
pub(super) const ACTION_WIDTH: f32 = 140.0;
pub(super) const ACTION_HEIGHT: f32 = 36.0;

pub(super) fn app_background(_: &Theme) -> container::Style {
    container::Style {
        background: Some(BG.into()),
        text_color: Some(TEXT),
        ..container::Style::default()
    }
}

pub(super) fn panel(_: &Theme) -> container::Style {
    container::Style {
        background: Some(SURFACE.into()),
        text_color: Some(TEXT),
        border: Border {
            radius: 16.0.into(),
            width: 1.0,
            color: BORDER,
        },
        shadow: Shadow {
            color: Color::BLACK.scale_alpha(0.12),
            offset: Vector::new(0.0, 5.0),
            blur_radius: 14.0,
        },
        ..container::Style::default()
    }
}

pub(super) fn inset(_: &Theme) -> container::Style {
    container::Style {
        background: Some(SURFACE_ALT.into()),
        text_color: Some(TEXT),
        border: Border {
            radius: 11.0.into(),
            width: 1.0,
            color: BORDER.scale_alpha(0.72),
        },
        ..container::Style::default()
    }
}

pub(super) fn tab_strip(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::from_rgb8(30, 34, 52).into()),
        text_color: Some(TEXT),
        border: Border {
            radius: 13.0.into(),
            width: 1.0,
            color: BORDER.scale_alpha(0.8),
        },
        ..container::Style::default()
    }
}

pub(super) fn muted(_: &Theme) -> text::Style {
    text::Style { color: Some(MUTED) }
}

pub(super) fn primary_button(_: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered => CYAN,
        button::Status::Pressed => Color::from_rgb8(91, 139, 231),
        _ => BLUE,
    };

    button::Style {
        background: Some(Background::Color(background)),
        text_color: BG,
        border: Border {
            radius: 11.0.into(),
            width: 1.0,
            color: background,
        },
        shadow: Shadow {
            color: BLUE.scale_alpha(0.13),
            offset: Vector::new(0.0, 2.0),
            blur_radius: 7.0,
        },
        ..button::Style::default()
    }
}

pub(super) fn secondary_button(_: &Theme, status: button::Status) -> button::Style {
    secondary_style(status)
}

fn secondary_style(status: button::Status) -> button::Style {
    let disabled = matches!(status, button::Status::Disabled);
    let background = if disabled {
        Color::from_rgb8(31, 34, 50)
    } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
        SURFACE_ALT
    } else {
        SURFACE
    };

    button::Style {
        background: Some(background.into()),
        text_color: if disabled { MUTED } else { TEXT },
        border: Border {
            radius: 10.0.into(),
            width: 1.0,
            color: BORDER,
        },
        ..button::Style::default()
    }
}

pub(super) fn danger_button(_: &Theme, status: button::Status) -> button::Style {
    let highlighted = matches!(status, button::Status::Hovered | button::Status::Pressed);
    let background = if highlighted {
        RED
    } else {
        Color::from_rgb8(65, 39, 58)
    };

    button::Style {
        background: Some(background.into()),
        text_color: if highlighted { BG } else { PINK },
        border: Border {
            radius: 10.0.into(),
            width: 1.0,
            color: PINK.scale_alpha(0.65),
        },
        ..button::Style::default()
    }
}

pub(super) fn tab_button(_: &Theme, status: button::Status, selected: bool) -> button::Style {
    let (background, text_color, border_color) = if selected {
        (Some(Color::from_rgb8(48, 63, 99)), CYAN, BLUE)
    } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
        (Some(SURFACE_ALT), TEXT, BORDER)
    } else {
        (None, MUTED, BORDER)
    };

    button::Style {
        background: background.map(Background::Color),
        text_color,
        border: Border {
            radius: 10.0.into(),
            width: if selected { 1.0 } else { 0.0 },
            color: border_color,
        },
        ..button::Style::default()
    }
}

pub(super) fn selected_button(_: &Theme, status: button::Status, selected: bool) -> button::Style {
    if selected {
        button::Style {
            background: Some(Color::from_rgb8(51, 66, 105).into()),
            text_color: CYAN,
            border: Border {
                radius: 10.0.into(),
                width: 1.0,
                color: BLUE,
            },
            ..button::Style::default()
        }
    } else {
        secondary_style(status)
    }
}

pub(super) fn voice_row(_: &Theme, status: button::Status, selected: bool) -> button::Style {
    let background = if selected {
        Some(Color::from_rgb8(47, 59, 94))
    } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
        Some(SURFACE_ALT)
    } else {
        None
    };

    button::Style {
        background: background.map(Background::Color),
        text_color: TEXT,
        border: Border {
            radius: 12.0.into(),
            width: if selected { 1.5 } else { 1.0 },
            color: if selected { BLUE } else { BORDER.scale_alpha(0.35) },
        },
        ..button::Style::default()
    }
}

pub(super) fn gender_tag(gender: VoiceGender) -> container::Style {
    let (background, text_color, border_color) = match gender {
        VoiceGender::Female => (
            Color::from_rgb8(75, 37, 60),
            PINK,
            Color::from_rgb8(171, 76, 116),
        ),
        VoiceGender::Male => (
            Color::from_rgb8(35, 54, 87),
            CYAN,
            Color::from_rgb8(71, 111, 178),
        ),
    };

    container::Style {
        background: Some(background.into()),
        text_color: Some(text_color),
        border: Border {
            radius: 7.0.into(),
            width: 1.0,
            color: border_color,
        },
        ..container::Style::default()
    }
}

pub(super) fn accent_tag(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::from_rgb8(62, 53, 91).into()),
        text_color: Some(PURPLE),
        border: Border {
            radius: 7.0.into(),
            width: 1.0,
            color: PURPLE.scale_alpha(0.55),
        },
        ..container::Style::default()
    }
}

pub(super) fn avatar_style(
    gender: VoiceGender,
    seed: &str,
    size: f32,
) -> impl Fn(&Theme) -> container::Style + '_ {
    let hash = seed.bytes().fold(0x811c_9dc5u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    });

    let pairs: &[(Color, Color)] = match gender {
        VoiceGender::Female => &[
            (Color::from_rgb8(247, 118, 142), Color::from_rgb8(187, 154, 247)),
            (Color::from_rgb8(255, 117, 160), Color::from_rgb8(224, 175, 104)),
            (Color::from_rgb8(187, 154, 247), Color::from_rgb8(247, 118, 142)),
            (Color::from_rgb8(224, 111, 182), Color::from_rgb8(157, 124, 216)),
        ],
        VoiceGender::Male => &[
            (Color::from_rgb8(122, 162, 247), Color::from_rgb8(125, 207, 255)),
            (Color::from_rgb8(125, 207, 255), Color::from_rgb8(157, 124, 216)),
            (Color::from_rgb8(122, 162, 247), Color::from_rgb8(86, 115, 210)),
            (Color::from_rgb8(125, 207, 255), Color::from_rgb8(122, 162, 247)),
        ],
    };
    let (start, end) = pairs[(hash as usize) % pairs.len()];

    move |_: &Theme| {
        let gradient = Linear::new(Radians(0.72))
            .add_stop(0.0, start)
            .add_stop(1.0, end);

        container::Style {
            background: Some(Background::Gradient(Gradient::Linear(gradient))),
            text_color: Some(BG),
            border: Border {
                radius: (size / 2.0).into(),
                width: 1.0,
                color: Color::WHITE.scale_alpha(0.24),
            },
            ..container::Style::default()
        }
    }
}

pub(super) fn avatar_chip(_: &Theme) -> container::Style {
    let gradient = Linear::new(Radians(0.72))
        .add_stop(0.0, BLUE)
        .add_stop(1.0, PURPLE);

    container::Style {
        background: Some(Background::Gradient(Gradient::Linear(gradient))),
        text_color: Some(BG),
        border: Border {
            radius: 12.0.into(),
            width: 1.0,
            color: BLUE.scale_alpha(0.55),
        },
        ..container::Style::default()
    }
}

pub(super) fn done_icon(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::from_rgb8(35, 65, 64).into()),
        text_color: Some(GREEN),
        border: Border {
            radius: 12.0.into(),
            width: 1.0,
            color: GREEN.scale_alpha(0.55),
        },
        ..container::Style::default()
    }
}

pub(super) fn empty_icon(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::from_rgb8(44, 52, 84).into()),
        text_color: Some(CYAN),
        border: Border {
            radius: 16.0.into(),
            width: 1.0,
            color: BLUE.scale_alpha(0.5),
        },
        ..container::Style::default()
    }
}

pub(super) fn active_card(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::from_rgb8(38, 50, 81).into()),
        text_color: Some(TEXT),
        border: Border {
            radius: 13.0.into(),
            width: 1.0,
            color: BLUE,
        },
        shadow: Shadow {
            color: BLUE.scale_alpha(0.12),
            offset: Vector::new(0.0, 3.0),
            blur_radius: 10.0,
        },
        ..container::Style::default()
    }
}

pub(super) fn empty_state(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::from_rgb8(30, 34, 52).into()),
        text_color: Some(TEXT),
        border: Border {
            radius: 14.0.into(),
            width: 1.0,
            color: BORDER.scale_alpha(0.65),
        },
        ..container::Style::default()
    }
}
