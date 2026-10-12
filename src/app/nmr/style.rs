//! NMR surface and control appearance.
use iced::widget::{button, container};
use iced::{Border, Color, Shadow, Theme, Vector};

pub(super) fn surface(theme: &Theme) -> container::Style {
    let dark = crate::appearance::is_dark(theme);
    container::Style {
        background: Some(
            if dark {
                theme.extended_palette().background.base.color
            } else {
                Color::WHITE
            }
            .into(),
        ),
        text_color: Some(theme.palette().text),
        border: Border {
            color: if dark {
                theme.extended_palette().background.strong.color
            } else {
                Color::from_rgb8(215, 223, 229)
            },
            width: 1.,
            radius: 6.into(),
        },
        shadow: crate::appearance::surface_shadow(Shadow {
            color: Color::BLACK.scale_alpha(if dark { 0.2 } else { 0.08 }),
            offset: Vector::new(0., 3.),
            blur_radius: 12.,
        }),
        ..Default::default()
    }
}
pub(super) fn table_surface(theme: &Theme) -> container::Style {
    container::Style {
        shadow: Shadow::default(),
        border: Border {
            radius: 4.into(),
            ..surface(theme).border
        },
        ..surface(theme)
    }
}
pub(super) fn rule(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(surface(theme).border.color.into()),
        ..Default::default()
    }
}
pub(super) fn quiet(theme: &Theme, status: button::Status) -> button::Style {
    let mut style = crate::app::workspace::control(false)(theme, status);
    style.border = Border::default();
    style
}
pub(super) fn outlined(theme: &Theme, status: button::Status) -> button::Style {
    let mut style = quiet(theme, status);
    style.border = Border {
        radius: 4.into(),
        ..surface(theme).border
    };
    style
}
pub(super) fn segment(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let mut style = crate::app::workspace::control(selected)(theme, status);
        style.border.radius = 4.into();
        if !selected {
            style.border = Border {
                radius: 4.into(),
                ..surface(theme).border
            };
        }
        style
    }
}
pub(super) fn result_row(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let mut style = crate::app::workspace::control(selected)(theme, status);
        style.border = Border::default();
        style
    }
}

#[cfg(test)]
mod tests;
