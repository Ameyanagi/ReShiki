use crate::canvas::{self, Camera, Edit, Tool};
use iced::{Color, Element, Subscription, Task, Theme};
use reshiki::{
    document::{Annotation, Arrow, Document, History, Point},
    editing::{self, Arrange, Transform},
    engine::{Analysis, ChemistryEngine, LocalEngine, Request, Response},
    graphics::{BracketSides, Graphic, GraphicChange, GraphicStyle},
    recovery::{Candidate, Recovery},
};
use std::path::PathBuf;
mod abbreviations;
mod accessibility;
mod arcs;
mod arrows;
mod assistant;
mod atom_edits;
mod atom_labels;
mod atom_text;
mod autosave;
mod bond_edits;
mod cleanup;
mod clipboard;
mod color_popover;
mod context_menu;
mod depth_appearance;
mod document_styles;
mod document_tab;
use document_tab::DocumentTab;
mod figure_export;
mod file_shortcuts;
mod files;
#[cfg(target_os = "macos")]
mod macos_files;
#[cfg(target_os = "macos")]
pub(crate) use macos_files::install_document_events;
mod graphics;
mod help;
mod icons;
mod import;
mod inline_text;
mod inspector;
mod joining;
mod keyboard_drawing;
mod label_refresh;
mod molecule_shortcuts;
mod numeric_transforms;
mod object_toolbar;
mod optimization;
mod pages;
mod palettes;
#[cfg(test)]
mod performance;
mod pictures;
mod popover;
mod printing;
mod reactions;
mod ring_edits;
#[cfg(test)]
mod rotation_tests;
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
mod typography;
mod updates;
#[cfg(all(target_os = "linux", feature = "wayland-qa"))]
mod wayland_qa;
#[cfg(windows)]
mod windows_libreoffice_save;
mod workspace;
pub(crate) use workspace::text_width;

/// The status bar's idle message.
const READY: &str = "Ready · Choose a tool to start drawing";

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InspectorTab {
    Reactions,
    DrawingStyle,
    ThemeGenerator,
    Assistant,
    Pages,
    Abbreviations,
    Properties,
    Labels,
    Templates,
    Import,
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
    Updates(updates::Action),
    Reaction(reactions::Action),
    DrawingStyle(document_styles::Action),
    DepthAppearance(depth_appearance::Action),
    InlineText(inline_text::Action),
    AtomText(atom_text::Action),
    Join(joining::Action),
    KeyboardDrawing(keyboard_drawing::Action),
    Optimization(optimization::Action),
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
    GraphicStyle(GraphicChange),
    GraphicWidth(String),
    ApplyGraphicWidth,
    GraphicStroke(String),
    ApplyGraphicStroke,
    GraphicFill(String),
    ApplyGraphicFill,
    GraphicSides(BracketSides),
    ScientificKind(reshiki::graphics::GraphicKind),
    OrbitalPhase(reshiki::scientific::Phase),
    FlipPhase(bool),
    AttachSymbols(bool),
    RemoveMark(u64, usize),
    RotateMark(u64, usize),
    AtomRadical(u8),
    ToggleInspector,
    Inspector(InspectorTab),
    Imports(import::Action),
    InsertInput,
    ToggleHelp,
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
    GroupLabelAlign(reshiki::abbreviations::LabelAlignment),
    TextSpacing(f32),
    TextWidth(String),
    ApplyTextWidth,
    Import,
    Example(&'static str),
    Clean,
    ApplyCleanup,
    CancelCleanup,
    CleanupOriginal(bool),
    CleanupScope(reshiki::cleanup::Scope),
    CleanupOrientation(bool),
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
    Grid,
    SmartGuides(bool),
    ToggleView,
    Appearance(crate::appearance::Mode),
    CanvasTheme(reshiki::canvas_theme::CanvasTheme),
    ColorTheme(reshiki::canvas_theme::ColorTheme),
    QuickDrawingStyle(document_styles::Choice),
    ThemeFile(theme_files::Action),
    ThemeGenerator(theme_generator::Action),
    Rulers(bool),
    Crosshair(bool),
    RulerUnit(canvas::guides::Unit),
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
    #[cfg(target_os = "macos")]
    MacFiles(macos_files::Action),
    Tabs(tabs::Action),
    /// A task result for the drawing in this tab, which may no longer be in front.
    Tab(document_tab::TabId, Box<Message>),
    Saved(u64, Box<Document>, Result<Option<PathBuf>, String>),
    Exported(Result<Option<PathBuf>, String>),
    FigureExported(Result<Option<figure_export::Saved>, String>),
    Close(iced::window::Id),
    Discard,
    Cancel,
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
    file_io: files::State,
    office_path: Option<PathBuf>,
    office_host: &'static str,
    pending: Option<Pending>,
    ring_size: u8,
    aromatic_ring: bool,
    custom_element: String,
    recovered: Vec<Candidate>,
    exit: autosave::Exit,
    inspector_open: bool,
    inspector_tab: InspectorTab,
    help_open: bool,
    viewport: iced::Size,
    template_index: usize,
    templates: template_library::State,
}
impl App {
    pub fn new() -> (Self, Task<Message>) {
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
            file_io: files::State::default(),
            office_path: None,
            office_host: "Office",
            pending: None,
            ring_size: 6,
            aromatic_ring: false,
            custom_element: String::new(),
            recovered,
            exit: autosave::Exit::default(),
            inspector_open: true,
            inspector_tab: InspectorTab::Properties,
            help_open: false,
            viewport: iced::Size::new(850.0, 600.0),
            template_index: 0,
            templates: template_library::State::load(),
        };
        let startup = if cfg!(test) {
            startup::Arguments::default()
        } else {
            startup::parse(std::env::args_os().skip(1))
        };
        let task = app.open_startup(startup);
        let update_check = app.update_action(updates::Action::Check(false));
        app.sync_keyboard_drawing();
        (app, Task::batch([task, update_check]))
    }
    pub fn title(&self) -> String {
        format!(
            "{}{} — ReShiki",
            self.document_name(),
            if self.dirty() { " •" } else { "" }
        )
    }
    fn document_name(&self) -> String {
        self.tab.name()
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
                iced::time::every(std::time::Duration::from_secs(5)).map(|_| Message::Tick)
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
        self.tab.path.is_some() && self.tab.path == self.office_path
    }
    fn run(&mut self, request: Request, kind: Job) -> Task<Message> {
        if self.tab.busy {
            return Task::none();
        }
        self.tab.busy = true;
        self.error = false;
        self.status = "Working…".into();
        let engine = self.engine.clone();
        let revision = self.tab.revision;
        let aromatic_selection = matches!(kind, Job::AromaticDisplay);
        Task::perform(
            async move {
                if aromatic_selection {
                    shortcuts::aromatic_selection(engine, request).await
                } else {
                    engine.execute(request).await
                }
            },
            move |result| Message::EngineDone {
                revision,
                kind: kind.clone(),
                result: Box::new(result),
            },
        )
    }
    fn changed(&mut self, before: Document) {
        if self.tab.doc != before {
            self.tab.erase_stroke = false;
        }
        self.changed_continuing(before, false);
    }
    fn changed_continuing(&mut self, before: Document, continuing: bool) {
        reshiki::projection::sync_centroids(&mut self.tab.doc);
        reshiki::ring_fills::prune(&mut self.tab.doc);
        reshiki::depth_appearance::prune(&mut self.tab.doc);
        self.tab.cleanup = None;
        self.tab.doc.reconcile_abbreviations(&before);
        if let Err(error) = reshiki::reactions::reconcile(&mut self.tab.doc) {
            self.tab.doc = before;
            self.error = true;
            self.status = error;
            return;
        }
        if self.tab.doc != before {
            if let Err(error) = self.tab.doc.validate() {
                self.tab.doc = before;
                self.error = true;
                self.status = format!("Edit cancelled: {error}");
                return;
            }
            self.tab.doc.reconcile_molecule_groups();
        }
        let drawing_style_changed = before.drawing_style != self.tab.doc.drawing_style;
        self.tab
            .recent_molecules
            .record(&before, &self.tab.doc, self.tab.file_epoch, continuing);
        self.tab
            .keyboard_drawing
            .record(&before, &self.tab.doc, self.tab.file_epoch, continuing);
        let chemistry_changed = chemistry_changed(&before, &self.tab.doc);
        if chemistry_changed {
            reshiki::atom_labels::clear_computed(&mut self.tab.doc);
            self.tab.labels_dirty = true;
            self.tab.chemistry_notice = None;
        }
        if self
            .tab
            .history
            .commit_continuing(before, &self.tab.doc, continuing)
        {
            self.tab.revision = self.tab.revision.wrapping_add(1);
            if chemistry_changed {
                self.tab.analysis = None;
            }
            self.error = false;
            self.status = if chemistry_changed {
                "Drawing changed · Check structure to refresh properties"
            } else {
                "Drawing updated"
            }
            .into();
            self.sync_pictures();
        }
        if drawing_style_changed {
            self.sync_drawing_defaults();
        }
        let existing: std::collections::HashSet<_> = self.tab.doc.all_ids().into_iter().collect();
        self.tab.selected.retain(|id| existing.contains(id));
    }
    fn fit(&mut self) {
        self.tab.pages.fit = None;
        if self.display_document().all_ids().is_empty() {
            self.tab.camera = Camera::default();
            self.tab.fit_to_view = false;
            return;
        }
        let (lo, hi) = self.display_document().bounds();
        self.tab.camera.center = Point::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0);
        let viewport = self.guides.paper(iced::Rectangle::with_size(self.viewport));
        self.tab.camera.zoom = ((viewport.width - 80.0).max(100.0) / (hi.x - lo.x).max(240.0))
            .min((viewport.height - 80.0).max(100.0) / (hi.y - lo.y).max(200.0))
            .clamp(0.005, 2.5);
        self.tab.fit_to_view = true;
    }
    /// Shows the save dialog for the tab in front; its answer continues `action`.
    fn ask(&mut self, action: Pending) -> Task<Message> {
        // The save dialog, or the save it started, still answers an earlier request.
        if self.pending.is_some() {
            return Task::none();
        }
        self.pending = Some(action);
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
    fn open_dialog(&mut self) -> Task<Message> {
        Task::perform(
            async {
                // Accept supported extensions without relying on macOS
                // type registration; the worker validates the content.
                let file = rfd::AsyncFileDialog::new()
                    .set_title("Open a ReShiki, MOL, RXN, CDXML, CDX or SMILES document")
                    .pick_file()
                    .await?;
                files::read(file.path().to_path_buf()).await
            },
            Message::FilePrepared,
        )
    }
    /// Opens a recovery draft in a new tab, or in the tab in front if it is an
    /// unchanged empty Untitled drawing.
    fn restore(&mut self, candidate: Candidate) {
        self.target_tab();
        let before = self.tab.doc.clone();
        self.tab.doc = candidate.snapshot.document;
        self.tab.doc.version = self.tab.doc.version.max(15);
        self.sync_drawing_defaults();
        self.tab.styles.editor = None;
        self.theme_library.editor = None;
        self.tab.path = None;
        self.tab.untitled_name = None;
        self.tab.saved = Document::default();
        self.tab.file_epoch = self.next_epoch();
        self.changed(before);
        self.tab.revision = self.tab.revision.wrapping_add(1);
        self.fit();
        self.tab.selected.clear();
        self.recover_candidate(candidate.path);
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
        if let Message::Accessibility(action) = message {
            return self.accessibility_action(action);
        }
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
        #[cfg(target_os = "macos")]
        if let Message::MacFiles(action) = message {
            return self.mac_file_action(action);
        }
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
        let opening_transform = matches!(&message, Message::Canvas(Edit::BeginTransform(_)));
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
        self.sync_numeric_transforms();
        if refresh_dimensions {
            self.refresh_numeric_dimensions();
        }
        // Include inspector changes made by tool-specific handlers, which can
        // return early. Ordinary updates within a panel retain its scroll state.
        if !background
            && !opening_transform
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

    fn update_inner(&mut self, message: Message) -> Task<Message> {
        if let Message::Autosaved(key, result) = message {
            return self.autosaved(key, result);
        }
        if let Message::Saved(epoch, snapshot, result) = message {
            return self.file_saved(epoch, snapshot, result);
        }
        if let Message::Templates(template_library::Action::Finished(serial, result)) = message {
            return self.template_finished(serial, result);
        }
        if let Message::Templates(template_library::Action::WarningAcknowledged(serial)) = message {
            return self.template_warning_acknowledged(serial);
        }
        if let Message::Templates(action @ template_library::Action::Imported(_)) = message {
            if let Some(task) = self.template_async(&action) {
                return task;
            }
            return self.template_action(action);
        }
        if self.exit.frozen()
            && !matches!(
                message,
                Message::Updates(updates::Action::RecoveryCleared | updates::Action::Restarted(_))
            )
        {
            if tabs::document_result(&message) {
                self.defer_document_result(self.tab.id, message);
            }
            return Task::none();
        }
        if let Message::LabelsReady(key, result) = message {
            self.labels_ready(key, result);
            return Task::none();
        }
        if let Some(task) = self.optimization_gate(&message) {
            return task;
        }
        if let Message::FilePrepared(opened) = message {
            return self.file_prepared(opened);
        }
        if self.updates.open {
            match message {
                Message::Updates(action) => return self.update_action(action),
                Message::Escape => return self.update_action(updates::Action::Show(false)),
                _ if !updates::background(&message) && !self.answers_save_dialog(&message) => {
                    return Task::none();
                }
                _ => {}
            }
        }
        let Some(message) = self.prepare_molecule_shortcut(message) else {
            return Task::none();
        };
        if self.updates.restarting && !matches!(message, Message::Updates(_)) {
            return Task::none();
        }
        // Before the atom label editor, which ignores other messages: it must
        // never open under the popover.
        if self.style_menu.is_some() {
            if matches!(message, Message::Escape) {
                return self.style_menu_action(self.style_menu_escape());
            }
            if !color_popover::keeps_open(&message) {
                self.close_style_menu();
            }
        }
        if let Message::Imports(
            action @ (import::Action::Hovered(_)
            | import::Action::Dropped(_)
            | import::Action::Left),
        ) = message
        {
            return self.import_drag(action);
        }
        if let Message::AtomText(action) = message {
            return self.atom_text_action(action);
        }
        if self.tab.atom_text.is_some() {
            if matches!(message, Message::Escape) {
                return self.atom_text_action(atom_text::Action::Cancel);
            }
            if !atom_text::background(&message) && !self.answers_save_dialog(&message) {
                return Task::none();
            }
        }
        if self.help_open && matches!(message, Message::Escape | Message::ToggleHelp) {
            self.help_open = false;
            return Task::none();
        }
        if let Message::ContextMenu(action) = message {
            return self.context_action(action);
        }
        if self.context_menu.is_some() && matches!(message, Message::Escape) {
            self.context_menu = None;
            return Task::none();
        }
        if (self.imports.menu || self.imports.examples_menu) && matches!(message, Message::Escape) {
            self.imports.menu = false;
            self.imports.examples_menu = false;
            return Task::none();
        }
        if self.tabs.menu && matches!(message, Message::Escape) {
            self.tabs.menu = false;
            return Task::none();
        }
        if !matches!(
            message,
            Message::Canvas(Edit::Hover(_))
                | Message::Tick
                | Message::InspectorScroll(_)
                | Message::EngineDone { .. }
                | Message::InspectorAction(_)
                | Message::Viewport(_)
                | Message::Updates(_)
                | Message::Imports(import::Action::Loaded(..))
        ) {
            self.context_menu = None;
            self.tab.inspector_ui.close_menu();
            if !matches!(
                message,
                Message::Imports(import::Action::Menu(_) | import::Action::ExamplesMenu(_))
            ) {
                self.imports.menu = false;
                self.imports.examples_menu = false;
            }
            if !matches!(message, Message::Tabs(tabs::Action::Menu(_))) {
                self.tabs.menu = false;
            }
        }
        if let Message::InspectorAction(action) = message {
            return self.inspector_action(action);
        }
        if let Message::Updates(action) = message {
            return self.update_action(action);
        }
        if let Message::Join(action) = message {
            return self.join_action(action);
        }
        if self.tab.joining.is_some()
            && matches!(message, Message::Escape | Message::TemplateNavigate(false))
        {
            return self.join_action(joining::Action::Cancel);
        }
        if self.tab.joining.is_some() && joining::cancels_draft(&message) {
            self.cancel_join();
        }
        if let Message::InlineText(action) = message {
            return self.inline_action(action);
        }
        if matches!(message, Message::Escape) && self.inspector_tab == InspectorTab::ThemeGenerator
        {
            return self.theme_generator_action(theme_generator::Action::Back);
        }
        if matches!(message, Message::Escape)
            && self.inspector_tab == InspectorTab::DrawingStyle
            && self.tab.styles.editor.is_some()
        {
            return self.drawing_style_action(document_styles::Action::Cancel);
        }
        if matches!(message, Message::Escape) {
            return if self.tab.inline_text.is_some() {
                self.inline_action(inline_text::Action::Finish(false))
            } else if self.tab.optimization.is_some() {
                self.optimization_action(optimization::Action::Cancel)
            } else {
                self.update(Message::Tool(Tool::Select))
            };
        }
        if self.tab.inline_text.is_some() && matches!(message, Message::Undo | Message::Redo) {
            return self.inline_action(inline_text::Action::Undo(matches!(message, Message::Redo)));
        }
        if let Message::Canvas(Edit::BeginText(id)) = message {
            return self.inline_action(inline_text::Action::Begin(Some(id), Point::default()));
        }
        if let Message::Canvas(Edit::Click(p)) = message
            && self.tool == Tool::Text
        {
            if let Some(id) = canvas::hit_object(&self.tab.doc, p, 8. / self.tab.camera.zoom)
                .filter(|id| self.tab.doc.atom(*id).is_some())
            {
                return self.atom_text_action(atom_text::Action::Begin(Some(id)));
            }
            let id = canvas::hit_object(&self.tab.doc, p, 8. / self.tab.camera.zoom)
                .filter(|id| self.tab.doc.annotations.iter().any(|a| a.id == *id));
            return self.inline_action(inline_text::Action::Begin(id, p));
        }
        if inline_text::commits_draft(&message) && !self.finish_inline(true) {
            return Task::none();
        }
        if let Message::Palette(action) = message {
            return self.palette_action(action);
        }
        if self.palette.is_some() && matches!(message, Message::Tool(Tool::Select)) {
            self.palette = None;
            return Task::none();
        }
        if self.assistant.menu.is_some() && matches!(message, Message::Tool(Tool::Select)) {
            self.assistant.menu = None;
            return Task::none();
        }
        if let Message::Assistant(action) = message {
            return self.assistant_action(action);
        }
        if self.tab.cleanup.is_some() {
            if matches!(message, Message::Tool(Tool::Select)) {
                return self.update(Message::CancelCleanup);
            }
            if !matches!(
                &message,
                Message::ApplyCleanup
                    | Message::CancelCleanup
                    | Message::CleanupOriginal(_)
                    | Message::CleanupScope(_)
                    | Message::CleanupOrientation(_)
                    | Message::Canvas(Edit::Pan(..) | Edit::Zoom(..) | Edit::Hover(_))
                    | Message::InspectorScroll(_)
                    | Message::Viewport(_)
                    | Message::Fit
                    | Message::Zoom(_)
                    | Message::ToggleInspector
                    | Message::Inspector(_)
                    | Message::Appearance(_)
                    | Message::ToggleView
                    | Message::ObjectToolbar(object_toolbar::Action::Visible(_))
                    | Message::Grid
                    | Message::SmartGuides(_)
                    | Message::Rulers(_)
                    | Message::Crosshair(_)
                    | Message::RulerUnit(_)
                    | Message::Tick
                    | Message::EngineDone { .. }
                    | Message::Close(_)
                    | Message::Discard
                    | Message::Cancel
                    | Message::New
                    | Message::Open
                    | Message::Tabs(_)
                    | Message::Saved(..)
                    | Message::Exported(_)
                    | Message::FigureExported(_)
                    | Message::Printing(
                        printing::Action::Prepared(..) | printing::Action::Finished(..)
                    )
                    | Message::Pictures(pictures::Action::Loaded(..))
                    | Message::Imports(import::Action::Loaded(..))
                    | Message::Opened(_)
                    | Message::ClipboardRead { .. }
                    | Message::ClipboardWritten { .. }
                    | Message::CopyAsPrepared(..)
                    | Message::CopyAsWritten(..)
            ) && !self.answers_save_dialog(&message)
            {
                if !matches!(message, Message::Canvas(_)) {
                    self.status = "Apply or cancel the cleanup preview to continue editing".into();
                }
                return Task::none();
            }
        }
        if matches!(
            &message,
            Message::Charge(_) | Message::AtomRadical(_) | Message::ApplyIsotope
        ) && self
            .tab
            .doc
            .abbreviations
            .iter()
            .any(|g| g.members.iter().any(|id| self.tab.selected.contains(id)))
        {
            self.status =
                "Expand the selected abbreviation before changing individual atom properties"
                    .into();
            self.error = true;
            return Task::none();
        }
        let reveal_inspector = matches!(
            &message,
            Message::Inspector(_)
                | Message::InsertTemplate(_)
                | Message::Tool(
                    Tool::Arrow | Tool::Graphic(_) | Tool::EditPoints | Tool::RingPreset(_)
                )
        ) || (self.inspector_tab != InspectorTab::Templates
            && matches!(&message, Message::Canvas(Edit::Select(ids)) if ids.iter().any(|id| self.tab.doc.annotations.iter().any(|a| a.id == *id) || self.tab.doc.graphics.iter().any(|g|g.id==*id))));
        match message {
            Message::Accessibility(_) => return Task::none(),
            Message::KeyboardDrawing(action) => return self.keyboard_drawing_action(action),
            Message::Optimization(action) => return self.optimization_action(action),
            Message::DepthAppearance(action) => return self.depth_appearance_action(action),
            Message::DrawingStyle(action) => return self.drawing_style_action(action),
            Message::Imports(action) => return self.import_action(action),
            Message::Pages(action) => return self.page_action(action),
            Message::Printing(action) => return self.print_action(action),
            Message::Pictures(action) => return self.picture_action(action),
            Message::Assistant(_)
            | Message::Updates(_)
            | Message::Palette(_)
            | Message::InlineText(_)
            | Message::AtomText(_)
            | Message::Join(_)
            | Message::Escape => {}
            Message::ContextKey(key) => return self.context_key(&key),
            Message::StyleMenu(action) => return self.style_menu_action(action),
            Message::Shortcut(action) => return self.shortcut_action(action),
            Message::AromaticDisplay => {
                if self.tab.selected.is_empty() {
                    self.status = "Select an aromatic ring first".into();
                    return Task::none();
                }
                let mut request = Request::molecule("aromatic", self.tab.doc.clone());
                request.selected_ids = Some(self.tab.selected.clone());
                return self.run(request, Job::AromaticDisplay);
            }
            Message::Abbreviations(action) => return self.abbreviation_action(action),
            Message::Labels(action) => self.label_action(action),
            Message::LabelsReady(..) => {}
            Message::InspectorScroll(y) => {
                if self.inspector_tab == InspectorTab::Templates && y.is_finite() {
                    self.templates.scroll = y.max(0.);
                }
            }
            Message::TemplateNavigate(forward) => {
                if self.inspector_open && self.inspector_tab == InspectorTab::Templates {
                    return self.template_action(if forward {
                        template_library::Action::Forward
                    } else {
                        template_library::Action::Browse
                    });
                }
            }
            Message::Reaction(action) => return self.reaction_action(action),
            Message::Templates(action) => {
                if let Some(task) = self.template_async(&action) {
                    return task;
                }
                return self.template_action(action);
            }
            Message::ResetBondDrawing => self.reset_bond_drawing(),
            Message::FixedLength(on) => self.tab.bond_drawing.fixed_length = on,
            Message::FixedAngles(on) => self.tab.bond_drawing.fixed_angles = on,
            Message::DrawingLength(value) => self.set_drawing_length(value),
            Message::ChainAtoms(value) => self.set_chain_atoms(value),
            Message::ChainAngle(value) => self.set_chain_angle(value),
            Message::ApplyBondPreset(preset) => self.apply_bond_preset(preset),
            Message::BondPosition(position) => self.set_double_position(position),
            Message::BondColor(value) => self.tab.bond_color_input = value,
            Message::ApplyBondColor => self.apply_bond_color(),
            Message::AddFrame(kind) => {
                let mut ids = self.tab.doc.complete_selection(&self.tab.selected);
                if let Some((lo, hi)) = reshiki::scene::selection_bounds(&self.tab.doc, &ids) {
                    let before = self.tab.doc.clone();
                    let id = self.tab.doc.next_id();
                    let padding = reshiki::style::DEFAULT.world(6.0);
                    self.tab.doc.graphics.push(Graphic::dragged(
                        id,
                        kind,
                        lo.offset(-padding, -padding),
                        hi.offset(padding, padding),
                        GraphicStyle {
                            width_pt: self.tab.doc.drawing_style.line_width_pt,
                            ..Default::default()
                        },
                        BracketSides::Both,
                        false,
                    ));
                    ids.push(id);
                    if let Ok(ids) = self.tab.doc.group_selection(&ids) {
                        self.tab.selected = ids;
                    }
                    self.changed(before);
                    self.tool = Tool::Select;
                    self.status = "Frame added and grouped with the selection".into();
                }
            }
            Message::Group => {
                let before = self.tab.doc.clone();
                match self.tab.doc.group_selection(&self.tab.selected) {
                    Ok(ids) => {
                        self.tab.selected = ids;
                        self.changed(before);
                        self.tool = Tool::Select;
                        self.status = format!(
                            "Grouped · {}-click selects a member · {} ungroups",
                            shortcuts::keys(iced::keyboard::Modifiers::ALT, ""),
                            shortcuts::label(&Message::Ungroup).unwrap_or_default()
                        );
                    }
                    Err(e) => {
                        self.status = e;
                        self.error = true;
                    }
                }
            }
            Message::Ungroup => {
                let before = self.tab.doc.clone();
                if self.tab.doc.ungroup_selection(&self.tab.selected) {
                    self.changed(before);
                    self.status = "Ungrouped one level".into();
                }
            }
            Message::IntegralGroup(integral) => {
                let before = self.tab.doc.clone();
                let ids = self.tab.doc.outer_selected_groups(&self.tab.selected);
                for g in &mut self.tab.doc.groups {
                    if ids.contains(&g.id) {
                        g.integral = integral;
                    }
                }
                self.changed(before);
            }
            Message::InvertSelection => {
                let selected = self.tab.doc.expand_groups(&self.tab.selected);
                self.tab.selected = self
                    .tab
                    .doc
                    .all_ids()
                    .into_iter()
                    .filter(|id| !selected.contains(id))
                    .collect();
                self.tool = Tool::Select;
                self.sync_typography();
                self.sync_graphics();
                self.sync_arrows();
            }
            Message::Arc(action) => self.update_arc(action),
            Message::GraphicStyle(change) => self.apply_graphic_style(change),
            Message::GraphicWidth(s) => self.tab.graphic_width_input = s,
            Message::ApplyGraphicWidth => self.apply_graphic_width(),
            Message::GraphicStroke(s) => self.tab.graphic_stroke_input = s,
            Message::ApplyGraphicStroke => self.apply_graphic_color(graphics::ColorField::Stroke),
            Message::GraphicFill(s) => self.tab.graphic_fill_input = s,
            Message::ApplyGraphicFill => self.apply_graphic_color(graphics::ColorField::Fill),
            Message::ScientificKind(kind) => self.set_scientific_kind(kind),
            Message::OrbitalPhase(phase) => self.set_orbital_phase(phase),
            Message::FlipPhase(value) => self.set_phase_flipped(value),
            Message::AttachSymbols(value) => self.tab.attach_symbols = value,
            Message::RotateMark(id, index) => self.rotate_mark(id, index),
            Message::RemoveMark(id, index) => self.remove_mark(id, index),
            Message::AtomRadical(value) => self.set_radical(value),
            Message::GraphicSides(sides) => self.set_graphic_sides(sides),
            Message::ToggleInspector => {
                self.inspector_open = !self.inspector_open;
                if self.inspector_open && self.inspector_tab == InspectorTab::Assistant {
                    return self.assistant_action(assistant::Action::Open);
                }
            }
            Message::Inspector(tab) => {
                if tab != InspectorTab::ThemeGenerator {
                    self.theme_library.editor = None;
                }
                if tab != InspectorTab::DrawingStyle {
                    self.tab.styles.editor = None;
                }
                if tab == InspectorTab::Labels {
                    let atoms: Vec<_> = self
                        .tab
                        .doc
                        .atoms
                        .iter()
                        .filter(|a| self.tab.selected.contains(&a.id))
                        .collect();
                    self.tab.labels.scope = if atoms.is_empty() {
                        atom_labels::Scope::Drawing
                    } else {
                        atom_labels::Scope::Selection
                    };
                    self.tab.labels.number = if let [atom] = atoms.as_slice() {
                        atom.display
                            .number
                            .as_ref()
                            .map(|n| n.text.clone())
                            .unwrap_or_default()
                    } else {
                        String::new()
                    };
                }
                self.inspector_tab = tab;
                self.inspector_open = true;
                if tab == InspectorTab::Import {
                    self.help_open = false;
                    return iced::widget::operation::focus(import::INPUT);
                }
            }
            Message::InsertInput => {
                return self.run(input_request(&self.imports.input.text()), Job::Insert);
            }
            Message::ToggleHelp => {
                self.help_open = !self.help_open;
                if self.help_open {
                    self.palette = None;
                }
            }
            Message::OpenShortcutExamples if self.pending.is_some() => {}
            Message::OpenShortcutExamples => return self.open_shortcut_examples(),
            Message::Viewport(size) => {
                if self.viewport != size {
                    self.viewport = size;
                    if let Some(index) = self.tab.pages.fit {
                        self.fit_pages(index);
                    } else if self.tab.fit_to_view {
                        self.fit();
                    }
                }
            }
            Message::InspectorAction(_) | Message::ContextMenu(_) => {}
            Message::Tool(tool) => {
                self.tab.erase_stroke = false;
                self.palette = None;
                self.toolbar.remember(tool);
                if let Some(option) = self.toolbar.graphic(tool) {
                    self.tab.graphic_style = option.style.clone();
                    self.tab.bracket_sides = option.sides;
                    self.tab.graphic_width_input = self.tab.graphic_style.width_pt.to_string();
                }
                self.tool = tool;
                self.error = false;
                if matches!(tool, Tool::Graphic(_) | Tool::RingPreset(_)) {
                    self.tab.selected.clear();
                }
                if matches!(
                    tool,
                    Tool::Arrow | Tool::Graphic(_) | Tool::EditPoints | Tool::RingPreset(_)
                ) {
                    self.inspector_open = true;
                    self.inspector_tab = InspectorTab::Properties;
                }
                if matches!(tool, Tool::Graphic(_)) {
                    self.sync_graphics();
                }
            }
            Message::Element(e) => self.choose_element(e),
            Message::CaptionAction(action) => self.caption_action(action),
            Message::TextStyle(change) => self.apply_text_style(change),
            Message::FontSize(value) => self.tab.font_size_input = value,
            Message::ApplyFontSize => match self.tab.font_size_input.parse::<f32>() {
                Ok(size) if size.is_finite() && (4.0..=144.0).contains(&size) => {
                    self.apply_text_style(reshiki::typography::StyleChange::Size(size))
                }
                _ => {
                    self.error = true;
                    self.status = "Enter a font size from 4 to 144 pt".into();
                }
            },
            Message::ClearRingFill => self.apply_ring_color(None),
            Message::ClearHighlights => self.apply_highlight_color(None),
            Message::ColorScope(scope) => {
                self.tab.color_scope = scope;
                self.sync_color_input();
                if scope == typography::ColorScope::Rings {
                    self.status = "Ring interiors · Select a ring, then choose a Tint color".into();
                } else if scope == typography::ColorScope::Highlights {
                    self.status =
                        "Highlights · Select atoms or bonds, then choose a Tint color".into();
                }
            }
            Message::TextColor(value) => {
                self.tab.text_color_input = value;
                self.flag_color_input(false);
            }
            Message::ApplyTextColor => {
                if let Some(rgb) = reshiki::palette::parse_color(&self.tab.text_color_input) {
                    // Typed colors are exact on both canvases.
                    let color = reshiki::palette::Color::Custom(rgb);
                    if self.tab.color_scope == typography::ColorScope::Rings {
                        self.apply_ring_color(Some(color));
                    } else {
                        self.apply_text_style(reshiki::typography::StyleChange::Color(color));
                    }
                } else {
                    self.error = true;
                    self.status = color_popover::HINT.into();
                    self.flag_color_input(true);
                }
            }
            Message::TextAlign(alignment) => self.apply_paragraph(Some(alignment), None, None),
            Message::GroupLabelAlign(alignment) => self.apply_group_alignment(alignment),
            Message::TextSpacing(spacing) => self.apply_paragraph(None, Some(spacing), None),
            Message::TextWidth(value) => self.tab.text_width_input = value,
            Message::ApplyTextWidth => {
                let width = self.tab.text_width_input.trim();
                if width.is_empty() {
                    self.apply_paragraph(None, None, Some(None));
                } else if let Ok(width) = width.parse::<f32>()
                    && width.is_finite()
                    && (10.0..=2000.0).contains(&width)
                {
                    self.apply_paragraph(None, None, Some(Some(width)));
                } else {
                    self.error = true;
                    self.status =
                        "Text width must be 10–2000 pt, or blank for automatic width".into();
                }
            }
            Message::Isotope(s) => self.tab.isotope = s,
            Message::RingSize(n) => self.set_ring_size(n),
            Message::AromaticRing(value) => self.set_aromatic_ring(value),
            Message::ToggleAromaticRing => return self.toggle_aromatic_ring(),
            Message::ToggleSelectedRing => self.toggle_selected_ring(),
            Message::ArrowStyle(style) => {
                self.tab.arrow_style = style;
                self.tab.arrows.style = reshiki::arrows::ArrowStyle::preset(style);
                self.tab.arrows.style.width_pt = self.tab.doc.drawing_style.line_width_pt;
                self.tool = Tool::Arrow;
                self.inspector_open = true;
                self.inspector_tab = InspectorTab::Properties;
                let before = self.tab.doc.clone();
                for a in &mut self.tab.doc.arrows {
                    if self.tab.selected.contains(&a.id) {
                        a.kind = style.kind().into();
                        a.control = None;
                        a.style = Some(self.tab.arrows.style.clone());
                    }
                }
                self.changed(before);
                self.sync_arrows();
            }
            Message::ArrowAction(action) => self.arrow_action(action),
            Message::CustomElement(s) => self.custom_element = s,
            Message::ApplyElement => self.apply_custom_element(),
            Message::CopyImage => return self.copy_native(false, true),
            Message::CopyAs(format) => return self.copy_as(format),
            Message::Copy(cut) => {
                if self.clipboard_working() {
                    self.status = "A clipboard operation is already in progress".into();
                    return Task::none();
                }
                if reshiki::clipboard::available() {
                    return self.copy_native(cut, false);
                }
                if self.tab.selected.is_empty() {
                    self.status = "Select objects to copy".into();
                    return Task::none();
                }
                let selection = editing::selection(&self.tab.doc, &self.tab.selected);
                if let Ok(json) = serde_json::to_string(&selection.current()) {
                    if cut {
                        let before = self.tab.doc.clone();
                        self.tab.doc.delete(&self.tab.selected);
                        self.tab.selected.clear();
                        self.changed(before);
                    }
                    self.status = if cut {
                        "Selection cut"
                    } else {
                        "Selection copied"
                    }
                    .into();
                    return iced::clipboard::write(format!("{}{json}", editing::CLIPBOARD_PREFIX));
                }
            }
            Message::PastePicture => return self.paste_native(true),
            Message::Paste => {
                if reshiki::clipboard::available() {
                    return self.paste_native(false);
                }
                return iced::clipboard::read().map(Message::Pasted);
            }
            Message::Duplicate => {
                let part = editing::selection(&self.tab.doc, &self.tab.selected);
                let before = self.tab.doc.clone();
                self.tab.selected =
                    editing::append(&mut self.tab.doc, &part, Point::new(28.0, 28.0));
                self.changed(before);
                self.tool = Tool::Select;
            }
            Message::Transform(transform) => {
                let before = self.tab.doc.clone();
                editing::transform(&mut self.tab.doc, &self.tab.selected, transform);
                self.changed(before);
            }
            Message::NumericTransform(action) => return self.numeric_transform_action(action),
            Message::Arrange(arrange) => {
                let before = self.tab.doc.clone();
                editing::arrange(&mut self.tab.doc, &self.tab.selected, arrange);
                self.changed(before);
            }
            Message::BondDepth(front) => self.layer_objects(front, false, true),
            Message::ReverseBonds => self.reverse_selected_bonds(),
            Message::InsertTemplate(index) => {
                if self.templates.library.get(index).is_some()
                    && (!self.templates.active || self.template_index != index)
                {
                    self.templates.remember(self.template_index);
                }
                if let Some(template) = self.templates.library.get(index) {
                    if !self.templates.active || self.template_index != index {
                        self.templates.anchor = template.anchor;
                        if matches!(template.anchor, reshiki::templates::Anchor::Bond(..)) {
                            self.templates.connection = reshiki::templates::Connection::FuseBond;
                        } else if matches!(template.anchor, reshiki::templates::Anchor::Atom(_))
                            && self.templates.connection == reshiki::templates::Connection::FuseBond
                        {
                            self.templates.connection = reshiki::templates::Connection::Connect;
                        }
                    }
                    self.template_index = index;
                    self.templates.active = true;
                    self.tool = Tool::Template;
                    self.error = false;
                    self.status = format!(
                        "{} · {} · Escape cancels",
                        template.name, self.templates.connection
                    );
                }
            }
            Message::Tick => self.request_drafts(),
            Message::Autosaved(..) => {} // Handled before editor/modal guards.
            Message::Restore if self.pending.is_some() => {}
            Message::Restore => {
                // Oldest first, so the latest draft ends up in front.
                let candidates = std::mem::take(&mut self.recovered);
                let count = candidates.len();
                for candidate in candidates.into_iter().rev() {
                    self.restore(candidate);
                }
                self.status = match count {
                    0 => return Task::none(),
                    1 => "Recovered drawing · Save to keep a new copy".into(),
                    n => format!("Recovered {n} drawings as tabs · Save them to keep new copies"),
                };
            }
            Message::DismissRecovery => {
                self.recovered.clear();
                if self.status.is_empty() {
                    self.status = READY.into();
                }
            }
            Message::Canvas(Edit::BeginTransform(field)) => {
                return self.begin_numeric_transform(field);
            }
            Message::Canvas(edit) => {
                if let Some(task) = self.optimization_edit(edit.clone()) {
                    return task;
                }
                self.edit(edit);
            }
            Message::Appearance(mode) => {
                self.appearance.mode = mode;
                if let Err(error) = self.appearance.save() {
                    self.status =
                        format!("Appearance changed, but could not save preference: {error}");
                    self.error = true;
                }
            }
            Message::ColorTheme(theme) => {
                if !self.finish_inline(true) {
                    return Task::none();
                }
                let before = self.tab.doc.clone();
                theme.apply(&mut self.tab.doc);
                self.changed(before);
                self.sync_color_input();
                self.status = format!(
                    "{theme} colors · Journal dimensions unchanged · Undo restores previous colors"
                );
            }
            Message::CanvasTheme(theme) => {
                if !self.finish_inline(true) {
                    return Task::none();
                }
                if self.tab.doc.canvas_theme != theme {
                    let before = self.tab.doc.clone();
                    self.tab.doc.canvas_theme = theme;
                    self.changed(before);
                    self.sync_color_input();
                    self.status = format!(
                        "{theme} canvas · Copies retain ink colors on a transparent background"
                    );
                }
            }
            Message::ThemeFile(action) => return self.theme_file_action(action),
            Message::ThemeGenerator(action) => return self.theme_generator_action(action),
            Message::QuickDrawingStyle(choice) => return self.quick_drawing_style(choice),
            Message::Grid => self.grid = !self.grid,
            Message::SmartGuides(enabled) => {
                self.appearance.smart_guides = enabled;
                if let Err(error) = self.appearance.save() {
                    self.status = format!("Could not save smart guides preference: {error}");
                    self.error = true;
                }
            }
            Message::ToggleView => self.view_open = !self.view_open,
            Message::ObjectToolbar(action) => self.object_toolbar_action(action),
            Message::Rulers(enabled) => {
                self.guides.rulers = enabled;
                if self.tab.fit_to_view {
                    self.fit();
                }
            }
            Message::Crosshair(enabled) => self.guides.crosshair = enabled,
            Message::RulerUnit(unit) => self.guides.unit = unit,
            Message::Fit => self.fit(),
            Message::Zoom(f) => {
                self.tab.pages.fit = None;
                self.tab.fit_to_view = false;
                self.tab.camera.zoom = (self.tab.camera.zoom * f).clamp(0.005, 5.0);
            }
            Message::Import => {
                return self.run(input_request(&self.imports.input.text()), Job::Import);
            }
            Message::Example(smiles) => {
                self.imports.set_text(smiles);
                return self.run(Request::import_smiles(smiles), Job::Insert);
            }
            Message::Analyze => {
                return self.run(
                    Request::molecule("analyze", self.tab.doc.clone()),
                    Job::Analyze,
                );
            }
            Message::CleanupScope(scope) => return self.begin_cleanup(Some(scope), None),
            Message::CleanupOrientation(on) => return self.begin_cleanup(None, Some(on)),
            Message::CleanupOriginal(original) => {
                if let Some(preview) = &mut self.tab.cleanup {
                    preview.original = original;
                }
            }
            Message::CancelCleanup => {
                self.tab.cleanup_serial = self.tab.cleanup_serial.wrapping_add(1);
                self.tab.cleanup = None;
                self.status = "Cleanup cancelled · Drawing unchanged".into();
                self.error = false;
            }
            Message::ApplyCleanup => {
                if self.tab.busy {
                    return Task::none();
                }
                if let Some(preview) = self.tab.cleanup.take() {
                    if preview.revision != self.tab.revision || preview.epoch != self.tab.file_epoch
                    {
                        self.status = "Drawing changed · Run cleanup again".into();
                        return Task::none();
                    }
                    let before = self.tab.doc.clone();
                    self.tab.doc = preview.document;
                    self.changed(before);
                    if !self.error {
                        self.tab.analysis = preview.analysis;
                        self.status = "Cleanup applied · Undo restores the original layout".into();
                    }
                }
            }
            Message::Clean => return self.begin_cleanup(None, None),
            Message::Undo | Message::Redo => {
                self.tab.erase_stroke = false;
                self.tab.cleanup = None;
                let before = self.tab.doc.clone();
                let selected_group = !before.outer_selected_groups(&self.tab.selected).is_empty();
                let changed = if matches!(message, Message::Undo) {
                    self.tab.history.undo(&mut self.tab.doc)
                } else {
                    self.tab.history.redo(&mut self.tab.doc)
                };
                if changed {
                    self.tab.keyboard_drawing.restore(
                        matches!(message, Message::Redo),
                        &self.tab.doc,
                        self.tab.file_epoch,
                    );
                    self.tab
                        .recent_molecules
                        .restore(matches!(message, Message::Redo), self.tab.file_epoch);
                    self.tab.revision = self.tab.revision.wrapping_add(1);
                    if chemistry_changed(&before, &self.tab.doc) {
                        self.tab.analysis = None;
                        reshiki::atom_labels::clear_computed(&mut self.tab.doc);
                        self.tab.labels_dirty = true;
                    }
                    let ids = self.tab.doc.all_ids();
                    self.tab.selected.retain(|id| ids.contains(id));
                    let previous_ids = before.all_ids();
                    let restored_group = self.tab.doc.groups.iter().any(|group| {
                        group
                            .members
                            .iter()
                            .any(|id| self.tab.selected.contains(id))
                            && group.members.iter().all(|id| {
                                self.tab.selected.contains(id) || !previous_ids.contains(id)
                            })
                    });
                    if selected_group || restored_group {
                        self.tab.selected = self.tab.doc.expand_groups(&self.tab.selected);
                    }
                    if self.tab.selected.is_empty()
                        && let Some(id) = self.tab.caption_target.filter(|id| ids.contains(id))
                    {
                        self.tab.selected.push(id);
                    }
                    self.sync_typography();
                    self.sync_graphics();
                    self.sync_arrows();
                    self.sync_bonds();
                    if before.drawing_style != self.tab.doc.drawing_style {
                        self.sync_drawing_defaults();
                        self.tab.styles.editor = None;
                    }
                    if before.page_layout != self.tab.doc.page_layout {
                        self.tab.pages.editor = self
                            .tab
                            .pages
                            .editor
                            .as_ref()
                            .map(|_| pages::Editor::new(&self.tab.doc, self.tab.file_epoch));
                        if let Some(layout) = &self.tab.doc.page_layout {
                            self.tab.pages.active =
                                self.tab.pages.active.min(layout.count().saturating_sub(1));
                            self.fit_pages(Some(self.tab.pages.active));
                        } else {
                            self.fit();
                        }
                    }
                    self.status = "History restored".into();
                    self.error = false;
                }
            }
            Message::Delete => {
                let before = self.tab.doc.clone();
                self.tab.doc.delete(&self.tab.selected);
                self.changed(before);
            }
            Message::SelectAll => {
                self.tab.selected = self.tab.doc.all_ids();
                self.tool = Tool::Select;
                self.sync_typography();
                self.sync_graphics();
                self.sync_arrows();
                self.sync_bonds();
            }
            Message::Charge(delta) => self.change_charge(delta),
            Message::ApplyIsotope => self.apply_isotope(),
            Message::CopySmiles => {
                if self.clipboard_working() {
                    self.status = "A clipboard operation is already in progress".into();
                    return Task::none();
                }
                if let Some(a) = self.property_analysis() {
                    return iced::clipboard::write(a.smiles.clone());
                }
            }
            // Tabs wait while the save dialog asks about the tab in front.
            Message::New | Message::Open if self.pending.is_some() => {}
            Message::New => return self.new_document(),
            Message::Open => return self.open_dialog(),
            Message::Tabs(action) => return self.tab_action(action),
            Message::Tab(..) => {}
            Message::Close(window) => {
                if self.pending.is_none() {
                    return self.close_window(window, vec![]);
                }
            }
            #[cfg(target_os = "linux")]
            Message::LinuxClipboardWindow(window) => {
                return iced::window::run(window, |window| {
                    reshiki_linux::initialize_clipboard(window);
                })
                .discard();
            }
            Message::Cancel => self.pending = None,
            Message::Discard => {
                self.front_pending();
                match self.pending.take() {
                    Some(Pending::CloseTab(id)) if id == self.tab.id => {
                        return self.close_active_tab();
                    }
                    Some(Pending::CloseWindow(window, id, mut discarded)) => {
                        discarded.push(id);
                        return self.close_window(window, discarded);
                    }
                    _ => {}
                }
            }
            #[cfg(target_os = "macos")]
            Message::MacFiles(_) => {}
            Message::Opened(file) => {
                if let Some((path, contents)) = file {
                    return Task::perform(
                        files::prepare_contents(path, contents),
                        Message::FilePrepared,
                    );
                }
            }
            Message::FilePrepared(..) => {}
            Message::Save | Message::SaveAs => {
                if matches!(message, Message::Save) {
                    self.front_pending();
                }
                let office_host = (self.office_document() && matches!(message, Message::Save))
                    .then_some(self.office_host);
                let (path, suggested_name) =
                    self.drawing_save_target(matches!(message, Message::SaveAs));
                if self.file_io.saving {
                    // Only this tab's own save can go on with the dialog's action.
                    if self.file_io.saving_tab != Some(self.tab.id) {
                        self.pending = None;
                    }
                    self.status = "A document save is already in progress".into();
                    return Task::none();
                }
                self.file_io.saving = true;
                self.file_io.saving_tab = Some(self.tab.id);
                if office_host == Some("Microsoft 365") {
                    self.status = "Saving recovery draft and waiting for Microsoft 365 to confirm the drawing update…".into();
                    self.error = false;
                }
                let snapshot = std::sync::Arc::new(self.tab.doc.clone());
                let save_snapshot = std::sync::Arc::clone(&snapshot);
                let epoch = self.tab.file_epoch;
                return Task::perform(
                    async move {
                        let path = if let Some(p) = path {
                            p
                        } else {
                            let extension = reshiki::compatibility::NATIVE_EXTENSION;
                            let Some(path) =
                                files::save_path("Save drawing", &suggested_name, extension).await
                            else {
                                return Ok(None);
                            };
                            path
                        };
                        let save_path = path.clone();
                        tokio::task::spawn_blocking(move || {
                            let bytes = save_snapshot.file_json()?;
                            if office_host == Some("Microsoft 365") {
                                reshiki::office_addin::save(&save_path, &bytes)?;
                            } else {
                                #[cfg(windows)]
                                windows_libreoffice_save::save(&save_path, &bytes, office_host)?;
                                #[cfg(not(windows))]
                                reshiki::storage::write_atomic(&save_path, &bytes)?;
                            }
                            Ok::<_, String>(())
                        })
                        .await
                        .map_err(|error| error.to_string())??;
                        Ok(Some(path))
                    },
                    move |result| {
                        Message::Saved(
                            epoch,
                            Box::new(std::sync::Arc::unwrap_or_clone(snapshot)),
                            result,
                        )
                    },
                );
            }
            Message::Saved(..) => {}
            Message::Export(format) => {
                if ["svg", "pdf", "png"].contains(&format) || cfg!(windows) && format == "emf" {
                    return self.export_figure(format, false);
                }
                let mut request = Request::molecule("export", self.tab.doc.clone());
                request.format = Some(format.into());
                return self.run(request, Job::Export(format));
            }
            Message::FigureExported(result) => self.figure_exported(result),
            message @ (Message::EngineDone { .. }
            | Message::ClipboardWritten { .. }
            | Message::CopyAsPrepared(..)
            | Message::CopyAsWritten(..)
            | Message::ClipboardRead { .. }
            | Message::Pasted(_)
            | Message::Exported(_)) => return self.update_document_result(message),
        }
        if reveal_inspector {
            iced::widget::operation::snap_to(
                "inspector-content",
                iced::widget::operation::RelativeOffset::START,
            )
        } else {
            Task::none()
        }
    }
    /// Background delivery calls these handlers without front-tab input or focus handling.
    fn update_document_result(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Optimization(action) => return self.optimization_action(action),
            Message::LabelsReady(key, result) => self.labels_ready(key, result),
            Message::FigureExported(result) => self.figure_exported(result),
            Message::Printing(action) => return self.print_action(action),
            Message::DrawingStyle(action) => return self.drawing_style_action(action),
            Message::Assistant(action) => return self.assistant_action(action),
            Message::InspectorAction(action) => return self.inspector_action(action),
            Message::Pictures(action) => return self.picture_action(action),
            Message::Imports(action) => return self.import_action(action),
            Message::Shortcut(action) => return self.shortcut_action(action),
            Message::CopyAsPrepared(key, result) => return self.copy_as_prepared(key, *result),
            Message::CopyAsWritten(key, result) => self.copy_as_written(key, result),
            Message::ClipboardWritten {
                epoch,
                revision,
                cut_ids,
                result,
            } => self.clipboard_written(epoch, revision, cut_ids, result),
            Message::ClipboardRead {
                epoch,
                revision,
                result,
            } => self.clipboard_read(epoch, revision, *result),
            Message::Pasted(contents) => {
                if let Some(contents) = contents.filter(|s| !s.trim().is_empty()) {
                    if let Some(json) = editing::clipboard_json(&contents) {
                        match Document::from_json(json.as_bytes()) {
                            Ok(part) => {
                                let part = reshiki::canvas_theme::for_native_paste(
                                    part,
                                    self.tab.doc.canvas_theme,
                                );
                                let center = editing::center(&part, &part.all_ids());
                                let before = self.tab.doc.clone();
                                self.tab.selected = editing::append(
                                    &mut self.tab.doc,
                                    &part,
                                    Point::new(
                                        self.tab.camera.center.x - center.x + 24.0,
                                        self.tab.camera.center.y - center.y + 24.0,
                                    ),
                                );
                                self.changed(before);
                                self.tool = Tool::Select;
                                self.status = "Selection pasted".into();
                            }
                            Err(e) => {
                                self.status = format!("Could not paste: {e}");
                                self.error = true;
                            }
                        }
                    } else {
                        return self.run(input_request(&contents), Job::Insert);
                    }
                }
            }
            Message::EngineDone {
                revision,
                kind,
                result,
            } => return self.engine_done(revision, kind, *result),
            Message::Exported(result) => match result {
                Ok(Some(path)) => {
                    self.status = format!(
                        "Exported {}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    );
                    self.error = false;
                }
                Ok(None) => {}
                Err(e) => {
                    self.status = e;
                    self.error = true;
                }
            },
            _ => {}
        }
        Task::none()
    }
    /// A chemistry job's result for its originating drawing.
    fn engine_done(
        &mut self,
        revision: u64,
        kind: Job,
        result: Result<Response, String>,
    ) -> Task<Message> {
        self.tab.busy = false;
        if matches!(&kind, Job::Clean(job) if job.serial != self.tab.cleanup_serial || job.epoch != self.tab.file_epoch)
        {
            return Task::none();
        }
        match result {
            Err(e) => {
                self.error = true;
                self.status = e;
            }
            Ok(response) => {
                if let Job::Export(format) = kind {
                    return export_file(response.output.unwrap_or_default(), format);
                }
                if self.tab.revision != revision {
                    self.status =
                        "Operation finished; newer edits were preserved. Run it again to update."
                            .into();
                    return Task::none();
                }
                if let Job::Clean(job) = &kind {
                    if let Some(document) = response.document {
                        if let Err(error) = document.validate() {
                            self.status = error;
                            self.error = true;
                        } else {
                            self.tab.cleanup = Some(CleanupPreview {
                                job: job.clone(),
                                warnings: response.warnings,
                                document,
                                analysis: response.analysis,
                                revision,
                                epoch: self.tab.file_epoch,
                                original: false,
                            });
                            self.status =
                                "Cleanup preview · Compare with the original, then Apply or Cancel"
                                    .into();
                            self.error = false;
                        }
                    }
                    return Task::none();
                }
                if matches!(kind, Job::Insert) {
                    if let Some(document) = response.document {
                        let before = self.tab.doc.clone();
                        let center = editing::center(&document, &document.all_ids());
                        let offset = if self.tab.doc.all_ids().is_empty() {
                            Point::new(
                                self.tab.camera.center.x - center.x,
                                self.tab.camera.center.y - center.y,
                            )
                        } else {
                            let (_, existing_max) = self.tab.doc.bounds();
                            let (insert_min, _) = document.bounds();
                            Point::new(
                                existing_max.x + self.tab.doc.drawing_style.bond_length_world
                                    - insert_min.x,
                                self.tab.camera.center.y - center.y,
                            )
                        };
                        self.tab.selected = editing::append(&mut self.tab.doc, &document, offset);
                        self.changed(before);
                        self.fit();
                        self.tool = Tool::Select;
                        self.status =
                            "Inserted structure · Drag to position · Delete or Undo to remove"
                                .into();
                        if !response.warnings.is_empty() {
                            self.status.push_str(" · ");
                            self.status.push_str(&response.warnings.join(" · "));
                        }
                    }
                    return Task::none();
                }
                if matches!(kind, Job::AromaticDisplay) {
                    if let Some(document) = response.document {
                        let before = self.tab.doc.clone();
                        self.tab.doc = document.clone();
                        self.changed(before);
                        if self.error {
                            return Task::none();
                        }
                        reshiki::atom_labels::refresh_computed(&mut self.tab.doc, &document);
                        self.tab.labels_dirty = false;
                        self.tab.analysis = response.analysis;
                        self.tool = Tool::Select;
                        self.status =
                            "Aromatic display changed · Molecular identity retained".into();
                    }
                    return Task::none();
                }
                if matches!(kind, Job::Abbreviate) {
                    if let Some(document) = response.document {
                        let before = self.tab.doc.clone();
                        let count = document.abbreviations.len();
                        self.tab.doc = document;
                        self.tab.selected = self
                            .tab
                            .doc
                            .expand_abbreviation_selection(&self.tab.selected);
                        self.changed(before);
                        self.tab.analysis = response.analysis;
                        self.tool = Tool::Select;
                        self.status = if count == 0 {
                            "No matching common groups in this selection".into()
                        } else {
                            format!(
                                "{count} abbreviation{} · Full chemistry retained · Expand to edit internal atoms",
                                if count == 1 { "" } else { "s" }
                            )
                        };
                    }
                    return Task::none();
                }
                if matches!(kind, Job::Analyze) {
                    // Checking is a read-only chemistry operation. Refresh
                    // computed H labels without rewriting the user's bond
                    // orders/stereo or inserting a step into Undo/Redo.
                    if let Some(document) = response.document {
                        reshiki::atom_labels::refresh_computed(&mut self.tab.doc, &document);
                    }
                    self.tab.analysis = response.analysis;
                    self.tab.chemistry_notice = None;
                    if matches!(kind, Job::Analyze) {
                        self.status = "No chemistry errors found".into();
                        self.error = false;
                    }
                    return Task::none();
                }
                if let Some(document) = response.document {
                    let before = self.tab.doc.clone();
                    self.tab.doc = document.clone();
                    self.changed(before);
                    reshiki::atom_labels::refresh_computed(&mut self.tab.doc, &document);
                    self.tab.labels_dirty = false;
                    self.tab.chemistry_notice = None;
                    self.tab.selected.clear();
                    if matches!(kind, Job::Import | Job::ImportFile) {
                        self.fit();
                    }
                    if matches!(kind, Job::ImportFile) {
                        self.tab.path = None;
                        self.tab.untitled_name = None;
                        self.tab.saved = Document::default();
                        self.tab.file_epoch = self.next_epoch();
                    }
                }
                self.tab.analysis = response.analysis;
                self.status = match kind {
                    Job::Clean(_) => "Structure cleaned",
                    Job::Analyze => "No chemistry errors found",
                    _ => "Structure imported · Undo restores the previous drawing",
                }
                .into();
                if !response.warnings.is_empty() {
                    self.status.push_str(" · ");
                    self.status.push_str(&response.warnings.join(" · "));
                }
                self.error = false;
            }
        }
        Task::none()
    }
    fn edit(&mut self, edit: Edit) {
        if let Edit::ContextMenu { position, selected } = edit {
            if self.tab.cleanup.is_none() {
                self.tab.selected = selected;
                self.tool = Tool::Select;
                self.sync_typography();
                self.context_menu = Some(context_menu::State::new(position, Default::default()));
            }
            return;
        }
        match edit {
            Edit::EraseStart(p) => {
                self.tab.erase_stroke = self.tool == Tool::Erase && self.tab.cleanup.is_none();
                self.tab.erase_committed = false;
                if self.tab.erase_stroke {
                    self.erase_segment(p, p);
                }
                return;
            }
            Edit::EraseTo(from, to) => {
                if self.tab.erase_stroke && self.tool == Tool::Erase {
                    self.erase_segment(from, to);
                }
                return;
            }
            Edit::EraseEnd => {
                self.tab.erase_stroke = false;
                self.tab.hover = None;
                return;
            }
            Edit::Hover(_) | Edit::ContextMenu { .. } => {}
            _ => self.tab.erase_stroke = false,
        }
        if matches!(edit, Edit::Pan(..) | Edit::Zoom(..)) {
            self.tab.pages.fit = None;
        }
        if let Edit::Hover(point) = edit {
            self.tab.hover = point.map(|p| (p, self.tab.file_epoch));
            self.keyboard_pointer_hover(point);
            return;
        }
        if matches!(edit, Edit::Pan(..) | Edit::Zoom(..)) {
            self.tab.hover = None;
        }
        if self.tab.cleanup.is_some() {
            match edit {
                Edit::Pan(dx, dy) => {
                    self.tab.camera.center = self
                        .tab
                        .camera
                        .center
                        .offset(-dx / self.tab.camera.zoom, -dy / self.tab.camera.zoom);
                    self.tab.fit_to_view = false;
                }
                Edit::Zoom(factor, _) => {
                    self.tab.camera.zoom = (self.tab.camera.zoom * factor).clamp(0.005, 5.);
                    self.tab.fit_to_view = false;
                }
                _ => {}
            }
            return;
        }
        let before = self.tab.doc.clone();
        match edit {
            Edit::ContextMenu { .. } => return,
            Edit::Hover(_)
            | Edit::RelaxDragStart { .. }
            | Edit::RelaxDragTarget { .. }
            | Edit::RelaxDragEnd { .. }
            | Edit::RelaxDragCancel { .. }
            | Edit::RelaxRotate { .. }
            | Edit::BeginText(_)
            | Edit::BeginTransform(_)
            | Edit::EraseStart(_)
            | Edit::EraseTo(..)
            | Edit::EraseEnd => return,
            Edit::ArrowClick(id) => self.apply_arrow_tool(id),
            Edit::Chain {
                points,
                source,
                target,
            } => {
                match reshiki::chains::place(
                    &self.tab.doc,
                    &points,
                    source,
                    target,
                    10.0 / self.tab.camera.zoom,
                ) {
                    Ok((doc, ids)) => {
                        self.tab.doc = doc;
                        self.tab.selected = ids;
                    }
                    Err(error) => {
                        self.status = error;
                        self.error = true;
                        return;
                    }
                }
            }
            Edit::Graphic(start, end, constrain) => {
                if let Tool::Graphic(kind) = self.tool {
                    if matches!(
                        kind,
                        reshiki::graphics::GraphicKind::Symbol(_)
                            | reshiki::graphics::GraphicKind::Orbital(_)
                    ) {
                        let drawing = reshiki::scientific::Drawing {
                            kind,
                            style: self.tab.graphic_style.clone(),
                            phase: self.tab.orbital_phase,
                            flipped: self.tab.phase_flipped,
                            attach: self.tab.attach_symbols,
                        };
                        match drawing.place(
                            &mut self.tab.doc,
                            start,
                            end,
                            constrain,
                            10. / self.tab.camera.zoom,
                        ) {
                            Ok(id) => self.tab.selected = vec![id],
                            Err(error) => {
                                self.status = error;
                                self.error = true;
                                return;
                            }
                        }
                    } else {
                        let id = self.tab.doc.next_id();
                        self.tab.doc.graphics.push(
                            Graphic::dragged(
                                id,
                                kind,
                                start,
                                end,
                                self.tab.graphic_style.clone(),
                                self.tab.bracket_sides,
                                constrain,
                            )
                            .with_arc(self.tab.arc_editor.geometry),
                        );
                        self.tab.selected = vec![id];
                    }
                    self.tool = Tool::Select;
                }
            }
            Edit::AtomIndicator(owner, p) => {
                if let Some(anchor) = owner.anchor(&self.tab.doc) {
                    owner.set_offset(
                        &mut self.tab.doc,
                        Some(Point::new(p.x - anchor.x, p.y - anchor.y)),
                    );
                }
            }
            Edit::AtomMark(id, index, p) => {
                if let Some(a) = self.tab.doc.atom_mut(id)
                    && let Some(mark) = a.marks.get_mut(index)
                {
                    mark.offset = Point::new(p.x - a.position.x, p.y - a.position.y);
                }
            }
            Edit::ArrowHandle(id, index, p) => {
                if let Some(a) = self.tab.doc.arrows.iter_mut().find(|a| a.id == id) {
                    a.edit_handle(index, p);
                }
            }
            Edit::GraphicPoint(id, index, p) => {
                if let Some(g) = self.tab.doc.graphics.iter_mut().find(|g| g.id == id) {
                    g.edit_point(index, p);
                }
                self.sync_arc();
            }
            Edit::Template(anchor, direction) => {
                if let Some(state) = &self.tab.joining {
                    if state.revision != self.tab.revision || state.epoch != self.tab.file_epoch {
                        self.cancel_join();
                        self.status = "The drawing changed. Start Move & attach again.".into();
                        self.error = true;
                        return;
                    }
                    match state.prepared.place(
                        anchor,
                        direction,
                        10. / self.tab.camera.zoom,
                        state.anchor,
                        state.mode,
                    ) {
                        Ok((document, selected)) => {
                            self.tab.doc = document;
                            self.tab.selected = selected;
                            self.tab.joining = None;
                            self.tool = Tool::Select;
                            self.changed(before);
                            self.status =
                                "Fragments joined · Undo restores their original positions".into();
                            self.sync_typography();
                        }
                        Err(error) => {
                            self.status = error;
                            self.error = true;
                        }
                    }
                    return;
                }
                if self.tool != Tool::Template {
                    return;
                }
                let Some(template) = self.templates.library.get(self.template_index) else {
                    return;
                };
                match template.place(
                    &self.tab.doc,
                    anchor,
                    direction,
                    10.0 / self.tab.camera.zoom,
                    self.templates.anchor,
                    self.templates.connection,
                ) {
                    Ok((document, selected)) => {
                        self.tab.doc = document;
                        self.tab.selected = selected;
                        if !self.templates.repeat {
                            self.tool = Tool::Select;
                        }
                    }
                    Err(error) => {
                        self.status = error.into();
                        self.error = true;
                        return;
                    }
                }
            }
            Edit::Transform {
                ids,
                pivot,
                scale,
                rotation,
            } => {
                editing::transform_about(&mut self.tab.doc, &ids, pivot, scale, rotation);
                self.tab.selected = ids;
            }
            Edit::Tilt { ids, x, y } => {
                crate::canvas::tilt::apply(&mut self.tab.doc, &ids, x, y);
                self.tab.selected = ids;
            }
            Edit::ScaleAxes { ids, pivot, x, y } => {
                editing::scale_axes_about(&mut self.tab.doc, &ids, pivot, x, y);
                self.tab.selected = ids;
            }
            Edit::RingPreset(preset, anchor, direction, connect, alternate) => {
                let drawing = reshiki::rings::Drawing {
                    preset,
                    length: self.tab.bond_drawing.length,
                    alternate,
                    connect,
                };
                match drawing.place(&self.tab.doc, anchor, direction, 10. / self.tab.camera.zoom) {
                    Ok((doc, ids)) => {
                        self.tab.doc = doc;
                        self.tab.selected = ids;
                    }
                    Err(error) => {
                        self.status = error.into();
                        self.error = true;
                        return;
                    }
                }
            }
            Edit::Ring(anchor, direction) | Edit::DelocalizedRing(anchor, direction, _) => {
                let (size, aromatic) = match edit {
                    Edit::DelocalizedRing(_, _, size) => (size, true),
                    _ => (self.ring_size, self.aromatic_ring),
                };
                match editing::ring_oriented(
                    &mut self.tab.doc,
                    anchor,
                    size,
                    aromatic,
                    10.0 / self.tab.camera.zoom,
                    direction,
                ) {
                    Ok(ids) => self.tab.selected = ids,
                    Err(error) => {
                        self.status = error.into();
                        self.error = true;
                        return;
                    }
                }
            }
            Edit::Select(ids) | Edit::SelectAt(ids, _) => {
                let inspector_width = self.inspector_width();
                self.tab.selected = ids;
                self.sync_typography();
                self.sync_graphics();
                self.sync_arrows();
                self.sync_bonds();
                // Auto-revealing object properties must not move the clicked
                // target. The inspector occupies the right edge, so compensate
                // for the centered camera's horizontal shift. Record the new
                // size now so its sensor event does not also trigger Fit.
                let width_change = inspector_width - self.inspector_width();
                self.tab.camera.center.x += width_change / (2. * self.tab.camera.zoom);
                self.viewport.width += width_change;
            }
            Edit::Move(ids, dx, dy) => {
                if let Some(snapped) = editing::snap_ring(
                    &mut self.tab.doc,
                    &ids,
                    Point::new(dx, dy),
                    14.0 / self.tab.camera.zoom,
                ) {
                    self.tab.selected = snapped;
                } else {
                    self.tab.doc.translate(&ids, dx, dy);
                    self.tab.selected = ids;
                }
            }
            Edit::Duplicate(ids, dx, dy) => {
                let part = editing::selection(&self.tab.doc, &ids);
                let copy = editing::append(&mut self.tab.doc, &part, Point::new(dx, dy));
                if !copy.is_empty() {
                    self.tab.selected = copy;
                }
            }
            Edit::Pan(dx, dy) => {
                self.tab.fit_to_view = false;
                self.tab.camera.center = self.tab.camera.center.offset(-dx, -dy);
            }
            Edit::Zoom(f, at) => {
                self.tab.fit_to_view = false;
                let old = self.tab.camera.zoom;
                self.tab.camera.zoom = (old * f).clamp(0.005, 5.0);
                let ratio = old / self.tab.camera.zoom;
                self.tab.camera.center = Point::new(
                    at.x + (self.tab.camera.center.x - at.x) * ratio,
                    at.y + (self.tab.camera.center.y - at.y) * ratio,
                );
            }
            Edit::PlaneBond(start, end) => {
                let preset = self
                    .tool
                    .bond_preset()
                    .unwrap_or(reshiki::bonds::BondPreset::Single);
                if preset == reshiki::bonds::BondPreset::Dotted {
                    self.status = "Drag from a bonded explicit H to an existing acceptor".into();
                    self.error = true;
                    return;
                }
                let element = if self.tool == Tool::Atom {
                    self.element.as_str()
                } else {
                    "C"
                };
                match reshiki::projection::growth::place(&self.tab.doc, start, end, element, preset)
                {
                    Ok((doc, id)) => {
                        self.tab.doc = doc;
                        self.tab.selected = vec![id];
                    }
                    Err(error) => {
                        self.status = error;
                        self.error = true;
                        return;
                    }
                }
            }
            Edit::Bond(start, end, a, b) => {
                if self.tool.bond_preset() == Some(reshiki::bonds::BondPreset::Dotted)
                    && !a.zip(b).is_some_and(|(a, b)| {
                        reshiki::bonds::hydrogen_endpoints(&self.tab.doc, a, b)
                    })
                {
                    self.status =
                        "Drag from a bonded explicit H to an existing N, O, F or S acceptor".into();
                    self.error = true;
                    return;
                }
                if self.tool == Tool::Atom {
                    let result = a
                        .ok_or_else(|| "Start the drag on an existing atom".to_string())
                        .and_then(|id| {
                            editing::add_bonded_atom(&self.tab.doc, id, end, b, &self.element)
                        });
                    match result {
                        Ok((doc, id)) => {
                            self.tab.doc = doc;
                            self.tab.selected = vec![id];
                        }
                        Err(error) => {
                            self.status = error;
                            self.error = true;
                            return;
                        }
                    }
                } else if self.tool == Tool::Arrow {
                    self.place_arrow(start, end);
                } else {
                    let a = a.unwrap_or_else(|| self.tab.doc.add_atom("C", start));
                    let b = b.unwrap_or_else(|| self.tab.doc.add_atom("C", end));
                    if let Some(preset) = self.tool.bond_preset() {
                        preset.place(&mut self.tab.doc, a, b);
                    } else {
                        let (order, display) = self.bond_style();
                        self.tab.doc.add_bond(a, b, order, display);
                    }
                    self.tab.selected = vec![b];
                }
            }
            Edit::Click(p) => {
                let hit = canvas::hit_object(&self.tab.doc, p, 10.0 / self.tab.camera.zoom);
                match self.tool {
                    Tool::Atom => {
                        if let Some(id) = self.tab.doc.nearest(p, 10.0 / self.tab.camera.zoom) {
                            self.tab.doc.invalidate_chemistry(&[id]);
                            if let Some(a) = self.tab.doc.atom_mut(id) {
                                a.element = self.element.clone();
                                a.display.variable = None;
                                a.explicit_h = 0;
                                a.no_implicit = false;
                                a.charge = 0;
                                a.isotope = 0;
                            }
                            self.tab.selected = vec![id];
                        } else {
                            let id = self.tab.doc.add_atom(&self.element, p);
                            self.tab.selected = vec![id];
                        }
                    }
                    tool if tool.bond_preset() == Some(reshiki::bonds::BondPreset::Dotted) => {
                        self.status =
                            "Drag from a bonded explicit H to an existing acceptor".into();
                        self.error = true;
                        return;
                    }
                    Tool::Bond(_) | Tool::StyledBond(_) | Tool::Wedge | Tool::Hash | Tool::Wavy => {
                        let atom = self.tab.doc.nearest(p, 10.0 / self.tab.camera.zoom);
                        let bond = self
                            .tab
                            .doc
                            .bonds
                            .iter()
                            .find(|b| {
                                self.tab
                                    .doc
                                    .atom(b.a)
                                    .zip(self.tab.doc.atom(b.b))
                                    .is_some_and(|(a, z)| {
                                        canvas::distance_to_segment(p, a.position, z.position)
                                            < 7.0 / self.tab.camera.zoom
                                    })
                            })
                            .cloned();
                        if let Some(b) = bond.filter(|_| atom.is_none()) {
                            let shift_double = self.tool.bond_preset().is_some_and(|preset| {
                                use reshiki::bonds::BondPreset as P;
                                matches!(
                                    preset,
                                    P::Double | P::BoldDouble | P::DashedDouble | P::DoubleDashed
                                ) && P::of(&b) == Some(preset)
                            });
                            if shift_double {
                                let position =
                                    reshiki::scene::effective_double_position(&self.tab.doc, &b)
                                        .cycled();
                                if let Some(bond) = self
                                    .tab
                                    .doc
                                    .bonds
                                    .iter_mut()
                                    .find(|bond| bond.a == b.a && bond.b == b.b)
                                {
                                    bond.double_position = position;
                                }
                                self.tab.selected = vec![b.a, b.b];
                                self.changed(before);
                                self.status = format!(
                                    "Double bond: {position} · Click again to shift its lines"
                                );
                                return;
                            }
                            let reverse = self.tool.bond_preset().is_some_and(|p| {
                                use reshiki::bonds::BondPreset as P;
                                matches!(
                                    p,
                                    P::Wedge
                                        | P::HashedWedge
                                        | P::HollowWedge
                                        | P::Hashed
                                        | P::Bold
                                        | P::Dative
                                        | P::Dashed
                                ) && P::of(&b) == Some(p)
                            });
                            let (order, display) = if self.tool == Tool::Bond(2) {
                                (2, "plain")
                            } else if matches!(self.tool, Tool::Bond(_)) {
                                (
                                    match b.order {
                                        1 => 2,
                                        2 => 3,
                                        _ => 1,
                                    },
                                    "plain",
                                )
                            } else {
                                self.bond_style()
                            };
                            if let Some(preset) = self
                                .tool
                                .bond_preset()
                                .filter(|p| p.preserves_chemistry(&b))
                            {
                                preset.place(&mut self.tab.doc, b.a, b.b);
                            } else {
                                self.tab.doc.add_bond(b.a, b.b, order, display);
                                self.apply_current_bond_preset(b.a, b.b);
                            }
                            if reverse
                                && let Some(bond) = self
                                    .tab
                                    .doc
                                    .bonds
                                    .iter_mut()
                                    .find(|bond| bond.a == b.a && bond.b == b.b)
                            {
                                bond.reverse();
                            }
                            self.tab.selected = vec![b.a, b.b];
                        } else {
                            let (order, display) = self.bond_style();
                            let a = atom.unwrap_or_else(|| self.tab.doc.add_atom("C", p));
                            let Some(start) = self.tab.doc.atom(a).map(|a| a.position) else {
                                self.status = "The bond's starting atom is unavailable".into();
                                self.error = true;
                                return;
                            };
                            if let Some(endpoint) =
                                reshiki::projection::growth::Plane::at(&self.tab.doc, a)
                                    .and_then(|plane| plane.outward(self.tab.bond_drawing.length))
                            {
                                let preset = self
                                    .tool
                                    .bond_preset()
                                    .unwrap_or(reshiki::bonds::BondPreset::Single);
                                match reshiki::projection::growth::place(
                                    &self.tab.doc,
                                    a,
                                    endpoint,
                                    "C",
                                    preset,
                                ) {
                                    Ok((doc, id)) => {
                                        self.tab.doc = doc;
                                        self.tab.selected = vec![id];
                                    }
                                    Err(error) => {
                                        self.status = error;
                                        self.error = true;
                                        return;
                                    }
                                }
                            } else {
                                let end =
                                    editing::bond_extension(&self.tab.doc, start, Some(a), order);
                                let ratio = self.tab.bond_drawing.length
                                    / reshiki::style::DEFAULT.bond_length_world;
                                let end = start
                                    .offset((end.x - start.x) * ratio, (end.y - start.y) * ratio);
                                let b = self.tab.doc.add_atom("C", end);
                                self.tab.doc.add_bond(a, b, order, display);
                                self.apply_current_bond_preset(a, b);
                                self.tab.selected = vec![b];
                            }
                        }
                    }
                    Tool::Ring => {
                        return self.edit(Edit::Ring(p, None));
                    }
                    Tool::Text => {
                        if let Some(label) =
                            hit.filter(|id| self.tab.doc.annotations.iter().any(|a| a.id == *id))
                        {
                            self.tab.selected = vec![label];
                            self.sync_typography();
                            return;
                        }
                        if !self.tab.caption.trim().is_empty() {
                            let id = self.tab.doc.next_id();
                            self.tab.doc.annotations.push(Annotation {
                                id,
                                position: p,
                                text: self.tab.caption.clone(),
                                format: self.tab.caption_format.clone(),
                            });
                            self.tab.selected = vec![id];
                            self.tab.caption_target = Some(id);
                            self.tool = Tool::Select;
                        }
                    }
                    Tool::Arrow => {
                        if let Some(id) =
                            hit.filter(|id| self.tab.doc.arrows.iter().any(|a| a.id == *id))
                        {
                            self.apply_arrow_tool(id);
                        } else {
                            let length = self.tab.doc.drawing_style.bond_length_world * 2.;
                            self.place_arrow(p, p.offset(length, 0.));
                        }
                    }
                    Tool::Erase => {
                        reshiki::erasing::stroke(&mut self.tab.doc, p, p, 7. / self.tab.camera.zoom)
                    }
                    _ => self.tab.selected = hit.into_iter().collect(),
                }
            }
        }
        self.changed(before);
    }
    fn place_arrow(&mut self, start: Point, end: Point) {
        let id = self.tab.doc.next_id();
        self.tab.doc.arrows.push(Arrow::new(
            id,
            start,
            end,
            self.tab.arrow_style,
            self.tab.arrows.style.clone(),
        ));
        self.tab.selected = vec![id];
    }
    fn erase_segment(&mut self, from: Point, to: Point) {
        let before = self.tab.doc.clone();
        reshiki::erasing::stroke(&mut self.tab.doc, from, to, 7. / self.tab.camera.zoom);
        if self.tab.doc != before {
            self.tab.selected.clear();
            let revision = self.tab.revision;
            self.changed_continuing(before, self.tab.erase_committed);
            self.tab.erase_committed |= self.tab.revision != revision;
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        reshiki::accessibility::focus_scope(file_shortcuts::wrap(
            self.with_updates(self.with_assistant_image(
                self.with_atom_text(self.with_help(self.with_palette(self.workspace()))),
            )),
            self.help_open,
            self.assistant.viewed_image.is_some(),
            self.updates.open,
            self.tab.atom_text.is_some(),
            self.inspector_open
                && self.inspector_tab == InspectorTab::DrawingStyle
                && self.tab.styles.editor.is_some()
                && self.tab.inline_text.is_none()
                && self.context_menu.is_none(),
        ))
    }
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
        && a.bonds.iter().zip(&b.bonds).all(|(a, b)| {
            let mut a = a.clone();
            let mut b = b.clone();
            a.cip_label = None;
            b.cip_label = None;
            a == b
        })
        && a.annotations == b.annotations
        && a.arrows == b.arrows
        && a.graphics == b.graphics
        && a.groups == b.groups
        && a.reactions == b.reactions
        && a.abbreviations == b.abbreviations
        && a.ring_fills == b.ring_fills
        && a.depth_appearance == b.depth_appearance
        && a.atoms.len() == b.atoms.len()
        && a.atoms.iter().zip(&b.atoms).all(|(a, b)| {
            let mut a = a.clone();
            let mut b = b.clone();
            a.label_h = 0;
            b.label_h = 0;
            a.cip_label = None;
            b.cip_label = None;
            a == b
        })
}

fn chemistry_changed(before: &Document, after: &Document) -> bool {
    before.bonds.len() != after.bonds.len()
        || before.bonds.iter().zip(&after.bonds).any(|(a, b)| {
            let mut a = a.clone();
            let mut b = b.clone();
            a.z_order = 0;
            b.z_order = 0;
            a.color = Default::default();
            b.color = Default::default();
            a.highlight = None;
            b.highlight = None;
            a.double_position = Default::default();
            b.double_position = Default::default();
            a.secondary_display = None;
            b.secondary_display = None;
            a.indicator = Default::default();
            b.indicator = Default::default();
            a.cip_label = None;
            b.cip_label = None;
            if a.order == 4 && b.order == 4 || a.order == b.order && a.projection && b.projection {
                a.display = "plain".into();
                b.display = "plain".into();
                a.projection = false;
                b.projection = false;
                if a.a > a.b {
                    a.reverse();
                }
                if b.a > b.b {
                    b.reverse();
                }
            }
            a != b
        })
        || before.atoms.len() != after.atoms.len()
        || before.atoms.iter().zip(&after.atoms).any(|(a, b)| {
            let mut a = a.clone();
            let mut b = b.clone();
            a.text_style = None;
            b.text_style = None;
            a.marks.clear();
            b.marks.clear();
            a.display = Default::default();
            b.display = Default::default();
            a.cip_label = None;
            b.cip_label = None;
            a.label_h = 0;
            b.label_h = 0;
            a != b
        })
}

/// Marks the results of `task` as belonging to tab `id`, unless already marked.
fn tagged(task: Task<Message>, id: document_tab::TabId) -> Task<Message> {
    task.map(move |message| match message {
        Message::Tab(..) => message,
        message => Message::Tab(id, Box::new(message)),
    })
}

fn export_file(contents: String, format: &'static str) -> Task<Message> {
    Task::perform(
        save_export(contents.into_bytes(), format),
        Message::Exported,
    )
}
async fn save_export(bytes: Vec<u8>, format: &'static str) -> Result<Option<PathBuf>, String> {
    let Some(path) =
        files::save_path("Export drawing", &format!("Molecule.{format}"), format).await
    else {
        return Ok(None);
    };
    reshiki::storage::write_atomic(&path, &bytes)?;
    Ok(Some(path))
}
fn input_request(text: &str) -> Request {
    reshiki::clipboard::text_request(text)
}

#[cfg(test)]
mod tests;
