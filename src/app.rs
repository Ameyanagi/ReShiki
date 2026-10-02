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
mod arcs;
mod arrows;
mod assistant;
mod atom_labels;
mod atom_text;
mod autosave;
mod cleanup;
mod clipboard;
mod color_popover;
mod context_menu;
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
mod label_refresh;
mod molecule_shortcuts;
mod numeric_transforms;
mod object_toolbar;
mod pages;
mod palettes;
#[cfg(test)]
mod performance;
mod pictures;
mod popover;
mod printing;
mod reactions;
#[cfg(test)]
mod rotation_tests;
mod shortcut_examples;
#[cfg(test)]
mod shortcut_focus_tests;
mod shortcuts;
mod startup;
mod tabs;
mod template_library;
mod theme_files;
mod theme_generator;
mod tool_button;
mod typography;
mod updates;
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
    ContextMenu(context_menu::Action),
    StyleMenu(color_popover::Action),
    ObjectToolbar(object_toolbar::Action),
    InspectorAction(inspector::Action),
    NumericTransform(numeric_transforms::Action),
    Updates(updates::Action),
    Reaction(reactions::Action),
    DrawingStyle(document_styles::Action),
    InlineText(inline_text::Action),
    AtomText(atom_text::Action),
    Join(joining::Action),
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
            #[cfg(target_os = "macos")]
            macos_files::subscription(),
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
                    ..
                }) = event
                else {
                    return None;
                };
                if status == iced::event::Status::Captured {
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
        self.tab
            .cleanup
            .as_ref()
            .filter(|p| {
                !p.original && p.revision == self.tab.revision && p.epoch == self.tab.file_epoch
            })
            .map(|p| &p.document)
            .unwrap_or(&self.tab.doc)
    }
    pub fn update(&mut self, mut message: Message) -> Task<Message> {
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
        if self.imports.menu && matches!(message, Message::Escape) {
            self.imports.menu = false;
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
            if !matches!(message, Message::Imports(import::Action::Menu(_))) {
                self.imports.menu = false;
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
            Message::ResetBondDrawing => {
                self.tab.bond_drawing = Default::default();
                self.tab.bond_drawing.length = self.tab.doc.drawing_style.bond_length_world;
                self.tab.drawing_length_input =
                    self.tab.doc.drawing_style.bond_length_pt.to_string();
                self.tab.chain_drawing.angle = 120.;
                self.tab.chain_angle_input = "120".into();
                self.status = format!(
                    "{} bond defaults · {} pt length · 120° chain angle",
                    self.tab.doc.drawing_style.name, self.tab.doc.drawing_style.bond_length_pt
                );
                self.error = false;
            }
            Message::FixedLength(on) => self.tab.bond_drawing.fixed_length = on,
            Message::FixedAngles(on) => self.tab.bond_drawing.fixed_angles = on,
            Message::DrawingLength(value) => {
                self.tab.drawing_length_input = value;
                if let Ok(points) = self.tab.drawing_length_input.parse::<f32>()
                    && points.is_finite()
                    && (1.0..=300.0).contains(&points)
                {
                    self.tab.bond_drawing.length = reshiki::style::DEFAULT.world(points);
                    self.error = false;
                } else {
                    self.status = "Bond length must be between 1 and 300 pt".into();
                    self.error = true;
                }
            }
            Message::ChainAtoms(value) => {
                self.tab.chain_atoms_input = value;
                if self.tab.chain_atoms_input.is_empty() {
                    self.tab.chain_drawing.atoms = None;
                    self.error = false;
                } else if let Ok(count) = self.tab.chain_atoms_input.parse::<usize>()
                    && (1..=reshiki::chains::MAX_ATOMS).contains(&count)
                {
                    self.tab.chain_drawing.atoms = Some(count);
                    self.error = false;
                } else {
                    self.status =
                        "Enter 1–512 chain atoms, or clear the field for automatic length".into();
                    self.error = true;
                }
            }
            Message::ChainAngle(value) => {
                self.tab.chain_angle_input = value;
                if let Ok(angle) = self.tab.chain_angle_input.parse::<f32>()
                    && angle.is_finite()
                    && (1.0..=179.0).contains(&angle)
                {
                    self.tab.chain_drawing.angle = angle;
                    self.error = false;
                } else {
                    self.status = "Chain angle must be between 1° and 179°".into();
                    self.error = true;
                }
            }
            Message::ApplyBondPreset(preset) => {
                if preset == reshiki::bonds::BondPreset::Dotted
                    && self.tab.doc.bonds.iter().any(|b| {
                        self.tab.selected.contains(&b.a)
                            && self.tab.selected.contains(&b.b)
                            && !reshiki::bonds::hydrogen_endpoints(&self.tab.doc, b.a, b.b)
                    })
                {
                    self.status = "Hydrogen bonds need a bonded explicit H and an acceptor".into();
                    self.error = true;
                    return Task::none();
                }
                let before = self.tab.doc.clone();
                let affected: Vec<_> = self
                    .tab
                    .doc
                    .bonds
                    .iter()
                    .filter(|bond| {
                        self.tab.selected.contains(&bond.a)
                            && self.tab.selected.contains(&bond.b)
                            && !preset.preserves_chemistry(bond)
                    })
                    .flat_map(|bond| [bond.a, bond.b])
                    .collect();
                self.tab.doc.invalidate_chemistry(&affected);
                for bond in &mut self.tab.doc.bonds {
                    if self.tab.selected.contains(&bond.a) && self.tab.selected.contains(&bond.b) {
                        preset.apply(bond);
                    }
                }
                self.changed(before);
            }
            Message::BondPosition(position) => {
                let before = self.tab.doc.clone();
                for bond in &mut self.tab.doc.bonds {
                    if [2, 7].contains(&bond.order)
                        && self.tab.selected.contains(&bond.a)
                        && self.tab.selected.contains(&bond.b)
                    {
                        bond.double_position = position;
                    }
                }
                self.changed(before);
            }
            Message::BondColor(value) => self.tab.bond_color_input = value,
            Message::ApplyBondColor => {
                if let Some(rgb) = graphics::parse_color(&self.tab.bond_color_input) {
                    let color = reshiki::palette::Color::Custom(rgb);
                    let before = self.tab.doc.clone();
                    for bond in &mut self.tab.doc.bonds {
                        if self.tab.selected.contains(&bond.a)
                            && self.tab.selected.contains(&bond.b)
                        {
                            bond.color = color;
                        }
                    }
                    self.remember_custom(Some(color), &before);
                    self.changed(before);
                } else {
                    self.error = true;
                    self.status = "Enter a six-digit bond color, such as #205091".into();
                }
            }
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
            Message::ApplyGraphicWidth => match self.tab.graphic_width_input.parse::<f32>() {
                Ok(w) if w.is_finite() && (0.1..=12.0).contains(&w) => {
                    self.apply_graphic_style(GraphicChange::Width(w))
                }
                _ => {
                    self.error = true;
                    self.status = "Line width must be 0.1–12 pt".into();
                }
            },
            Message::GraphicStroke(s) => self.tab.graphic_stroke_input = s,
            Message::ApplyGraphicStroke => {
                if let Some(c) = graphics::parse_color(&self.tab.graphic_stroke_input) {
                    self.apply_graphic_style(GraphicChange::Stroke(
                        reshiki::palette::Color::Custom(c),
                    ));
                } else {
                    self.error = true;
                    self.status = "Enter a six-digit hex color, such as #117E6C".into();
                }
            }
            Message::GraphicFill(s) => self.tab.graphic_fill_input = s,
            Message::ApplyGraphicFill => {
                if let Some(c) = graphics::parse_color(&self.tab.graphic_fill_input) {
                    self.apply_graphic_style(GraphicChange::Fill(Some(
                        reshiki::palette::Color::Custom(c),
                    )));
                } else {
                    self.error = true;
                    self.status = "Enter a six-digit hex color, such as #DCEFE9".into();
                }
            }
            Message::ScientificKind(kind) => {
                self.toolbar.remember(Tool::Graphic(kind));
                let before = self.tab.doc.clone();
                for g in self
                    .tab
                    .doc
                    .graphics
                    .iter_mut()
                    .filter(|g| self.tab.selected.contains(&g.id))
                {
                    if matches!(
                        (g.kind, kind),
                        (
                            reshiki::graphics::GraphicKind::Symbol(_),
                            reshiki::graphics::GraphicKind::Symbol(_)
                        ) | (
                            reshiki::graphics::GraphicKind::Orbital(_),
                            reshiki::graphics::GraphicKind::Orbital(_)
                        )
                    ) {
                        g.kind = kind;
                    }
                }
                self.tool = Tool::Graphic(kind);
                self.changed(before);
            }
            Message::OrbitalPhase(phase) => {
                self.tab.orbital_phase = phase;
                let before = self.tab.doc.clone();
                for g in self
                    .tab
                    .doc
                    .graphics
                    .iter_mut()
                    .filter(|g| self.tab.selected.contains(&g.id))
                {
                    g.phase = phase;
                }
                self.changed(before);
            }
            Message::FlipPhase(value) => {
                self.tab.phase_flipped = value;
                let before = self.tab.doc.clone();
                for g in self
                    .tab
                    .doc
                    .graphics
                    .iter_mut()
                    .filter(|g| self.tab.selected.contains(&g.id))
                {
                    g.phase_flipped = value;
                }
                self.changed(before);
            }
            Message::AttachSymbols(value) => self.tab.attach_symbols = value,
            Message::RotateMark(id, index) => {
                let before = self.tab.doc.clone();
                if let Some(a) = self.tab.doc.atom_mut(id)
                    && let Some(m) = a.marks.get_mut(index)
                {
                    m.angle = (m.angle + 45.).rem_euclid(360.);
                }
                self.changed(before);
            }
            Message::RemoveMark(id, index) => {
                let before = self.tab.doc.clone();
                if let Some(a) = self.tab.doc.atom_mut(id)
                    && index < a.marks.len()
                {
                    let mark = a.marks.remove(index);
                    if mark.kind.charge() {
                        a.charge = 0;
                    }
                    if mark.kind.radical() {
                        a.radical_electrons = 0;
                    }
                    if mark.kind.charge() || mark.kind.radical() {
                        a.explicit_h = 0;
                        a.no_implicit = false;
                        self.tab.doc.invalidate_chemistry(&[id]);
                    }
                }
                self.changed(before);
            }
            Message::AtomRadical(value) => {
                let before = self.tab.doc.clone();
                self.tab.doc.invalidate_chemistry(&self.tab.selected);
                for atom in self
                    .tab
                    .doc
                    .atoms
                    .iter_mut()
                    .filter(|a| self.tab.selected.contains(&a.id))
                {
                    atom.radical_electrons = value;
                    atom.explicit_h = 0;
                    atom.no_implicit = false;
                }
                self.changed(before);
            }
            Message::GraphicSides(sides) => {
                let before = self.tab.doc.clone();
                self.tab.bracket_sides = sides;
                for g in &mut self.tab.doc.graphics {
                    if self.tab.selected.contains(&g.id) {
                        g.sides = sides;
                    }
                }
                self.changed(before);
            }
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
            Message::Element(e) => {
                self.element = e;
                self.tool = Tool::Atom;
            }
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
            Message::ColorScope(scope) => {
                self.tab.color_scope = scope;
                self.sync_color_input();
                if scope == typography::ColorScope::Rings {
                    self.status = "Ring interiors · Select a ring, then choose a Tint color".into();
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
            Message::RingSize(n) => {
                self.toolbar.ring = Tool::Ring;
                self.ring_size = n;
                self.tool = Tool::Ring;
            }
            Message::AromaticRing(value) => {
                self.toolbar.ring = Tool::Ring;
                self.aromatic_ring = value;
                self.tool = Tool::Ring;
            }
            Message::ToggleAromaticRing => {
                if self.tool.selects()
                    && reshiki::rings::selected_cycle(&self.tab.doc, &self.tab.selected).is_some()
                {
                    return self.update(Message::ToggleSelectedRing);
                }
                return self.update(Message::AromaticRing(!self.aromatic_ring));
            }
            Message::ToggleSelectedRing => {
                let before = self.tab.doc.clone();
                match reshiki::rings::toggle_selected_aromatic(
                    &mut self.tab.doc,
                    &self.tab.selected,
                ) {
                    Ok(aromatic) => {
                        self.changed(before);
                        self.status = if aromatic {
                            "Selected ring set to aromatic"
                        } else {
                            "Selected ring set to saturated"
                        }
                        .into();
                    }
                    Err(error) => {
                        self.error = true;
                        self.status = error;
                    }
                }
            }
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
            Message::ApplyElement => {
                let symbol = self.custom_element.trim();
                if editing::ELEMENTS.contains(&symbol) {
                    self.element = symbol.into();
                    self.tool = Tool::Atom;
                    self.status = format!("Place {} atoms", self.element);
                    self.error = false;
                } else {
                    self.status = "Enter an element symbol, for example Si, Fe, Na or H".into();
                    self.error = true;
                }
            }
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
            Message::ReverseBonds => {
                let before = self.tab.doc.clone();
                self.tab.doc.invalidate_chemistry(&self.tab.selected);
                for b in &mut self.tab.doc.bonds {
                    if self.tab.selected.contains(&b.a) && self.tab.selected.contains(&b.b) {
                        b.reverse();
                    }
                }
                self.changed(before);
            }
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
            Message::Canvas(edit) => self.edit(edit),
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
            Message::Charge(delta) => {
                let before = self.tab.doc.clone();
                self.tab.doc.invalidate_chemistry(&self.tab.selected);
                for id in &self.tab.selected {
                    if let Some(a) = self.tab.doc.atom_mut(*id) {
                        a.charge = a.charge.saturating_add(delta).clamp(-8, 8);
                        a.explicit_h = 0;
                        a.no_implicit = false;
                    }
                }
                self.changed(before);
            }
            Message::ApplyIsotope => match self.tab.isotope.parse::<u32>() {
                Ok(value) if value <= 300 => {
                    let before = self.tab.doc.clone();
                    for id in &self.tab.selected {
                        if let Some(a) = self.tab.doc.atom_mut(*id) {
                            a.isotope = value;
                        }
                    }
                    self.changed(before);
                }
                _ => {
                    self.status = "Enter an isotope mass number from 0 to 300 (0 clears it)".into();
                    self.error = true;
                }
            },
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
                #[cfg(windows)]
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
                            #[cfg(windows)]
                            windows_libreoffice_save::save(&save_path, &bytes, office_host)?;
                            #[cfg(not(windows))]
                            reshiki::storage::write_atomic(&save_path, &bytes)?;
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
                self.context_menu = Some(context_menu::State {
                    position,
                    page: Default::default(),
                });
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
                match reshiki::templates::place_with_mode(
                    &self.tab.doc,
                    &template.document,
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
            Edit::Select(ids) => {
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
    fn sync_bonds(&mut self) {
        if let Some(b) = self
            .tab
            .doc
            .bonds
            .iter()
            .find(|b| self.tab.selected.contains(&b.a) && self.tab.selected.contains(&b.b))
        {
            self.tab.bond_color_input =
                reshiki::palette::hex(reshiki::palette::Palette::of(&self.tab.doc).rgb(b.color));
        }
    }
    fn apply_current_bond_preset(&mut self, a: u64, b: u64) {
        if let Tool::StyledBond(preset) = self.tool
            && let Some(bond) = self
                .tab
                .doc
                .bonds
                .iter_mut()
                .find(|bond| (bond.a == a && bond.b == b) || (bond.a == b && bond.b == a))
        {
            preset.apply(bond);
        }
    }
    fn bond_style(&self) -> (u8, &'static str) {
        match self.tool {
            Tool::Bond(n) => (n, "plain"),
            Tool::StyledBond(preset) => {
                let (n, s, _) = preset.parts();
                (n, s)
            }
            Tool::Wedge => (1, "wedge"),
            Tool::Hash => (1, "hash"),
            Tool::Wavy => (1, "wavy"),
            _ => (1, "plain"),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        file_shortcuts::wrap(
            self.with_updates(self.with_assistant_image(
                self.with_atom_text(self.with_help(self.with_palette(self.workspace()))),
            )),
            self.help_open,
            self.assistant.viewed_image.is_some(),
            self.updates.open,
            self.tab.atom_text.is_some(),
        )
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
mod tests {
    #[test]
    fn atom_drag_and_click_are_separate_undoable_actions() -> Result<(), String> {
        let (mut app, _) = App::new();
        let source = app.tab.doc.add_atom("C", Point::default());
        let initial = app.tab.doc.clone();
        app.tool = Tool::Atom;
        app.element = "O".into();
        app.edit(Edit::Bond(
            Point::default(),
            Point::new(42., 0.),
            Some(source),
            None,
        ));
        assert_eq!(app.tab.doc.atoms.len(), 2);
        assert_eq!(app.tab.doc.atom(source).ok_or("Source")?.element, "C");
        assert_eq!(app.tab.doc.atoms.last().ok_or("Oxygen")?.element, "O");
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, initial);
        app.edit(Edit::Click(Point::default()));
        assert_eq!(app.tab.doc.atoms.len(), 1);
        assert_eq!(app.tab.doc.atom(source).ok_or("Replacement")?.element, "O");
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, initial);
        Ok(())
    }

    use super::*;

    #[test]
    fn haworth_tools_and_edge_styles_are_undoable_without_erasing_sugar_stereo()
    -> anyhow::Result<()> {
        use anyhow::Context;
        use reshiki::{
            bonds::BondPreset as P,
            haworth::{Anomer, Sugar, sugar_document},
            rings::Preset,
        };
        for preset in [Preset::HaworthFive, Preset::HaworthSix] {
            let (mut app, _) = App::new();
            app.tab.doc = Document::default();
            let before = app.tab.doc.clone();
            app.edit(Edit::RingPreset(
                preset,
                Point::default(),
                None,
                false,
                false,
            ));
            assert!(!app.error, "{}", app.status);
            let placed = app.tab.doc.clone();
            assert!(placed.bonds.iter().all(|b| b.projection));
            let _ = app.update(Message::Undo);
            assert_eq!(app.tab.doc, before);
            let _ = app.update(Message::Redo);
            assert_eq!(app.tab.doc, placed);
        }
        for preset in [P::Wedge, P::HashedWedge, P::Bold, P::Single] {
            let (mut app, _) = App::new();
            app.tab.doc =
                sugar_document(Sugar::Glucose, Anomer::Alpha, 42.).map_err(anyhow::Error::msg)?;
            app.tab.doc.reconcile_molecule_groups();
            let source = app.tab.doc.clone();
            let bond = source
                .bonds
                .iter()
                .find(|b| b.display == "bold")
                .context("Front edge")?;
            app.tab.selected = vec![bond.a, bond.b];
            let _ = app.update(Message::ApplyBondPreset(preset));
            assert!(!app.error, "{}", app.status);
            assert_eq!(app.tab.doc.atoms, source.atoms);
            assert!(!chemistry_changed(&source, &app.tab.doc));
            if app.tab.doc != source {
                let _ = app.update(Message::Undo);
                assert_eq!(app.tab.doc, source);
            }
            let a = source.atom(bond.a).context("Front atom")?.position;
            let b = source.atom(bond.b).context("Front atom")?.position;
            app.tool = Tool::StyledBond(preset);
            app.edit(Edit::Click(Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.)));
            assert!(!app.error, "{}", app.status);
            assert_eq!(app.tab.doc.atoms, source.atoms);
            assert!(!chemistry_changed(&source, &app.tab.doc));
        }
        Ok(())
    }

    #[test]
    fn inspector_changes_reset_scrolling_but_normal_updates_keep_the_position() {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        assert_eq!(app.inspector_tab, InspectorTab::Properties);
        // Reaction handlers previously bypassed the normal tab navigation task.
        let task = app.update(Message::Reaction(reactions::Action::Open));
        assert_eq!(app.inspector_tab, InspectorTab::Reactions);
        assert!(task.units() > 0);
        assert_eq!(app.update(Message::InspectorScroll(180.)).units(), 0);
        assert_eq!(
            app.update(Message::Reaction(reactions::Action::Open))
                .units(),
            0
        );
    }

    fn subscriptions(app: &App) -> usize {
        iced::advanced::subscription::into_recipes(app.subscription()).len()
    }

    #[test]
    fn idle_windows_stop_polling_and_pending_work_restarts_timers() -> Result<(), String> {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let idle = subscriptions(&app);
        // Window close and keyboard/mouse events, plus the event-driven
        // Finder receiver on macOS. None of these schedules a polling timer.
        assert_eq!(idle, 2 + usize::from(cfg!(target_os = "macos")));
        app.assistant.busy = true;
        assert_eq!(subscriptions(&app), idle + 1);
        app.assistant.busy = false;
        app.tab.labels_dirty = true;
        assert_eq!(subscriptions(&app), idle); // Labels use a task, not a polling timer.
        app.tab.busy = true;
        assert_eq!(subscriptions(&app), idle);
        app.tab.busy = false;
        app.tab.labels_dirty = false;

        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        app.tab.recovery = Some(Recovery::in_directory(directory.path())?);
        app.inspector_open = false;
        assert_eq!(subscriptions(&app), idle);
        app.tab.doc.add_atom("O", Point::default());
        assert_eq!(subscriptions(&app), idle + 1);
        let _ = app.update(Message::Tick);
        assert_eq!(subscriptions(&app), idle);
        autosave::tests::finish_pending(&mut app);
        assert_eq!(subscriptions(&app), idle);
        let path = app
            .tab
            .recovery
            .as_ref()
            .ok_or("Missing recovery")?
            .session
            .clone();
        assert!(path.exists());
        app.tab.saved = app.tab.doc.clone();
        assert_eq!(subscriptions(&app), idle + 1);
        let _ = app.update(Message::Tick);
        autosave::tests::finish_pending(&mut app);
        assert!(!path.exists());
        assert_eq!(subscriptions(&app), idle);
        Ok(())
    }

    #[test]
    fn dismissing_the_recovery_offer_restores_the_ready_message() {
        use reshiki::recovery::Snapshot;
        let (mut app, _) = App::new();
        let candidate = Candidate {
            path: "draft.json".into(),
            snapshot: Snapshot {
                document: Document::default(),
                source: None,
                saved_at: 0,
            },
        };
        // At launch the offer replaces the ready message.
        app.recovered = vec![candidate.clone()];
        app.status.clear();
        let _ = app.update(Message::DismissRecovery);
        assert!(app.recovered.is_empty());
        assert_eq!(app.status, READY);
        // A newer message stays.
        app.recovered = vec![candidate];
        app.status = "Drawing updated".into();
        let _ = app.update(Message::DismissRecovery);
        assert_eq!(app.status, "Drawing updated");
    }

    fn checked_labels(app: &mut App) {
        let mut checked = app.tab.doc.clone();
        for atom in &mut checked.atoms {
            atom.label_h = 2;
        }
        // A check must not adopt the engine's normalized bond depiction.
        if let Some(bond) = checked.bonds.first_mut() {
            bond.order = 4;
        }
        let _ = app.update(Message::EngineDone {
            revision: app.tab.revision,
            kind: Job::Analyze,
            result: Box::new(Ok(Response {
                document: Some(checked),
                analysis: None,
                output: None,
                engine_version: "test".into(),
                warnings: vec![],
            })),
        });
    }

    #[test]
    fn double_tool_cycles_only_line_position_and_preserves_chemistry() {
        use reshiki::bonds::DoublePosition as P;
        let (mut app, _) = App::new();
        app.tab.busy = false;
        app.tool = Tool::Bond(2);
        let c = app.tab.doc.add_atom("C", Point::default());
        let o = app.tab.doc.add_atom("O", Point::new(0., -42.));
        let methyl = app.tab.doc.add_atom("C", Point::new(36.373, 21.));
        app.tab.doc.add_bond(c, o, 2, "plain");
        app.tab.doc.add_bond(c, methyl, 1, "plain");
        app.tab.doc.bonds[0].color = reshiki::palette::Color::Custom([32, 80, 145]);
        app.tab.doc.atom_mut(c).unwrap().label_h = 1;
        let original = app.tab.doc.clone();
        let mut scenes = std::collections::BTreeSet::new();
        for position in [P::Left, P::Right, P::Center] {
            app.edit(Edit::Click(Point::new(0., -21.)));
            let mut expected = original.clone();
            expected.bonds[0].double_position = position;
            assert_eq!(app.tab.doc, expected);
            assert!(!chemistry_changed(&original, &app.tab.doc));
            scenes.insert(reshiki::scene::svg(&app.tab.doc));
        }
        assert_eq!(scenes.len(), 3);
        for _ in 0..3 {
            let _ = app.update(Message::Undo);
        }
        assert_eq!(app.tab.doc, original);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc.bonds[0].double_position, P::Left);
        app.tab.doc.bonds[0].order = 1;
        app.edit(Edit::Click(Point::new(0., -21.)));
        assert_eq!(app.tab.doc.bonds[0].order, 2);
    }

    #[test]
    fn cancelling_a_running_cleanup_refresh_prevents_late_preview_or_apply() {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let a = app.tab.doc.add_atom("C", Point::default());
        let b = app.tab.doc.add_atom("C", Point::new(80., 0.));
        app.tab.doc.add_bond(a, b, 1, "plain");
        app.tab.selected = vec![b];
        let original = app.tab.doc.clone();
        let _ = app.update(Message::Clean);
        let job = cleanup::CleanupJob {
            options: reshiki::cleanup::Options {
                scope: reshiki::cleanup::Scope::SelectedAtoms,
                ..Default::default()
            },
            selection: vec![b],
            serial: app.tab.cleanup_serial,
            epoch: app.tab.file_epoch,
        };
        let response = || {
            Box::new(Ok(Response {
                document: Some(original.clone()),
                analysis: None,
                output: None,
                engine_version: "test".into(),
                warnings: vec![],
            }))
        };
        let _ = app.update(Message::EngineDone {
            revision: app.tab.revision,
            kind: Job::Clean(job.clone()),
            result: response(),
        });
        assert_eq!(
            app.tab.cleanup.as_ref().unwrap().job.options.scope,
            reshiki::cleanup::Scope::SelectedAtoms
        );
        let _ = app.update(Message::CleanupScope(
            reshiki::cleanup::Scope::SelectedMolecules,
        ));
        assert!(app.tab.busy);
        let mut pending = job;
        pending.serial = app.tab.cleanup_serial;
        let _ = app.update(Message::ApplyCleanup);
        assert_eq!(app.tab.doc, original);
        assert!(app.tab.cleanup.is_some());
        let _ = app.update(Message::CancelCleanup);
        let _ = app.update(Message::EngineDone {
            revision: app.tab.revision,
            kind: Job::Clean(pending),
            result: response(),
        });
        assert!(app.tab.cleanup.is_none());
        assert_eq!(app.tab.doc, original);
        assert!(!app.tab.history.can_undo());
    }

    #[test]
    fn abbreviation_display_changes_are_unsaved_and_undoable() {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let a = app.tab.doc.add_atom("C", Point::default());
        let b = app.tab.doc.add_atom("O", Point::new(42., 0.));
        let c = app.tab.doc.add_atom("C", Point::new(63., 36.));
        app.tab.doc.add_bond(a, b, 1, "plain");
        app.tab.doc.add_bond(b, c, 1, "plain");
        app.tab.doc.contract(&[b, c], "OMe", "MeO").unwrap();
        app.tab.saved = app.tab.doc.clone();
        let saved = app.tab.doc.clone();
        let _ = app.update(Message::Abbreviations(abbreviations::Action::ExpandAll));
        assert!(app.dirty());
        assert!(app.title().contains('•'));
        assert!(app.tab.doc.abbreviations.is_empty());
        let _ = app.update(Message::Undo);
        assert!(!app.dirty());
        assert_eq!(app.tab.doc, saved);
        let _ = app.update(Message::Redo);
        assert!(app.dirty());
        assert!(app.tab.doc.abbreviations.is_empty());
    }

    #[test]
    fn cleanup_requires_apply_can_cancel_and_rejects_stale_results() {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let a = app.tab.doc.add_atom("C", Point::default());
        let b = app.tab.doc.add_atom("O", Point::new(70., 12.));
        app.tab.doc.add_bond(a, b, 1, "plain");
        app.tab.saved = app.tab.doc.clone();
        app.tab.selected = vec![a, b];
        let original = app.tab.doc.clone();
        let mut cleaned = original.clone();
        cleaned.atom_mut(b).unwrap().position = Point::new(42., 0.);
        let response = || {
            Box::new(Ok(Response {
                document: Some(cleaned.clone()),
                analysis: None,
                output: None,
                engine_version: "test".into(),
                warnings: vec![],
            }))
        };
        let _ = app.update(Message::EngineDone {
            revision: app.tab.revision,
            kind: Job::Clean(cleanup::CleanupJob {
                options: Default::default(),
                selection: vec![a, b],
                serial: app.tab.cleanup_serial,
                epoch: app.tab.file_epoch,
            }),
            result: response(),
        });
        assert_eq!(app.tab.doc, original);
        assert_eq!(app.display_document(), &cleaned);
        assert!(!app.dirty());
        let _ = app.update(Message::Delete);
        assert_eq!(app.tab.doc, original);
        let _ = app.update(Message::CleanupOriginal(true));
        assert_eq!(app.display_document(), &original);
        let _ = app.update(Message::CancelCleanup);
        assert_eq!(app.tab.doc, original);
        assert!(!app.tab.history.can_undo());
        let _ = app.update(Message::EngineDone {
            revision: app.tab.revision,
            kind: Job::Clean(cleanup::CleanupJob {
                options: Default::default(),
                selection: vec![a, b],
                serial: app.tab.cleanup_serial,
                epoch: app.tab.file_epoch,
            }),
            result: response(),
        });
        let _ = app.update(Message::ApplyCleanup);
        assert_eq!(app.tab.doc, cleaned);
        assert_eq!(app.tab.selected, vec![a, b]);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, original);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, cleaned);
        let _ = app.update(Message::EngineDone {
            revision: app.tab.revision.wrapping_sub(1),
            kind: Job::Clean(cleanup::CleanupJob {
                options: Default::default(),
                selection: vec![a, b],
                serial: app.tab.cleanup_serial,
                epoch: app.tab.file_epoch,
            }),
            result: response(),
        });
        assert!(app.tab.cleanup.is_none());
    }

    #[test]
    fn label_edits_and_indicator_drags_are_atomic_and_do_not_change_chemistry() {
        use atom_labels::Action;
        use reshiki::atom_labels::{Carbons, Owner};
        let (mut app, _) = App::new();
        let _ = app.update(Message::New);
        let a = app.tab.doc.add_atom("N", Point::default());
        app.tab.history = History::default();
        let original = app.tab.doc.clone();
        app.label_action(Action::Number);
        assert!(!chemistry_changed(&original, &app.tab.doc));
        let numbered = app.tab.doc.clone();
        app.edit(Edit::AtomIndicator(Owner::Number(a), Point::new(20., -30.)));
        assert_eq!(
            app.tab
                .doc
                .atom(a)
                .unwrap()
                .display
                .number
                .as_ref()
                .unwrap()
                .offset,
            Some(Point::new(20., -30.))
        );
        let _ = app.update(Message::Undo);
        assert!(same_drawing(&app.tab.doc, &numbered));
        let _ = app.update(Message::Undo);
        assert!(same_drawing(&app.tab.doc, &original));
        let _ = app.update(Message::Redo);
        assert!(same_drawing(&app.tab.doc, &numbered));
        app.label_action(Action::Carbons(Carbons::All));
        app.label_action(Action::Hydrogens(false));
        app.label_action(Action::Stereo(true));
        let _ = app.update(Message::New);
        assert_eq!(app.tab.doc.atom_labels, Default::default());
        assert_eq!(app.current_text_style().size_pt, 10.);
        assert_eq!(app.current_text_style().family, "Arial");
    }

    #[test]
    fn invalid_edit_is_rolled_back_without_an_undo_entry() {
        let (mut app, _) = App::new();
        let _ = app.update(Message::New);
        app.tab.doc.add_atom("C", Point::default());
        app.tab.history = History::default();
        let before = app.tab.doc.clone();
        app.tab.doc.atoms[0].position.x = f32::NAN;
        app.changed(before.clone());
        assert_eq!(app.tab.doc, before);
        assert!(app.error);
        assert!(!app.tab.history.undo(&mut app.tab.doc));
        for color in ["αβγ", "💚AB", "#GG0000", "12345", "1234567"] {
            assert!(graphics::parse_color(color).is_none());
        }
    }

    #[test]
    fn chemistry_check_keeps_placement_atomic_and_preserves_redo() {
        use reshiki::rings::Preset;
        let (mut app, _) = App::new();
        let _ = app.update(Message::New);
        app.edit(Edit::RingPreset(
            Preset::ChairUp,
            Point::default(),
            None,
            false,
            false,
        ));
        let first = app.tab.doc.clone();
        let selected = app.tab.selected.clone();
        let revision = app.tab.revision;
        checked_labels(&mut app);
        assert_eq!(app.tab.revision, revision);
        assert_eq!(app.tab.selected, selected);
        assert!(same_drawing(&first, &app.tab.doc));
        assert!(app.tab.doc.atoms.iter().all(|atom| atom.label_h == 2));
        let checked_first = app.tab.doc.clone();
        let _ = app.update(Message::Undo);
        assert!(app.tab.doc.atoms.is_empty());
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, checked_first);
        app.edit(Edit::RingPreset(
            Preset::ChairDown,
            Point::new(300., 0.),
            None,
            false,
            false,
        ));
        let second = app.tab.doc.clone();
        let _ = app.update(Message::Undo);
        checked_labels(&mut app);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, second);
    }

    #[test]
    fn ring_color_toolbar_changes_only_fills_and_undo_restores_them() -> Result<(), String> {
        let (mut app, _) = App::new();
        app.tab.selected = editing::ring(&mut app.tab.doc, Point::default(), 6, true, 0.);
        let original = app.tab.doc.clone();
        let _ = app.update(Message::ColorScope(typography::ColorScope::Rings));
        let tint = reshiki::palette::Color::Palette(
            reshiki::palette::Hue::Blue,
            reshiki::palette::Row::Tint,
        );
        let _ = app.update(Message::TextStyle(reshiki::typography::StyleChange::Color(
            tint,
        )));
        assert_eq!(app.tab.doc.ring_fills.len(), 1);
        assert_eq!(app.tab.doc.atoms, original.atoms);
        assert_eq!(app.tab.doc.bonds, original.bonds);
        assert_eq!(app.current_selection_color(), Some(tint));
        assert!(
            app.tab.doc.recent_colors.is_empty(),
            "palette colors are not recent customs"
        );
        let colored = app.tab.doc.clone();
        let _ = app.update(Message::ClearRingFill);
        assert!(app.tab.doc.ring_fills.is_empty());
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, colored);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, original);
        app.tab.doc.validate()?;
        Ok(())
    }

    #[test]
    fn custom_ring_hex_is_exact_in_dark_mode_and_undoable() {
        let (mut app, _) = App::new();
        app.tab.selected = editing::ring(&mut app.tab.doc, Point::default(), 6, false, 0.);
        app.tab.doc.canvas_theme = reshiki::canvas_theme::CanvasTheme::Dark;
        let _ = app.update(Message::ColorScope(typography::ColorScope::Rings));
        let original = app.tab.doc.clone();
        let _ = app.update(Message::TextColor("#C9E0F8".into()));
        let _ = app.update(Message::ApplyTextColor);
        let fill = app.tab.doc.ring_fills.first().unwrap();
        assert_eq!(fill.color, reshiki::palette::Color::Custom([201, 224, 248]));
        assert_eq!(app.tab.doc.recent_colors, [[201, 224, 248]]);
        assert_eq!(app.tab.text_color_input, "#C9E0F8");
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, original);
    }

    #[test]
    fn computed_hydrogen_labels_do_not_make_a_saved_drawing_dirty() {
        use reshiki::rings::Preset;
        let (mut app, _) = App::new();
        let _ = app.update(Message::New);
        app.tab.doc = Preset::ChairUp.document(42., false);
        app.tab.saved = app.tab.doc.clone();
        checked_labels(&mut app);
        assert!(!app.dirty());
        assert!(!app.tab.history.can_undo());
        assert_ne!(app.tab.doc, app.tab.saved);
        app.tab.doc.atoms[0].charge = 1;
        assert!(app.dirty());
        app.tab.doc = app.tab.saved.clone();
        app.tab.doc.atoms[0].position.x += 1.;
        assert!(app.dirty());
    }

    #[test]
    fn circle_palette_and_modifier_share_atomic_attachment_and_history() {
        for modifier in [false, true] {
            let (mut app, _) = App::new();
            app.tab.doc = reshiki::rings::Preset::Benzene.document(42., false);
            app.aromatic_ring = true;
            app.ring_size = 6;
            let anchor = app.tab.doc.atoms[0].position;
            let edit = if modifier {
                Edit::DelocalizedRing(anchor, None, 6)
            } else {
                Edit::Ring(anchor, None)
            };
            let original = app.tab.doc.clone();
            app.edit(edit.clone());
            assert!(!app.error, "{}", app.status);
            assert_eq!((app.tab.doc.atoms.len(), app.tab.doc.bonds.len()), (12, 13));
            assert!(!reshiki::aromatic::circles(&app.tab.doc).is_empty());
            reshiki::chemistry::document::prepare(&app.tab.doc).unwrap();
            let placed = app.tab.doc.clone();
            let selected = app.tab.selected.clone();
            let revision = app.tab.revision;
            app.edit(edit); // The same host carbon has no remaining valence.
            assert!(app.error);
            assert_eq!(app.tab.doc, placed);
            assert_eq!(app.tab.selected, selected);
            assert_eq!(app.tab.revision, revision);
            let _ = app.update(Message::Undo);
            assert_eq!(app.tab.doc, original);
            let _ = app.update(Message::Redo);
            assert_eq!(app.tab.doc, placed);
        }
    }

    #[test]
    fn regular_ring_rejection_preserves_selection_history_and_redo() {
        for legacy_click in [false, true] {
            let (mut app, _) = App::new();
            let _ = app.update(Message::New);
            app.tool = Tool::Ring;
            app.aromatic_ring = false;
            app.ring_size = 6;
            app.tab.doc = Document::from_json(include_bytes!(
                "../tests/fixtures/ui-declutter/ring-rejection.rsk"
            ))
            .unwrap();
            let carbon = app.tab.doc.atoms[0].id;
            let original = app.tab.doc.clone();
            // A valid placement is one history entry; an invalid attempt after
            // Undo must leave that entry available to Redo.
            app.edit(Edit::Ring(Point::new(300., 0.), None));
            assert!(!app.error, "{}", app.status);
            let placed = app.tab.doc.clone();
            let _ = app.update(Message::Undo);
            assert_eq!(app.tab.doc, original);
            app.tab.selected = vec![carbon];
            let revision = app.tab.revision;
            app.edit(if legacy_click {
                Edit::Click(Point::default())
            } else {
                Edit::Ring(Point::default(), None)
            });
            assert!(app.error);
            assert_eq!(app.tab.doc, original);
            assert_eq!(app.tab.selected, vec![carbon]);
            assert_eq!(app.tab.revision, revision);
            assert!(!app.tab.history.can_undo());
            assert!(app.tab.history.can_redo());
            let _ = app.update(Message::Redo);
            assert_eq!(app.tab.doc, placed);
        }

        let (mut app, _) = App::new();
        let _ = app.update(Message::New);
        app.tab.doc = reshiki::rings::Preset::Regular.document(42., false);
        app.aromatic_ring = false;
        app.ring_size = 6;
        app.tab.selected = app.tab.doc.all_ids();
        let original = app.tab.doc.clone();
        let selected = app.tab.selected.clone();
        let a = app.tab.doc.atom(app.tab.doc.bonds[0].a).unwrap().position;
        let b = app.tab.doc.atom(app.tab.doc.bonds[0].b).unwrap().position;
        let p = Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.);
        let revision = app.tab.revision;
        app.edit(Edit::Ring(p, Some(Point::default())));
        assert!(app.error);
        assert_eq!(app.tab.doc, original);
        assert_eq!(app.tab.selected, selected);
        assert_eq!(app.tab.revision, revision);
        assert!(!app.tab.history.can_undo());
        app.edit(Edit::Ring(p, Some(Point::new(p.x * 2., p.y * 2.))));
        assert_eq!((app.tab.doc.atoms.len(), app.tab.doc.bonds.len()), (10, 11));
        let placed = app.tab.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, original);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, placed);
    }

    #[test]
    fn ring_presets_use_atomic_history_and_leave_invalid_hosts_untouched() {
        use reshiki::rings::Preset;
        let (mut app, _) = App::new();
        app.tab.doc = Document::default();
        let before = app.tab.doc.clone();
        app.edit(Edit::RingPreset(
            Preset::ChairUp,
            Point::default(),
            Some(Point::new(0., 80.)),
            false,
            false,
        ));
        let placed = app.tab.doc.clone();
        assert_eq!(placed.atoms.len(), 6);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, placed);
        app.edit(Edit::RingPreset(
            Preset::ChairDown,
            Point::new(300., 0.),
            None,
            false,
            false,
        ));
        assert_eq!(app.tab.doc.atoms.len(), 12);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, placed);
        let _ = app.update(Message::Tool(Tool::RingPreset(Preset::ChairUp)));
        let _ = app.update(Message::New);
        assert_eq!(app.tool, Tool::Select);
        assert_eq!(app.tab.bond_drawing.length, 42.);
        let c = app.tab.doc.add_atom("C", Point::default());
        app.tab.doc.atom_mut(c).unwrap().radical_electrons = 1;
        let before = app.tab.doc.clone();
        app.edit(Edit::RingPreset(
            Preset::ChairUp,
            Point::default(),
            None,
            false,
            false,
        ));
        assert_eq!(app.tab.doc, before);
        assert!(app.error);
    }

    #[test]
    fn arrow_click_places_a_fixed_rightward_arrow_and_repeated_click_reverses_it() {
        use reshiki::arrows::{ArrowStyle, Preset};
        use reshiki::graphics::LinePattern;
        for zoom in [0.5, 2.5] {
            let (mut app, _) = App::new();
            app.tab.camera.zoom = zoom;
            let style = ArrowStyle {
                pattern: LinePattern::Dashed,
                ..ArrowStyle::preset(Preset::Forward)
            };
            let _ = app.update(Message::Palette(palettes::Action::ArrowVariant(
                Preset::Forward,
                style.clone(),
            )));
            let blank = app.tab.doc.clone();
            let start = Point::new(-100., 30.);
            app.edit(Edit::Click(start));
            assert_eq!(app.tab.doc.arrows.len(), 1);
            let arrow = app.tab.doc.arrows[0].clone();
            assert_eq!(arrow.start, start);
            assert_eq!(
                arrow.end,
                start.offset(blank.drawing_style.bond_length_world * 2., 0.)
            );
            assert_eq!(arrow.appearance(), style);
            assert_eq!(app.tab.selected, [arrow.id]);
            let placed = app.tab.doc.clone();
            app.tab.selected.clear();
            app.edit(Edit::Click(arrow.point(0.5)));
            assert_eq!(app.tab.doc.arrows.len(), 1);
            assert_eq!(app.tab.doc.arrows[0].start, arrow.end);
            assert_eq!(app.tab.doc.arrows[0].end, arrow.start);
            assert_eq!(app.tab.selected, [arrow.id]);
            let reversed = app.tab.doc.clone();
            let _ = app.update(Message::Undo);
            assert_eq!(app.tab.doc, placed);
            let _ = app.update(Message::Undo);
            assert_eq!(app.tab.doc, blank);
            let _ = app.update(Message::Redo);
            assert_eq!(app.tab.doc, placed);
            let _ = app.update(Message::Redo);
            assert_eq!(app.tab.doc, reversed);
        }
    }

    #[test]
    fn arrow_tool_click_applies_variants_and_cycles_half_heads_without_adding_objects() {
        use reshiki::arrows::{ArrowStyle, Head, Preset};
        let (mut app, _) = App::new();
        let _ = app.update(Message::Tool(Tool::Arrow));
        app.edit(Edit::Click(Point::default()));
        let before = app.tab.doc.clone();
        let style = ArrowStyle {
            head: Head::Left,
            ..ArrowStyle::default()
        };
        let _ = app.update(Message::Palette(palettes::Action::ArrowVariant(
            Preset::Forward,
            style.clone(),
        )));
        let midpoint = app.tab.doc.arrows[0].point(0.5);
        app.edit(Edit::Click(midpoint));
        assert_eq!(app.tab.doc.arrows.len(), 1);
        assert_eq!(app.tab.doc.arrows[0].appearance(), style);
        app.edit(Edit::Click(midpoint));
        assert_eq!(app.tab.doc.arrows[0].appearance().head, Head::Right);
        app.edit(Edit::Click(midpoint));
        assert_eq!(app.tab.doc.arrows[0].appearance().head, Head::Left);
        for _ in 0..3 {
            let _ = app.update(Message::Undo);
        }
        assert_eq!(app.tab.doc, before);
    }

    #[test]
    fn repeated_arrow_click_reverses_reaction_roles_and_undo_restores_them() {
        use reshiki::reactions::{Participant, Reaction};
        let (mut app, _) = App::new();
        let reactant = app.tab.doc.add_atom("O", Point::new(-100., 0.));
        let product = app.tab.doc.add_atom("N", Point::new(200., 0.));
        let _ = app.update(Message::Tool(Tool::Arrow));
        app.edit(Edit::Click(Point::default()));
        let arrow = &app.tab.doc.arrows[0];
        let midpoint = arrow.point(0.5);
        let mut reaction = Reaction::new(arrow.id);
        reaction.reactants.push(Participant {
            atoms: vec![reactant],
            coefficient: 1,
        });
        reaction.products.push(Participant {
            atoms: vec![product],
            coefficient: 1,
        });
        app.tab.doc.reactions.push(reaction);
        let before = app.tab.doc.clone();
        app.edit(Edit::Click(midpoint));
        assert_eq!(
            app.tab.doc.reactions[0].reactants,
            before.reactions[0].products
        );
        assert_eq!(
            app.tab.doc.reactions[0].products,
            before.reactions[0].reactants
        );
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
    }

    #[test]
    fn arrow_edits_keep_bend_history_and_new_resets_jacs_defaults() {
        use arrows::{Action, Field};
        use reshiki::arrows::{ArrowStyle, Head, Preset};
        let (mut app, _) = App::new();
        app.tab.doc = Document::default();
        app.tab.saved = app.tab.doc.clone();
        let _ = app.update(Message::ArrowStyle(Preset::Fishhook));
        app.edit(Edit::Bond(
            Point::new(0., 0.),
            Point::new(120., 0.),
            None,
            None,
        ));
        let id = app.tab.selected[0];
        app.edit(Edit::ArrowHandle(id, 2, Point::new(60., -50.)));
        let bent = app.tab.doc.clone();
        app.arrow_action(Action::Number(Field::Line, "1.5".into()));
        app.arrow_action(Action::ApplyNumber(Field::Line));
        assert_eq!(app.tab.doc.arrows[0].appearance().width_pt, 1.5);
        assert_eq!(app.tab.doc.arrows[0].control, bent.arrows[0].control);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, bent);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.arrows.style.width_pt, 1.5);
        app.arrow_action(Action::Tail(Head::Full));
        app.arrow_action(Action::Reverse);
        assert_eq!(app.tab.doc.arrows[0].end, Point::default());
        app.arrow_action(Action::Number(Field::Length, "NaN".into()));
        let before = app.tab.doc.clone();
        app.arrow_action(Action::ApplyNumber(Field::Length));
        assert_eq!(app.tab.doc, before);
        assert!(app.error);
        let _ = app.update(Message::New);
        assert_eq!(app.tab.arrows.style, ArrowStyle::default());
        assert_eq!(app.tab.arrow_style, Preset::Forward);
        assert_eq!(app.tab.caption_format.style.family, "Arial");
        assert_eq!(app.tab.caption_format.style.size_pt, 10.);
        assert_eq!(
            app.tab.bond_drawing.length,
            reshiki::style::DEFAULT.bond_length_world
        );
        assert!(app.tab.doc.arrows.is_empty());
    }

    #[test]
    fn arrow_width_in_mixed_selection_preserves_bonds_and_other_objects() {
        use arrows::{Action, Field};

        for selected in [vec![10], vec![1, 2, 3, 10, 20]] {
            let (mut app, _) = App::new();
            app.tab.doc = Document::from_json(include_bytes!(
                "../tests/fixtures/ui-declutter/mixed-arrow-width.rsk"
            ))
            .unwrap();
            app.tab.saved = app.tab.doc.clone();
            let _ = app.update(Message::Canvas(Edit::Select(selected.clone())));
            let before = app.tab.doc.clone();
            assert!(!app.tab.history.can_undo());

            let _ = app.update(Message::ArrowAction(Action::Number(
                Field::Line,
                "1.5".into(),
            )));
            assert_eq!(app.tab.doc, before, "Typing must not change the drawing");
            assert!(!app.tab.history.can_undo());
            let _ = app.update(Message::ArrowAction(Action::ApplyNumber(Field::Line)));
            assert!(!app.error, "{}", app.status);

            let mut expected = before.clone();
            expected
                .arrows
                .iter_mut()
                .find(|arrow| arrow.id == 10)
                .unwrap()
                .style
                .as_mut()
                .unwrap()
                .width_pt = 1.5;
            assert_eq!(app.tab.doc.drawing_style, before.drawing_style);
            assert_eq!(app.tab.doc.bonds, before.bonds);
            assert_eq!(
                app.tab.doc, expected,
                "Only the selected arrow width changes"
            );
            assert_eq!(app.tab.selected, selected);
            let saved = serde_json::to_vec(&app.tab.doc).unwrap();
            assert_eq!(Document::from_json(&saved).unwrap(), expected);

            let _ = app.update(Message::Undo);
            assert_eq!(app.tab.doc, before);
            assert_eq!(app.tab.selected, selected);
            assert!(
                !app.tab.history.can_undo(),
                "Apply is exactly one Undo step"
            );
            let _ = app.update(Message::Redo);
            assert_eq!(app.tab.doc, expected);
            assert_eq!(app.tab.selected, selected);
            assert_eq!(app.tab.arrows.style.width_pt, 1.5);
        }
    }

    #[test]
    fn library_authoring_is_independent_of_drawing_history_and_repeat_placement_keeps_anchor() {
        use reshiki::templates::Anchor;
        use template_library::Action as A;
        let (mut app, _) = App::new();
        let a = app.tab.doc.add_atom("C", Point::default());
        let b = app.tab.doc.add_atom("O", Point::new(42., 0.));
        app.tab.doc.add_bond(a, b, 1, "plain");
        app.tab.selected = vec![a, b];
        let before = app.tab.doc.clone();
        let revision = app.tab.revision;
        let _ = app.update(Message::Templates(A::BeginSave));
        app.tab.selected.clear();
        let _ = app.update(Message::Templates(A::Name("Methanol".into())));
        let _ = app.update(Message::Templates(A::SaveDetails));
        assert_eq!(app.templates.library.templates[0].document, before);
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.revision, revision);
        let index = app.template_index;
        let _ = app.update(Message::InsertTemplate(index));
        let _ = app.update(Message::Templates(A::Anchor(Anchor::Atom(b))));
        let _ = app.update(Message::Templates(A::RememberAnchor));
        let _ = app.update(Message::Templates(A::Browse));
        app.templates.connection = reshiki::templates::Connection::FuseBond;
        let _ = app.update(Message::InsertTemplate(index));
        assert_eq!(app.templates.anchor, Anchor::Atom(b));
        assert_eq!(
            app.templates.connection,
            reshiki::templates::Connection::Connect
        );
        let _ = app.update(Message::Templates(A::Repeat(true)));
        app.edit(Edit::Template(Point::new(250., 100.), None));
        assert_eq!(app.tool, Tool::Template);
        assert_eq!(app.tab.doc.atoms.len(), 4);
        assert_eq!(app.tab.doc.atoms[3].position, Point::new(250., 100.));
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        let _ = app.update(Message::Templates(A::Remove));
        assert!(app.templates.library.templates.is_empty());
        assert_eq!(app.tab.doc, before);
        let _ = app.update(Message::Templates(A::Restore));
        assert_eq!(app.templates.library.templates.len(), 1);
        assert_eq!(app.templates.library.templates[0].anchor, Anchor::Atom(b));
        let caption = app.tab.doc.next_id();
        app.tab.doc.annotations.push(Annotation {
            id: caption,
            position: Point::new(0., 60.),
            text: "Label".into(),
            format: Default::default(),
        });
        app.inspector_tab = InspectorTab::Templates;
        let _ = app.update(Message::Canvas(Edit::Select(vec![caption])));
        assert_eq!(app.inspector_tab, InspectorTab::Templates);
    }

    #[test]
    fn attached_marks_are_single_history_edits_and_removal_updates_chemistry() {
        use reshiki::scientific::{MarkKind, SymbolKind};
        let (mut app, _) = App::new();
        app.tab.doc = Document::default();
        let id = app.tab.doc.add_atom("N", Point::default());
        let before = app.tab.doc.clone();
        app.tool = Tool::Graphic(reshiki::graphics::GraphicKind::Symbol(
            SymbolKind::CirclePlus,
        ));
        app.edit(Edit::Graphic(Point::default(), Point::default(), false));
        assert_eq!(app.tab.selected, vec![id]);
        assert_eq!(app.tab.doc.atoms[0].charge, 1);
        assert_eq!(app.tab.doc.atoms[0].marks[0].kind, MarkKind::CircledCharge);
        let attached = app.tab.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, attached);
        app.edit(Edit::AtomMark(id, 0, Point::new(-30., 20.)));
        assert_eq!(app.tab.doc.atoms[0].marks[0].offset, Point::new(-30., 20.));
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, attached);
        let _ = app.update(Message::RemoveMark(id, 0));
        assert_eq!(app.tab.doc.atoms[0].charge, 0);
        assert!(app.tab.doc.atoms[0].marks.is_empty());
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, attached);
    }

    #[test]
    fn every_new_document_starts_with_jacs_drawing_and_typography_defaults() {
        let (mut app, _) = App::new();
        app.tab.orbital_phase = reshiki::scientific::Phase::Shaded;
        app.tab.phase_flipped = true;
        app.tab.attach_symbols = false;
        app.tab.graphic_style.width_pt = 3.;
        app.tab.caption_format.style.family = "Times New Roman".into();
        app.tab.caption_format.style.size_pt = 18.;
        app.tab.caption_format.style.color = reshiki::palette::Color::Custom([190, 30, 40]);
        app.tab.caption_format.style.bold = true;
        let _ = app.update(Message::DrawingLength("30".into()));
        let _ = app.update(Message::FixedLength(false));
        let _ = app.update(Message::FixedAngles(false));
        let _ = app.update(Message::ChainAngle("90".into()));
        let _ = app.update(Message::New);
        assert_eq!(
            app.tab.caption_format.style,
            reshiki::typography::TextStyle::default()
        );
        assert_eq!(app.tab.caption_format.style.family, "Arial");
        assert_eq!(app.tab.font_size_input, "10");
        assert_eq!(app.tab.text_color_input, "#000000");
        assert_eq!(app.tab.drawing_length_input, "14.4");
        assert_eq!(app.tab.bond_drawing.length, 42.);
        assert_eq!(app.tab.graphic_width_input, "0.6");
        assert_eq!(app.tab.orbital_phase, reshiki::scientific::Phase::Solid);
        assert!(!app.tab.phase_flipped && app.tab.attach_symbols);
        assert_eq!(app.tab.graphic_style.width_pt, 0.6);
        assert_eq!(app.tab.chain_drawing.angle, 120.);
        assert!(app.tab.bond_drawing.fixed_angles && app.tab.bond_drawing.fixed_length);
        assert_eq!(app.tool, Tool::Select);
    }

    #[test]
    fn entire_chain_is_one_history_step_and_draw_settings_do_not_edit_the_document() {
        let (mut app, _) = App::new();
        app.tab.doc = Document::default();
        let before = app.tab.doc.clone();
        let _ = app.update(Message::ChainAtoms("8".into()));
        let _ = app.update(Message::DrawingLength("20".into()));
        let _ = app.update(Message::ChainAngle("110".into()));
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.revision, 0);
        let points = reshiki::chains::straight(
            Point::default(),
            Point::new(350., 0.),
            false,
            app.tab.bond_drawing,
            app.tab.chain_drawing,
            false,
        );
        app.tool = Tool::Chain(reshiki::chains::ChainMode::Straight);
        app.edit(Edit::Chain {
            points,
            source: None,
            target: None,
        });
        assert_eq!((app.tab.doc.atoms.len(), app.tab.doc.bonds.len()), (8, 7));
        let drawn = app.tab.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, drawn);
        app.edit(Edit::Chain {
            points: vec![Point::default(), Point::new(42., 0.)],
            source: None,
            target: None,
        });
        assert!(app.error);
        assert_eq!(app.tab.doc, drawn);
        app.tool = Tool::Bond(1);
        let last = app.tab.doc.atoms.last().unwrap().position;
        app.edit(Edit::Click(last));
        assert!(
            (app.tab.doc.atoms.last().unwrap().position.distance(last)
                - reshiki::style::DEFAULT.world(20.))
            .abs()
                < 0.001
        );
        let drawing = app.tab.doc.clone();
        let _ = app.update(Message::ResetBondDrawing);
        assert_eq!(
            app.tab.bond_drawing.length,
            reshiki::style::DEFAULT.bond_length_world
        );
        assert_eq!(app.tab.drawing_length_input, "14.4");
        assert_eq!(app.tab.chain_drawing.angle, 120.);
        assert!(app.tab.bond_drawing.fixed_length && app.tab.bond_drawing.fixed_angles);
        assert_eq!(app.tab.doc, drawing);
    }

    #[test]
    fn palette_keeps_text_range_formatting_and_recolors_graphics_only_in_all_scope() {
        use iced::widget::text_editor::{Action, Motion};
        use reshiki::typography::StyleChange;
        use typography::ColorScope;
        let (mut app, _) = App::new();
        app.tab.doc = Document::default();
        app.tab.doc.annotations.push(Annotation {
            id: 1,
            position: Point::default(),
            text: "AB CD".into(),
            format: Default::default(),
        });
        app.tab.doc.graphics.push(Graphic::dragged(
            2,
            reshiki::graphics::GraphicKind::Rectangle,
            Point::new(0., 80.),
            Point::new(84., 120.),
            GraphicStyle {
                fill: Some(reshiki::palette::Color::Custom([200, 200, 200])),
                ..Default::default()
            },
            Default::default(),
            false,
        ));
        app.tab.selected = vec![1];
        app.sync_typography();
        app.caption_action(Action::Move(Motion::DocumentStart));
        app.caption_action(Action::Select(Motion::Right));
        app.caption_action(Action::Select(Motion::Right));
        assert_eq!(app.text_range(), Some(0..2));
        let original = app.tab.doc.clone();
        let red = reshiki::palette::Color::Custom([180, 50, 55]);
        let _ = app.update(Message::TextStyle(StyleChange::Color(red)));
        assert_eq!(app.tab.doc.annotations[0].format.at(0).color, red);
        assert_eq!(
            app.tab.doc.annotations[0].format.at(3).color,
            reshiki::palette::Color::Ink
        );
        assert_eq!(app.tab.doc.graphics, original.graphics);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, original);
        // A stale range in the inspector must not constrain Select All.
        let _ = app.update(Message::SelectAll);
        let _ = app.update(Message::ColorScope(ColorScope::Text));
        let _ = app.update(Message::TextStyle(StyleChange::Color(red)));
        assert_eq!(app.tab.doc.graphics, original.graphics);
        assert_eq!(app.tab.doc.annotations[0].format.at(3).color, red);
        let _ = app.update(Message::ColorScope(ColorScope::All));
        let _ = app.update(Message::TextStyle(StyleChange::Color(red)));
        assert_eq!(app.tab.doc.graphics[0].style.stroke, red);
        assert_eq!(app.tab.doc.graphics[0].style.fill, Some(red));
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc.graphics, original.graphics);
    }

    #[test]
    fn palette_scopes_recolor_selected_bonds_and_objects_in_one_undo() {
        use reshiki::typography::StyleChange;
        use typography::ColorScope;
        let (mut app, _) = App::new();
        app.tab.doc = Document::default();
        let a = app.tab.doc.add_atom("C", Point::default());
        let b = app.tab.doc.add_atom("O", Point::new(42., 0.));
        let c = app.tab.doc.add_atom("N", Point::new(84., 0.));
        app.tab.doc.add_bond(a, b, 1, "plain");
        app.tab.doc.add_bond(b, c, 2, "plain");
        app.tab.doc.arrows.push(Arrow::new(
            4,
            Point::new(0., 80.),
            Point::new(84., 80.),
            Default::default(),
            Default::default(),
        ));
        app.tab.doc.annotations.push(Annotation {
            id: 5,
            position: Point::new(0., 120.),
            text: "Label".into(),
            format: Default::default(),
        });
        app.tab.doc.atom_mut(b).unwrap().display.number = Some(reshiki::atom_labels::Number {
            text: "2".into(),
            offset: None,
            style: reshiki::atom_labels::number_style(),
        });
        let original = app.tab.doc.clone();
        let blue = reshiki::palette::Color::Palette(
            reshiki::palette::Hue::Blue,
            reshiki::palette::Row::Strong,
        );
        // Typed colors are custom and exact.
        let red = reshiki::palette::Color::Custom([180, 50, 55]);
        let _ = app.update(Message::SelectAll);
        let _ = app.update(Message::TextStyle(StyleChange::Color(blue)));
        assert!(
            app.tab
                .doc
                .bonds
                .iter()
                .all(|b| b.color == blue && b.indicator.style.color == blue)
        );
        assert!(
            app.tab
                .doc
                .atoms
                .iter()
                .all(|a| a.text_style.as_ref().unwrap().color == blue)
        );
        assert_eq!(
            app.tab
                .doc
                .atom(b)
                .unwrap()
                .display
                .number
                .as_ref()
                .unwrap()
                .style
                .color,
            blue
        );
        assert_eq!(app.tab.doc.arrows[0].appearance().color, blue);
        assert_eq!(app.tab.doc.annotations[0].format.style.color, blue);
        let recolored = app.tab.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, original);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, recolored);
        app.tab.selected = vec![a, b];
        let _ = app.update(Message::ColorScope(ColorScope::Bonds));
        let _ = app.update(Message::TextColor("#B43237".into()));
        let _ = app.update(Message::ApplyTextColor);
        assert_eq!(app.tab.doc.bonds[0].color, red);
        assert_eq!(app.tab.doc.bonds[1].color, blue);
        assert_eq!(
            app.tab.doc.atoms[1].text_style.as_ref().unwrap().color,
            blue
        );
        assert_eq!(app.tab.doc.arrows, recolored.arrows);
        let _ = app.update(Message::ColorScope(ColorScope::Text));
        let _ = app.update(Message::TextStyle(StyleChange::Color(red)));
        assert_eq!(app.tab.doc.atoms[0].text_style.as_ref().unwrap().color, red);
        assert_eq!(app.tab.doc.atoms[2], recolored.atoms[2]);
        assert_eq!(app.tab.doc.bonds[1].color, blue);
        assert_eq!(app.tab.doc.arrows, recolored.arrows);
        // Selecting only one end of a bond does not recolor that bond.
        app.tab.selected = vec![c];
        let _ = app.update(Message::ColorScope(ColorScope::All));
        let _ = app.update(Message::TextStyle(StyleChange::Color(red)));
        assert_eq!(app.tab.doc.bonds[1].color, blue);
    }

    #[test]
    fn bond_styles_position_color_and_direction_are_undoable() {
        use reshiki::bonds::{BondPreset, DoublePosition};
        let (mut app, _) = App::new();
        app.tab.doc.add_atom("C", Point::new(0., 0.));
        app.tab.doc.add_atom("C", Point::new(84., 0.));
        app.tab.doc.add_atom("C", Point::new(168., 0.));
        app.tab.doc.add_bond(1, 2, 2, "plain");
        app.tab.doc.add_bond(2, 3, 1, "plain");
        app.tab.selected = vec![1, 2];
        let other = app.tab.doc.bonds[1].clone();
        let _ = app.update(Message::BondPosition(DoublePosition::Left));
        let _ = app.update(Message::BondColor("#205091".into()));
        let _ = app.update(Message::ApplyBondColor);
        assert_eq!(
            app.tab.doc.bonds[0].color,
            reshiki::palette::Color::Custom([32, 80, 145])
        );
        assert_eq!(app.tab.doc.bonds[1], other);
        let _ = app.update(Message::ApplyBondPreset(BondPreset::HollowWedge));
        assert_eq!(app.tab.doc.bonds[0].display, "hollow_wedge");
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc.bonds[0].order, 2);
        let _ = app.update(Message::Redo);
        app.tool = Tool::StyledBond(BondPreset::HollowWedge);
        app.edit(Edit::Click(Point::new(42., 0.)));
        assert_eq!((app.tab.doc.bonds[0].a, app.tab.doc.bonds[0].b), (2, 1));
        let _ = app.update(Message::Undo);
        assert_eq!((app.tab.doc.bonds[0].a, app.tab.doc.bonds[0].b), (1, 2));
        let before = app.tab.doc.clone();
        app.tool = Tool::StyledBond(BondPreset::Dotted);
        app.edit(Edit::Bond(
            Point::new(0., 0.),
            Point::new(168., 0.),
            Some(1),
            Some(3),
        ));
        assert!(app.error);
        assert_eq!(app.tab.doc, before);
    }

    #[test]
    fn aromatic_bond_tools_preserve_circles_and_undo_direction_changes() -> anyhow::Result<()> {
        use anyhow::Context;
        use reshiki::bonds::BondPreset as P;
        for preset in [P::Wedge, P::HashedWedge, P::HollowWedge, P::Bold, P::Hashed] {
            let (mut app, _) = App::new();
            let ids = editing::ring(&mut app.tab.doc, Point::default(), 6, true, 0.);
            reshiki::projection::tilt(&mut app.tab.doc, &ids, 35., true);
            app.tab.doc.reconcile_molecule_groups();
            let source = app.tab.doc.clone();
            let bond = source.bonds.first().context("Missing ring edge")?;
            let a = source.atom(bond.a).context("Missing ring atom")?.position;
            let b = source.atom(bond.b).context("Missing ring atom")?.position;
            let midpoint = Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.);
            app.tab.camera.zoom = 2.;
            app.tool = match preset {
                P::Wedge => Tool::Wedge,
                P::HashedWedge => Tool::Hash,
                p => Tool::StyledBond(p),
            };
            app.edit(Edit::Click(midpoint));
            assert!(!app.error, "{}", app.status);
            assert_eq!(app.tab.doc.bonds[0].display, preset.parts().1);
            assert!(app.tab.doc.bonds.iter().all(|b| b.order == 4));
            assert_eq!(reshiki::aromatic::circles(&app.tab.doc).len(), 1);
            assert!(!chemistry_changed(&source, &app.tab.doc));
            assert_eq!(app.tab.doc.atoms, source.atoms);
            let styled = app.tab.doc.clone();
            app.edit(Edit::Click(midpoint));
            assert_eq!(
                (app.tab.doc.bonds[0].a, app.tab.doc.bonds[0].b),
                (bond.b, bond.a)
            );
            assert_eq!(reshiki::aromatic::circles(&app.tab.doc).len(), 1);
            assert!(!chemistry_changed(&source, &app.tab.doc));
            let _ = app.update(Message::Undo);
            assert_eq!(app.tab.doc, styled);
            let _ = app.update(Message::Undo);
            assert_eq!(app.tab.doc, source);
            let _ = app.update(Message::Redo);
            assert_eq!(app.tab.doc, styled);
            app.tool = Tool::Bond(1);
            app.edit(Edit::Click(midpoint));
            assert_eq!(
                app.tab.doc, source,
                "Plain appearance retains aromatic order"
            );
            app.tool = Tool::Bond(2);
            app.edit(Edit::Click(midpoint));
            assert_eq!(
                app.tab.doc.bonds[0].order, 2,
                "Explicit double order still works"
            );
        }
        Ok(())
    }

    #[test]
    fn aromatic_bond_properties_and_dragging_keep_ring_chemistry() -> anyhow::Result<()> {
        use anyhow::Context;
        use reshiki::bonds::BondPreset as P;
        for preset in [
            P::Wedge,
            P::HashedWedge,
            P::HollowWedge,
            P::Bold,
            P::Hashed,
            P::Wavy,
            P::Single,
        ] {
            let (mut app, _) = App::new();
            app.tab.selected = editing::ring(&mut app.tab.doc, Point::default(), 6, true, 0.);
            reshiki::projection::tilt(&mut app.tab.doc, &app.tab.selected, 65., true);
            app.tab.doc.reconcile_molecule_groups();
            let source = app.tab.doc.clone();
            let _ = app.update(Message::ApplyBondPreset(preset));
            assert!(!app.error, "{}", app.status);
            assert!(app.tab.doc.bonds.iter().all(|b| b.order == 4));
            assert!(
                app.tab
                    .doc
                    .bonds
                    .iter()
                    .all(|b| b.display == preset.parts().1)
            );
            assert_eq!(reshiki::aromatic::circles(&app.tab.doc).len(), 1);
            assert!(!chemistry_changed(&source, &app.tab.doc));
            assert_eq!(app.tab.doc.atoms, source.atoms);
            if preset != P::Single {
                let _ = app.update(Message::Undo);
                assert_eq!(app.tab.doc, source);
            }
            let bond = source.bonds.first().context("Missing ring bond")?;
            let a = source.atom(bond.a).context("Missing ring atom")?.position;
            let b = source.atom(bond.b).context("Missing ring atom")?.position;
            app.tool = Tool::StyledBond(preset);
            app.edit(Edit::Bond(b, a, Some(bond.b), Some(bond.a)));
            assert!(!app.error, "{}", app.status);
            assert_eq!(
                (app.tab.doc.bonds[0].a, app.tab.doc.bonds[0].b),
                (bond.b, bond.a)
            );
            assert!(app.tab.doc.bonds.iter().all(|b| b.order == 4));
            assert_eq!(reshiki::aromatic::circles(&app.tab.doc).len(), 1);
            assert!(!chemistry_changed(&source, &app.tab.doc));
            let _ = app.update(Message::Undo);
            assert_eq!(app.tab.doc, source);
        }
        Ok(())
    }

    #[test]
    fn group_frame_and_ungroup_are_individually_undoable() {
        let (mut app, _) = App::new();
        let a = app.tab.doc.add_atom("C", Point::default());
        let b = app.tab.doc.add_atom("O", Point::new(42., 0.));
        app.tab.doc.add_bond(a, b, 1, "plain");
        app.tab.selected = vec![a];
        let initial = app.tab.doc.clone();
        let _ = app.update(Message::AddFrame(reshiki::graphics::GraphicKind::Brackets));
        assert_eq!(app.tab.doc.groups.len(), 1);
        assert_eq!(app.tab.doc.graphics.len(), 1);
        assert_eq!(app.tab.selected.len(), 3);
        assert_eq!(app.tab.doc.atoms, initial.atoms);
        let framed = app.tab.doc.clone();
        let _ = app.update(Message::Ungroup);
        assert!(app.tab.doc.groups.is_empty());
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, framed);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, initial);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, framed);
        assert_eq!(app.tab.selected.len(), 3);
        let _ = app.update(Message::InvertSelection);
        assert!(app.tab.selected.is_empty());
    }

    #[tokio::test]
    async fn graphic_style_point_edits_and_undo_retain_editable_selection() {
        use reshiki::graphics::GraphicKind;
        let (mut app, _) = App::new();
        let result = app
            .engine
            .execute(Request::import_smiles("CCO"))
            .await
            .unwrap();
        app.tab.doc = result.document.unwrap();
        app.tab.analysis = result.analysis;
        let _ = app.update(Message::Tool(Tool::Graphic(GraphicKind::Curve)));
        app.edit(Edit::Graphic(
            Point::default(),
            Point::new(100., 40.),
            false,
        ));
        let id = app.tab.doc.graphics[0].id;
        assert_eq!(app.tool, Tool::Select);
        app.apply_graphic_style(GraphicChange::Stroke(reshiki::palette::Color::Custom([
            32, 80, 145,
        ])));
        let before = app.tab.doc.clone();
        let _ = app.update(Message::Tool(Tool::EditPoints));
        app.edit(Edit::GraphicPoint(id, 1, Point::new(20., -50.)));
        assert_eq!(app.tab.doc.graphics[0].kind, GraphicKind::Path);
        let after = app.tab.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.selected, vec![id]);
        assert_eq!(app.tab.analysis.as_ref().unwrap().formula, "C2H6O");
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, after);
        assert_eq!(app.tab.selected, vec![id]);
        assert!(app.tab.analysis.is_some());
        let _ = app.update(Message::Tool(Tool::Graphic(GraphicKind::Ellipse)));
        assert!(app.tab.selected.is_empty());
        app.apply_graphic_style(GraphicChange::Stroke(reshiki::palette::Color::Custom([
            180, 50, 55,
        ])));
        assert_eq!(
            app.tab.doc, after,
            "new drawing style must not change the previous object"
        );
    }

    #[test]
    fn partial_typography_edit_and_repeated_backspace_restore_with_undo() {
        use iced::widget::text_editor::{Action, Edit as TextEdit, Motion};
        use reshiki::typography::{StyleChange, TextFormat};
        let (mut app, _) = App::new();
        app.tab.doc.annotations.push(Annotation {
            id: 1,
            position: Point::default(),
            text: "AAA".into(),
            format: TextFormat::default(),
        });
        app.tab.selected = vec![1];
        app.sync_typography();
        app.caption_action(Action::Move(Motion::DocumentStart));
        app.caption_action(Action::Move(Motion::Right));
        app.caption_action(Action::Select(Motion::Right));
        assert_eq!(app.text_range(), Some(1..2));
        app.apply_text_style(StyleChange::Bold(true));
        assert!(!app.tab.doc.annotations[0].format.at(0).bold);
        assert!(app.tab.doc.annotations[0].format.at(1).bold);
        let styled = app.tab.doc.clone();
        app.caption_action(Action::Move(Motion::DocumentStart));
        app.caption_action(Action::Move(Motion::Right));
        app.caption_action(Action::Edit(TextEdit::Backspace));
        assert_eq!(app.tab.doc.annotations[0].text, "AA");
        assert!(app.tab.doc.annotations[0].format.at(0).bold);
        assert!(!app.tab.doc.annotations[0].format.at(1).bold);
        let edited = app.tab.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, styled);
        assert_eq!(app.tab.caption, "AAA");
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, edited);
        assert_eq!(app.tab.caption, "AA");
    }

    #[test]
    fn one_off_template_choice_is_nonmutating_and_attachment_is_one_undo_step() {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let a = app.tab.doc.add_atom("C", Point::new(-30.0, 0.0));
        let b = app.tab.doc.add_atom("C", Point::new(30.0, 0.0));
        app.tab.doc.add_bond(a, b, 1, "plain");
        app.tab.saved = app.tab.doc.clone();
        let before = app.tab.doc.clone();
        let index = reshiki::templates::LIBRARY
            .iter()
            .position(|t| t.name == "Cyclopentane")
            .unwrap();
        let _ = app.update(Message::InsertTemplate(index));
        assert_eq!(app.tab.doc, before);
        assert!(!app.dirty());
        assert_eq!(app.tool, Tool::Template);
        let _ = app.update(Message::Templates(template_library::Action::Repeat(false)));
        app.templates.connection = reshiki::templates::Connection::FuseBond;
        app.edit(Edit::Template(Point::default(), None));
        assert_eq!(app.tool, Tool::Select);
        let placed = app.tab.doc.clone();
        assert_eq!(placed.atoms.len(), 5);
        assert_eq!(placed.bonds.len(), 5);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, placed);
        let _ = app.update(Message::Tool(Tool::Select));
        app.edit(Edit::Template(Point::new(500.0, 500.0), None));
        assert_eq!(app.tab.doc, placed);
    }

    #[test]
    fn templates_repeat_bond_fusion_by_default_until_cancelled() {
        use reshiki::templates::{Anchor, Connection};
        use template_library::Action as A;
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let a = app.tab.doc.add_atom("C", Point::new(0., -21.));
        let b = app.tab.doc.add_atom("C", Point::new(0., 21.));
        app.tab.doc.add_bond(a, b, 1, "plain");
        let index = reshiki::templates::LIBRARY
            .iter()
            .position(|t| t.name == "Cyclohexane")
            .unwrap();
        let source = &reshiki::templates::LIBRARY[index].document.bonds[0];
        let anchor = Anchor::Bond(source.a, source.b);
        let _ = app.update(Message::InsertTemplate(index));
        let _ = app.update(Message::Templates(A::Anchor(anchor)));
        assert!(app.templates.repeat);
        let mut snapshots = vec![app.tab.doc.clone()];
        for (atoms, bonds) in [(6, 6), (10, 11), (14, 16)] {
            let point = app
                .tab
                .doc
                .bonds
                .iter()
                .map(|bond| {
                    let p = app.tab.doc.atom(bond.a).unwrap().position;
                    let q = app.tab.doc.atom(bond.b).unwrap().position;
                    Point::new((p.x + q.x) / 2., (p.y + q.y) / 2.)
                })
                .max_by(|p, q| p.x.total_cmp(&q.x))
                .unwrap();
            app.edit(Edit::Template(point, None));
            assert!(!app.error, "{}", app.status);
            assert_eq!(
                (app.tab.doc.atoms.len(), app.tab.doc.bonds.len()),
                (atoms, bonds)
            );
            assert_eq!(app.tool, Tool::Template);
            assert_eq!(app.templates.anchor, anchor);
            assert_eq!(app.templates.connection, Connection::FuseBond);
            app.tab.doc.validate().unwrap();
            snapshots.push(app.tab.doc.clone());
        }
        // A misplaced click must preserve both the drawing and placement mode.
        app.edit(Edit::Template(app.tab.doc.atoms[0].position, None));
        assert!(app.error);
        assert_eq!(app.tab.doc, snapshots[3]);
        assert_eq!(app.tool, Tool::Template);
        for document in snapshots[..3].iter().rev() {
            let _ = app.update(Message::Undo);
            assert_eq!(&app.tab.doc, document);
            assert_eq!(app.tool, Tool::Template);
        }
        for document in &snapshots[1..] {
            let _ = app.update(Message::Redo);
            assert_eq!(&app.tab.doc, document);
        }
        let _ = app.update(Message::Escape);
        assert_eq!(app.tool, Tool::Select);
        app.edit(Edit::Template(Point::new(500., 500.), None));
        assert_eq!(app.tab.doc, snapshots[3]);
    }

    #[test]
    fn axis_resize_has_one_step_undo_and_ignores_invalid_scales() -> Result<(), String> {
        let (mut app, _) = App::new();
        let a = app.tab.doc.add_atom("C", Point::new(-20., -10.));
        let b = app.tab.doc.add_atom("C", Point::new(20., 10.));
        let remote = app.tab.doc.add_atom("O", Point::new(150., 80.));
        app.tab.doc.add_bond(a, b, 1, "wedge");
        let original = app.tab.doc.clone();
        app.edit(Edit::ScaleAxes {
            ids: vec![a, b],
            pivot: Point::new(-20., -10.),
            x: 2.,
            y: 1.,
        });
        assert_eq!(
            app.tab.doc.atom(b).ok_or("Atom")?.position,
            Point::new(60., 10.)
        );
        assert_eq!(app.tab.doc.atom(remote), original.atom(remote));
        assert_eq!(app.tab.doc.bonds, original.bonds);
        let resized = app.tab.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, original);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, resized);
        for (x, y) in [(1., 1.), (f32::NAN, 1.), (0., 1.), (1., -1.)] {
            app.edit(Edit::ScaleAxes {
                ids: vec![a, b],
                pivot: Point::default(),
                x,
                y,
            });
            assert_eq!(app.tab.doc, resized);
        }
        let _ = app.update(Message::Undo);
        assert_eq!(
            app.tab.doc, original,
            "No-op drags do not consume undo steps"
        );
        Ok(())
    }

    #[test]
    fn selection_handle_transforms_preserve_other_objects_and_undo_in_one_step() {
        let (mut app, _) = App::new();
        let a = app.tab.doc.add_atom("C", Point::new(-20.0, -10.0));
        let b = app.tab.doc.add_atom("C", Point::new(20.0, 10.0));
        let other = app.tab.doc.add_atom("O", Point::new(150.0, 80.0));
        app.tab.doc.add_bond(a, b, 1, "wedge");
        let original = app.tab.doc.clone();
        app.edit(Edit::Transform {
            ids: vec![a, b],
            pivot: Point::new(-20.0, -10.0),
            scale: 2.0,
            rotation: 0.0,
        });
        let resized = app.tab.doc.clone();
        assert_eq!(
            resized.atom(a).unwrap().position,
            original.atom(a).unwrap().position
        );
        assert_eq!(resized.atom(b).unwrap().position, Point::new(60.0, 30.0));
        app.edit(Edit::Transform {
            ids: vec![a, b],
            pivot: Point::new(20.0, 10.0),
            scale: 1.0,
            rotation: 90.0,
        });
        let rotated = app.tab.doc.clone();
        assert!(
            rotated
                .atom(b)
                .unwrap()
                .position
                .distance(Point::new(0.0, 50.0))
                < 0.001
        );
        assert_eq!(rotated.atom(other), original.atom(other));
        assert_eq!(rotated.bonds, original.bonds);
        assert_eq!(app.tab.selected, vec![a, b]);
        for expected in [&resized, &original] {
            let _ = app.update(Message::Undo);
            assert_eq!(&app.tab.doc, expected);
        }
        for expected in [&resized, &rotated] {
            let _ = app.update(Message::Redo);
            assert_eq!(&app.tab.doc, expected);
        }
        app.edit(Edit::Transform {
            ids: vec![a, b],
            pivot: Point::default(),
            scale: 1.0,
            rotation: 0.0,
        });
        let _ = app.update(Message::Undo);
        assert_eq!(
            app.tab.doc, resized,
            "clicking a handle without dragging adds no history"
        );
        app.tool = Tool::Ring;
        let _ = app.update(Message::SelectAll);
        assert_eq!(app.tool, Tool::Select);
        assert_eq!(app.tab.selected, app.tab.doc.all_ids());
    }

    #[test]
    fn snapping_a_ring_is_one_undoable_edit_with_original_atom_ids_restored() {
        let (mut app, _) = App::new();
        let a = app.tab.doc.add_atom("C", Point::default());
        let b = app.tab.doc.add_atom("C", Point::new(60.0, 0.0));
        app.tab.doc.add_bond(a, b, 1, "plain");
        let ids = editing::ring(&mut app.tab.doc, Point::new(200.0, 200.0), 5, false, 5.0);
        let p = app.tab.doc.atom(ids[0]).unwrap().position;
        let q = app.tab.doc.atom(ids[1]).unwrap().position;
        let before = app.tab.doc.clone();
        app.edit(Edit::Move(
            ids,
            30.0 - (p.x + q.x) / 2.0,
            -(p.y + q.y) / 2.0,
        ));
        let snapped = app.tab.doc.clone();
        assert_eq!((snapped.atoms.len(), snapped.bonds.len()), (5, 5));
        assert_eq!(app.tab.selected.len(), 5);
        assert!(app.tab.selected.contains(&a) && app.tab.selected.contains(&b));
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, snapped);
    }

    #[test]
    fn a_smart_guide_drag_is_one_undoable_edit() {
        use reshiki::graphics::{Graphic, GraphicKind};
        let (mut app, _) = App::new();
        for (lo, hi) in [((-150., -100.), (-90., -60.)), ((0., 0.), (60., 40.))] {
            let id = app.tab.doc.next_id();
            app.tab.doc.graphics.push(Graphic::dragged(
                id,
                GraphicKind::Rectangle,
                Point::new(lo.0, lo.1),
                Point::new(hi.0, hi.1),
                Default::default(),
                Default::default(),
                false,
            ));
        }
        let moving = app.tab.doc.graphics[1].id;
        let before = app.tab.doc.clone();
        let edits = crate::canvas::select_drag(
            &before,
            &[],
            Point::new(0., 20.),
            Point::new(-3., -77.),
            Default::default(),
        );
        for edit in edits {
            let _ = app.update(Message::Canvas(edit));
        }
        let mut expected = before.clone();
        expected.translate(&[moving], -3., -100.);
        assert_eq!(app.tab.doc, expected, "the top edges snapped together");
        assert_eq!(app.tab.selected, [moving]);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, expected);
    }

    #[test]
    fn drag_duplicate_keeps_the_original_and_is_one_undoable_edit() {
        let (mut app, _) = App::new();
        let a = app.tab.doc.add_atom("C", Point::default());
        let b = app.tab.doc.add_atom("O", Point::new(42.0, 0.0));
        let c = app.tab.doc.add_atom("N", Point::new(84.0, 0.0));
        app.tab.doc.add_bond(a, b, 2, "plain");
        app.tab.doc.add_bond(b, c, 1, "plain");
        let before = app.tab.doc.clone();
        app.edit(Edit::Duplicate(vec![a, b], 0.0, 90.0));
        let copied = app.tab.doc.clone();
        assert_eq!((copied.atoms.len(), copied.bonds.len()), (5, 3));
        for id in [a, b, c] {
            assert_eq!(copied.atom(id), before.atom(id), "originals stay in place");
        }
        assert_eq!(
            app.tab.selected.len(),
            2,
            "the copy of the two atoms is selected"
        );
        assert!(!app.tab.selected.iter().any(|id| [a, b, c].contains(id)));
        let symbols: Vec<_> = app
            .tab
            .selected
            .iter()
            .filter_map(|id| copied.atom(*id))
            .map(|atom| (atom.element.as_str(), atom.position))
            .collect();
        assert_eq!(
            symbols,
            vec![("C", Point::new(0.0, 90.0)), ("O", Point::new(42.0, 90.0))]
        );
        copied.validate().unwrap();
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, copied);
    }

    #[test]
    fn clicking_existing_bonds_cycles_order_and_can_be_undone() {
        for tool in [Tool::Bond(1), Tool::Bond(3)] {
            let (mut app, _) = App::new();
            app.tool = tool;
            let a = app.tab.doc.add_atom("C", Point::default());
            let b = app.tab.doc.add_atom("C", Point::new(42.0, 0.0));
            app.tab.doc.add_bond(a, b, 1, "plain");
            let original = app.tab.doc.clone();
            for order in [2, 3, 1] {
                app.edit(Edit::Click(Point::new(21.0, 0.0)));
                assert_eq!(app.tab.doc.atoms, original.atoms);
                assert_eq!(app.tab.doc.bonds.len(), 1);
                assert_eq!(app.tab.doc.bonds[0].order, order);
                assert_eq!(app.tab.doc.bonds[0].display, "plain");
            }
            assert_eq!(app.tab.doc, original);
            for order in [3, 2, 1] {
                let _ = app.update(Message::Undo);
                assert_eq!(app.tab.doc.bonds[0].order, order);
            }
            assert_eq!(app.tab.doc, original);
            app.tool = Tool::Wedge;
            app.edit(Edit::Click(Point::new(21.0, 0.0)));
            assert_eq!(app.tab.doc.bonds[0].order, 1);
            assert_eq!(app.tab.doc.bonds[0].display, "wedge");
        }
    }

    #[tokio::test]
    async fn endpoint_clicks_grow_a_connected_zigzag_with_undo_and_redo() {
        let (mut app, _) = App::new();
        app.tool = Tool::Bond(1);
        app.edit(Edit::Click(Point::default()));
        for _ in 0..5 {
            let endpoint = app
                .tab
                .doc
                .atom(*app.tab.selected.first().unwrap())
                .unwrap()
                .position;
            app.edit(Edit::Click(endpoint));
        }
        assert_eq!((app.tab.doc.atoms.len(), app.tab.doc.bonds.len()), (7, 6));
        for three in app.tab.doc.atoms.windows(3) {
            let a = three[0].position;
            let b = three[1].position;
            let c = three[2].position;
            let cosine = ((a.x - b.x) * (c.x - b.x) + (a.y - b.y) * (c.y - b.y))
                / (a.distance(b) * c.distance(b));
            assert!((cosine + 0.5).abs() < 0.001, "chain needs 120° junctions");
            assert!(c.x > b.x && b.x > a.x, "chain must keep extending forward");
            assert!((a.y - c.y).abs() < 0.001, "successive turns must alternate");
        }
        let complete = app.tab.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!((app.tab.doc.atoms.len(), app.tab.doc.bonds.len()), (6, 5));
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, complete);
        let analysis = app
            .engine
            .execute(Request::molecule("analyze", complete))
            .await
            .unwrap()
            .analysis
            .unwrap();
        assert_eq!(analysis.smiles, "CCCCCCC");
        assert_eq!(analysis.formula, "C7H16");
    }

    #[test]
    fn aromatic_plane_bond_matches_preview_and_undo_restores_xyz() -> Result<(), String> {
        use reshiki::projection::growth::{self, Plane};
        let (mut app, _) = App::new();
        app.tab.doc = reshiki::rings::Preset::Benzene.document(42., false);
        let ids = app.tab.doc.all_ids();
        reshiki::projection::tilt(&mut app.tab.doc, &ids, 55., false);
        let id = app.tab.doc.atoms.get(1).ok_or("Carbon")?.id;
        let end = Plane::at(&app.tab.doc, id)
            .ok_or("Plane")?
            .outward(42.)
            .ok_or("Endpoint")?;
        let before = app.tab.doc.clone();
        app.tool = Tool::Atom;
        app.element = "O".into();
        let (preview, added) =
            growth::place(&before, id, end, "O", reshiki::bonds::BondPreset::Single)?;
        app.edit(Edit::PlaneBond(id, end));
        assert_eq!(app.tab.doc, preview);
        assert_eq!(app.tab.selected, vec![added]);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, preview);
        Ok(())
    }

    #[test]
    fn bond_tools_grow_carbon_after_using_an_atom_label_and_still_edit_bonds() {
        let (mut app, _) = App::new();
        let _ = app.update(Message::Element("O".into()));
        app.edit(Edit::Click(Point::default()));
        let oxygen = app.tab.doc.atoms[0].id;
        let _ = app.update(Message::Tool(Tool::Bond(1)));
        app.edit(Edit::Click(Point::default()));
        let carbon = app.tab.doc.atoms[1].clone();
        assert_eq!(carbon.element, "C");
        assert_eq!(app.tab.doc.atom(oxygen).unwrap().element, "O");
        app.edit(Edit::Bond(
            carbon.position,
            carbon.position.offset(36.373066, 21.0),
            Some(carbon.id),
            None,
        ));
        assert_eq!(app.tab.doc.atoms[2].element, "C");
        app.tool = Tool::Bond(2);
        app.edit(Edit::Click(Point::new(
            carbon.position.x / 2.0,
            carbon.position.y / 2.0,
        )));
        assert_eq!((app.tab.doc.atoms.len(), app.tab.doc.bonds.len()), (3, 2));
        assert_eq!(app.tab.doc.bonds[0].order, 2);
    }

    #[test]
    fn blank_drawings_keep_starting_zoom_through_resize_and_first_edits() {
        let (mut app, _) = App::new();
        assert_eq!(app.tab.camera.zoom, 1.0);
        let _ = app.update(Message::Viewport(iced::Size::new(1000., 700.)));
        assert_eq!(app.tab.camera.zoom, 1.0);

        let _ = app.update(Message::Tool(Tool::Atom));
        app.edit(Edit::Click(Point::default()));
        assert!(!app.tab.doc.all_ids().is_empty());
        let _ = app.update(Message::Viewport(iced::Size::new(700., 500.)));
        assert_eq!(app.tab.camera.zoom, 1.0);

        let _ = app.update(Message::Zoom(1.2));
        let _ = app.update(Message::Viewport(iced::Size::new(1000., 700.)));
        assert_eq!(app.tab.camera.zoom, 1.2);

        let _ = app.update(Message::New);
        assert!(app.tab.doc.all_ids().is_empty());
        assert_eq!(app.tab.camera.zoom, 1.0);
        let _ = app.update(Message::Viewport(iced::Size::new(700., 500.)));
        assert_eq!(app.tab.camera.zoom, 1.0);
    }

    #[test]
    fn fitting_an_empty_drawing_restores_starting_view() {
        let (mut app, _) = App::new();
        app.edit(Edit::Pan(60., -20.));
        let _ = app.update(Message::Zoom(2.));
        let _ = app.update(Message::Fit);
        assert_eq!(app.tab.camera.zoom, 1.0);
        assert_eq!(app.tab.camera.center, Point::default());
        let _ = app.update(Message::Viewport(iced::Size::new(1000., 700.)));
        assert_eq!(app.tab.camera.zoom, 1.0);
    }

    #[test]
    fn fit_uses_available_canvas_and_respects_manual_pan() {
        let (mut app, _) = App::new();
        app.tab.doc.add_atom("C", Point::new(-250.0, -100.0));
        app.tab.doc.add_atom("O", Point::new(250.0, 100.0));
        let document = app.tab.doc.clone();
        let _ = app.update(Message::Viewport(iced::Size::new(600.0, 400.0)));
        let _ = app.update(Message::Fit);
        let small_zoom = app.tab.camera.zoom;
        let _ = app.update(Message::Viewport(iced::Size::new(1000.0, 700.0)));
        assert!(app.tab.camera.zoom > small_zoom);
        app.edit(Edit::Pan(60.0, -20.0));
        let camera = app.tab.camera;
        let _ = app.update(Message::Viewport(iced::Size::new(700.0, 500.0)));
        assert_eq!(app.tab.camera.center, camera.center);
        assert_eq!(app.tab.camera.zoom, camera.zoom);
        assert_eq!(app.tab.doc, document);
    }

    #[test]
    fn view_aids_preserve_drawing_selection_history_and_manual_camera() {
        let (mut app, _) = App::new();
        let id = app.tab.doc.add_atom("O", Point::new(50., 20.));
        app.tab.selected = vec![id];
        app.edit(Edit::Pan(60., -20.));
        let document = app.tab.doc.clone();
        let camera = app.tab.camera;
        let revision = app.tab.revision;
        let history = app.tab.history.can_undo();
        let export = reshiki::export::drawing(&app.tab.doc, "svg").expect("SVG before view change");
        for message in [
            Message::ToggleView,
            Message::Rulers(true),
            Message::Crosshair(true),
            Message::RulerUnit(canvas::guides::Unit::Inches),
            Message::Grid,
            Message::SmartGuides(false),
        ] {
            let _ = app.update(message);
        }
        // Smart guides are on by default, also for settings saved before them.
        assert!(!app.appearance.smart_guides);
        let legacy: crate::appearance::Settings =
            serde_json::from_str(r#"{"mode":"light","arrange_controls":false}"#).unwrap();
        assert!(legacy.smart_guides && !legacy.arrange_controls);
        assert_eq!(app.tab.doc, document);
        assert_eq!(app.tab.selected, [id]);
        assert_eq!(app.tab.revision, revision);
        assert_eq!(app.tab.history.can_undo(), history);
        assert_eq!(app.tab.camera.center, camera.center);
        assert_eq!(app.tab.camera.zoom, camera.zoom);
        assert_eq!(
            reshiki::export::drawing(&app.tab.doc, "svg").expect("SVG after view change"),
            export
        );
    }

    #[test]
    fn native_extensions_open_the_same_editable_document_and_keep_the_path() {
        let mut document: Document =
            serde_json::from_str(include_str!("../tests/fixtures/ui-drawn-ethanol.reshiki"))
                .unwrap();
        document.version = 15;
        reshiki::atom_labels::clear_computed(&mut document);
        let contents = serde_json::to_string_pretty(&document).unwrap();
        for extension in ["rsk", "RSK", "reshiki", "moruno"] {
            let (mut app, _) = App::new();
            let path = PathBuf::from(format!("Ethanol.{extension}"));
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let opened = runtime.block_on(files::prepare_contents(
                path.clone(),
                Ok(contents.clone().into_bytes()),
            ));
            let _ = app.update(Message::FilePrepared(opened));
            assert!(!app.error && !app.dirty(), "{extension}: {}", app.status);
            assert_eq!(app.tab.doc, document);
            assert_eq!(app.tab.path, Some(path));
            assert_eq!(app.status, "Document opened");
            assert!(app.tab.fit_to_view);
            assert!(app.tab.camera.zoom > Camera::default().zoom);
            assert!(app.tab.camera.zoom <= 2.5);
        }
    }

    #[test]
    fn drawings_from_a_newer_reshiki_ask_for_an_update_and_keep_the_current_drawing() {
        let (mut app, _) = App::new();
        app.tab.doc.add_atom("O", Point::default());
        let before = app.tab.doc.clone();
        let newer = format!(
            r#"{{"version": {}, "atoms": [], "bonds": [], "future": "blue.strong"}}"#,
            reshiki::document::VERSION + 1
        );
        let path = PathBuf::from("newer.rsk");
        let task = app.update(Message::Opened(Some((
            path.clone(),
            Ok(newer.clone().into_bytes()),
        ))));
        assert!(task.units() > 0);
        files::finish_dispatched_open(&mut app, path, Ok(newer.into_bytes()));
        assert!(app.error);
        assert!(
            app.status.ends_with(&format!(
                "This drawing was made with a newer version of ReShiki (document version {}). Update ReShiki to open it.",
                reshiki::document::VERSION + 1
            )),
            "{}",
            app.status
        );
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.path, None);
    }

    #[test]
    fn late_save_does_not_mark_newer_edits_as_saved() {
        let (mut app, _) = App::new();
        let snapshot = app.tab.doc.clone();
        app.tab.doc.add_atom("O", Point::default());
        let _ = app.update(Message::Saved(
            0,
            Box::new(snapshot),
            Ok(Some("example.reshiki".into())),
        ));
        assert!(app.dirty());
    }

    #[test]
    fn unsaved_changes_wait_for_one_save_dialog_answer() {
        let (mut app, _) = App::new();
        app.tab.doc.add_atom("O", Point::default());
        let edited = app.tab.doc.clone();
        let window = iced::window::Id::unique();
        assert!(app.update(Message::Close(window)).units() > 0);
        assert!(matches!(app.pending, Some(Pending::CloseWindow(id, ..)) if id == window));
        // Further requests are ignored while the dialog is open.
        for message in [
            Message::Close(window),
            Message::New,
            Message::Open,
            Message::Tabs(tabs::Action::Close(None)),
        ] {
            assert_eq!(app.update(message).units(), 0);
        }
        assert!(matches!(app.pending, Some(Pending::CloseWindow(..))));
        assert_eq!(app.strip().count(), 1);
        let _ = app.update(Message::Cancel);
        assert!(app.pending.is_none());
        assert_eq!(app.tab.doc, edited);
        let _ = app.update(Message::Tabs(tabs::Action::Close(None)));
        assert!(matches!(app.pending, Some(Pending::CloseTab(_))));
        let _ = app.update(Message::Discard);
        assert!(app.pending.is_none());
        assert!(
            app.tab.doc.all_ids().is_empty(),
            "The last tab gives way to an empty one"
        );
    }

    #[test]
    fn save_answer_closes_the_tab_only_after_it_is_saved() {
        let (mut app, _) = App::new();
        app.tab.doc.add_atom("O", Point::default());
        let _ = app.update(Message::Tabs(tabs::Action::Close(None)));
        // Cancelling Save As cancels the close too, so a later save does not continue it.
        let epoch = app.tab.file_epoch;
        let _ = app.update(Message::Saved(
            epoch,
            Box::new(app.tab.doc.clone()),
            Ok(None),
        ));
        assert!(app.pending.is_none());
        let saved = Ok(Some(PathBuf::from("drawing.reshiki")));
        let _ = app.update(Message::Saved(
            epoch,
            Box::new(app.tab.doc.clone()),
            saved.clone(),
        ));
        assert_eq!(app.tab.doc.atoms.len(), 1);
        app.tab.doc.add_atom("N", Point::new(80., 0.));
        let _ = app.update(Message::Tabs(tabs::Action::Close(None)));
        let _ = app.update(Message::Saved(epoch, Box::new(app.tab.doc.clone()), saved));
        assert!(app.pending.is_none());
        assert!(app.tab.doc.all_ids().is_empty() && app.tab.path.is_none());
    }

    #[test]
    fn save_answer_passes_an_open_atom_editor() {
        let (mut app, _) = App::new();
        let atom = app.tab.doc.add_atom("C", Point::default());
        app.tab.selected = vec![atom];
        let _ = app.update(Message::AtomText(atom_text::Action::Begin(None)));
        assert_eq!(app.update(Message::Save).units(), 0);
        let _ = app.update(Message::Close(iced::window::Id::unique()));
        assert!(app.pending.is_some());
        assert!(app.update(Message::Save).units() > 0);
    }

    #[test]
    fn late_save_does_not_retarget_another_document() {
        let (mut app, _) = App::new();
        let snapshot = app.tab.doc.clone();
        let _ = app.update(Message::New);
        let _ = app.update(Message::Saved(
            0,
            Box::new(snapshot),
            Ok(Some("previous.reshiki".into())),
        ));
        assert!(app.tab.path.is_none());
    }

    #[test]
    fn startup_is_a_blank_saved_canvas_without_an_import_job() {
        let (app, _) = App::new();
        assert_eq!(app.tab.doc, Document::default());
        assert!(app.tab.doc.all_ids().is_empty());
        assert!(!app.tab.busy && !app.dirty() && !app.tab.history.can_undo());
        assert!(app.tab.analysis.is_none() && app.tab.path.is_none());
        assert!(app.imports.is_blank());
    }

    #[test]
    fn new_document_invalidates_inflight_import_even_when_empty() {
        let (mut app, _) = App::new();
        let revision = app.tab.revision;
        let _ = app.update(Message::New);
        let mut old = Document::default();
        old.add_atom("O", Point::default());
        let _ = app.update(Message::EngineDone {
            revision,
            kind: Job::Import,
            result: Box::new(Ok(Response {
                document: Some(old),
                analysis: None,
                output: None,
                engine_version: "test".into(),
                warnings: vec![],
            })),
        });
        assert!(app.tab.doc.atoms.is_empty());
    }
}
