use crate::canvas::{self, Camera, Edit, Tool};
use iced::{Color, Element, Subscription, Task, Theme};
use reshiki::{
    document::{Arrow, Document, History, Point},
    editing::{self, Arrange, Transform},
    engine::{Analysis, LocalEngine, Request, Response},
    graphics::{BracketSides, Graphic, GraphicChange},
    recovery::{Candidate, Recovery},
    transaction::chemistry_changed,
};
use std::ops::ControlFlow;
use std::path::PathBuf;
mod abbreviations;
mod accessibility;
mod arcs;
mod arrows;
#[cfg_attr(target_arch = "wasm32", path = "app/assistant_browser.rs")]
mod assistant;
mod atom_edits;
mod atom_labels;
mod atom_text;
mod autosave;
mod bond_edits;
#[cfg(test)]
mod browser_fonts_tests;
mod canvas_edit;
mod cleanup;
mod clipboard;
mod color_popover;
mod command_palette;
mod context_menu;
mod depth_appearance;
mod dispatch;
mod document_styles;
mod document_tab;
use document_tab::DocumentTab;
mod engine_jobs;
mod event_slot;
mod figure_export;
mod file_shortcuts;
mod files;
#[cfg(target_os = "macos")]
mod macos_files;
#[cfg(target_os = "macos")]
pub(crate) use macos_files::install_document_events;
#[cfg(any(windows, target_os = "linux"))]
mod desktop;
mod gates;
mod generic;
mod graphics;
mod help;
mod history;
#[cfg(target_arch = "wasm32")]
mod host;
mod icons;
mod import;
mod inline_text;
mod inspector;
mod joining;
mod keyboard_drawing;
mod label_refresh;
mod molecule_shortcuts;
mod naming;
mod nmr;
mod numeric_transforms;
mod object_toolbar;
mod office;
#[cfg(any(windows, target_os = "linux"))]
pub(crate) use desktop::install_document_events as install_desktop_events;
mod models3d;
#[cfg(test)]
mod ops_parity_tests;
mod optimization;
mod pages;
mod palettes;
#[cfg(test)]
mod performance;
mod pictures;
mod popover;
mod printing;
mod prompt;
mod reaction_mapping;
mod reactions;
#[cfg(test)]
mod recovery_capture;
mod recovery_manager;
mod results;
mod ring_edits;
#[cfg(test)]
mod rotation_tests;
mod selection_edits;
mod shortcut_examples;
#[cfg(test)]
mod shortcut_focus_tests;
mod shortcuts;
mod startup;
mod tabs;
mod template_library;
#[cfg(test)]
mod template_style_evidence;
mod theme_files;
mod theme_generator;
mod tool_button;
#[cfg(test)]
mod transaction_tests;
mod typography;
mod updates;
mod view_settings;
#[cfg(all(target_os = "linux", feature = "wayland-qa"))]
mod wayland_qa;
#[cfg(windows)]
mod windows_libreoffice_save;
mod workspace;
#[cfg(target_arch = "wasm32")]
pub(crate) use files::read_length;
pub(crate) use workspace::text_width;

/// The status bar's idle message.
const READY: &str = "Ready · Choose a tool to start drawing";

// App::fit_bounds: screen pixels kept free around the fitted drawing, the
// smallest usable viewport side (pixels) and the smallest fitted extent (world
// units), so a tiny drawing is not zoomed in without bound.
const FIT_MARGIN: f32 = 80.0;
const MIN_VIEWPORT: f32 = 100.0;
const MIN_FIT_WIDTH: f32 = 240.0;
const MIN_FIT_HEIGHT: f32 = 200.0;
/// Fit's zoom range; interactive zoom elsewhere clamps to 0.005..5.
const MIN_ZOOM: f32 = 0.005;
const MAX_FIT_ZOOM: f32 = 2.5;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InspectorTab {
    Reactions,
    DrawingStyle,
    ThemeGenerator,
    Assistant,
    Pages,
    Abbreviations,
    Properties,
    /// Slow, model-based estimates that come with a report; NMR is its one
    /// section today. Instant calculated values stay in Properties.
    Predict,
    Labels,
    Templates,
    Import,
    Names,
    Export,
}

#[derive(Debug, Clone)]
pub enum Message {
    #[cfg(target_os = "linux")]
    LinuxClipboardWindow(iced::window::Id),
    Accessibility(accessibility::Action),
    ContextMenu(context_menu::Action),
    StyleMenu(color_popover::Action),
    ObjectToolbar(object_toolbar::Action),
    InspectorAction(inspector::Action),
    NumericTransform(numeric_transforms::Action),
    Nmr(nmr::Action),
    Updates(updates::Action),
    Reaction(reactions::Action),
    Mapping(reaction_mapping::Action),
    Generic(generic::Action),
    DrawingStyle(document_styles::Action),
    DepthAppearance(depth_appearance::Action),
    InlineText(inline_text::Action),
    AtomText(atom_text::Action),
    Join(joining::Action),
    KeyboardDrawing(keyboard_drawing::Action),
    Optimization(optimization::Action),
    Models3d(models3d::Action),
    Pages(pages::Action),
    Printing(printing::Action),
    Pictures(pictures::Action),
    Escape,
    Palette(palettes::Action),
    Assistant(assistant::Action),
    ContextKey(String),
    Shortcut(shortcuts::Action),
    AromaticDisplay,
    Abbreviations(abbreviations::Action),
    Labels(atom_labels::Action),
    LabelsReady(
        label_refresh::Key,
        Result<std::sync::Arc<reshiki::atom_labels::refresh::Refresh>, String>,
    ),
    Templates(template_library::Action),
    TemplateNavigate(bool),
    InspectorScroll(f32),
    ResetBondDrawing,
    FixedLength(bool),
    FixedAngles(bool),
    DrawingLength(String),
    ChainAtoms(String),
    ChainAngle(String),
    ApplyBondPreset(reshiki::bonds::BondPreset),
    BondPosition(reshiki::bonds::DoublePosition),
    BondColor(String),
    ApplyBondColor,
    Arc(arcs::Action),
    Graphics(graphics::Action),
    RemoveMark(u64, usize),
    RotateMark(u64, usize),
    AtomRadical(u8),
    ToggleInspector,
    Inspector(InspectorTab),
    Imports(import::Action),
    Naming(naming::Action),
    InsertInput,
    ToggleHelp,
    Help(help::Action),
    Commands(command_palette::Action),
    OpenShortcutExamples,
    Viewport(iced::Size),
    Canvas(Edit),
    Tool(Tool),
    Element(String),
    CaptionAction(iced::widget::text_editor::Action),
    TextStyle(reshiki::typography::StyleChange),
    FontSize(String),
    ApplyFontSize,
    ColorScope(typography::ColorScope),
    ClearRingFill,
    ClearHighlights,
    TextColor(String),
    ApplyTextColor,
    TextAlign(reshiki::typography::TextAlign),
    LabelAlign(reshiki::abbreviations::LabelAlignment),
    TextSpacing(f32),
    TextWidth(String),
    ApplyTextWidth,
    Import,
    Example(&'static str),
    Cleanup(cleanup::Action),
    Analyze,
    Undo,
    Redo,
    Delete,
    SelectAll,
    InvertSelection,
    Group,
    Ungroup,
    IntegralGroup(bool),
    AddFrame(reshiki::graphics::GraphicKind),
    View(view_settings::Action),
    Appearance(crate::appearance::Mode),
    CanvasTheme(reshiki::canvas_theme::CanvasTheme),
    ColorTheme(reshiki::canvas_theme::ColorTheme),
    QuickDrawingStyle(document_styles::Choice),
    ThemeFile(theme_files::Action),
    ThemeGenerator(theme_generator::Action),
    Fit,
    Zoom(f32),
    New,
    Open,
    Save,
    Export(&'static str),
    CopySmiles,
    Copy(bool),
    CopyImage,
    CopyAs(reshiki::clipboard::CopyFormat),
    CopyAsPrepared(
        clipboard::CopyAsKey,
        Box<Result<reshiki::clipboard::PreparedCopy, String>>,
    ),
    CopyAsWritten(clipboard::CopyAsKey, Result<Vec<String>, String>),
    ClipboardWritten {
        epoch: u64,
        revision: u64,
        cut_ids: Vec<u64>,
        result: Result<reshiki::clipboard::CopyOutcome, String>,
    },
    ClipboardRead {
        epoch: u64,
        revision: u64,
        result: Box<Result<reshiki::clipboard::PasteOutcome, String>>,
    },
    Paste,
    PastePicture,
    Pasted(Option<String>),
    Duplicate,
    Transform(Transform),
    Arrange(Arrange),
    ReverseBonds,
    BondDepth(bool),
    RingSize(u8),
    AromaticRing(bool),
    ToggleAromaticRing,
    ToggleSelectedRing,
    ArrowStyle(reshiki::arrows::Preset),
    ArrowAction(arrows::Action),
    CustomElement(String),
    ApplyElement,
    InsertTemplate(usize),
    SaveAs,
    Tick,
    Autosaved(autosave::Key, Result<Option<PathBuf>, String>),
    Restore,
    DismissRecovery,
    Recovery(recovery_manager::Action),
    Charge(i32),
    Isotope(String),
    ApplyIsotope,
    EngineDone {
        revision: u64,
        kind: Job,
        result: Box<Result<Response, String>>,
    },
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Legacy open event retained for editor-boundary tests"
        )
    )]
    Opened(Option<(PathBuf, Result<Vec<u8>, String>)>),
    FilePrepared(files::Opened),
    OfficePrepared(crate::session::Binding, files::Opened),
    #[cfg(any(windows, target_os = "linux"))]
    Desktop(desktop::Action),
    /// A request from the page hosting the browser editor.
    #[cfg(target_arch = "wasm32")]
    Host(crate::web::Request),
    #[cfg(target_os = "macos")]
    MacFiles(macos_files::Action),
    Tabs(tabs::Action),
    /// A task result for the drawing in this tab, which may no longer be in front.
    Tab(document_tab::TabId, Box<Message>),
    Saved(u64, Box<Document>, Result<Option<PathBuf>, String>),
    Exported(Result<Option<figure_export::Saved>, String>),
    FigureExported(Result<Option<figure_export::Saved>, String>),
    Close(iced::window::Id),
    Discard,
    Cancel,
    /// The answer to the app's own dialog; see prompt.rs.
    Prompt(prompt::Answer),
}
#[derive(Debug, Clone)]
pub enum Job {
    AromaticDisplay,
    Abbreviate,
    Import,
    ImportFile,
    Insert,
    Analyze,
    Clean(cleanup::CleanupJob),
    Export(&'static str),
}
/// What the save dialog's answer continues.
#[derive(Debug, Clone)]
enum Pending {
    /// Close this tab, which is in front.
    CloseTab(document_tab::TabId),
    /// Close the window after asking about each unsaved tab: the one asked
    /// about now, and those whose changes were discarded.
    CloseWindow(
        iced::window::Id,
        document_tab::TabId,
        Vec<document_tab::TabId>,
    ),
}

struct CleanupPreview {
    job: cleanup::CleanupJob,
    warnings: Vec<String>,
    document: Document,
    analysis: Option<Analysis>,
    revision: u64,
    epoch: u64,
    original: bool,
}

pub struct App {
    accessibility: accessibility::State,
    /// The active document; see `document_tab` for what is per document.
    tab: DocumentTab,
    tabs: tabs::State,
    context_menu: Option<context_menu::State>,
    style_menu: Option<color_popover::Menu>,
    updates: updates::State,
    theme_library: theme_files::State,
    palette: Option<palettes::Family>,
    toolbar: palettes::Memory,
    assistant: assistant::State,
    abbreviations: abbreviations::State,
    generic: generic::State,
    tool: Tool,
    element: String,
    printing: printing::State,
    font_options: iced::widget::combo_box::State<String>,
    imports: import::State,
    grid: bool,
    guides: canvas::guides::Guides,
    view_open: bool,
    appearance: crate::appearance::Settings,
    engine: LocalEngine,
    figure_exporting: bool,
    /// Serializes clipboard preparation/publication across document tabs.
    copy_as_busy: bool,
    /// Native writes can outlive a closed tab; do not let them overtake a new copy.
    native_copy_busy: bool,
    // The current tab's status; parked with its document when it leaves the front.
    status: String,
    error: bool,
    bond_text_hint: Option<(String, bool, String)>,
    file_io: files::State,
    office: office::State,
    #[cfg(any(windows, target_os = "linux"))]
    desktop: desktop::State,
    pending: Option<Pending>,
    /// The question the app's own dialog asks, in the browser.
    prompt: Option<prompt::Prompt>,
    ring_size: u8,
    aromatic_ring: bool,
    custom_element: String,
    recovered: Vec<Candidate>,
    recovery_manager: recovery_manager::State,
    exit: autosave::Exit,
    inspector_open: bool,
    inspector_tab: InspectorTab,
    help_open: bool,
    help: help::State,
    commands: command_palette::State,
    viewport: iced::Size,
    template_index: usize,
    templates: template_library::State,
    /// The last committed atom label, which a Text-tool double-click repeats.
    last_label: Option<String>,
    /// The last Text-tool click on an atom, to recognize a double-click.
    text_click: Option<(iced::time::Instant, u64)>,
}
/// How often drafts catch up with their drawings. A browser page can be
/// reloaded or closed at any moment, and cannot write when it unloads, so it
/// writes sooner; a draft is written only when its drawing changed.
const DRAFT_INTERVAL: std::time::Duration = if cfg!(target_arch = "wasm32") {
    std::time::Duration::from_secs(1)
} else {
    std::time::Duration::from_secs(5)
};

impl App {
    /// Starts a file epoch, dropping per-file work so late results are ignored and new work can start.
    pub(super) fn begin_file_epoch(&mut self) {
        self.tab.file_epoch = self.next_epoch();
        // Invalidate tickets before clearing gestures that may switch the front tab's tool.
        self.tab.naming.invalidate_name();
        self.tab.mapping.clear_pending();
        self.tab.arrows.cancel_source();
        self.cancel_reference_stretch();
        self.tab.nmr.clear_pending();
    }

    #[cfg(test)]
    pub fn new() -> (Self, Task<Message>) {
        Self::with_startup(crate::session::Arguments::default())
    }

    pub fn with_startup(startup: crate::session::Arguments) -> (Self, Task<Message>) {
        let recovery = if cfg!(test) {
            None
        } else {
            Recovery::standard().ok()
        };
        let recovered = recovery
            .as_ref()
            .map(|r| r.candidates())
            .unwrap_or_default();
        let recovery_root = recovery
            .as_ref()
            .and_then(|r| r.session.parent().map(PathBuf::from));
        let mut app = Self {
            accessibility: accessibility::State::default(),
            tab: DocumentTab::new(recovery),
            tabs: tabs::State::new(recovery_root),
            updates: updates::State::new(),
            theme_library: theme_files::State::load(),
            palette: None,
            toolbar: palettes::Memory::default(),
            assistant: assistant::State::new(),
            abbreviations: Default::default(),
            generic: Default::default(),
            context_menu: None,
            style_menu: None,
            tool: Tool::Select,
            element: "C".into(),
            printing: printing::State::default(),
            font_options: iced::widget::combo_box::State::new(
                reshiki::style::font_families()
                    .iter()
                    .map(|s| (*s).to_owned())
                    .collect(),
            ),
            imports: Default::default(),
            grid: false,
            guides: Default::default(),
            view_open: false,
            appearance: crate::appearance::Settings::load(),
            engine: LocalEngine::default(),
            figure_exporting: false,
            copy_as_busy: false,
            native_copy_busy: false,
            // A recovery offer in the status bar is the launch message.
            status: if recovered.is_empty() { READY } else { "" }.into(),
            error: false,
            bond_text_hint: None,
            file_io: files::State::default(),
            office: office::State::default(),
            #[cfg(any(windows, target_os = "linux"))]
            desktop: desktop::State::default(),
            pending: None,
            prompt: None,
            ring_size: 6,
            aromatic_ring: false,
            custom_element: String::new(),
            recovered,
            recovery_manager: recovery_manager::State::default(),
            exit: autosave::Exit::default(),
            inspector_open: true,
            inspector_tab: InspectorTab::Properties,
            help_open: false,
            help: help::State::default(),
            commands: command_palette::State::default(),
            viewport: iced::Size::new(850.0, 600.0),
            template_index: 0,
            templates: template_library::State::load(),
            last_label: None,
            text_click: None,
        };
        if let Some(message) = abbreviations::load_user_catalog() {
            if app.recovered.is_empty() {
                app.status = message.clone();
                app.error = true;
            }
            // Opening a drawing replaces the status; the panel keeps it.
            app.abbreviations.catalog_warning = Some(message);
        }
        let task = app.open_startup(startup);
        #[cfg(any(windows, target_os = "linux"))]
        let task = task.chain(Task::done(Message::Desktop(desktop::Action::Ready)));
        let update_check = app.update_action(updates::Action::Check(false));
        app.sync_keyboard_drawing();
        (app, Task::batch([task, update_check]))
    }
    pub fn title(&self) -> String {
        window_title(&self.document_name(), self.tab.shown_dirty())
    }
    fn document_name(&self) -> String {
        self.tab.name()
    }
    /// The saved document's file name without its extension, for exports.
    fn export_stem(&self) -> Option<String> {
        let stem = self.tab.path.as_ref()?.file_stem()?;
        Some(stem.to_string_lossy().into_owned())
    }
    pub fn theme(&self) -> Theme {
        if self.appearance.mode.is_dark(self.tab.doc.canvas_theme) {
            return Theme::custom(
                "ReShiki Dark",
                iced::theme::Palette {
                    background: Color::from_rgb8(20, 23, 28),
                    text: Color::from_rgb8(231, 236, 241),
                    primary: Color::from_rgb8(82, 193, 163),
                    success: Color::from_rgb8(82, 193, 163),
                    danger: Color::from_rgb8(239, 119, 111),
                    warning: Color::from_rgb8(225, 176, 86),
                },
            );
        }
        Theme::custom(
            "ReShiki",
            iced::theme::Palette {
                background: Color::from_rgb8(239, 241, 244),
                text: Color::from_rgb8(37, 43, 51),
                primary: Color::from_rgb8(17, 126, 108),
                success: Color::from_rgb8(17, 126, 108),
                danger: Color::from_rgb8(182, 66, 61),
                warning: Color::from_rgb8(174, 120, 42),
            },
        )
    }
    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            accessibility::subscription(),
            #[cfg(any(windows, target_os = "linux"))]
            desktop::subscription(self),
            #[cfg(target_os = "macos")]
            macos_files::subscription(),
            #[cfg(target_os = "linux")]
            iced::window::open_events().map(Message::LinuxClipboardWindow),
            self.updates.subscription(),
            self.properties_subscription(),
            if self.assistant.needs_poll() {
                iced::time::every(std::time::Duration::from_millis(200))
                    .map(|_| Message::Assistant(assistant::Action::Poll))
            } else {
                Subscription::none()
            },
            if self.strip().any(|tab| self.needs_draft(tab)) {
                iced::time::every(DRAFT_INTERVAL).map(|_| Message::Tick)
            } else {
                Subscription::none()
            },
            iced::window::close_requests().map(Message::Close),
            iced::event::listen_with(|event, status, _window| {
                if let iced::Event::Window(event) = &event {
                    return import::drag_event(event);
                }
                if status == iced::event::Status::Ignored
                    && let Some(forward) = template_library::navigation_event(&event)
                {
                    return Some(Message::TemplateNavigate(forward));
                }
                let event = crate::platform::shortcut_event(&event).unwrap_or(event);
                let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key,
                    modified_key,
                    modifiers,
                    repeat,
                    ..
                }) = event
                else {
                    return None;
                };
                if status == iced::event::Status::Captured {
                    return None;
                }
                if repeat
                    && modifiers.is_empty()
                    && key == iced::keyboard::Key::Named(iced::keyboard::key::Named::F8)
                {
                    return None;
                }
                shortcuts::key_message(&key, &modified_key, modifiers)
            }),
        ])
    }
    fn dirty(&self) -> bool {
        self.tab.dirty()
    }
    fn office_document(&self) -> bool {
        self.tab.office.as_ref().is_some_and(|binding| {
            self.tab.path.as_ref() == Some(&binding.path) && binding.writable()
        })
    }
    fn changed(&mut self, before: Document) {
        if self.tab.doc != before {
            self.tab.erase_stroke = false;
        }
        self.changed_continuing(before, false);
    }
    fn changed_continuing(&mut self, before: Document, continuing: bool) {
        if self.settles_unchanged(&before) {
            return;
        }
        self.prepare_document_change(self.tab.doc != before);
        let reconciled = match reshiki::transaction::reconcile(&mut self.tab.doc, before) {
            Ok(reconciled) => reconciled,
            Err(rejection) => {
                self.error = true;
                self.status = match rejection {
                    reshiki::transaction::Rejection::Reactions(error) => error,
                    reshiki::transaction::Rejection::Invalid(error) => {
                        format!("Edit cancelled: {error}")
                    }
                };
                return;
            }
        };
        self.commit_changed(reconciled, continuing);
    }
    fn try_replace_document(
        &mut self,
        mut candidate: Document,
        selected: Vec<u64>,
    ) -> Result<(), String> {
        if candidate == self.tab.doc && self.settles_unchanged(&candidate) {
            self.tab.selected = selected;
            return Ok(());
        }
        let before = self.tab.doc.clone();
        let reconciled = reshiki::transaction::reconcile(&mut candidate, before)
            .map_err(|error| error.message().to_owned())?;
        let changed = candidate != *reconciled.before();
        // Reference edits used to reconcile twice (a preflight, then changed).
        // Only the caption pass is not idempotent: run again, it detaches a
        // linked caption that the first pass moved while its molecule stayed.
        // Keep that pass so caption links come out as before.
        reshiki::molecule_names::reconcile(&mut candidate, reconciled.before());
        if changed {
            self.tab.erase_stroke = false;
        }
        self.prepare_document_change(changed);
        self.tab.doc = candidate;
        self.tab.selected = selected;
        self.commit_changed(reconciled, false);
        Ok(())
    }
    /// A commit that changes nothing, such as applying an unchanged text
    /// draft, settles a waiting label-layout repair the way closing the
    /// preview that held it back would, instead of recording it as an edit.
    fn settles_unchanged(&mut self, before: &Document) -> bool {
        if !self.label_layout_waiting() || self.tab.doc != *before {
            return false;
        }
        self.prepare_document_change(false);
        self.settle_label_layout();
        true
    }
    fn prepare_document_change(&mut self, changed: bool) {
        if changed {
            self.tab.arrows.cancel_source();
        }
        self.tab.cleanup = None;
    }
    fn commit_changed(&mut self, reconciled: reshiki::transaction::Reconciled, continuing: bool) {
        self.tab.recent_molecules.record(
            reconciled.before(),
            &self.tab.doc,
            self.tab.file_epoch,
            continuing,
        );
        self.tab.keyboard_drawing.record(
            reconciled.before(),
            &self.tab.doc,
            self.tab.file_epoch,
            continuing,
        );
        self.tab
            .nmr
            .invalidate_if_changed(&self.tab.doc, self.tab.file_epoch);
        let committed = reconciled.commit(&mut self.tab.doc, &mut self.tab.history, continuing);
        self.label_layout_settled();
        if committed.chemistry_changed {
            self.tab.labels_dirty = true;
            self.tab.chemistry_notice = None;
        }
        if committed.recorded {
            self.tab.revision = self.tab.revision.wrapping_add(1);
            if committed.chemistry_changed {
                self.tab.analysis = None;
            }
            self.error = false;
            self.status = if committed.chemistry_changed {
                "Drawing changed · Check structure to refresh properties"
            } else {
                "Drawing updated"
            }
            .into();
            self.sync_pictures();
        }
        if committed.drawing_style_changed {
            self.sync_drawing_defaults();
        }
        let existing: std::collections::HashSet<_> = self.tab.doc.all_ids().into_iter().collect();
        self.tab.selected.retain(|id| existing.contains(id));
    }
    fn fit(&mut self) {
        if self.display_document().all_ids().is_empty() {
            self.tab.pages.fit = None;
            self.tab.camera = Camera::default();
            self.tab.fit_to_view = false;
            return;
        }
        self.fit_bounds(self.display_document().bounds());
    }
    /// Centers and zooms the camera so the world rectangle `(lo, hi)` fits the paper viewport.
    pub(super) fn fit_bounds(&mut self, (lo, hi): (Point, Point)) {
        self.tab.pages.fit = None;
        self.tab.camera.center = Point::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0);
        let viewport = self.guides.paper(iced::Rectangle::with_size(self.viewport));
        let across =
            (viewport.width - FIT_MARGIN).max(MIN_VIEWPORT) / (hi.x - lo.x).max(MIN_FIT_WIDTH);
        let down =
            (viewport.height - FIT_MARGIN).max(MIN_VIEWPORT) / (hi.y - lo.y).max(MIN_FIT_HEIGHT);
        self.tab.camera.zoom = across.min(down).clamp(MIN_ZOOM, MAX_FIT_ZOOM);
        self.tab.fit_to_view = true;
    }
    /// Shows the save dialog for the tab in front; its answer continues `action`.
    fn ask(&mut self, action: Pending) -> Task<Message> {
        // The save dialog, or the save it started, still answers an earlier request.
        if self.pending.is_some() {
            return Task::none();
        }
        self.pending = Some(action);
        if prompt::IN_APP {
            self.prompt = Some(prompt::Prompt::Save(self.document_name()));
            return Task::none();
        }
        files::ask_to_save(self.document_name())
    }
    /// Save chosen in the save dialog passes the gates of transient editors,
    /// whose Close already reached the dialog.
    fn answers_save_dialog(&self, message: &Message) -> bool {
        self.pending.is_some() && matches!(message, Message::Save)
    }
    /// Continues the save dialog's action once the tab in front is saved.
    fn perform(&mut self, action: Pending) -> Task<Message> {
        match action {
            Pending::CloseTab(id) if id == self.tab.id => self.close_active_tab(),
            Pending::CloseTab(_) => Task::none(),
            Pending::CloseWindow(window, _, discarded) => self.close_window(window, discarded),
        }
    }
    /// The save dialog's answer acts on the tab it asked about, even if a file
    /// opened from Finder took the front meanwhile.
    fn front_pending(&mut self) {
        let (Some(Pending::CloseTab(id)) | Some(Pending::CloseWindow(_, id, _))) = self.pending
        else {
            return;
        };
        if let Some(index) = self.tab_index(id) {
            self.select_tab(index);
        }
    }
    /// A new drawing in a new tab, or in the tab in front if it is an unchanged
    /// empty Untitled drawing.
    fn new_document(&mut self) -> Task<Message> {
        if self.tab.reusable() {
            self.reset_tab();
        } else {
            self.add_tab();
        }
        self.tool = Tool::Select;
        self.sync_typography();
        self.error = false;
        self.status = "New document".into();
        iced::advanced::widget::operate(iced::advanced::widget::operation::focusable::unfocus())
    }
    fn display_document(&self) -> &Document {
        if let Some(session) =
            self.tab.optimization.as_ref().filter(|session| {
                session.current(self.tab.id, self.tab.file_epoch, self.tab.revision)
            })
        {
            return session.preview_document();
        }
        self.tab
            .cleanup
            .as_ref()
            .filter(|p| {
                !p.original && p.revision == self.tab.revision && p.epoch == self.tab.file_epoch
            })
            .map(|p| &p.document)
            .unwrap_or(&self.tab.doc)
    }
    pub fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.handle(message);
        self.settle_deferred_label_layout();
        // Any message may change the drawing, some (previews, background
        // labels) without a new revision, so the next view compares afresh.
        self.tab.forget_unsaved();
        task
    }
    fn handle(&mut self, message: Message) -> Task<Message> {
        let message = match self.update_untagged(message) {
            ControlFlow::Break(task) => return task,
            ControlFlow::Continue(message) => message,
        };
        let keyboard_before = (
            self.tab.keyboard_drawing.target(),
            self.tab.keyboard_drawing.marked(),
        );
        let controls_unchanged = matches!(
            &message,
            Message::Canvas(
                Edit::Hover(_) | Edit::RelaxDragTarget { .. } | Edit::RelaxRotate { .. }
            )
        );
        let task = self.update_routed(message);
        if controls_unchanged
            && keyboard_before
                == (
                    self.tab.keyboard_drawing.target(),
                    self.tab.keyboard_drawing.marked(),
                )
        {
            task
        } else {
            Task::batch([task, self.accessibility_refresh()])
        }
    }

    /// Native desktop ownership and accessibility actions belong to the window.
    /// Handle them before tab routing, modal gates and the routing tail; their
    /// tasks must not acquire a document tag.
    fn update_untagged(&mut self, message: Message) -> ControlFlow<Task<Message>, Message> {
        let message = match message {
            Message::Recovery(action) if recovery_manager::completion(&action) => {
                return ControlFlow::Break(self.recovery_action(action));
            }
            message => message,
        };
        #[cfg(any(windows, target_os = "linux"))]
        if let Message::Desktop(action) = message {
            return ControlFlow::Break(self.desktop_update(action));
        }
        #[cfg(target_arch = "wasm32")]
        if let Message::Host(request) = message {
            return ControlFlow::Break(self.host_request(request));
        }
        if let Message::Accessibility(action) = message {
            return ControlFlow::Break(self.accessibility_action(action));
        }
        // The answer goes on as the system dialog's would have.
        if let Message::Prompt(answer) = message {
            return ControlFlow::Break(match self.prompt.take() {
                Some(prompt) => self.update(prompt.answer(answer)),
                None => Task::none(),
            });
        }
        ControlFlow::Continue(message)
    }

    fn update_routed(&mut self, mut message: Message) -> Task<Message> {
        #[cfg(all(target_os = "linux", feature = "wayland-qa"))]
        let qa_event = wayland_qa::event(&message);
        // Timer polls belong to the Assistant's drawing before any front-tab
        // focus or menu handling runs.
        if matches!(&message, Message::Assistant(assistant::Action::Poll))
            && let Some(id) = self.assistant.tab.filter(|id| *id != self.tab.id)
        {
            message = Message::Tab(id, Box::new(message));
        }
        let task = match message {
            Message::Tab(id, message) if id == self.tab.id => self.update(*message),
            Message::Tab(id, message) => {
                let task = tagged(self.background_result(id, *message), id);
                Task::batch([task, self.start_autosave()])
            }
            message => {
                let task = self.update_front(message, false);
                // Results belong to the tab this message opened or brought forward.
                tagged(task, self.tab.id)
            }
        };
        #[cfg(all(target_os = "linux", feature = "wayland-qa"))]
        if let Some(event) = qa_event {
            wayland_qa::record(self, event);
        }
        if self.exit.frozen() {
            return task;
        }
        let task = Task::batch([task, self.finish_office_closes()]);
        // A failed draft removal, explicit save or library write may have
        // cancelled exit. Deliver held completions with their original tags
        // and ordinary epoch/revision guards before accepting more input.
        let deferred = std::mem::take(&mut self.tabs.deferred_results);
        Task::batch(
            std::iter::once(task).chain(
                deferred
                    .into_iter()
                    .map(|(id, message)| self.update(Message::Tab(id, Box::new(message)))),
            ),
        )
    }

    fn update_front(&mut self, message: Message, background: bool) -> Task<Message> {
        let message = match self.update_ungated(message) {
            ControlFlow::Break(task) => return task,
            ControlFlow::Continue(message) => message,
        };
        let previous = self.inspector_tab;
        let pointer_selection = match &message {
            Message::Canvas(Edit::Select(_)) => Some(
                self.tab
                    .hover
                    .filter(|(_, epoch)| *epoch == self.tab.file_epoch)
                    .map(|(point, _)| point),
            ),
            Message::Canvas(Edit::Click(point) | Edit::SelectAt(_, point)) => Some(Some(*point)),
            _ => None,
        };
        let message = match message {
            Message::Canvas(Edit::SelectAt(ids, _)) => Message::Canvas(Edit::Select(ids)),
            message => message,
        };
        // These open the inspector at the section they edit and scroll to it.
        let reveals_section = matches!(
            &message,
            Message::Canvas(Edit::BeginTransform(_))
                | Message::Mapping(reaction_mapping::Action::Open)
        );
        let refresh_dimensions = matches!(
            &message,
            Message::EngineDone { .. } | Message::LabelsReady(..)
        );
        let task = if background {
            self.update_document_result(message)
        } else {
            self.update_inner(message)
        };
        if background {
            // A background result belongs to this document, but the selected
            // tool belongs to the front tab. Only discard stale targets here.
            self.tab
                .keyboard_drawing
                .reconcile(&self.tab.doc, self.tab.file_epoch);
        } else {
            self.sync_keyboard_drawing();
        }
        if let Some(point) = pointer_selection {
            self.keyboard_pointer_selection(point);
        }
        let task = Task::batch([task, self.start_label_refresh(), self.start_autosave()]);
        self.tab.nmr.invalidate_if_changed_cached(
            &self.tab.doc,
            self.tab.file_epoch,
            self.tab.revision,
        );
        self.sync_numeric_transforms();
        if refresh_dimensions {
            self.refresh_numeric_dimensions();
        }
        self.sync_mapping_preview();
        // Include inspector changes made by tool-specific handlers, which can
        // return early. Ordinary updates within a panel retain its scroll state.
        if !background
            && !reveals_section
            && previous != self.inspector_tab
            && self.inspector_tab != InspectorTab::Assistant
        {
            Task::batch([
                task,
                iced::widget::operation::snap_to(
                    "inspector-content",
                    iced::widget::operation::RelativeOffset::START,
                ),
            ])
        } else {
            task
        }
    }

    /// Open completions bypass modal gates inside the routed front update,
    /// including its background-tab path. Their tasks still receive tab tags
    /// and run the routing tail that closes Office tabs and replays held results.
    fn update_ungated(&mut self, message: Message) -> ControlFlow<Task<Message>, Message> {
        if let Message::OfficePrepared(binding, opened) = message {
            return ControlFlow::Break(self.office_prepared(binding, opened));
        }
        #[cfg(target_os = "macos")]
        if let Message::MacFiles(action) = message {
            return ControlFlow::Break(self.mac_file_action(action));
        }
        ControlFlow::Continue(message)
    }

    fn update_inner(&mut self, message: Message) -> Task<Message> {
        let message = match self.gate(message) {
            ControlFlow::Continue(message) => message,
            ControlFlow::Break(task) => return task,
        };
        // Read before dispatch: handlers change the inspector tab and the selection.
        let reveal_inspector = self.reveals_inspector(&message);
        match self.dispatch_message(message) {
            Some(task) => task,
            None if reveal_inspector => iced::widget::operation::snap_to(
                "inspector-content",
                iced::widget::operation::RelativeOffset::START,
            ),
            None => Task::none(),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        // The command palette's Tab cycles its filter, unless a dialog is over it.
        let traverse =
            !self.commands.open || self.updates.open || self.assistant.viewed_image.is_some();
        reshiki::accessibility::focus_scope_with(
            file_shortcuts::wrap(
                // The command palette sits above the floating NMR panel so its
                // modal also keeps that panel from taking clicks.
                self.with_prompt(self.with_recovery_manager(self.with_updates(
                    self.with_assistant_image(self.with_commands(self.with_nmr(
                        self.with_atom_text(self.with_help(self.with_palette(self.workspace()))),
                    ))),
                ))),
                if self.prompt.is_some() {
                    Some(prompt::modal_key)
                } else if self.recovery_manager.open() {
                    Some(recovery_manager::modal_key)
                } else if self.help_open {
                    Some(help::modal_key)
                } else if self.commands.open {
                    Some(command_palette::modal_key)
                } else {
                    None
                },
                self.assistant.viewed_image.is_some(),
                self.updates.open,
                self.tab.atom_text.is_some(),
                self.inspector_open
                    && self.inspector_tab == InspectorTab::DrawingStyle
                    && self.tab.styles.editor.is_some()
                    && self.tab.inline_text.is_none()
                    && self.context_menu.is_none(),
            ),
            traverse,
        )
    }
}

/// The window's title: the document's name, marked while it has unsaved changes.
fn window_title(name: &str, dirty: bool) -> String {
    format!("{name}{} — ReShiki", if dirty { " •" } else { "" })
}

/// Hydrogen labels are a computed display cache, not unsaved drawing edits.
fn same_drawing(a: &Document, b: &Document) -> bool {
    a.version == b.version
        && a.canvas_theme == b.canvas_theme
        && a.color_theme == b.color_theme
        && a.custom_theme == b.custom_theme
        && a.drawing_style == b.drawing_style
        && a.page_layout == b.page_layout
        && a.atom_labels == b.atom_labels
        && a.bonds.len() == b.bonds.len()
        && a.bonds.iter().zip(&b.bonds).all(|(a, b)| same_bond(a, b))
        && a.annotations == b.annotations
        && a.molecule_names == b.molecule_names
        && a.arrows == b.arrows
        && a.arrow_labels == b.arrow_labels
        && a.graphics == b.graphics
        && a.groups == b.groups
        && a.reactions == b.reactions
        && a.models == b.models
        && a.generic == b.generic
        && a.abbreviations == b.abbreviations
        && a.ring_fills == b.ring_fills
        && a.depth_appearance == b.depth_appearance
        && a.atoms.len() == b.atoms.len()
        && a.atoms.iter().zip(&b.atoms).all(|(a, b)| same_atom(a, b))
}

/// Compares bonds without their computed CIP label; listing every field makes a
/// new one a compile error here instead of an edit the unsaved check misses.
fn same_bond(a: &reshiki::document::Bond, b: &reshiki::document::Bond) -> bool {
    let reshiki::document::Bond {
        highlight,
        ring_arc,
        projection,
        stereo_authoritative,
        z_order,
        indicator,
        cip_label: _,
        a: start,
        b: end,
        order,
        display,
        stereo,
        stereo_atoms,
        double_position,
        secondary_display,
        color,
    } = a;
    *highlight == b.highlight
        && *ring_arc == b.ring_arc
        && *projection == b.projection
        && *stereo_authoritative == b.stereo_authoritative
        && *z_order == b.z_order
        && *indicator == b.indicator
        && *start == b.a
        && *end == b.b
        && *order == b.order
        && *display == b.display
        && *stereo == b.stereo
        && *stereo_atoms == b.stereo_atoms
        && *double_position == b.double_position
        && *secondary_display == b.secondary_display
        && *color == b.color
}

/// Compares atoms without their computed hydrogen count and CIP label.
fn same_atom(a: &reshiki::document::Atom, b: &reshiki::document::Atom) -> bool {
    let reshiki::document::Atom {
        display,
        cip_label: _,
        id,
        element,
        position,
        depth,
        centroid,
        attachment,
        charge,
        radical_electrons,
        marks,
        mark_serial,
        isotope,
        explicit_h,
        no_implicit,
        aromatic,
        stereo,
        map_num,
        label_h: _,
        text_style,
    } = a;
    *display == b.display
        && *id == b.id
        && *element == b.element
        && *position == b.position
        && *depth == b.depth
        && *centroid == b.centroid
        && *attachment == b.attachment
        && *charge == b.charge
        && *radical_electrons == b.radical_electrons
        && *marks == b.marks
        && *mark_serial == b.mark_serial
        && *isotope == b.isotope
        && *explicit_h == b.explicit_h
        && *no_implicit == b.no_implicit
        && *aromatic == b.aromatic
        && *stereo == b.stereo
        && *map_num == b.map_num
        && *text_style == b.text_style
}

/// Marks the results of `task` as belonging to tab `id`, unless already marked.
fn tagged(task: Task<Message>, id: document_tab::TabId) -> Task<Message> {
    task.map(move |message| match message {
        Message::Tab(..) => message,
        Message::Recovery(action) if recovery_manager::completion(&action) => {
            Message::Recovery(action)
        }
        message => Message::Tab(id, Box::new(message)),
    })
}

fn export_file(
    contents: String,
    format: &'static str,
    stem: Option<String>,
    details: Vec<String>,
) -> Task<Message> {
    Task::perform(
        async move {
            Ok(save_export(contents.into_bytes(), format, stem)
                .await?
                .map(|path| figure_export::Saved { path, details }))
        },
        Message::Exported,
    )
}
/// Exports are named after the document, or "Molecule" while it is untitled.
async fn save_export(
    bytes: Vec<u8>,
    format: &'static str,
    stem: Option<String>,
) -> Result<Option<PathBuf>, String> {
    let extension = match format {
        "svg-office" => "svg",
        _ => format,
    };
    let stem = stem.as_deref().unwrap_or("Molecule");
    files::save_bytes(
        "Export drawing",
        &format!("{stem}.{extension}"),
        extension,
        &bytes,
    )
    .await
}
fn input_request(text: &str) -> Request {
    reshiki::clipboard::text_request(text)
}

#[cfg(test)]
mod tests;
