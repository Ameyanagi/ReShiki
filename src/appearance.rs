//! Interface preferences and defaults for new drawings. Existing drawings keep their page colors.
use iced::advanced::{Clipboard, Layout, Shell, layout, overlay, renderer, widget};
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
#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub mode: Mode,
    /// Last explicitly chosen canvas brightness, used only for new blank drawings.
    pub canvas_theme: reshiki::canvas_theme::CanvasTheme,
    /// Arrange group in the Select context row. A new key: the retired
    /// `object_toolbar` preference (off by default) must not hide it.
    pub arrange_controls: bool,
    /// Snap dragged objects to other objects' edges, centers and gaps.
    pub smart_guides: bool,
    /// Display/input preference for the five physical Drawing Style fields.
    pub drawing_style_unit: reshiki::style::units::Unit,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: Mode::default(),
            canvas_theme: Default::default(),
            arrange_controls: true,
            smart_guides: true,
            drawing_style_unit: Default::default(),
        }
    }
}

#[cfg(test)]
mod settings_tests;
impl Settings {
    fn path() -> Result<std::path::PathBuf, String> {
        let root = std::env::var_os("RESHIKI_DATA_DIR")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                directories_next::ProjectDirs::from("dev", "reshiki", "ReShiki")
                    .map(|dirs| dirs.data_local_dir().to_owned())
            })
            .or_else(reshiki::storage::browser_data_directory)
            .ok_or("No application data directory")?;
        Ok(root.join("appearance.json"))
    }
    pub fn load() -> Self {
        if cfg!(test) {
            return Self::default();
        }
        Self::path()
            .map(|path| Self::load_from(&path))
            .unwrap_or_default()
    }
    fn load_from(path: &std::path::Path) -> Self {
        reshiki::storage::read(path)
            .ok()
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
        self.save_to(&Self::path()?)
    }
    fn save_to(&self, path: &std::path::Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            reshiki::storage::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        reshiki::storage::write_atomic(path, &serde_json::to_vec(self).map_err(|e| e.to_string())?)
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

pub fn dropdown(
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
        shadow: surface_shadow(iced::Shadow {
            color: Color::from_rgba(0., 0., 0., if dark { 0.25 } else { 0.1 }),
            offset: iced::Vector::new(0., 3.),
            blur_radius: 12.,
        }),
    }
}

pub fn surface_shadow(shadow: iced::Shadow) -> iced::Shadow {
    // Windows uses Tiny Skia. Its shadow pass ignores the damage clip and
    // repeatedly blends over retained pixels during partial redraws, darkening
    // even the inside of menus and dialogs. Keep their surface borders instead.
    if cfg!(windows) {
        iced::Shadow::default()
    } else {
        shadow
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

/// Multi-line and rich text fields look like [`input_style`] inputs.
pub fn editor_style(
    theme: &Theme,
    status: iced::widget::text_editor::Status,
) -> iced::widget::text_editor::Style {
    use iced::widget::{text_editor, text_input::Status as Input};
    let field = input_style(
        theme,
        match status {
            text_editor::Status::Active => Input::Active,
            text_editor::Status::Hovered => Input::Hovered,
            text_editor::Status::Focused { is_hovered } => Input::Focused { is_hovered },
            text_editor::Status::Disabled => Input::Disabled,
        },
    );
    text_editor::Style {
        background: field.background,
        border: field.border,
        placeholder: field.placeholder,
        value: field.value,
        selection: field.selection,
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
mod tests;

/// Closes an open drop-down list on Escape, like the app's own popovers.
/// iced's pick list ignores the key. Its open menu takes Escape first, so
/// nothing behind it (the drawing, a style draft) acts on the key; the list
/// then closes on the next event, as a press it did not get from its menu.
pub fn close_on_escape<'a, Message: 'a>(
    content: impl Into<iced::Element<'a, Message>>,
) -> iced::Element<'a, Message> {
    iced::Element::new(CloseOnEscape(content.into()))
}

struct CloseOnEscape<'a, Message>(iced::Element<'a, Message>);

/// Set by the open menu on Escape; the list closes on the next update.
struct Closing(bool);

impl<Message> widget::Widget<Message, Theme, iced::Renderer> for CloseOnEscape<'_, Message> {
    fn size(&self) -> iced::Size<iced::Length> {
        self.0.as_widget().size()
    }
    fn size_hint(&self) -> iced::Size<iced::Length> {
        self.0.as_widget().size_hint()
    }
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<Closing>()
    }
    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(Closing(false))
    }
    fn children(&self) -> Vec<widget::Tree> {
        vec![widget::Tree::new(&self.0)]
    }
    fn diff(&self, tree: &mut widget::Tree) {
        tree.diff_children(std::slice::from_ref(&self.0));
    }
    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let Some(child) = tree.children.first_mut() else {
            return layout::Node::default();
        };
        self.0.as_widget_mut().layout(child, renderer, limits)
    }
    fn draw(
        &self,
        tree: &widget::Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: iced::mouse::Cursor,
        viewport: &iced::Rectangle,
    ) {
        if let Some(child) = tree.children.first() {
            self.0
                .as_widget()
                .draw(child, renderer, theme, style, layout, cursor, viewport);
        }
    }
    fn operate(
        &mut self,
        tree: &mut widget::Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        if let Some(child) = tree.children.first_mut() {
            self.0
                .as_widget_mut()
                .operate(child, layout, renderer, operation);
        }
    }
    fn update(
        &mut self,
        tree: &mut widget::Tree,
        event: &iced::Event,
        layout: Layout<'_>,
        cursor: iced::mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &iced::Rectangle,
    ) {
        let Some(child) = tree.children.first_mut() else {
            return;
        };
        let closing = tree.state.downcast_mut::<Closing>();
        if std::mem::replace(&mut closing.0, false) {
            // An open list closes on a press its menu did not take. Without a
            // cursor, a closed list ignores it.
            let mut messages = Vec::new();
            self.0.as_widget_mut().update(
                child,
                &iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)),
                layout,
                iced::mouse::Cursor::Unavailable,
                renderer,
                clipboard,
                &mut Shell::new(&mut messages),
                viewport,
            );
            messages.into_iter().for_each(|m| shell.publish(m));
            // The menu is gone: lay out the overlays left, such as a hint.
            shell.invalidate_layout();
            shell.request_redraw();
        }
        self.0.as_widget_mut().update(
            child, event, layout, cursor, renderer, clipboard, shell, viewport,
        );
    }
    fn mouse_interaction(
        &self,
        tree: &widget::Tree,
        layout: Layout<'_>,
        cursor: iced::mouse::Cursor,
        viewport: &iced::Rectangle,
        renderer: &iced::Renderer,
    ) -> iced::mouse::Interaction {
        tree.children
            .first()
            .map_or_else(Default::default, |child| {
                self.0
                    .as_widget()
                    .mouse_interaction(child, layout, cursor, viewport, renderer)
            })
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut widget::Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &iced::Rectangle,
        translation: iced::Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, iced::Renderer>> {
        let menu = self.0.as_widget_mut().overlay(
            tree.children.first_mut()?,
            layout,
            renderer,
            viewport,
            translation,
        )?;
        let closing = &mut tree.state.downcast_mut::<Closing>().0;
        Some(overlay::Element::new(Box::new(EscapeMenu {
            menu,
            closing,
        })))
    }
}

struct EscapeMenu<'b, Message> {
    menu: overlay::Element<'b, Message, Theme, iced::Renderer>,
    closing: &'b mut bool,
}

impl<Message> overlay::Overlay<Message, Theme, iced::Renderer> for EscapeMenu<'_, Message> {
    fn layout(&mut self, renderer: &iced::Renderer, bounds: iced::Size) -> layout::Node {
        self.menu.as_overlay_mut().layout(renderer, bounds)
    }
    fn draw(
        &self,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: iced::mouse::Cursor,
    ) {
        self.menu
            .as_overlay()
            .draw(renderer, theme, style, layout, cursor);
    }
    fn operate(
        &mut self,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.menu
            .as_overlay_mut()
            .operate(layout, renderer, operation);
    }
    fn update(
        &mut self,
        event: &iced::Event,
        layout: Layout<'_>,
        cursor: iced::mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        use iced::keyboard::{Event::KeyPressed, Key, key::Named};
        // Closing: later events in the batch go to the list, which closes first.
        if *self.closing {
            return;
        }
        if let iced::Event::Keyboard(KeyPressed {
            key: Key::Named(Named::Escape),
            ..
        }) = event
        {
            *self.closing = true;
            shell.capture_event();
            shell.request_redraw();
            return;
        }
        self.menu
            .as_overlay_mut()
            .update(event, layout, cursor, renderer, clipboard, shell);
    }
    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: iced::mouse::Cursor,
        renderer: &iced::Renderer,
    ) -> iced::mouse::Interaction {
        self.menu
            .as_overlay()
            .mouse_interaction(layout, cursor, renderer)
    }
    fn overlay<'c>(
        &'c mut self,
        layout: Layout<'c>,
        renderer: &iced::Renderer,
    ) -> Option<overlay::Element<'c, Message, Theme, iced::Renderer>> {
        self.menu.as_overlay_mut().overlay(layout, renderer)
    }
    fn index(&self) -> f32 {
        self.menu.as_overlay().index()
    }
}
