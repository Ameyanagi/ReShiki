//! A draft-only theme workbench: full periodic-table previews and four controls.
use super::{
    App, InspectorTab, Message,
    icons::{Glyph, Icon},
};
use crate::canvas::{DrawingThumbnail, layered::canvas};
use iced::widget::{Space, button, column, container, row, scrollable, slider, text, tooltip};
use iced::{Alignment, Border, Color, Element, Length, Task};
use reshiki::{
    canvas_theme::{CanvasTheme, ColorTheme},
    color_contrast,
    document::{Document, Point},
    theme_files::{self, ThemeFile},
    theme_generator::{Recipe, Reference, Tone},
};

const ROWS: [&str; 9] = [
    "H . . . . . . . . . . . . . . . . He",
    "Li Be . . . . . . . . . . B C N O F Ne",
    "Na Mg . . . . . . . . . . Al Si P S Cl Ar",
    "K Ca Sc Ti V Cr Mn Fe Co Ni Cu Zn Ga Ge As Se Br Kr",
    "Rb Sr Y Zr Nb Mo Tc Ru Rh Pd Ag Cd In Sn Sb Te I Xe",
    "Cs Ba La Hf Ta W Re Os Ir Pt Au Hg Tl Pb Bi Po At Rn",
    "Fr Ra Ac Rf Db Sg Bh Hs Mt Ds Rg Cn Nh Fl Mc Lv Ts Og",
    ". . Ce Pr Nd Pm Sm Eu Gd Tb Dy Ho Er Tm Yb Lu . .",
    ". . Th Pa U Np Pu Am Cm Bk Cf Es Fm Md No Lr . .",
];
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preview {
    Labels,
    Colors,
    Tiles,
    Rings,
}
impl std::fmt::Display for Preview {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Labels => "Element labels",
            Self::Colors => "Color tiles",
            Self::Tiles => "Interface tiles",
            Self::Rings => "On ring fills",
        })
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReferenceChoice {
    Jmol,
    Theme { id: String, name: String },
}
impl std::fmt::Display for ReferenceChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Jmol => "Jmol",
            Self::Theme { name, .. } => name,
        })
    }
}
impl From<&Reference> for ReferenceChoice {
    fn from(reference: &Reference) -> Self {
        Self::Theme {
            id: reference.id.clone(),
            name: reference.name.clone(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    New,
    Builtin(ColorTheme),
    Saved { id: String, name: String },
    Draft,
}
impl std::fmt::Display for Selection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::New => f.write_str("New theme…"),
            Self::Builtin(theme) => theme.fmt(f),
            Self::Saved { name, .. } => f.write_str(name),
            Self::Draft => f.write_str("Unsaved theme"),
        }
    }
}
fn fresh_id() -> String {
    format!(
        "generated-{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    )
}
fn unique_name(name: &str, library: &[ThemeFile]) -> String {
    let mut result = name.to_owned();
    let mut suffix = 2;
    while library.iter().any(|t| t.name == result) {
        result = format!("{name} {suffix}");
        suffix += 1;
    }
    result
}
#[derive(Debug, Clone)]
pub enum Action {
    Open,
    Back,
    Select(Selection),
    Import,
    Delete,
    Name(String),
    Preset(ColorTheme),
    Reference(ReferenceChoice),
    Lightness(CanvasTheme, f64),
    Chroma(CanvasTheme, f64),
    Preview(Preview),
    Bold(bool),
    Fill(String),
    Element(String),
    Apply,
    Export,
}
pub(super) struct Editor {
    epoch: u64,
    selection: Selection,
    original: ThemeFile,
    template: ThemeFile,
    recipe: Recipe,
    references: Vec<Reference>,
    palette: ThemeFile,
    name: String,
    preview: Preview,
    bold: bool,
    fill: String,
    element: String,
}
impl Editor {
    fn new(doc: &Document, epoch: u64, library: &[ThemeFile]) -> Self {
        let original = ThemeFile::capture(doc);
        let recipe =
            original
                .generator
                .clone()
                .unwrap_or(if doc.color_theme == ColorTheme::Pastel {
                    Recipe::PASTEL
                } else {
                    Recipe::PRESENTATION
                });
        let mut template = original.clone();
        let selection = if doc.custom_theme.is_none() {
            template.id = fresh_id();
            template.name = unique_name(&format!("{} copy", original.name), library);
            Selection::Builtin(doc.color_theme)
        } else if library.iter().any(|t| t.id == original.id) {
            Selection::Saved {
                id: original.id.clone(),
                name: original.name.clone(),
            }
        } else {
            Selection::Draft
        };
        let mut references: Vec<_> = [
            ColorTheme::Publication,
            ColorTheme::Presentation,
            ColorTheme::Pastel,
        ]
        .into_iter()
        .map(|color_theme| {
            Reference::capture(&ThemeFile::capture(&Document {
                color_theme,
                ..Default::default()
            }))
        })
        .collect();
        for theme in library.iter().chain(doc.custom_theme.as_deref()) {
            references.retain(|r| r.id != theme.id);
            references.push(Reference::capture(theme));
        }
        if let Some(reference) = &recipe.reference {
            // Reopening uses the saved snapshot, even if the library later changed.
            references.retain(|r| r.id != reference.id);
            references.push(reference.clone());
        }
        // Preview the authored palette exactly. A recipe is applied only after
        // a generation control changes, never merely by opening/importing it.
        let palette = template.clone();
        Self {
            epoch,
            selection,
            name: template.name.clone(),
            original,
            template,
            recipe,
            references,
            palette,
            preview: Preview::Labels,
            bold: true,
            fill: "Sky".into(),
            element: "N".into(),
        }
    }
    fn load_theme(&mut self, theme: ThemeFile, selection: Selection) {
        self.recipe = theme
            .generator
            .clone()
            .unwrap_or(if theme.base == ColorTheme::Pastel {
                Recipe::PASTEL
            } else {
                Recipe::PRESENTATION
            });
        if let Some(reference) = &self.recipe.reference {
            self.references.retain(|r| r.id != reference.id);
            self.references.push(reference.clone());
        }
        self.name = theme.name.clone();
        self.template = theme.clone();
        self.palette = theme;
        self.selection = selection;
    }
    fn candidate(&self) -> Result<ThemeFile, String> {
        let mut theme = self.palette.clone();
        theme.name = self.name.trim().into();
        theme.validate()?;
        Ok(theme)
    }
    fn refresh(&mut self) {
        if let Ok(palette) = self.recipe.generate(&self.template) {
            self.palette = palette;
        }
    }
    fn fill_color(&self, key: [u8; 3], mode: CanvasTheme) -> [u8; 3] {
        self.palette
            .fill_color(key, mode)
            .unwrap_or_else(|| reshiki::ring_fills::palette_color(key, mode))
    }
    fn colors(&self, element: &str, mode: CanvasTheme) -> ([u8; 3], [u8; 3]) {
        let paper = mode.background();
        let ink = self.palette.element_color(element, mode);
        match self.preview {
            Preview::Labels => (ink, paper),
            Preview::Colors => {
                let text = if color_contrast::contrast([0; 3], ink)
                    >= color_contrast::contrast([255; 3], ink)
                {
                    [0; 3]
                } else {
                    [255; 3]
                };
                (text, ink)
            }
            Preview::Tiles => {
                let bg = self
                    .palette
                    .element_swatch(element, mode)
                    .map(|seed| color_contrast::tile(seed, mode.is_dark(), false, false))
                    .unwrap_or(paper);
                (
                    if mode.is_dark() {
                        [231, 236, 241]
                    } else {
                        [37, 43, 51]
                    },
                    bg,
                )
            }
            Preview::Rings => {
                let bg = reshiki::ring_fills::PALETTE
                    .iter()
                    .find(|(name, _)| *name == self.fill)
                    .map(|(_, key)| self.fill_color(*key, mode))
                    .unwrap_or(paper);
                let ink =
                    color_contrast::ensure_contrast(ink, &[paper, bg], color_contrast::TEXT_TARGET)
                        .or_else(|| {
                            color_contrast::ensure_contrast(
                                ink,
                                &[paper, bg],
                                color_contrast::TEXT_MIN,
                            )
                        })
                        .unwrap_or(ink);
                (ink, bg)
            }
        }
    }
}
fn action(action: Action) -> Message {
    Message::ThemeGenerator(action)
}
fn command(label: &str, message: Action) -> iced::widget::Button<'_, Message> {
    button(text(label).size(12))
        .padding([8, 10])
        .on_press(action(message))
        .style(crate::appearance::secondary)
}
fn rgb(c: [u8; 3]) -> Color {
    crate::appearance::from_rgb(c)
}
fn hex([r, g, b]: [u8; 3]) -> String {
    format!("#{r:02X}{g:02X}{b:02X}")
}

fn preview_control<'a>(
    content: impl Into<Element<'a, Message>>,
    hint: &'static str,
    event: Action,
    selected: bool,
) -> Element<'a, Message> {
    super::workspace::hover_hint(
        button(container(content).center(Length::Fill))
            .width(36)
            .height(34)
            .padding(5)
            .style(super::workspace::control(selected))
            .on_press(action(event)),
        hint,
        tooltip::Position::Bottom,
    )
    .into()
}

fn icon_control(
    icon: Icon,
    hint: &'static str,
    event: Action,
    enabled: bool,
) -> Element<'static, Message> {
    super::workspace::hover_hint(
        button(canvas(Glyph(icon, enabled)).width(24).height(24))
            .padding(5)
            .width(36)
            .height(34)
            .style(super::workspace::control(false))
            .on_press_maybe(enabled.then_some(action(event))),
        hint,
        tooltip::Position::Bottom,
    )
    .into()
}

/// Piperidin-3-one: one NH and one carbonyl, with H above the ring.
fn preview_molecule() -> Document {
    let mut doc = reshiki::rings::Preset::Regular.document(42., false);
    if let Some(atom) = doc.atoms.first_mut() {
        atom.element = "N".into();
    }
    if let Some(atom) = doc.atoms.get(2) {
        let (id, p) = (atom.id, atom.position);
        let oxygen = doc.add_atom("O", Point::new(p.x * 2., p.y * 2.));
        doc.add_bond(id, oxygen, 2, "plain");
    }
    // Bond edits clear computed labels, so fill this cache after the graph is complete.
    if let Some(atom) = doc.atoms.first_mut() {
        atom.label_h = 1;
    }
    doc
}

impl App {
    pub(super) fn review_imported_theme(&mut self, theme: ThemeFile) {
        let Some(editor) = &mut self.theme_library.editor else {
            return;
        };
        if let Err(error) = theme.validate() {
            self.status = error;
            self.error = true;
            return;
        }
        let reference = Reference::capture(&theme);
        editor.references.retain(|r| r.id != reference.id);
        editor.references.push(reference);
        editor.load_theme(theme, Selection::Draft);
        self.status = "Imported for preview · Save to add it to your library and drawing".into();
        self.error = false;
    }
    pub(super) fn theme_generator_action(&mut self, event: Action) -> Task<Message> {
        // Any intervening manager action invalidates an older import dialog.
        self.theme_library.serial = self.theme_library.serial.wrapping_add(1);
        match event {
            Action::Open => {
                self.cancel_join();
                if self.cleanup.is_some() || !self.finish_inline(true) {
                    return Task::none();
                }
                self.theme_library.editor = Some(Editor::new(
                    &self.doc,
                    self.file_epoch,
                    self.theme_library.themes(),
                ));
                self.inspector_tab = InspectorTab::ThemeGenerator;
                self.inspector_open = true;
                self.palette = None;
                self.styles.editor = None;
                self.error = false;
                self.status = "Preview only · Save keeps and applies your theme".into();
            }
            Action::Back => {
                self.theme_library.editor = None;
                self.inspector_tab = InspectorTab::Properties;
            }
            Action::Import => return self.theme_file_action(super::theme_files::Action::Import),
            Action::Select(selection) => {
                let theme = match &selection {
                    Selection::New => {
                        let mut template = ThemeFile::capture(&Document {
                            color_theme: ColorTheme::Presentation,
                            ..Default::default()
                        });
                        template.id = fresh_id();
                        template.name = unique_name("My theme", self.theme_library.themes());
                        Recipe::PRESENTATION.generate(&template).ok()
                    }
                    Selection::Builtin(color_theme) => {
                        let mut theme = ThemeFile::capture(&Document {
                            color_theme: *color_theme,
                            ..Default::default()
                        });
                        theme.id = fresh_id();
                        theme.name = unique_name(
                            &format!("{} copy", theme.name),
                            self.theme_library.themes(),
                        );
                        Some(theme)
                    }
                    Selection::Saved { id, .. } => self
                        .theme_library
                        .themes()
                        .iter()
                        .find(|t| t.id == *id)
                        .cloned(),
                    Selection::Draft => None,
                };
                if let Some(theme) = theme
                    && let Some(editor) = &mut self.theme_library.editor
                {
                    editor.load_theme(
                        theme,
                        if selection == Selection::New {
                            Selection::Draft
                        } else {
                            selection
                        },
                    );
                    self.error = false;
                    self.status = "Preview only · Save to keep and apply this theme".into();
                }
            }
            Action::Delete => {
                let Some(editor) = &self.theme_library.editor else {
                    return Task::none();
                };
                let Selection::Saved { id, .. } = &editor.selection else {
                    return Task::none();
                };
                let id = id.clone();
                if let Err(error) = self.theme_library.remove(&id) {
                    self.status = error;
                    self.error = true;
                } else {
                    if let Some(editor) = &mut self.theme_library.editor {
                        editor.references.retain(|r| r.id != id);
                    }
                    let _ = self.theme_generator_action(Action::Select(Selection::New));
                    self.status =
                        "Theme removed from library · Existing drawings keep their colors".into();
                    self.error = false;
                }
            }
            Action::Apply | Action::Export => {
                let Some(editor) = &self.theme_library.editor else {
                    return Task::none();
                };
                let candidate = if editor.epoch != self.file_epoch
                    || editor.original != ThemeFile::capture(&self.doc)
                {
                    Err("The document theme changed. Reopen the generator before saving.".into())
                } else {
                    editor.candidate()
                };
                let theme = match candidate {
                    Ok(theme) => theme,
                    Err(error) => {
                        self.status = error;
                        self.error = true;
                        return Task::none();
                    }
                };
                if matches!(event, Action::Export) {
                    // Freeze inherited roles for a portable export while retaining
                    // authored RGB/OKLCH values and the generator recipe.
                    let mut snapshot = Document::default();
                    if let Err(error) = theme.apply(&mut snapshot) {
                        self.status = error;
                        self.error = true;
                        return Task::none();
                    }
                    let theme = ThemeFile::capture(&snapshot);
                    return Task::perform(
                        async move {
                            let Some(path) = super::files::save_path(
                                "Export theme (light and dark)",
                                &format!("{}.reshiki-theme", theme.id),
                                "reshiki-theme",
                            )
                            .await
                            else {
                                return Ok(false);
                            };
                            tokio::task::spawn_blocking(move || theme_files::save(&path, &theme))
                                .await
                                .map_err(|e| e.to_string())??;
                            Ok(true)
                        },
                        |r| Message::ThemeFile(super::theme_files::Action::Saved(r)),
                    );
                }
                // Persist first: a failed save must not change the drawing or lose the draft.
                if let Err(error) = self.theme_library.persist(theme.clone()) {
                    self.status = format!("Could not save theme: {error}");
                    self.error = true;
                    return Task::none();
                }
                if self.apply_theme_file(theme.clone()) {
                    let reference = Reference::capture(&theme);
                    let original = ThemeFile::capture(&self.doc);
                    if let Some(editor) = &mut self.theme_library.editor {
                        editor.references.retain(|r| r.id != reference.id);
                        editor.references.push(reference);
                        editor.load_theme(
                            theme.clone(),
                            Selection::Saved {
                                id: theme.id.clone(),
                                name: theme.name.clone(),
                            },
                        );
                        editor.original = original;
                    }
                    self.status =
                        "Theme saved and applied · Undo in the drawing restores previous colors"
                            .into();
                    self.error = false;
                }
            }

            edit => {
                let Some(editor) = &mut self.theme_library.editor else {
                    return Task::none();
                };
                self.error = false;
                match edit {
                    Action::Name(name) => editor.name = name,
                    Action::Reference(choice) => {
                        editor.recipe.reference = match choice {
                            ReferenceChoice::Jmol => None,
                            ReferenceChoice::Theme { id, .. } => {
                                let Some(reference) = editor.references.iter().find(|r| r.id == id)
                                else {
                                    return Task::none();
                                };
                                Some(reference.clone())
                            }
                        };
                        editor.refresh();
                    }
                    Action::Preset(preset) => {
                        editor.recipe = if preset == ColorTheme::Pastel {
                            Recipe::PASTEL
                        } else {
                            Recipe::PRESENTATION
                        };
                        editor.refresh();
                    }
                    Action::Lightness(mode, value) | Action::Chroma(mode, value)
                        if value.is_finite() =>
                    {
                        let tone = if mode.is_dark() {
                            &mut editor.recipe.dark
                        } else {
                            &mut editor.recipe.light
                        };
                        if matches!(edit, Action::Lightness(_, _)) {
                            tone.lightness = value.clamp(0., 100.) / 100.;
                        } else {
                            tone.chroma = value.clamp(0., Tone::MAX_CHROMA_FACTOR * 100.) / 100.;
                        }
                        editor.refresh();
                    }
                    Action::Preview(preview) => editor.preview = preview,
                    Action::Bold(bold) => editor.bold = bold,
                    Action::Fill(fill)
                        if reshiki::ring_fills::PALETTE.iter().any(|(n, _)| *n == fill) =>
                    {
                        editor.fill = fill
                    }
                    Action::Element(element)
                        if reshiki::editing::ELEMENTS.contains(&element.as_str()) =>
                    {
                        editor.element = element
                    }
                    _ => {}
                }
            }
        }
        Task::none()
    }
    pub(super) fn theme_generator_workspace(&self) -> Element<'_, Message> {
        let Some(editor) = &self.theme_library.editor else {
            return self.theme_generator_panel();
        };
        let valid = editor.candidate().is_ok();
        let mut choices: Vec<_> = ColorTheme::ALL
            .into_iter()
            .map(Selection::Builtin)
            .collect();
        choices.extend(
            self.theme_library
                .themes()
                .iter()
                .map(|t| Selection::Saved {
                    id: t.id.clone(),
                    name: t.name.clone(),
                }),
        );
        if editor.selection == Selection::Draft {
            choices.push(Selection::Draft);
        }
        choices.push(Selection::New);
        let can_delete = matches!(&editor.selection, Selection::Saved { id, .. } if self.theme_library.themes().iter().any(|t| t.id == *id));
        let controls = row![
            command("‹ Back to drawing", Action::Back),
            text("Themes").size(20),
            crate::appearance::pick_list(choices, Some(editor.selection.clone()), |s| action(
                Action::Select(s)
            ))
            .width(220)
            .text_size(13),
            icon_control(
                Icon::Trash,
                if can_delete {
                    "Delete saved theme from library"
                } else {
                    "Only saved custom themes can be deleted"
                },
                Action::Delete,
                can_delete
            ),
            Space::new().width(Length::Fill),
            preview_control(
                text("B").size(18).font(iced::Font {
                    weight: iced::font::Weight::Bold,
                    ..iced::Font::with_name(reshiki::style::ui_font_family())
                }),
                "Bold preview symbols",
                Action::Bold(!editor.bold),
                editor.bold,
            ),
            icon_control(
                Icon::Import,
                "Import theme for preview",
                Action::Import,
                true
            ),
            icon_control(
                Icon::Save,
                "Save theme to library and apply to drawing",
                Action::Apply,
                valid
            ),
            icon_control(Icon::Export, "Export theme…", Action::Export, valid),
        ]
        .spacing(16)
        .align_y(Alignment::Center);
        let mut tables = column![].spacing(18);
        for mode in CanvasTheme::ALL {
            tables = tables.push(self.theme_table(editor, mode));
        }
        tables = tables.push(
            text("Light and dark previews · Select an element to inspect its contrast")
                .size(11)
                .style(super::workspace::muted_text),
        );
        column![
            container(controls)
                .padding([14, 20])
                .style(super::workspace::panel),
            row![
                scrollable(container(tables).padding(20))
                    .id("theme-preview")
                    .width(Length::Fill)
                    .height(Length::Fill),
                container(self.theme_generator_panel())
                    .width(310)
                    .height(Length::Fill)
                    .style(super::workspace::panel)
            ]
            .height(Length::Fill),
        ]
        .height(Length::Fill)
        .into()
    }
    fn theme_table<'a>(&self, editor: &'a Editor, mode: CanvasTheme) -> Element<'a, Message> {
        let paper = rgb(mode.background());
        let foreground = rgb(mode.color([0; 3]));
        let mut molecule = preview_molecule();
        molecule.canvas_theme = mode;
        let _ = editor.palette.clone().apply(&mut molecule);
        if editor.preview == Preview::Rings
            && let Some((_, key)) = reshiki::ring_fills::PALETTE
                .iter()
                .find(|(name, _)| *name == editor.fill)
        {
            let ids = molecule.all_ids();
            reshiki::ring_fills::apply(&mut molecule, &ids, Some(*key));
        }
        let mut body = column![
            row![
                text(format!("{mode} · {}", editor.preview))
                    .size(17)
                    .color(foreground),
                Space::new().width(Length::Fill),
                canvas(DrawingThumbnail(molecule)).width(150).height(100),
            ]
            .align_y(Alignment::Center)
        ]
        .spacing(5);
        for symbols in ROWS {
            let mut cells = row![].spacing(4);
            for symbol in symbols.split_whitespace() {
                if symbol == "." {
                    cells = cells.push(Space::new().width(Length::Fill).height(46));
                    continue;
                }
                let (ink, bg) = editor.colors(symbol, mode);
                let number = reshiki::editing::ELEMENTS
                    .iter()
                    .position(|e| *e == symbol)
                    .map_or(0, |i| i + 1);
                let selected = editor.element == symbol;
                let symbol_font = iced::Font {
                    weight: if editor.bold {
                        iced::font::Weight::Bold
                    } else {
                        iced::font::Weight::Normal
                    },
                    ..iced::Font::with_name(reshiki::style::ui_font_family())
                };
                let label = column![
                    text(number.to_string()).size(9),
                    text(symbol).size(16).font(symbol_font)
                ]
                .spacing(1)
                .width(Length::Fill)
                .align_x(Alignment::Center);
                cells = cells.push(
                    button(label)
                        .padding(2)
                        .width(Length::Fill)
                        .height(46)
                        .on_press(action(Action::Element(symbol.into())))
                        .style(move |_, status| button::Style {
                            text_color: rgb(ink),
                            background: Some(rgb(bg).into()),
                            border: Border {
                                color: rgb(ink),
                                width: if selected || matches!(status, button::Status::Hovered) {
                                    1.5
                                } else {
                                    0.
                                },
                                radius: 5.into(),
                            },
                            ..Default::default()
                        }),
                );
            }
            body = body.push(cells);
        }
        let (ink, bg) = editor.colors(&editor.element, mode);
        let role = if matches!(editor.preview, Preview::Colors | Preview::Tiles) {
            "Symbol / tile"
        } else {
            "Label / background"
        };
        body = body.push(super::workspace::hover_hint(
            text(format!(
                "{}   {} on {}   ·   {role} {:.2}:1",
                editor.element,
                hex(ink),
                hex(bg),
                color_contrast::contrast(ink, bg)
            ))
            .size(11)
            .color(foreground),
            "Contrast measures brightness separation: 1:1 is identical, 21:1 is black and white. Small text needs at least 4.5:1; automatic canvas labels target 5:1.",
            tooltip::Position::Top,
        ));
        container(body)
            .padding(16)
            .width(Length::Fill)
            .style(move |_| container::Style {
                background: Some(paper.into()),
                border: Border {
                    color: Color::from_rgb8(120, 130, 130),
                    width: 0.5,
                    radius: 10.into(),
                },
                ..Default::default()
            })
            .into()
    }
    pub(super) fn theme_generator_panel(&self) -> Element<'_, Message> {
        let Some(editor) = &self.theme_library.editor else {
            return command("Manage themes…", Action::Open).into();
        };
        let mut body = column![
            text("Make it yours").size(19),
            text("Choose a reference. Adjust its lightness and color intensity.")
                .size(12)
                .style(super::workspace::muted_text),
            row![
                preview_control(
                    text("N").size(17),
                    "Element labels",
                    Action::Preview(Preview::Labels),
                    editor.preview == Preview::Labels
                ),
                preview_control(
                    canvas(Glyph(Icon::ColorTiles(false), true))
                        .width(24)
                        .height(24),
                    "Color tiles",
                    Action::Preview(Preview::Colors),
                    editor.preview == Preview::Colors
                ),
                preview_control(
                    canvas(Glyph(Icon::ColorTiles(true), true))
                        .width(24)
                        .height(24),
                    "Interface tiles",
                    Action::Preview(Preview::Tiles),
                    editor.preview == Preview::Tiles
                ),
                preview_control(
                    canvas(Glyph(Icon::Ring(6, false), true))
                        .width(24)
                        .height(24),
                    "Labels on ring fills",
                    Action::Preview(Preview::Rings),
                    editor.preview == Preview::Rings
                ),
            ]
            .spacing(6),
            text(editor.preview.to_string())
                .size(12)
                .style(super::workspace::muted_text),
        ]
        .spacing(12);
        if editor.preview == Preview::Rings {
            let fills = reshiki::ring_fills::PALETTE.map(|(name, key)| {
                let color = editor.fill_color(key, CanvasTheme::Light);
                preview_control(
                    container(Space::new().width(18).height(18)).style(move |_| container::Style {
                        background: Some(rgb(color).into()),
                        border: Border {
                            radius: 4.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
                    name,
                    Action::Fill(name.into()),
                    editor.fill == name,
                )
            });
            body = body.push(row(fills).spacing(5));
        }
        if matches!(editor.selection, Selection::Builtin(_)) {
            body = body.push(
                text("Built-in theme · Save creates a custom copy.")
                    .size(11)
                    .style(super::workspace::muted_text),
            );
        }
        if editor.selection == Selection::Draft
            && let Some(existing) = self
                .theme_library
                .themes()
                .iter()
                .find(|t| t.id == editor.palette.id)
        {
            body = body.push(
                text(format!(
                    "Save will update {} in your library.",
                    existing.name
                ))
                .size(11)
                .style(super::workspace::muted_text),
            );
        }
        if editor.palette.generator.is_none() {
            body = body.push(
                text("Original palette shown. Adjust a generation control to create new colors.")
                    .size(11)
                    .style(super::workspace::muted_text),
            );
        }
        body = body.push(
            column![
                text("Theme name").size(12),
                crate::appearance::text_input("My theme", &editor.name)
                    .on_input(|v| action(Action::Name(v)))
                    .padding(8)
                    .size(13),
                text("Reference theme").size(12),
                crate::appearance::pick_list(
                    std::iter::once(ReferenceChoice::Jmol)
                        .chain(editor.references.iter().map(ReferenceChoice::from))
                        .collect::<Vec<_>>(),
                    Some(
                        editor
                            .recipe
                            .reference
                            .as_ref()
                            .map_or(ReferenceChoice::Jmol, ReferenceChoice::from)
                    ),
                    |choice| action(Action::Reference(choice)),
                )
                .text_size(13)
                .width(Length::Fill),
                row![
                    super::workspace::hover_hint(
                        command("Presentation", Action::Preset(ColorTheme::Presentation)),
                        "Use Presentation settings with Jmol as the reference",
                        tooltip::Position::Top
                    ),
                    super::workspace::hover_hint(
                        command("Pastel", Action::Preset(ColorTheme::Pastel)),
                        "Use Pastel settings with Jmol as the reference",
                        tooltip::Position::Top
                    )
                ]
                .spacing(6),
            ]
            .spacing(12),
        );
        for mode in CanvasTheme::ALL {
            let tone = editor.recipe.tone(mode);
            body = body
                .push(super::workspace::horizontal_line())
                .push(text(format!("{mode} mode")).size(15));
            for (label, value, is_lightness) in [
                ("Lightness", tone.lightness, true),
                ("Color intensity", tone.chroma, false),
            ] {
                let maximum = if is_lightness {
                    100.
                } else {
                    Tone::MAX_CHROMA_FACTOR * 100.
                };
                body = body
                    .push(row![
                        text(label).size(12),
                        Space::new().width(Length::Fill),
                        text(format!("{:.0}%", value * 100.)).size(12)
                    ])
                    .push(
                        slider(0. ..=maximum, value * 100., move |v| {
                            action(if is_lightness {
                                Action::Lightness(mode, v)
                            } else {
                                Action::Chroma(mode, v)
                            })
                        })
                        .step(1.),
                    );
            }
        }
        body = body.push(text("Lightness: dark → light\nIntensity: 0% gray · 100% reference chroma · 200% twice reference chroma").size(11).style(super::workspace::muted_text))
            .push(text("Very vivid colors may reach the RGB display limit before 200%.").size(11).style(super::workspace::muted_text))
            .push(text("Contrast: 4.5:1 minimum for small text. Canvas labels aim for 5:1 and may adjust lightness to stay readable.").size(11).style(super::workspace::muted_text));
        body = body.push(super::workspace::horizontal_line())
            .push(text("Generation formula · OKLCH").size(13))
            .push(text("Reference RGB → OKLCH\nL = lightness / 100\nC = reference C × intensity / 100\nh = reference h").size(12))
            .push(text(format!("Light: L = {:.2}, C = reference C × {:.2}\nDark: L = {:.2}, C = reference C × {:.2}",
                editor.recipe.light.lightness, editor.recipe.light.chroma, editor.recipe.dark.lightness, editor.recipe.dark.chroma)).size(11).style(super::workspace::muted_text))
            .push(text("Each mode uses its matching reference palette. C and H labels stay neutral. RGB conversion reduces chroma if needed; the contrast check can then adjust label lightness.").size(11).style(super::workspace::muted_text));
        let candidate = editor.candidate();
        let mut footer = column![].spacing(8);
        if let Err(error) = &candidate {
            footer = footer.push(text(error.clone()).size(11));
        }
        if self.error {
            footer = footer.push(text(&self.status).size(11));
        }
        if !self.error {
            footer = footer.push(
                text(&self.status)
                    .size(11)
                    .style(super::workspace::muted_text),
            );
        }
        container(
            column![
                scrollable(container(body).padding(iced::Padding {
                    right: 14.,
                    ..Default::default()
                }))
                .height(Length::Fill),
                footer
            ]
            .spacing(14),
        )
        .padding(18)
        .height(Length::Fill)
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imported_inherited_ring_fills_preview_like_the_canvas() {
        let mut theme = ThemeFile::capture(&Document::default());
        theme.light.ring_fills.clear();
        theme.dark.ring_fills.clear();
        let mut editor = Editor::new(&Document::default(), 0, &[]);
        editor.load_theme(theme.clone(), Selection::Draft);
        editor.preview = Preview::Rings;
        for mode in CanvasTheme::ALL {
            let mut doc = preview_molecule();
            doc.canvas_theme = mode;
            theme.clone().apply(&mut doc).unwrap();
            for (name, key) in reshiki::ring_fills::PALETTE {
                editor.fill = name.into();
                let ids = doc.all_ids();
                reshiki::ring_fills::apply(&mut doc, &ids, Some(key));
                let expected = reshiki::canvas_theme::fill_color(&doc, &doc.ring_fills[0]);
                assert_eq!(editor.colors("N", mode).1, expected);
                assert_eq!(editor.fill_color(key, mode), expected);
            }
        }
    }
    #[test]
    fn imports_preserve_authored_palettes_until_controls_change_and_back_discards() {
        let (mut app, _) = App::new();
        let original = app.doc.clone();
        let _ = app.theme_generator_action(Action::Open);
        let template = theme_files::bundled().unwrap().remove(0);
        let mut imported = Recipe::PRESENTATION.generate(&template).unwrap();
        // Explicit palette values are authoritative even if the recipe differs.
        imported
            .light
            .elements
            .insert("N".into(), theme_files::ColorValue::Rgb([50, 110, 65]));
        app.review_imported_theme(imported.clone());
        assert_eq!(
            app.theme_library
                .editor
                .as_ref()
                .unwrap()
                .candidate()
                .unwrap(),
            imported
        );
        let _ = app.theme_generator_action(Action::Bold(false));
        let _ = app.theme_generator_action(Action::Preview(Preview::Tiles));
        assert_eq!(
            app.theme_library
                .editor
                .as_ref()
                .unwrap()
                .candidate()
                .unwrap(),
            imported
        );
        assert_eq!(app.doc, original);
        assert!(app.theme_library.themes().is_empty());
        let _ = app.theme_generator_action(Action::Back);
        assert_eq!(app.doc, original);
        assert!(app.theme_library.themes().is_empty());
        let _ = app.theme_generator_action(Action::Open);
        app.review_imported_theme(imported.clone());
        let _ = app.theme_generator_action(Action::Chroma(CanvasTheme::Light, 80.));
        assert_ne!(
            app.theme_library
                .editor
                .as_ref()
                .unwrap()
                .candidate()
                .unwrap()
                .light,
            imported.light
        );
        assert_eq!(app.doc, original);
    }
    #[test]
    fn manager_protects_defaults_updates_customs_and_deletes_without_recoloring() {
        let (mut app, _) = App::new();
        let original = app.doc.clone();
        let _ = app.theme_generator_action(Action::Open);
        for theme in ColorTheme::ALL {
            let _ = app.theme_generator_action(Action::Select(Selection::Builtin(theme)));
            let _ = app.theme_generator_action(Action::Delete);
            assert_eq!(app.doc, original);
            assert!(app.theme_library.themes().is_empty());
        }
        let _ = app
            .theme_generator_action(Action::Select(Selection::Builtin(ColorTheme::Presentation)));
        let _ = app.theme_generator_action(Action::Apply);
        let saved = app.theme_library.themes()[0].clone();
        assert_ne!(saved.id, "presentation");
        let _ = app.theme_generator_action(Action::Name("Renamed custom".into()));
        let _ = app.theme_generator_action(Action::Apply);
        assert_eq!(app.theme_library.themes().len(), 1);
        assert_eq!(app.theme_library.themes()[0].id, saved.id);
        assert_eq!(app.theme_library.themes()[0].name, "Renamed custom");
        let themed = app.doc.clone();
        let _ = app.theme_generator_action(Action::Delete);
        assert!(app.theme_library.themes().is_empty());
        assert_eq!(app.doc, themed);
        let (choices, _) = app.theme_choices();
        assert_eq!(choices.len(), 5); // Four protected defaults and Manage themes.
        assert_eq!(
            choices.last(),
            Some(&super::super::theme_files::Choice::Manage)
        );
        assert!(
            !choices
                .iter()
                .any(|c| matches!(c, super::super::theme_files::Choice::Library { .. }))
        );
    }
    #[test]
    fn new_drafts_get_distinct_names_and_imports_cannot_replace_a_newer_draft() {
        let (mut app, _) = App::new();
        let _ = app.theme_generator_action(Action::Open);
        let _ = app.theme_generator_action(Action::Select(Selection::New));
        let _ = app.theme_generator_action(Action::Apply);
        let _ = app.theme_generator_action(Action::Select(Selection::New));
        assert_eq!(
            app.theme_library.editor.as_ref().unwrap().name,
            "My theme 2"
        );
        let serial = app.theme_library.serial;
        let theme = theme_files::bundled().unwrap().remove(0);
        let _ = app.theme_generator_action(Action::Name("Newer draft".into()));
        let _ = app.theme_file_action(super::super::theme_files::Action::Loaded(
            app.file_epoch,
            serial,
            Ok(Some(Box::new(theme))),
        ));
        assert_eq!(
            app.theme_library.editor.as_ref().unwrap().name,
            "Newer draft"
        );
    }
    #[test]
    fn reference_choice_changes_both_modes_and_presets_restore_jmol() {
        let (mut app, _) = App::new();
        let _ = app.theme_generator_action(Action::Open);
        let editor = app.theme_library.editor.as_ref().unwrap();
        let reference = editor
            .references
            .iter()
            .find(|r| r.name == "Pastel")
            .unwrap()
            .clone();
        let before = editor.palette.clone();
        let _ = app.theme_generator_action(Action::Reference((&reference).into()));
        let editor = app.theme_library.editor.as_ref().unwrap();
        assert_eq!(editor.recipe.reference, Some(reference.clone()));
        assert_ne!(editor.palette.light.elements, before.light.elements);
        assert_ne!(editor.palette.dark.elements, before.dark.elements);
        let _ = app.theme_generator_action(Action::Apply);
        let _ = app.theme_generator_action(Action::Open);
        assert_eq!(
            app.theme_library.editor.as_ref().unwrap().recipe.reference,
            Some(reference)
        );
        let _ = app.theme_generator_action(Action::Preset(ColorTheme::Presentation));
        assert_eq!(
            app.theme_library.editor.as_ref().unwrap().recipe,
            Recipe::PRESENTATION
        );
    }
    #[test]
    fn preview_nitrogen_has_a_chemically_correct_visible_hydrogen() {
        let doc = preview_molecule();
        let molecule = reshiki::chemistry::document::prepare(&doc).unwrap();
        let nitrogen = doc.atoms.iter().position(|a| a.element == "N").unwrap();
        assert_eq!(molecule.state.valences[nitrogen].implicit_hydrogens, 1);
        assert_eq!(doc.atoms[nitrogen].label_h, 1);
        let svg = reshiki::scene::svg(&doc);
        let tree = roxmltree::Document::parse(&svg).unwrap();
        assert!(
            tree.descendants()
                .any(|n| n.is_text() && n.text() == Some("H")),
            "{svg}"
        );
    }
    #[test]
    fn color_tiles_keep_exact_element_colors_and_readable_symbols() {
        let mut editor = Editor::new(&Document::default(), 0, &[]);
        editor.preview = Preview::Colors;
        for lightness in [0., 0.52, 1.] {
            for chroma in [0., 0.65, 1., 1.5, 2.] {
                editor.recipe.light = reshiki::theme_generator::Tone { lightness, chroma };
                editor.recipe.dark = editor.recipe.light;
                editor.refresh();
                for mode in CanvasTheme::ALL {
                    for &element in reshiki::editing::ELEMENTS {
                        let (ink, background) = editor.colors(element, mode);
                        assert_eq!(background, editor.palette.element_color(element, mode));
                        assert!(ink == [0; 3] || ink == [255; 3]);
                        assert!(
                            color_contrast::contrast(ink, background) >= color_contrast::TEXT_MIN,
                            "{element}/{mode}/{lightness}/{chroma}"
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn drafts_cancel_without_edits_and_saved_recipes_reopen_with_undo() {
        let (mut app, _) = App::new();
        app.doc = reshiki::rings::Preset::Regular.document(42., false);
        let before = app.doc.clone();
        let _ = app.theme_generator_action(Action::Open);
        let _ = app.theme_generator_action(Action::Lightness(CanvasTheme::Light, 43.));
        let _ = app.theme_generator_action(Action::Chroma(CanvasTheme::Dark, 150.));
        let recipe = app.theme_library.editor.as_ref().unwrap().recipe.clone();
        assert_eq!(recipe.dark.chroma, 1.5);
        assert_eq!(app.doc, before);
        let _ = app.update(Message::Escape);
        assert!(app.theme_library.editor.is_none());
        assert_eq!(app.doc, before);
        let _ = app.theme_generator_action(Action::Open);
        let _ = app.theme_generator_action(Action::Name("My palette".into()));
        let _ = app.theme_generator_action(Action::Lightness(CanvasTheme::Light, 43.));
        let _ = app.theme_generator_action(Action::Chroma(CanvasTheme::Dark, 150.));
        let _ = app.theme_generator_action(Action::Apply);
        assert!(!app.error, "{}", app.status);
        let applied = app.doc.clone();
        assert_eq!(
            applied.custom_theme.as_ref().unwrap().generator,
            Some(recipe.clone())
        );
        assert_eq!(applied.drawing_style, before.drawing_style);
        assert_eq!(applied.atoms, before.atoms);
        assert_eq!(applied.canvas_theme, before.canvas_theme);
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, before);
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, applied);
        let _ = app.theme_generator_action(Action::Open);
        let editor = app.theme_library.editor.as_ref().unwrap();
        assert_eq!(editor.recipe, recipe);
        assert_eq!(editor.name, "My palette");
        assert_eq!(editor.palette.id, applied.custom_theme.unwrap().id);
    }
    #[test]
    fn stale_and_invalid_drafts_cannot_apply_and_tables_cover_every_element() {
        let (mut app, _) = App::new();
        let before = app.doc.clone();
        let _ = app.theme_generator_action(Action::Open);
        let _ = app.theme_generator_action(Action::Name(" ".into()));
        let _ = app.theme_generator_action(Action::Apply);
        assert!(app.error);
        assert_eq!(app.doc, before);
        let _ = app.theme_generator_action(Action::Name("Valid".into()));
        app.file_epoch += 1;
        let _ = app.theme_generator_action(Action::Apply);
        assert!(app.error);
        assert_eq!(app.doc, before);
        let symbols: std::collections::HashSet<_> = ROWS
            .iter()
            .flat_map(|row| row.split_whitespace())
            .filter(|s| *s != ".")
            .collect();
        assert_eq!(symbols.len(), 118);
        assert!(
            reshiki::editing::ELEMENTS
                .iter()
                .all(|e| symbols.contains(e))
        );
    }
    #[tokio::test]
    #[ignore = "Manual theme workbench snapshots in a temporary directory"]
    async fn theme_generator_headless_snapshot() {
        use iced::advanced::{layout, mouse, renderer::Headless, widget::Tree};
        let (mut app, _) = App::new();
        app.busy = false;
        let _ = app.theme_generator_action(Action::Open);
        let _ = app
            .theme_generator_action(Action::Select(Selection::Builtin(ColorTheme::Presentation)));
        let directory = std::env::temp_dir().join("reshiki-theme-generator-qa");
        std::fs::create_dir_all(&directory).unwrap();
        for (name, width, height, dark, preview) in [
            ("light", 1280, 820, false, Preview::Labels),
            ("regular", 1280, 820, false, Preview::Labels),
            ("dark", 1280, 820, true, Preview::Tiles),
            ("compact", 1040, 680, false, Preview::Rings),
            ("color-tiles-light", 1280, 820, false, Preview::Colors),
            ("color-tiles-dark", 1280, 820, true, Preview::Colors),
            ("high-intensity", 1280, 820, true, Preview::Colors),
            ("reference-formula", 1280, 820, true, Preview::Labels),
        ] {
            app.appearance.mode = if dark {
                crate::appearance::Mode::Dark
            } else {
                crate::appearance::Mode::Light
            };
            let _ = app.theme_generator_action(Action::Preview(preview));
            let _ = app.theme_generator_action(Action::Bold(name != "regular"));
            if name == "high-intensity" {
                let _ = app.theme_generator_action(Action::Chroma(CanvasTheme::Light, 150.));
                let _ = app.theme_generator_action(Action::Chroma(CanvasTheme::Dark, 200.));
            }
            if name == "reference-formula" {
                let reference = app
                    .theme_library
                    .editor
                    .as_ref()
                    .unwrap()
                    .references
                    .iter()
                    .find(|r| r.name == "Pastel")
                    .unwrap()
                    .clone();
                let _ = app.theme_generator_action(Action::Reference((&reference).into()));
            }
            let mut renderer = <iced::Renderer as Headless>::new(
                iced::Font::with_name(reshiki::style::ui_font_family()),
                iced::Pixels(16.),
                None,
            )
            .await
            .unwrap();
            let size = iced::Size::new(width as f32, height as f32);
            let theme = app.theme();
            let mut view = app.view();
            let mut tree = Tree::new(view.as_widget());
            let node =
                view.as_widget_mut()
                    .layout(&mut tree, &renderer, &layout::Limits::new(size, size));
            let mut messages = Vec::new();
            view.as_widget_mut().update(
                &mut tree,
                &iced::Event::Window(iced::window::Event::RedrawRequested(
                    std::time::Instant::now(),
                )),
                iced::advanced::Layout::new(&node),
                mouse::Cursor::Unavailable,
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut iced::advanced::Shell::new(&mut messages),
                &iced::Rectangle::with_size(size),
            );
            if name == "reference-formula" {
                for x in [300., width as f32 - 40.] {
                    view.as_widget_mut().update(
                        &mut tree,
                        &iced::Event::Mouse(mouse::Event::WheelScrolled {
                            delta: mouse::ScrollDelta::Pixels { x: 0., y: -2000. },
                        }),
                        iced::advanced::Layout::new(&node),
                        mouse::Cursor::Available(iced::Point::new(x, 350.)),
                        &renderer,
                        &mut iced::advanced::clipboard::Null,
                        &mut iced::advanced::Shell::new(&mut messages),
                        &iced::Rectangle::with_size(size),
                    );
                }
            }
            view.as_widget().draw(
                &tree,
                &mut renderer,
                &theme,
                &iced::advanced::renderer::Style::default(),
                iced::advanced::Layout::new(&node),
                mouse::Cursor::Unavailable,
                &iced::Rectangle::with_size(size),
            );
            let pixels = Headless::screenshot(
                &mut renderer,
                iced::Size::new(width, height),
                1.,
                theme.palette().background,
            );
            image::save_buffer(
                directory.join(format!("{name}.png")),
                &pixels,
                width,
                height,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
        eprintln!("Theme generator screenshots: {}", directory.display());
    }
}
