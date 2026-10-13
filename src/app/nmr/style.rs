//! NMR surface and control appearance, built on the shared floating surface.
use iced::widget::{button, container};
use iced::{Border, Shadow, Theme};

/// The floating palette card and its label menu: the app's shared floating
/// surface, including its platform shadow policy.
pub(super) use crate::app::color_popover::surface;

/// The results table: the shared surface's border and fill, without a shadow.
pub(super) fn table_surface(theme: &Theme) -> container::Style {
    let surface = surface(theme);
    container::Style {
        shadow: Shadow::default(),
        border: Border {
            radius: 6.into(),
            ..surface.border
        },
        ..surface
    }
}
pub(super) fn rule(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(table_surface(theme).border.color.into()),
        ..Default::default()
    }
}
pub(super) fn quiet(theme: &Theme, status: button::Status) -> button::Style {
    let mut style = crate::app::workspace::control(false)(theme, status);
    style.border = Border::default();
    style
}
/// A neutral command with the table's outline, such as the docked Undock.
pub(super) fn outlined(theme: &Theme, status: button::Status) -> button::Style {
    let mut style = crate::app::workspace::control(false)(theme, status);
    style.border.color = table_surface(theme).border.color;
    style.border.width = 1.;
    style
}
pub(super) fn segment(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let mut style = crate::app::workspace::control(selected)(theme, status);
        style.border.radius = 4.into();
        if !selected {
            style.border = Border {
                radius: 4.into(),
                ..table_surface(theme).border
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
