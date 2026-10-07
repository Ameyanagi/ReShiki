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
mod assistant;
mod atom_edits;
mod atom_labels;
mod atom_text;
mod autosave;
mod bond_edits;
mod canvas_edit;
mod cleanup;
mod clipboard;
mod color_popover;
mod context_menu;
mod depth_appearance;
mod dispatch;
mod document_styles;
mod document_tab;
use document_tab::DocumentTab;
mod engine_jobs;
mod figure_export;
mod file_shortcuts;
mod files;
#[cfg(target_os = "macos")]
mod macos_files;
#[cfg(target_os = "macos")]
pub(crate) use macos_files::install_document_events;
mod gates;
mod graphics;
mod help;
mod history;
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
mod reactions;
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
    Graphics(graphics::Action),
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
    fn changed(&mut self, before: Document) {
        if self.tab.doc != before {
            self.tab.erase_stroke = false;
        }
        self.changed_continuing(before, false);
    }
    fn changed_continuing(&mut self, before: Document, continuing: bool) {
        self.tab.cleanup = None;
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
        let committed = reconciled.commit(&mut self.tab.doc, &mut self.tab.history, continuing);
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
