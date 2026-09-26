//! Application chrome only. Document page colors live in canvas_theme.
use iced::{Color, Theme};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    MatchCanvas,
    Light,
    Dark,
}
impl Mode {
    pub const ALL: [Self; 3] = [Self::MatchCanvas, Self::Light, Self::Dark];
    pub fn is_dark(self, canvas: reshiki::canvas_theme::CanvasTheme) -> bool {
        match self {
            Self::MatchCanvas => canvas.is_dark(),
            Self::Light => false,
            Self::Dark => true,
        }
    }
}
impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::MatchCanvas => "Match canvas",
            Self::Light => "Light",
            Self::Dark => "Dark",
        })
    }
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub mode: Mode,
}
impl Settings {
    fn path() -> Result<std::path::PathBuf, String> {
        let root = std::env::var_os("RESHIKI_DATA_DIR")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                directories_next::ProjectDirs::from("dev", "reshiki", "ReShiki")
                    .map(|dirs| dirs.data_local_dir().to_owned())
            })
            .ok_or("No application data directory")?;
        Ok(root.join("appearance.json"))
    }
    pub fn load() -> Self {
        if cfg!(test) {
            return Self::default();
        }
        Self::path()
            .ok()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| {
                let mut settings: Self = serde_json::from_slice(&bytes).ok()?;
                let legacy: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
                if legacy.get("mode").is_none()
                    && let Some(dark) = legacy.get("dark").and_then(|v| v.as_bool())
                {
                    settings.mode = if dark { Mode::Dark } else { Mode::Light };
                }
                Some(settings)
            })
            .unwrap_or_default()
    }
    pub fn save(&self) -> Result<(), String> {
        if cfg!(test) {
            return Ok(());
        }
        let path = Self::path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        reshiki::storage::write_atomic(&path, &serde_json::to_vec(self).map_err(|e| e.to_string())?)
    }
}

pub fn is_dark(theme: &Theme) -> bool {
    theme.extended_palette().is_dark
}

/// Invert lightness while retaining hue, saturation and alpha. Black ink becomes
/// white and white paper becomes black; chemical highlight colors retain their hue.
pub fn color(dark: bool, c: Color) -> Color {
    if !dark {
        return c;
    }
    let high = c.r.max(c.g).max(c.b);
    let low = c.r.min(c.g).min(c.b);
    let offset = 1. - high - low;
    Color {
        r: (c.r + offset).clamp(0., 1.),
        g: (c.g + offset).clamp(0., 1.),
        b: (c.b + offset).clamp(0., 1.),
        a: c.a,
    }
}
pub fn themed(theme: &Theme, c: Color) -> Color {
    color(is_dark(theme), c)
}

pub fn rgb(color: Color) -> [u8; 3] {
    [color.r, color.g, color.b].map(|v| (v * 255.).round().clamp(0., 255.) as u8)
}
pub fn from_rgb([r, g, b]: [u8; 3]) -> Color {
    Color::from_rgb8(r, g, b)
}

/// Semantic ink and indicator roles are checked against the actual surfaces.
pub fn readable(seed: Color, backgrounds: &[Color], target: f64) -> Color {
    let backgrounds: Vec<_> = backgrounds.iter().map(|&c| rgb(c)).collect();
    reshiki::color_contrast::ensure_contrast(rgb(seed), &backgrounds, target)
        .map(from_rgb)
        .unwrap_or(seed)
}
pub fn muted(theme: &Theme) -> Color {
    readable(
        themed(theme, Color::from_rgb8(107, 116, 127)),
        &[theme.palette().background, surface(theme, Color::WHITE)],
        reshiki::color_contrast::TEXT_TARGET,
    )
}
pub fn focus_border(theme: &Theme, inner: Color) -> Color {
    readable(
        theme.palette().primary,
        &[
            inner,
            theme.palette().background,
            surface(theme, Color::WHITE),
        ],
        reshiki::color_contrast::OUTLINE_TARGET,
    )
}

/// Shared dropdown styling for toolbar controls and inspector fields.
pub fn pick_list<'a, T, L, V, Message>(
    options: L,
    selected: Option<V>,
    on_selected: impl Fn(T) -> Message + 'a,
) -> iced::widget::PickList<'a, T, L, V, Message>
where
    T: ToString + PartialEq + Clone + 'a,
    L: std::borrow::Borrow<[T]> + 'a,
    V: std::borrow::Borrow<T> + 'a,
    Message: Clone + 'a,
{
    iced::widget::pick_list(options, selected, on_selected)
        .handle(iced::widget::pick_list::Handle::Arrow {
            size: Some(iced::Pixels(9.)),
        })
        .style(dropdown)
        .menu_style(dropdown_menu)
}

fn dropdown(
    theme: &Theme,
    status: iced::widget::pick_list::Status,
) -> iced::widget::pick_list::Style {
    let dark = is_dark(theme);
    let active = !matches!(status, iced::widget::pick_list::Status::Active);
    let rgb = |light, night| {
        let [r, g, b] = if dark { night } else { light };
        Color::from_rgb8(r, g, b)
    };
    let background = if active {
        rgb([242, 248, 246], [29, 43, 41])
    } else {
        rgb([247, 249, 250], [32, 37, 43])
    };
    iced::widget::pick_list::Style {
        text_color: theme.palette().text,
        placeholder_color: readable(
            muted(theme),
            &[background],
            reshiki::color_contrast::TEXT_TARGET,
        ),
        handle_color: rgb([90, 109, 115], [163, 187, 181]),
        background: background.into(),
        border: iced::Border {
            color: if active {
                focus_border(theme, background)
            } else {
                rgb([210, 219, 224], [67, 78, 87])
            },
            width: if active { 2. } else { 1. },
            radius: 6.0.into(),
        },
    }
}

pub fn dropdown_menu(theme: &Theme) -> iced::overlay::menu::Style {
    let dark = is_dark(theme);
    let rgb = |light, night| {
        let [r, g, b] = if dark { night } else { light };
        Color::from_rgb8(r, g, b)
    };
    iced::overlay::menu::Style {
        background: rgb([255, 255, 255], [27, 32, 38]).into(),
        border: iced::Border {
            color: rgb([210, 219, 224], [67, 78, 87]),
            width: 1.,
            radius: 6.0.into(),
        },
        text_color: theme.palette().text,
        selected_text_color: rgb([15, 103, 85], [162, 230, 207]),
        selected_background: rgb([222, 240, 234], [29, 64, 55]).into(),
        shadow: iced::Shadow {
            color: Color::from_rgba(0., 0., 0., if dark { 0.25 } else { 0.1 }),
            offset: iced::Vector::new(0., 3.),
            blur_radius: 12.,
        },
    }
}

// Application surfaces use charcoal rather than the canvas's pure black paper.
fn surface(theme: &Theme, c: Color) -> Color {
    if is_dark(theme)
        && c.r.min(c.g).min(c.b) > 0.9
        && c.r.max(c.g).max(c.b) - c.r.min(c.g).min(c.b) < 0.06
    {
        return Color {
            a: c.a,
            ..Color::from_rgb8(24, 28, 34)
        };
    }
    themed(theme, c)
}

pub fn container(
    theme: &Theme,
    mut style: iced::widget::container::Style,
) -> iced::widget::container::Style {
    if let Some(iced::Background::Color(c)) = style.background
        && c.a == 1.
    {
        style.background = Some(surface(theme, c).into());
    }
    style.text_color = Some(
        style
            .text_color
            .map_or(theme.palette().text, |c| themed(theme, c)),
    );
    style.border.color = themed(theme, style.border.color);
    style
}
pub fn button(
    theme: &Theme,
    mut style: iced::widget::button::Style,
) -> iced::widget::button::Style {
    if let Some(iced::Background::Color(c)) = style.background {
        style.background = Some(surface(theme, c).into());
    }
    style.text_color = themed(theme, style.text_color);
    style.border.color = themed(theme, style.border.color);
    style
}
pub fn text_color(color: Color) -> impl Fn(&Theme) -> iced::widget::text::Style {
    move |theme| iced::widget::text::Style {
        color: Some(themed(theme, color)),
    }
}

/// Neutral rounded inputs share surfaces and focus accents with dropdowns.
pub fn text_input<'a, Message: Clone + 'a>(
    placeholder: &str,
    value: &str,
) -> iced::widget::TextInput<'a, Message> {
    iced::widget::text_input(placeholder, value).style(input_style)
}
pub fn input_style(
    theme: &Theme,
    status: iced::widget::text_input::Status,
) -> iced::widget::text_input::Style {
    use iced::widget::{pick_list, text_input};
    let focused = matches!(status, text_input::Status::Focused { .. });
    let hovered = matches!(status, text_input::Status::Hovered);
    let dropdown = dropdown(
        theme,
        if focused || hovered {
            pick_list::Status::Hovered
        } else {
            pick_list::Status::Active
        },
    );
    let disabled = matches!(status, text_input::Status::Disabled);
    text_input::Style {
        background: dropdown.background,
        border: dropdown.border,
        icon: dropdown.handle_color,
        placeholder: dropdown.placeholder_color,
        value: if disabled {
            dropdown.placeholder_color
        } else {
            dropdown.text_color
        },
        selection: if is_dark(theme) {
            Color::from_rgb8(39, 87, 75)
        } else {
            Color::from_rgb8(194, 228, 217)
        },
    }
}

pub fn primary(theme: &Theme, status: iced::widget::button::Status) -> iced::widget::button::Style {
    let mut style = iced::widget::button::primary(theme, status);
    style.border.radius = 6.0.into();
    style
}
pub fn secondary(
    theme: &Theme,
    status: iced::widget::button::Status,
) -> iced::widget::button::Style {
    use iced::widget::{button, pick_list};
    let field = dropdown(
        theme,
        if matches!(status, button::Status::Hovered | button::Status::Pressed) {
            pick_list::Status::Hovered
        } else {
            pick_list::Status::Active
        },
    );
    button::Style {
        background: Some(field.background),
        text_color: if matches!(status, button::Status::Disabled) {
            field.placeholder_color
        } else {
            field.text_color
        },
        border: field.border,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn field_and_menu_roles_pass_on_both_interface_surfaces() {
        use iced::widget::pick_list;
        use reshiki::color_contrast::{OUTLINE_TARGET, TEXT_TARGET, contrast};
        let (mut app, _) = crate::app::App::new();
        for mode in Mode::ALL {
            for canvas in reshiki::canvas_theme::CanvasTheme::ALL {
                let _ = app.update(crate::app::Message::CanvasTheme(canvas));
                let _ = app.update(crate::app::Message::Appearance(mode));
                let theme = app.theme();
                for background in [theme.palette().background, surface(&theme, Color::WHITE)] {
                    assert!(contrast(rgb(muted(&theme)), rgb(background)) >= TEXT_TARGET);
                }
                for status in [pick_list::Status::Active, pick_list::Status::Hovered] {
                    let style = dropdown(&theme, status);
                    let iced::Background::Color(bg) = style.background else {
                        panic!("solid field");
                    };
                    for ink in [style.text_color, style.placeholder_color] {
                        assert!(contrast(rgb(ink), rgb(bg)) >= TEXT_TARGET);
                    }
                    assert!(contrast(rgb(style.handle_color), rgb(bg)) >= 3.);
                    if matches!(status, pick_list::Status::Hovered) {
                        for background in [bg, theme.palette().background] {
                            assert!(
                                contrast(rgb(style.border.color), rgb(background))
                                    >= OUTLINE_TARGET
                            );
                        }
                    }
                }
                let menu = dropdown_menu(&theme);
                let iced::Background::Color(bg) = menu.background else {
                    panic!("solid menu");
                };
                let iced::Background::Color(selected) = menu.selected_background else {
                    panic!("solid selection");
                };
                assert!(contrast(rgb(menu.text_color), rgb(bg)) >= TEXT_TARGET);
                assert!(contrast(rgb(menu.selected_text_color), rgb(selected)) >= TEXT_TARGET);
            }
        }
    }

    #[test]
    fn display_colors_preserve_hue_and_alpha() {
        assert_eq!(color(true, Color::BLACK), Color::WHITE);
        assert_eq!(color(true, Color::WHITE), Color::BLACK);
        let red = Color::from_rgba8(180, 50, 55, 0.5);
        let shown = color(true, red);
        assert!(shown.r > shown.b && shown.b > shown.g);
        assert_eq!(shown.a, red.a);
        assert_eq!(color(false, red), red);
    }
}
