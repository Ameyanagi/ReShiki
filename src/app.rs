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
mod arrows;
mod assistant;
mod atom_labels;
mod atom_text;
mod cleanup;
mod clipboard;
mod context_menu;
mod document_styles;
mod file_shortcuts;
mod files;
mod graphics;
mod help;
mod icons;
mod inline_text;
mod inspector;
mod joining;
mod pages;
mod palettes;
mod pictures;
mod printing;
mod reactions;
mod shortcuts;
mod template_library;
mod tool_button;
mod typography;
mod updates;
mod workspace;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InspectorTab {
    Reactions,
    DrawingStyle,
    Assistant,
    Pages,
    Abbreviations,
    Properties,
    Labels,
    Templates,
    Export,
}

#[derive(Debug, Clone)]
pub enum Message {
    ContextMenu(context_menu::Action),
    InspectorAction(inspector::Action),
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
    RefreshLabels,
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
    GraphicLayer(bool),
    ToggleInspector,
    Inspector(InspectorTab),
    ToggleImport,
    InsertInput,
    ToggleHelp,
    Viewport(iced::Size),
    Canvas(Edit),
    Tool(Tool),
    Element(String),
    CaptionAction(iced::widget::text_editor::Action),
    TextStyle(reshiki::typography::StyleChange),
    FontSize(String),
    ApplyFontSize,
    ColorScope(typography::ColorScope),
    TextColor(String),
    ApplyTextColor,
    TextAlign(reshiki::typography::TextAlign),
    GroupLabelAlign(reshiki::abbreviations::LabelAlignment),
    TextSpacing(f32),
    TextWidth(String),
    ApplyTextWidth,
    Smiles(String),
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
    ToggleView,
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
    ClipboardWritten {
        epoch: u64,
        revision: u64,
        cut_ids: Vec<u64>,
        result: Result<reshiki::clipboard::CopyOutcome, String>,
    },
    ClipboardRead {
        epoch: u64,
        revision: u64,
        result: Box<Result<Document, String>>,
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
    Opened(Option<(PathBuf, Result<String, String>)>),
    Saved(u64, Box<Document>, Result<Option<PathBuf>, String>),
    Exported(Result<Option<PathBuf>, String>),
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
    RefreshLabels,
    Clean(cleanup::CleanupJob),
    Export(&'static str),
}
#[derive(Debug, Clone)]
enum Pending {
    New,
    Open,
    Close(iced::window::Id),
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
    context_menu: Option<context_menu::State>,
    updates: updates::State,
    styles: document_styles::State,
    palette: Option<palettes::Family>,
    toolbar: palettes::Memory,
    erase_stroke: bool,
    erase_committed: bool,
    assistant: assistant::State,
    hover: Option<(Point, u64)>,
    cleanup: Option<CleanupPreview>,
    cleanup_serial: u64,
    abbreviations: abbreviations::State,
    labels: atom_labels::State,
    refresh_due: Option<std::time::Instant>,
    chemistry_notice: Option<String>,
    bond_drawing: reshiki::chains::BondDrawing,
    chain_drawing: reshiki::chains::ChainDrawing,
    drawing_length_input: String,
    chain_atoms_input: String,
    chain_angle_input: String,
    graphic_style: GraphicStyle,
    orbital_phase: reshiki::scientific::Phase,
    phase_flipped: bool,
    attach_symbols: bool,
    graphic_width_input: String,
    graphic_stroke_input: String,
    graphic_fill_input: String,
    bracket_sides: BracketSides,
    doc: Document,
    history: History,
    selected: Vec<u64>,
    camera: Camera,
    tool: Tool,
    element: String,
    caption: String,
    caption_editor: iced::widget::text_editor::Content,
    caption_format: reshiki::typography::TextFormat,
    caption_target: Option<u64>,
    inline_text: Option<inline_text::State>,
    atom_text: Option<atom_text::State>,
    joining: Option<joining::State>,
    pages: pages::State,
    reactions: reactions::State,
    printing: printing::State,
    pictures: pictures::State,
    font_options: iced::widget::combo_box::State<String>,
    font_size_input: String,
    text_color_input: String,
    color_scope: typography::ColorScope,
    text_width_input: String,
    bond_color_input: String,
    smiles: String,
    isotope: String,
    grid: bool,
    guides: canvas::guides::Guides,
    view_open: bool,
    analysis: Option<Analysis>,
    engine: LocalEngine,
    revision: u64,
    busy: bool,
    clipboard_busy: bool,
    status: String,
    error: bool,
    path: Option<PathBuf>,
    #[cfg(windows)]
    office_path: Option<PathBuf>,
    saved: Document,
    pending: Option<Pending>,
    file_epoch: u64,
    ring_size: u8,
    aromatic_ring: bool,
    arrow_style: reshiki::arrows::Preset,
    arrows: arrows::State,
    custom_element: String,
    recovery: Option<Recovery>,
    recovered: Vec<Candidate>,
    autosaved_revision: Option<u64>,
    autosave_status: String,
    inspector_open: bool,
    inspector_ui: inspector::State,
    inspector_tab: InspectorTab,
    import_open: bool,
    help_open: bool,
    viewport: iced::Size,
    fit_to_view: bool,
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
        let mut app = Self {
            updates: updates::State::new(),
            styles: Default::default(),
            reactions: Default::default(),
            palette: None,
            toolbar: palettes::Memory::default(),
            erase_stroke: false,
            erase_committed: false,
            assistant: assistant::State::new(),
            hover: None,
            cleanup: None,
            cleanup_serial: 0,
            labels: Default::default(),
            abbreviations: Default::default(),
            refresh_due: None,
            chemistry_notice: None,
            context_menu: None,
            bond_drawing: Default::default(),
            chain_drawing: Default::default(),
            drawing_length_input: reshiki::style::DEFAULT.bond_length_pt.to_string(),
            chain_atoms_input: String::new(),
            chain_angle_input: "120".into(),
            graphic_style: GraphicStyle::default(),
            orbital_phase: Default::default(),
            phase_flipped: false,
            attach_symbols: true,
            graphic_width_input: "0.6".into(),
            graphic_stroke_input: "#000000".into(),
            graphic_fill_input: "#DCEFE9".into(),
            bracket_sides: BracketSides::Both,
            doc: Document::default(),
            history: History::default(),
            selected: vec![],
            camera: Camera::default(),
            tool: Tool::Select,
            element: "C".into(),
            caption: "Reaction conditions".into(),
            caption_editor: iced::widget::text_editor::Content::with_text("Reaction conditions"),
            caption_format: Default::default(),
            caption_target: None,
            inline_text: None,
            atom_text: None,
            joining: None,
            pages: pages::State::default(),
            printing: printing::State::default(),
            pictures: pictures::State::default(),
            font_options: iced::widget::combo_box::State::new(
                reshiki::style::font_families()
                    .iter()
                    .map(|s| (*s).to_owned())
                    .collect(),
            ),
            font_size_input: "10".into(),
            text_color_input: "#000000".into(),
            color_scope: Default::default(),
            bond_color_input: "#000000".into(),
            text_width_input: String::new(),
            smiles: String::new(),
            isotope: String::new(),
            grid: false,
            guides: Default::default(),
            view_open: false,
            analysis: None,
            engine: LocalEngine::default(),
            revision: 0,
            busy: false,
            clipboard_busy: false,
            status: "Ready · Choose a tool to start drawing".into(),
            error: false,
            path: None,
            #[cfg(windows)]
            office_path: None,
            saved: Document::default(),
            pending: None,
            file_epoch: 0,
            ring_size: 6,
            aromatic_ring: false,
            arrow_style: Default::default(),
            arrows: Default::default(),
            custom_element: String::new(),
            recovery,
            recovered,
            autosaved_revision: None,
            autosave_status: String::new(),
            inspector_open: true,
            inspector_ui: inspector::State::default(),
            inspector_tab: InspectorTab::Properties,
            import_open: false,
            help_open: false,
            viewport: iced::Size::new(850.0, 600.0),
            fit_to_view: true,
            template_index: 0,
            templates: template_library::State::load(),
        };
        let startup_path = if cfg!(test) {
            None
        } else {
            let mut args = std::env::args_os().skip(1);
            args.find(|arg| arg == "--open")
                .and_then(|_| args.next())
                .map(PathBuf::from)
        };
        #[cfg(windows)]
        if !cfg!(test) && std::env::args_os().any(|arg| arg == "--office-edit") {
            app.office_path = startup_path.clone();
        }
        let task = if let Some(path) = startup_path {
            Task::perform(
                async move {
                    let read_path = path.clone();
                    let result = tokio::task::spawn_blocking(move || {
                        std::fs::read_to_string(read_path).map_err(|e| e.to_string())
                    })
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r);
                    Some((path, result))
                },
                Message::Opened,
            )
        } else {
            Task::none()
        };
        let update_check = app.update_action(updates::Action::Check(false));
        (app, Task::batch([task, update_check]))
    }
    pub fn title(&self) -> String {
        format!(
            "{}{} — ReShiki",
            self.path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Untitled".into()),
            if self.dirty() { " •" } else { "" }
        )
    }
    pub fn theme(&self) -> Theme {
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
            self.updates.subscription(),
            self.properties_subscription(),
            if self.assistant.needs_poll() {
                iced::time::every(std::time::Duration::from_millis(200))
                    .map(|_| Message::Assistant(assistant::Action::Poll))
            } else {
                Subscription::none()
            },
            if self.refresh_due.is_some()
                && !self.busy
                && self.cleanup.is_none()
                && !self.erase_stroke
            {
                iced::time::every(std::time::Duration::from_millis(250))
                    .map(|_| Message::RefreshLabels)
            } else {
                Subscription::none()
            },
            if self.recovery.is_some()
                && ((self.dirty() && self.autosaved_revision != Some(self.revision))
                    || (!self.dirty() && self.autosaved_revision.is_some()))
            {
                iced::time::every(std::time::Duration::from_secs(5)).map(|_| Message::Tick)
            } else {
                Subscription::none()
            },
            iced::window::close_requests().map(Message::Close),
            iced::event::listen_with(|event, status, _window| {
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
        self.inline_changed() || !same_drawing(&self.doc, &self.saved)
    }
    fn office_document(&self) -> bool {
        #[cfg(windows)]
        {
            self.path.is_some() && self.path == self.office_path
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
    fn clear_recovery(&mut self) {
        if let Some(recovery) = &self.recovery {
            let _ = recovery.clear();
        }
        self.autosaved_revision = None;
        self.autosave_status.clear();
    }
    fn run(&mut self, request: Request, kind: Job) -> Task<Message> {
        if self.busy {
            return Task::none();
        }
        self.busy = true;
        if !matches!(kind, Job::RefreshLabels) {
            self.error = false;
            self.status = "Working…".into();
        }
        let engine = self.engine.clone();
        let revision = self.revision;
        Task::perform(
            async move { engine.execute(request).await },
            move |result| Message::EngineDone {
                revision,
                kind: kind.clone(),
                result: Box::new(result),
            },
        )
    }
    fn changed(&mut self, before: Document) {
        if self.doc != before {
            self.erase_stroke = false;
        }
        self.changed_continuing(before, false);
    }
    fn changed_continuing(&mut self, before: Document, continuing: bool) {
        reshiki::projection::sync_centroids(&mut self.doc);
        self.cleanup = None;
        self.doc.reconcile_abbreviations(&before);
        if let Err(error) = reshiki::reactions::reconcile(&mut self.doc) {
            self.doc = before;
            self.error = true;
            self.status = error;
            return;
        }
        if self.doc != before {
            if let Err(error) = self.doc.validate() {
                self.doc = before;
                self.error = true;
                self.status = format!("Edit cancelled: {error}");
                return;
            }
            self.doc.reconcile_molecule_groups();
        }
        let drawing_style_changed = before.drawing_style != self.doc.drawing_style;
        let chemistry_changed = chemistry_changed(&before, &self.doc);
        if chemistry_changed {
            reshiki::atom_labels::clear_computed(&mut self.doc);
            self.refresh_due =
                Some(std::time::Instant::now() + std::time::Duration::from_millis(350));
            self.chemistry_notice = None;
        }
        if self
            .history
            .commit_continuing(before, &self.doc, continuing)
        {
            self.revision = self.revision.wrapping_add(1);
            if chemistry_changed {
                self.analysis = None;
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
        self.selected.retain(|id| self.doc.all_ids().contains(id));
    }
    fn fit(&mut self) {
        self.pages.fit = None;
        let (lo, hi) = self.display_document().bounds();
        self.camera.center = Point::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0);
        let viewport = self.guides.paper(iced::Rectangle::with_size(self.viewport));
        self.camera.zoom = ((viewport.width - 80.0).max(100.0) / (hi.x - lo.x).max(240.0))
            .min((viewport.height - 80.0).max(100.0) / (hi.y - lo.y).max(200.0))
            .clamp(0.25, 2.5);
        self.fit_to_view = true;
    }
    fn pending(&mut self, action: Pending) -> Task<Message> {
        if self.dirty() {
            self.pending = Some(action);
            Task::none()
        } else {
            self.perform(action)
        }
    }
    fn perform(&mut self, action: Pending) -> Task<Message> {
        match action {
            Pending::New => {
                self.labels = Default::default();
                self.refresh_due = None;
                self.chemistry_notice = None;
                self.clear_recovery();
                self.file_epoch = self.file_epoch.wrapping_add(1);
                self.revision = self.revision.wrapping_add(1);
                let before = self.doc.clone();
                self.doc = Document::default();
                self.changed(before);
                self.saved = self.doc.clone();
                self.path = None;
                self.selected.clear();
                self.camera = Camera::default();
                self.pages = pages::State::default();
                self.styles.editor = None;
                self.fit_to_view = true;
                self.analysis = None;
                self.error = false;
                self.bond_drawing = Default::default();
                self.chain_drawing = Default::default();
                self.drawing_length_input = reshiki::style::DEFAULT.bond_length_pt.to_string();
                self.chain_atoms_input.clear();
                self.chain_angle_input = "120".into();
                self.caption_format = Default::default();
                self.caption = "Reaction conditions".into();
                self.caption_target = None;
                self.graphic_style = Default::default();
                self.orbital_phase = Default::default();
                self.phase_flipped = false;
                self.attach_symbols = true;
                self.arrow_style = Default::default();
                self.arrows = Default::default();
                self.graphic_width_input = reshiki::style::DEFAULT.line_width_pt.to_string();
                self.graphic_stroke_input = "#000000".into();
                self.graphic_fill_input = "#DCEFE9".into();
                self.color_scope = Default::default();
                self.bond_color_input = "#000000".into();
                self.tool = Tool::Select;
                self.sync_typography();
                self.status = "New document".into();
                iced::advanced::widget::operate(
                    iced::advanced::widget::operation::focusable::unfocus(),
                )
            }
            Pending::Open => Task::perform(
                async {
                    // Accept supported extensions without relying on macOS
                    // type registration; validate the selected content below.
                    let file = rfd::AsyncFileDialog::new()
                        .set_title("Open a ReShiki, MOL, RXN, CDXML, or SMILES document")
                        .pick_file()
                        .await?;
                    let path = file.path().to_path_buf();
                    let contents = std::fs::read_to_string(&path).map_err(|e| e.to_string());
                    Some((path, contents))
                },
                Message::Opened,
            ),
            Pending::Close(id) => {
                self.clear_recovery();
                iced::window::close(id)
            }
        }
    }
    fn display_document(&self) -> &Document {
        self.cleanup
            .as_ref()
            .filter(|p| !p.original && p.revision == self.revision && p.epoch == self.file_epoch)
            .map(|p| &p.document)
            .unwrap_or(&self.doc)
    }
    pub fn update(&mut self, message: Message) -> Task<Message> {
        let previous = self.inspector_tab;
        let task = self.update_inner(message);
        // Include inspector changes made by tool-specific handlers, which can
        // return early. Ordinary updates within a panel retain its scroll state.
        if previous != self.inspector_tab && self.inspector_tab != InspectorTab::Assistant {
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
        if self.updates.restarting && !matches!(message, Message::Updates(_)) {
            return Task::none();
        }
        if let Message::AtomText(action) = message {
            return self.atom_text_action(action);
        }
        if self.atom_text.is_some() {
            if matches!(message, Message::Escape) {
                return self.atom_text_action(atom_text::Action::Cancel);
            }
            if !atom_text::background(&message) {
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
        if !matches!(
            message,
            Message::Canvas(Edit::Hover(_))
                | Message::Tick
                | Message::RefreshLabels
                | Message::InspectorScroll(_)
                | Message::EngineDone { .. }
                | Message::InspectorAction(_)
                | Message::Viewport(_)
                | Message::Updates(_)
        ) {
            self.context_menu = None;
        }
        if let Message::InspectorAction(action) = message {
            return self.inspector_action(action);
        }
        if let Message::Updates(action) = message {
            return self.update_action(action);
        }
        if self.updates.open && matches!(message, Message::Escape) {
            self.updates.open = false;
            return Task::none();
        }
        if let Message::Join(action) = message {
            return self.join_action(action);
        }
        if self.joining.is_some()
            && matches!(message, Message::Escape | Message::TemplateNavigate(false))
        {
            return self.join_action(joining::Action::Cancel);
        }
        if self.joining.is_some() && joining::cancels_draft(&message) {
            self.cancel_join();
        }
        if let Message::InlineText(action) = message {
            return self.inline_action(action);
        }
        if matches!(message, Message::Escape)
            && self.inspector_tab == InspectorTab::DrawingStyle
            && self.styles.editor.is_some()
        {
            return self.drawing_style_action(document_styles::Action::Cancel);
        }
        if matches!(message, Message::Escape) {
            return if self.inline_text.is_some() {
                self.inline_action(inline_text::Action::Finish(false))
            } else {
                self.update(Message::Tool(Tool::Select))
            };
        }
        if self.inline_text.is_some() && matches!(message, Message::Undo | Message::Redo) {
            return self.inline_action(inline_text::Action::Undo(matches!(message, Message::Redo)));
        }
        if let Message::Canvas(Edit::BeginText(id)) = message {
            return self.inline_action(inline_text::Action::Begin(Some(id), Point::default()));
        }
        if let Message::Canvas(Edit::Click(p)) = message
            && self.tool == Tool::Text
        {
            if let Some(id) = canvas::hit_object(&self.doc, p, 8. / self.camera.zoom)
                .filter(|id| self.doc.atom(*id).is_some())
            {
                return self.atom_text_action(atom_text::Action::Begin(Some(id)));
            }
            let id = canvas::hit_object(&self.doc, p, 8. / self.camera.zoom)
                .filter(|id| self.doc.annotations.iter().any(|a| a.id == *id));
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
        if self.cleanup.is_some() {
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
                    | Message::ToggleView
                    | Message::Grid
                    | Message::Rulers(_)
                    | Message::Crosshair(_)
                    | Message::RulerUnit(_)
                    | Message::Tick
                    | Message::EngineDone { .. }
                    | Message::Close(_)
                    | Message::Discard
                    | Message::Cancel
                    | Message::Saved(..)
                    | Message::Exported(_)
                    | Message::Printing(
                        printing::Action::Prepared(..) | printing::Action::Finished(..)
                    )
                    | Message::Pictures(pictures::Action::Loaded(..))
                    | Message::Opened(_)
                    | Message::ClipboardRead { .. }
                    | Message::ClipboardWritten { .. }
            ) {
                if !matches!(message, Message::RefreshLabels | Message::Canvas(_)) {
                    self.status = "Apply or cancel the cleanup preview to continue editing".into();
                }
                return Task::none();
            }
        }
        if matches!(
            &message,
            Message::Charge(_) | Message::AtomRadical(_) | Message::ApplyIsotope
        ) && self
            .doc
            .abbreviations
            .iter()
            .any(|g| g.members.iter().any(|id| self.selected.contains(id)))
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
            && matches!(&message, Message::Canvas(Edit::Select(ids)) if ids.iter().any(|id| self.doc.annotations.iter().any(|a| a.id == *id) || self.doc.graphics.iter().any(|g|g.id==*id))));
        match message {
            Message::DrawingStyle(action) => return self.drawing_style_action(action),
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
            Message::Shortcut(action) => return self.shortcut_action(action),
            Message::AromaticDisplay => {
                if self.selected.is_empty() {
                    self.status = "Select an aromatic ring first".into();
                    return Task::none();
                }
                let mut request = Request::molecule("aromatic", self.doc.clone());
                request.selected_ids = Some(self.selected.clone());
                return self.run(request, Job::AromaticDisplay);
            }
            Message::Abbreviations(action) => return self.abbreviation_action(action),
            Message::Labels(action) => self.label_action(action),
            Message::RefreshLabels => {
                if !self.busy
                    && self
                        .refresh_due
                        .is_some_and(|due| std::time::Instant::now() >= due)
                {
                    self.refresh_due = None;
                    if !self.doc.atoms.is_empty() && !reshiki::attachments::present(&self.doc) {
                        return self.run(
                            Request::molecule("analyze", self.doc.clone()),
                            Job::RefreshLabels,
                        );
                    }
                }
            }
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
                self.bond_drawing = Default::default();
                self.bond_drawing.length = self.doc.drawing_style.bond_length_world;
                self.drawing_length_input = self.doc.drawing_style.bond_length_pt.to_string();
                self.chain_drawing.angle = 120.;
                self.chain_angle_input = "120".into();
                self.status = format!(
                    "{} bond defaults · {} pt length · 120° chain angle",
                    self.doc.drawing_style.name, self.doc.drawing_style.bond_length_pt
                );
                self.error = false;
            }
            Message::FixedLength(on) => self.bond_drawing.fixed_length = on,
            Message::FixedAngles(on) => self.bond_drawing.fixed_angles = on,
            Message::DrawingLength(value) => {
                self.drawing_length_input = value;
                if let Ok(points) = self.drawing_length_input.parse::<f32>()
                    && points.is_finite()
                    && (1.0..=300.0).contains(&points)
                {
                    self.bond_drawing.length = reshiki::style::DEFAULT.world(points);
                    self.error = false;
                } else {
                    self.status = "Bond length must be between 1 and 300 pt".into();
                    self.error = true;
                }
            }
            Message::ChainAtoms(value) => {
                self.chain_atoms_input = value;
                if self.chain_atoms_input.is_empty() {
                    self.chain_drawing.atoms = None;
                    self.error = false;
                } else if let Ok(count) = self.chain_atoms_input.parse::<usize>()
                    && (1..=reshiki::chains::MAX_ATOMS).contains(&count)
                {
                    self.chain_drawing.atoms = Some(count);
                    self.error = false;
                } else {
                    self.status =
                        "Enter 1–512 chain atoms, or clear the field for automatic length".into();
                    self.error = true;
                }
            }
            Message::ChainAngle(value) => {
                self.chain_angle_input = value;
                if let Ok(angle) = self.chain_angle_input.parse::<f32>()
                    && angle.is_finite()
                    && (1.0..=179.0).contains(&angle)
                {
                    self.chain_drawing.angle = angle;
                    self.error = false;
                } else {
                    self.status = "Chain angle must be between 1° and 179°".into();
                    self.error = true;
                }
            }
            Message::ApplyBondPreset(preset) => {
                if preset == reshiki::bonds::BondPreset::Dotted
                    && self.doc.bonds.iter().any(|b| {
                        self.selected.contains(&b.a)
                            && self.selected.contains(&b.b)
                            && !reshiki::bonds::hydrogen_endpoints(&self.doc, b.a, b.b)
                    })
                {
                    self.status = "Hydrogen bonds need a bonded explicit H and an acceptor".into();
                    self.error = true;
                    return Task::none();
                }
                let before = self.doc.clone();
                let affected: Vec<_> = self
                    .doc
                    .bonds
                    .iter()
                    .filter(|bond| {
                        self.selected.contains(&bond.a)
                            && self.selected.contains(&bond.b)
                            && !preset.preserves_chemistry(bond)
                    })
                    .flat_map(|bond| [bond.a, bond.b])
                    .collect();
                self.doc.invalidate_chemistry(&affected);
                for bond in &mut self.doc.bonds {
                    if self.selected.contains(&bond.a) && self.selected.contains(&bond.b) {
                        preset.apply(bond);
                    }
                }
                self.changed(before);
            }
            Message::BondPosition(position) => {
                let before = self.doc.clone();
                for bond in &mut self.doc.bonds {
                    if [2, 7].contains(&bond.order)
                        && self.selected.contains(&bond.a)
                        && self.selected.contains(&bond.b)
                    {
                        bond.double_position = position;
                    }
                }
                self.changed(before);
            }
            Message::BondColor(value) => self.bond_color_input = value,
            Message::ApplyBondColor => {
                if let Some(color) = graphics::parse_color(&self.bond_color_input) {
                    let before = self.doc.clone();
                    for bond in &mut self.doc.bonds {
                        if self.selected.contains(&bond.a) && self.selected.contains(&bond.b) {
                            bond.color = color;
                        }
                    }
                    self.changed(before);
                } else {
                    self.error = true;
                    self.status = "Enter a six-digit bond color, such as #205091".into();
                }
            }
            Message::AddFrame(kind) => {
                let mut ids = self.doc.complete_selection(&self.selected);
                if let Some((lo, hi)) = reshiki::scene::selection_bounds(&self.doc, &ids) {
                    let before = self.doc.clone();
                    let id = self.doc.next_id();
                    let padding = reshiki::style::DEFAULT.world(6.0);
                    self.doc.graphics.push(Graphic::dragged(
                        id,
                        kind,
                        lo.offset(-padding, -padding),
                        hi.offset(padding, padding),
                        GraphicStyle {
                            width_pt: self.doc.drawing_style.line_width_pt,
                            ..Default::default()
                        },
                        BracketSides::Both,
                        false,
                    ));
                    ids.push(id);
                    if let Ok(ids) = self.doc.group_selection(&ids) {
                        self.selected = ids;
                    }
                    self.changed(before);
                    self.tool = Tool::Select;
                    self.status = "Frame added and grouped with the selection".into();
                }
            }
            Message::Group => {
                let before = self.doc.clone();
                match self.doc.group_selection(&self.selected) {
                    Ok(ids) => {
                        self.selected = ids;
                        self.changed(before);
                        self.tool = Tool::Select;
                        self.status="Grouped · Option/Alt-click selects a member · Shift+Cmd/Ctrl+G ungroups".into();
                    }
                    Err(e) => {
                        self.status = e;
                        self.error = true;
                    }
                }
            }
            Message::Ungroup => {
                let before = self.doc.clone();
                if self.doc.ungroup_selection(&self.selected) {
                    self.changed(before);
                    self.status = "Ungrouped one level".into();
                }
            }
            Message::IntegralGroup(integral) => {
                let before = self.doc.clone();
                let ids = self.doc.outer_selected_groups(&self.selected);
                for g in &mut self.doc.groups {
                    if ids.contains(&g.id) {
                        g.integral = integral;
                    }
                }
                self.changed(before);
            }
            Message::InvertSelection => {
                let selected = self.doc.expand_groups(&self.selected);
                self.selected = self
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
            Message::GraphicStyle(change) => self.apply_graphic_style(change),
            Message::GraphicWidth(s) => self.graphic_width_input = s,
            Message::ApplyGraphicWidth => match self.graphic_width_input.parse::<f32>() {
                Ok(w) if w.is_finite() && (0.1..=12.0).contains(&w) => {
                    self.apply_graphic_style(GraphicChange::Width(w))
                }
                _ => {
                    self.error = true;
                    self.status = "Line width must be 0.1–12 pt".into();
                }
            },
            Message::GraphicStroke(s) => self.graphic_stroke_input = s,
            Message::ApplyGraphicStroke => {
                if let Some(c) = graphics::parse_color(&self.graphic_stroke_input) {
                    self.apply_graphic_style(GraphicChange::Stroke(c));
                } else {
                    self.error = true;
                    self.status = "Enter a six-digit hex color, such as #117E6C".into();
                }
            }
            Message::GraphicFill(s) => self.graphic_fill_input = s,
            Message::ApplyGraphicFill => {
                if let Some(c) = graphics::parse_color(&self.graphic_fill_input) {
                    self.apply_graphic_style(GraphicChange::Fill(Some(c)));
                } else {
                    self.error = true;
                    self.status = "Enter a six-digit hex color, such as #DCEFE9".into();
                }
            }
            Message::ScientificKind(kind) => {
                self.toolbar.remember(Tool::Graphic(kind));
                let before = self.doc.clone();
                for g in self
                    .doc
                    .graphics
                    .iter_mut()
                    .filter(|g| self.selected.contains(&g.id))
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
                self.orbital_phase = phase;
                let before = self.doc.clone();
                for g in self
                    .doc
                    .graphics
                    .iter_mut()
                    .filter(|g| self.selected.contains(&g.id))
                {
                    g.phase = phase;
                }
                self.changed(before);
            }
            Message::FlipPhase(value) => {
                self.phase_flipped = value;
                let before = self.doc.clone();
                for g in self
                    .doc
                    .graphics
                    .iter_mut()
                    .filter(|g| self.selected.contains(&g.id))
                {
                    g.phase_flipped = value;
                }
                self.changed(before);
            }
            Message::AttachSymbols(value) => self.attach_symbols = value,
            Message::RotateMark(id, index) => {
                let before = self.doc.clone();
                if let Some(a) = self.doc.atom_mut(id)
                    && let Some(m) = a.marks.get_mut(index)
                {
                    m.angle = (m.angle + 45.).rem_euclid(360.);
                }
                self.changed(before);
            }
            Message::RemoveMark(id, index) => {
                let before = self.doc.clone();
                if let Some(a) = self.doc.atom_mut(id)
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
                        self.doc.invalidate_chemistry(&[id]);
                    }
                }
                self.changed(before);
            }
            Message::AtomRadical(value) => {
                let before = self.doc.clone();
                self.doc.invalidate_chemistry(&self.selected);
                for atom in self
                    .doc
                    .atoms
                    .iter_mut()
                    .filter(|a| self.selected.contains(&a.id))
                {
                    atom.radical_electrons = value;
                    atom.explicit_h = 0;
                    atom.no_implicit = false;
                }
                self.changed(before);
            }
            Message::GraphicSides(sides) => {
                let before = self.doc.clone();
                self.bracket_sides = sides;
                for g in &mut self.doc.graphics {
                    if self.selected.contains(&g.id) {
                        g.sides = sides;
                    }
                }
                self.changed(before);
            }
            Message::GraphicLayer(front) => {
                let before = self.doc.clone();
                let edge = if front {
                    self.doc
                        .graphics
                        .iter()
                        .map(|g| g.layer)
                        .max()
                        .unwrap_or(0)
                        .max(0)
                        .saturating_add(1)
                } else {
                    self.doc
                        .graphics
                        .iter()
                        .map(|g| g.layer)
                        .min()
                        .unwrap_or(0)
                        .min(0)
                        .saturating_sub(1)
                };
                for g in &mut self.doc.graphics {
                    if self.selected.contains(&g.id) {
                        g.layer = edge;
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
                if tab != InspectorTab::DrawingStyle {
                    self.styles.editor = None;
                }
                if tab == InspectorTab::Labels {
                    let atoms: Vec<_> = self
                        .doc
                        .atoms
                        .iter()
                        .filter(|a| self.selected.contains(&a.id))
                        .collect();
                    self.labels.scope = if atoms.is_empty() {
                        atom_labels::Scope::Drawing
                    } else {
                        atom_labels::Scope::Selection
                    };
                    self.labels.number = if let [atom] = atoms.as_slice() {
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
            }
            Message::ToggleImport => {
                self.import_open = !self.import_open;
                if self.import_open {
                    self.help_open = false;
                }
            }
            Message::InsertInput => return self.run(input_request(&self.smiles), Job::Insert),
            Message::ToggleHelp => {
                self.help_open = !self.help_open;
                if self.help_open {
                    self.palette = None;
                }
            }
            Message::Viewport(size) => {
                self.viewport = size;
                if let Some(index) = self.pages.fit {
                    self.fit_pages(index);
                } else if self.fit_to_view {
                    self.fit();
                }
            }
            Message::InspectorAction(_) | Message::ContextMenu(_) => {}
            Message::Tool(tool) => {
                self.erase_stroke = false;
                self.palette = None;
                self.toolbar.remember(tool);
                if let Some(option) = self.toolbar.graphic(tool) {
                    self.graphic_style = option.style.clone();
                    self.bracket_sides = option.sides;
                    self.graphic_width_input = self.graphic_style.width_pt.to_string();
                }
                self.tool = tool;
                self.error = false;
                if matches!(tool, Tool::Graphic(_) | Tool::RingPreset(_)) {
                    self.selected.clear();
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
            Message::FontSize(value) => self.font_size_input = value,
            Message::ApplyFontSize => match self.font_size_input.parse::<f32>() {
                Ok(size) if size.is_finite() && (4.0..=144.0).contains(&size) => {
                    self.apply_text_style(reshiki::typography::StyleChange::Size(size))
                }
                _ => {
                    self.error = true;
                    self.status = "Enter a font size from 4 to 144 pt".into();
                }
            },
            Message::ColorScope(scope) => {
                self.color_scope = scope;
                self.sync_color_input();
            }
            Message::TextColor(value) => self.text_color_input = value,
            Message::ApplyTextColor => {
                let hex = self.text_color_input.trim().trim_start_matches('#');
                if hex.len() == 6
                    && let Ok(value) = u32::from_str_radix(hex, 16)
                {
                    self.apply_text_style(reshiki::typography::StyleChange::Color([
                        (value >> 16) as u8,
                        (value >> 8) as u8,
                        value as u8,
                    ]));
                } else {
                    self.error = true;
                    self.status = "Enter a color such as #174A7E".into();
                }
            }
            Message::TextAlign(alignment) => self.apply_paragraph(Some(alignment), None, None),
            Message::GroupLabelAlign(alignment) => self.apply_group_alignment(alignment),
            Message::TextSpacing(spacing) => self.apply_paragraph(None, Some(spacing), None),
            Message::TextWidth(value) => self.text_width_input = value,
            Message::ApplyTextWidth => {
                let width = self.text_width_input.trim();
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
            Message::Smiles(s) => self.smiles = s,
            Message::Isotope(s) => self.isotope = s,
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
                    && reshiki::rings::selected_cycle(&self.doc, &self.selected).is_some()
                {
                    return self.update(Message::ToggleSelectedRing);
                }
                return self.update(Message::AromaticRing(!self.aromatic_ring));
            }
            Message::ToggleSelectedRing => {
                let before = self.doc.clone();
                match reshiki::rings::toggle_selected_aromatic(&mut self.doc, &self.selected) {
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
                self.arrow_style = style;
                self.arrows.style = reshiki::arrows::ArrowStyle::preset(style);
                self.arrows.style.width_pt = self.doc.drawing_style.line_width_pt;
                self.tool = Tool::Arrow;
                self.inspector_open = true;
                self.inspector_tab = InspectorTab::Properties;
                let before = self.doc.clone();
                for a in &mut self.doc.arrows {
                    if self.selected.contains(&a.id) {
                        a.kind = style.kind().into();
                        a.control = None;
                        a.style = Some(self.arrows.style.clone());
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
            Message::Copy(cut) => {
                if reshiki::clipboard::available() {
                    return self.copy_native(cut, false);
                }
                if self.selected.is_empty() {
                    self.status = "Select objects to copy".into();
                    return Task::none();
                }
                let selection = editing::selection(&self.doc, &self.selected);
                if let Ok(json) = serde_json::to_string(&selection) {
                    if cut {
                        let before = self.doc.clone();
                        self.doc.delete(&self.selected);
                        self.selected.clear();
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
            Message::Pasted(contents) => {
                if let Some(contents) = contents.filter(|s| !s.trim().is_empty()) {
                    if let Some(json) = editing::clipboard_json(&contents) {
                        match serde_json::from_str::<Document>(json)
                            .map_err(|e| e.to_string())
                            .and_then(|d| {
                                d.validate()?;
                                Ok(d)
                            }) {
                            Ok(part) => {
                                let center = editing::center(&part, &part.all_ids());
                                let before = self.doc.clone();
                                self.selected = editing::append(
                                    &mut self.doc,
                                    &part,
                                    Point::new(
                                        self.camera.center.x - center.x + 24.0,
                                        self.camera.center.y - center.y + 24.0,
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
            Message::Duplicate => {
                let part = editing::selection(&self.doc, &self.selected);
                let before = self.doc.clone();
                self.selected = editing::append(&mut self.doc, &part, Point::new(28.0, 28.0));
                self.changed(before);
                self.tool = Tool::Select;
            }
            Message::Transform(transform) => {
                let before = self.doc.clone();
                editing::transform(&mut self.doc, &self.selected, transform);
                self.changed(before);
            }
            Message::Arrange(arrange) => {
                let before = self.doc.clone();
                editing::arrange(&mut self.doc, &self.selected, arrange);
                self.changed(before);
            }
            Message::BondDepth(front) => {
                let before = self.doc.clone();
                let z = if front {
                    self.doc
                        .bonds
                        .iter()
                        .map(|b| b.z_order)
                        .max()
                        .unwrap_or(0)
                        .saturating_add(1)
                } else {
                    self.doc
                        .bonds
                        .iter()
                        .map(|b| b.z_order)
                        .min()
                        .unwrap_or(0)
                        .saturating_sub(1)
                };
                for bond in &mut self.doc.bonds {
                    if self.selected.contains(&bond.a) && self.selected.contains(&bond.b) {
                        bond.z_order = z;
                    }
                }
                self.changed(before);
            }
            Message::ReverseBonds => {
                let before = self.doc.clone();
                self.doc.invalidate_chemistry(&self.selected);
                for b in &mut self.doc.bonds {
                    if self.selected.contains(&b.a) && self.selected.contains(&b.b) {
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
            Message::Tick => {
                if self.dirty() && self.autosaved_revision != Some(self.revision) {
                    if let Some(recovery) = &self.recovery {
                        match recovery.save(&self.recovery_document(), self.path.clone()) {
                            Ok(()) => {
                                self.autosaved_revision = Some(self.revision);
                                self.autosave_status = "Recovery draft saved".into();
                            }
                            Err(e) => self.autosave_status = format!("Recovery save failed: {e}"),
                        }
                    }
                } else if !self.dirty() {
                    self.clear_recovery();
                }
            }
            Message::Restore => {
                if self.dirty() {
                    self.status =
                        "Save the current drawing before restoring a previous session".into();
                    return Task::none();
                }
                if let Some(candidate) = self.recovered.first().cloned() {
                    let before = self.doc.clone();
                    self.doc = candidate.snapshot.document;
                    self.doc.version = 15;
                    self.sync_drawing_defaults();
                    self.styles.editor = None;
                    self.path = None;
                    self.saved = Document::default();
                    self.file_epoch = self.file_epoch.wrapping_add(1);
                    self.changed(before);
                    self.revision = self.revision.wrapping_add(1);
                    self.fit();
                    self.selected.clear();
                    if let Some(store) = &self.recovery
                        && store.save(&self.doc, None).is_ok()
                    {
                        let _ = reshiki::recovery::remove(&candidate.path);
                        self.recovered.remove(0);
                    }
                    self.status = "Recovered drawing · Save to keep a new copy".into();
                }
            }
            Message::DismissRecovery => {
                self.recovered.clear();
            }
            Message::Canvas(edit) => self.edit(edit),
            Message::Grid => self.grid = !self.grid,
            Message::ToggleView => self.view_open = !self.view_open,
            Message::Rulers(enabled) => {
                self.guides.rulers = enabled;
                if self.fit_to_view {
                    self.fit();
                }
            }
            Message::Crosshair(enabled) => self.guides.crosshair = enabled,
            Message::RulerUnit(unit) => self.guides.unit = unit,
            Message::Fit => self.fit(),
            Message::Zoom(f) => {
                self.pages.fit = None;
                self.fit_to_view = false;
                self.camera.zoom = (self.camera.zoom * f).clamp(0.005, 5.0);
            }
            Message::Import => return self.run(input_request(&self.smiles), Job::Import),
            Message::Example(smiles) => {
                self.smiles = smiles.into();
                return self.run(Request::import_smiles(smiles), Job::Insert);
            }
            Message::Analyze => {
                return self.run(Request::molecule("analyze", self.doc.clone()), Job::Analyze);
            }
            Message::CleanupScope(scope) => return self.begin_cleanup(Some(scope), None),
            Message::CleanupOrientation(on) => return self.begin_cleanup(None, Some(on)),
            Message::CleanupOriginal(original) => {
                if let Some(preview) = &mut self.cleanup {
                    preview.original = original;
                }
            }
            Message::CancelCleanup => {
                self.cleanup_serial = self.cleanup_serial.wrapping_add(1);
                self.cleanup = None;
                self.status = "Cleanup cancelled · Drawing unchanged".into();
                self.error = false;
            }
            Message::ApplyCleanup => {
                if self.busy {
                    return Task::none();
                }
                if let Some(preview) = self.cleanup.take() {
                    if preview.revision != self.revision || preview.epoch != self.file_epoch {
                        self.status = "Drawing changed · Run cleanup again".into();
                        return Task::none();
                    }
                    let before = self.doc.clone();
                    self.doc = preview.document;
                    self.changed(before);
                    if !self.error {
                        self.analysis = preview.analysis;
                        self.status = "Cleanup applied · Undo restores the original layout".into();
                    }
                }
            }
            Message::Clean => return self.begin_cleanup(None, None),
            Message::EngineDone {
                revision,
                kind,
                result,
            } => {
                self.busy = false;
                if matches!(&kind, Job::Clean(job) if job.serial != self.cleanup_serial || job.epoch != self.file_epoch)
                {
                    return Task::none();
                }
                match *result {
                    Err(e) => {
                        if matches!(kind, Job::RefreshLabels) {
                            if self.revision == revision {
                                self.chemistry_notice = Some(e);
                            } else {
                                self.refresh_due = Some(std::time::Instant::now());
                            }
                            return Task::none();
                        }
                        self.error = true;
                        self.status = e;
                    }
                    Ok(response) => {
                        if let Job::Export(format) = kind {
                            return export_file(response.output.unwrap_or_default(), format);
                        }
                        if self.revision != revision {
                            if matches!(kind, Job::RefreshLabels) {
                                self.refresh_due = Some(std::time::Instant::now());
                                return Task::none();
                            }
                            self.status="Operation finished; newer edits were preserved. Run it again to update.".into();
                            return Task::none();
                        }
                        if let Job::Clean(job) = &kind {
                            if let Some(document) = response.document {
                                if let Err(error) = document.validate() {
                                    self.status = error;
                                    self.error = true;
                                } else {
                                    self.cleanup = Some(CleanupPreview {
                                        job: job.clone(),
                                        warnings: response.warnings,
                                        document,
                                        analysis: response.analysis,
                                        revision,
                                        epoch: self.file_epoch,
                                        original: false,
                                    });
                                    self.status = "Cleanup preview · Compare with the original, then Apply or Cancel".into();
                                    self.error = false;
                                }
                            }
                            return Task::none();
                        }
                        if matches!(kind, Job::Insert) {
                            if let Some(document) = response.document {
                                let before = self.doc.clone();
                                let center = editing::center(&document, &document.all_ids());
                                let offset = if self.doc.all_ids().is_empty() {
                                    Point::new(
                                        self.camera.center.x - center.x,
                                        self.camera.center.y - center.y,
                                    )
                                } else {
                                    let (_, existing_max) = self.doc.bounds();
                                    let (insert_min, _) = document.bounds();
                                    Point::new(
                                        existing_max.x + self.doc.drawing_style.bond_length_world
                                            - insert_min.x,
                                        self.camera.center.y - center.y,
                                    )
                                };
                                self.selected = editing::append(&mut self.doc, &document, offset);
                                self.changed(before);
                                self.fit();
                                self.tool = Tool::Select;
                                self.status =
                                    "Inserted structure · Drag to position · Delete or Undo to remove".into();
                            }
                            return Task::none();
                        }
                        if matches!(kind, Job::AromaticDisplay) {
                            if let Some(document) = response.document {
                                let before = self.doc.clone();
                                self.doc = document.clone();
                                self.changed(before);
                                if self.error {
                                    return Task::none();
                                }
                                reshiki::atom_labels::refresh_computed(&mut self.doc, &document);
                                self.refresh_due = None;
                                self.analysis = response.analysis;
                                self.tool = Tool::Select;
                                self.status =
                                    "Aromatic display changed · Molecular identity retained".into();
                            }
                            return Task::none();
                        }
                        if matches!(kind, Job::Abbreviate) {
                            if let Some(document) = response.document {
                                let before = self.doc.clone();
                                let count = document.abbreviations.len();
                                self.doc = document;
                                self.selected =
                                    self.doc.expand_abbreviation_selection(&self.selected);
                                self.changed(before);
                                self.analysis = response.analysis;
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
                        if matches!(kind, Job::Analyze | Job::RefreshLabels) {
                            // Checking is a read-only chemistry operation. Refresh
                            // computed H labels without rewriting the user's bond
                            // orders/stereo or inserting a step into Undo/Redo.
                            if let Some(document) = response.document {
                                reshiki::atom_labels::refresh_computed(&mut self.doc, &document);
                            }
                            self.analysis = response.analysis;
                            self.chemistry_notice = None;
                            if matches!(kind, Job::Analyze) {
                                self.status = "No chemistry errors found".into();
                                self.error = false;
                            }
                            return Task::none();
                        }
                        if let Some(document) = response.document {
                            let before = self.doc.clone();
                            self.doc = document.clone();
                            self.changed(before);
                            reshiki::atom_labels::refresh_computed(&mut self.doc, &document);
                            self.refresh_due = None;
                            self.chemistry_notice = None;
                            self.selected.clear();
                            if matches!(kind, Job::Import | Job::ImportFile) {
                                self.fit();
                            }
                            if matches!(kind, Job::ImportFile) {
                                self.path = None;
                                self.saved = Document::default();
                                self.file_epoch = self.file_epoch.wrapping_add(1);
                            }
                        }
                        self.analysis = response.analysis;
                        self.status = match kind {
                            Job::Clean(_) => "Structure cleaned",
                            Job::Analyze | Job::RefreshLabels => "No chemistry errors found",
                            _ => "Structure imported · Undo restores the previous drawing",
                        }
                        .into();
                        self.error = false;
                    }
                }
            }
            Message::Undo | Message::Redo => {
                self.erase_stroke = false;
                self.cleanup = None;
                let before = self.doc.clone();
                let selected_group = !before.outer_selected_groups(&self.selected).is_empty();
                let changed = if matches!(message, Message::Undo) {
                    self.history.undo(&mut self.doc)
                } else {
                    self.history.redo(&mut self.doc)
                };
                if changed {
                    self.revision = self.revision.wrapping_add(1);
                    if chemistry_changed(&before, &self.doc) {
                        self.analysis = None;
                        reshiki::atom_labels::clear_computed(&mut self.doc);
                        self.refresh_due = Some(std::time::Instant::now());
                    }
                    let ids = self.doc.all_ids();
                    self.selected.retain(|id| ids.contains(id));
                    let previous_ids = before.all_ids();
                    let restored_group = self.doc.groups.iter().any(|group| {
                        group.members.iter().any(|id| self.selected.contains(id))
                            && group
                                .members
                                .iter()
                                .all(|id| self.selected.contains(id) || !previous_ids.contains(id))
                    });
                    if selected_group || restored_group {
                        self.selected = self.doc.expand_groups(&self.selected);
                    }
                    if self.selected.is_empty()
                        && let Some(id) = self.caption_target.filter(|id| ids.contains(id))
                    {
                        self.selected.push(id);
                    }
                    self.sync_typography();
                    self.sync_graphics();
                    self.sync_arrows();
                    self.sync_bonds();
                    if before.drawing_style != self.doc.drawing_style {
                        self.sync_drawing_defaults();
                        self.styles.editor = None;
                    }
                    if before.page_layout != self.doc.page_layout {
                        self.pages.editor = self
                            .pages
                            .editor
                            .as_ref()
                            .map(|_| pages::Editor::new(&self.doc, self.file_epoch));
                        if let Some(layout) = &self.doc.page_layout {
                            self.pages.active =
                                self.pages.active.min(layout.count().saturating_sub(1));
                            self.fit_pages(Some(self.pages.active));
                        } else {
                            self.fit();
                        }
                    }
                    self.status = "History restored".into();
                    self.error = false;
                }
            }
            Message::Delete => {
                let before = self.doc.clone();
                self.doc.delete(&self.selected);
                self.changed(before);
            }
            Message::SelectAll => {
                self.selected = self.doc.all_ids();
                self.tool = Tool::Select;
                self.sync_typography();
                self.sync_graphics();
                self.sync_arrows();
                self.sync_bonds();
            }
            Message::Charge(delta) => {
                let before = self.doc.clone();
                self.doc.invalidate_chemistry(&self.selected);
                for id in &self.selected {
                    if let Some(a) = self.doc.atom_mut(*id) {
                        a.charge = a.charge.saturating_add(delta).clamp(-8, 8);
                        a.explicit_h = 0;
                        a.no_implicit = false;
                    }
                }
                self.changed(before);
            }
            Message::ApplyIsotope => match self.isotope.parse::<u32>() {
                Ok(value) if value <= 300 => {
                    let before = self.doc.clone();
                    for id in &self.selected {
                        if let Some(a) = self.doc.atom_mut(*id) {
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
                if let Some(a) = self.property_analysis() {
                    return iced::clipboard::write(a.smiles.clone());
                }
            }
            Message::New => return self.pending(Pending::New),
            Message::Open => return self.pending(Pending::Open),
            Message::Close(id) => return self.pending(Pending::Close(id)),
            Message::Cancel => self.pending = None,
            Message::Discard => {
                if let Some(action) = self.pending.take() {
                    return self.perform(action);
                }
            }
            Message::Opened(file) => {
                if let Some((path, result)) = file {
                    match result {
                        Err(e) => {
                            self.error = true;
                            self.status = e;
                        }
                        Ok(contents) => {
                            let extension = path
                                .extension()
                                .and_then(|e| e.to_str())
                                .unwrap_or_default()
                                .to_ascii_lowercase();
                            if reshiki::compatibility::is_native_extension(&extension) {
                                match serde_json::from_str::<Document>(&contents)
                                    .map_err(|e| e.to_string())
                                    .and_then(|doc| {
                                        doc.validate()?;
                                        Ok(doc)
                                    }) {
                                    Ok(mut doc) => {
                                        doc.version = 15;
                                        reshiki::atom_labels::clear_computed(&mut doc);
                                        self.clear_recovery();
                                        self.file_epoch = self.file_epoch.wrapping_add(1);
                                        self.doc = doc;
                                        self.sync_drawing_defaults();
                                        self.styles.editor = None;
                                        self.saved = self.doc.clone();
                                        self.path = Some(path);
                                        self.history = History::default();
                                        self.revision = self.revision.wrapping_add(1);
                                        self.analysis = None;
                                        self.refresh_due = Some(std::time::Instant::now());
                                        self.selected.clear();
                                        self.pages = pages::State::default();
                                        if self.doc.page_layout.is_some() {
                                            self.fit_pages(Some(0));
                                        } else {
                                            self.fit();
                                        }
                                        self.status = "Document opened".into();
                                        self.error = false;
                                    }
                                    Err(e) => {
                                        self.error = true;
                                        self.status = format!("Could not open document: {e}");
                                    }
                                }
                            } else {
                                let format = match extension.as_str() {
                                    "mol" => "mol",
                                    "rxn" => "rxn",
                                    "rsmi" => "rsmi",
                                    "cdxml" => "cdxml",
                                    "inchi" => "inchi",
                                    _ => "smiles",
                                };
                                return self
                                    .run(Request::import(format, &contents), Job::ImportFile);
                            }
                        }
                    }
                }
            }
            Message::Save | Message::SaveAs => {
                #[cfg(windows)]
                let office_save = self.office_document() && matches!(message, Message::Save);
                let path = if matches!(message, Message::SaveAs) {
                    None
                } else {
                    self.path.clone()
                };
                let bytes = match serde_json::to_vec_pretty(&self.doc) {
                    Ok(b) => b,
                    Err(e) => {
                        self.status = e.to_string();
                        self.error = true;
                        return Task::none();
                    }
                };
                let snapshot = self.doc.clone();
                let epoch = self.file_epoch;
                return Task::perform(
                    async move {
                        let path = if let Some(p) = path {
                            p
                        } else {
                            let extension = reshiki::compatibility::NATIVE_EXTENSION;
                            let Some(path) = files::save_path(
                                "Save drawing",
                                &format!("Untitled.{extension}"),
                                extension,
                            )
                            .await
                            else {
                                return Ok(None);
                            };
                            path
                        };
                        let save_path = path.clone();
                        tokio::task::spawn_blocking(move || {
                            #[cfg(windows)]
                            if office_save {
                                reshiki_windows::prepare_office_save(&save_path);
                            }
                            reshiki::storage::write_atomic(&save_path, &bytes)?;
                            #[cfg(windows)]
                            if office_save {
                                reshiki_windows::wait_for_office_save(&save_path, &bytes)?;
                            }
                            Ok::<_, String>(())
                        })
                        .await
                        .map_err(|error| error.to_string())??;
                        Ok(Some(path))
                    },
                    move |result| Message::Saved(epoch, Box::new(snapshot.clone()), result),
                );
            }
            Message::Saved(epoch, snapshot, result) => match result {
                Ok(Some(path)) => {
                    if epoch != self.file_epoch {
                        self.status = "Previous document saved".into();
                        return Task::none();
                    }
                    self.saved = *snapshot;
                    self.path = Some(path);
                    self.status = if self.office_document() {
                        "Drawing updated in Office. Save the Office document to keep it."
                    } else {
                        "Document saved"
                    }
                    .into();
                    self.error = false;
                    if !self.dirty() {
                        self.clear_recovery();
                    }
                    if !self.dirty()
                        && let Some(action) = self.pending.take()
                    {
                        return self.perform(action);
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    self.status = e;
                    self.error = true;
                }
            },
            Message::Export(format) => {
                if ["svg", "pdf", "png"].contains(&format) {
                    let doc = self.doc.clone();
                    let engine = self.engine.clone();
                    return Task::perform(
                        async move {
                            let doc = reshiki::export::checked_document(&engine, doc).await?;
                            let bytes = tokio::task::spawn_blocking(move || {
                                reshiki::export::drawing(&doc, format)
                            })
                            .await
                            .map_err(|e| e.to_string())??;
                            save_export(bytes, format).await
                        },
                        Message::Exported,
                    );
                }
                let mut request = Request::molecule("export", self.doc.clone());
                request.format = Some(format.into());
                return self.run(request, Job::Export(format));
            }
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
    fn edit(&mut self, edit: Edit) {
        if let Edit::ContextMenu { position, selected } = edit {
            if self.cleanup.is_none() {
                self.selected = selected;
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
                self.erase_stroke = self.tool == Tool::Erase && self.cleanup.is_none();
                self.erase_committed = false;
                if self.erase_stroke {
                    self.erase_segment(p, p);
                }
                return;
            }
            Edit::EraseTo(from, to) => {
                if self.erase_stroke && self.tool == Tool::Erase {
                    self.erase_segment(from, to);
                }
                return;
            }
            Edit::EraseEnd => {
                self.erase_stroke = false;
                self.hover = None;
                return;
            }
            Edit::Hover(_) | Edit::ContextMenu { .. } => {}
            _ => self.erase_stroke = false,
        }
        if matches!(edit, Edit::Pan(..) | Edit::Zoom(..)) {
            self.pages.fit = None;
        }
        if let Edit::Hover(point) = edit {
            self.hover = point.map(|p| (p, self.file_epoch));
            return;
        }
        if matches!(edit, Edit::Pan(..) | Edit::Zoom(..)) {
            self.hover = None;
        }
        if self.cleanup.is_some() {
            match edit {
                Edit::Pan(dx, dy) => {
                    self.camera.center = self
                        .camera
                        .center
                        .offset(-dx / self.camera.zoom, -dy / self.camera.zoom);
                    self.fit_to_view = false;
                }
                Edit::Zoom(factor, _) => {
                    self.camera.zoom = (self.camera.zoom * factor).clamp(0.005, 5.);
                    self.fit_to_view = false;
                }
                _ => {}
            }
            return;
        }
        let before = self.doc.clone();
        match edit {
            Edit::ContextMenu { .. } => return,
            Edit::Hover(_)
            | Edit::BeginText(_)
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
                    &self.doc,
                    &points,
                    source,
                    target,
                    10.0 / self.camera.zoom,
                ) {
                    Ok((doc, ids)) => {
                        self.doc = doc;
                        self.selected = ids;
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
                            style: self.graphic_style.clone(),
                            phase: self.orbital_phase,
                            flipped: self.phase_flipped,
                            attach: self.attach_symbols,
                        };
                        match drawing.place(
                            &mut self.doc,
                            start,
                            end,
                            constrain,
                            10. / self.camera.zoom,
                        ) {
                            Ok(id) => self.selected = vec![id],
                            Err(error) => {
                                self.status = error;
                                self.error = true;
                                return;
                            }
                        }
                    } else {
                        let id = self.doc.next_id();
                        self.doc.graphics.push(Graphic::dragged(
                            id,
                            kind,
                            start,
                            end,
                            self.graphic_style.clone(),
                            self.bracket_sides,
                            constrain,
                        ));
                        self.selected = vec![id];
                    }
                    self.tool = Tool::Select;
                }
            }
            Edit::AtomIndicator(owner, p) => {
                if let Some(anchor) = owner.anchor(&self.doc) {
                    owner.set_offset(
                        &mut self.doc,
                        Some(Point::new(p.x - anchor.x, p.y - anchor.y)),
                    );
                }
            }
            Edit::AtomMark(id, index, p) => {
                if let Some(a) = self.doc.atom_mut(id)
                    && let Some(mark) = a.marks.get_mut(index)
                {
                    mark.offset = Point::new(p.x - a.position.x, p.y - a.position.y);
                }
            }
            Edit::ArrowHandle(id, index, p) => {
                if let Some(a) = self.doc.arrows.iter_mut().find(|a| a.id == id) {
                    a.edit_handle(index, p);
                }
            }
            Edit::GraphicPoint(id, index, p) => {
                if let Some(g) = self.doc.graphics.iter_mut().find(|g| g.id == id) {
                    g.edit_point(index, p);
                }
            }
            Edit::Template(anchor, direction) => {
                if let Some(state) = &self.joining {
                    if state.revision != self.revision || state.epoch != self.file_epoch {
                        self.cancel_join();
                        self.status = "The drawing changed. Start Move & attach again.".into();
                        self.error = true;
                        return;
                    }
                    match state.prepared.place(
                        anchor,
                        direction,
                        10. / self.camera.zoom,
                        state.anchor,
                        state.mode,
                    ) {
                        Ok((document, selected)) => {
                            self.doc = document;
                            self.selected = selected;
                            self.joining = None;
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
                    &self.doc,
                    &template.document,
                    anchor,
                    direction,
                    10.0 / self.camera.zoom,
                    self.templates.anchor,
                    self.templates.connection,
                ) {
                    Ok((document, selected)) => {
                        self.doc = document;
                        self.selected = selected;
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
                editing::transform_about(&mut self.doc, &ids, pivot, scale, rotation);
                self.selected = ids;
            }
            Edit::Tilt { ids, x, y } => {
                crate::canvas::tilt::apply(&mut self.doc, &ids, x, y);
                self.selected = ids;
            }
            Edit::ScaleAxes { ids, pivot, x, y } => {
                editing::scale_axes_about(&mut self.doc, &ids, pivot, x, y);
                self.selected = ids;
            }
            Edit::RingPreset(preset, anchor, direction, connect, alternate) => {
                let drawing = reshiki::rings::Drawing {
                    preset,
                    length: self.bond_drawing.length,
                    alternate,
                    connect,
                };
                match drawing.place(&self.doc, anchor, direction, 10. / self.camera.zoom) {
                    Ok((doc, ids)) => {
                        self.doc = doc;
                        self.selected = ids;
                    }
                    Err(error) => {
                        self.status = error.into();
                        self.error = true;
                        return;
                    }
                }
            }
            Edit::Ring(anchor, direction) => {
                self.selected = editing::ring_oriented(
                    &mut self.doc,
                    anchor,
                    self.ring_size,
                    self.aromatic_ring,
                    10.0 / self.camera.zoom,
                    direction,
                );
            }
            Edit::Select(ids) => {
                self.selected = ids;
                self.sync_typography();
                self.sync_graphics();
                self.sync_arrows();
                self.sync_bonds();
            }
            Edit::Move(ids, dx, dy) => {
                if let Some(snapped) = editing::snap_ring(
                    &mut self.doc,
                    &ids,
                    Point::new(dx, dy),
                    14.0 / self.camera.zoom,
                ) {
                    self.selected = snapped;
                } else {
                    self.doc.translate(&ids, dx, dy);
                    self.selected = ids;
                }
            }
            Edit::Pan(dx, dy) => {
                self.fit_to_view = false;
                self.camera.center = self.camera.center.offset(-dx, -dy);
            }
            Edit::Zoom(f, at) => {
                self.fit_to_view = false;
                let old = self.camera.zoom;
                self.camera.zoom = (old * f).clamp(0.005, 5.0);
                let ratio = old / self.camera.zoom;
                self.camera.center = Point::new(
                    at.x + (self.camera.center.x - at.x) * ratio,
                    at.y + (self.camera.center.y - at.y) * ratio,
                );
            }
            Edit::Bond(start, end, a, b) => {
                if self.tool.bond_preset() == Some(reshiki::bonds::BondPreset::Dotted)
                    && !a
                        .zip(b)
                        .is_some_and(|(a, b)| reshiki::bonds::hydrogen_endpoints(&self.doc, a, b))
                {
                    self.status =
                        "Drag from a bonded explicit H to an existing N, O, F or S acceptor".into();
                    self.error = true;
                    return;
                }
                if self.tool == Tool::Arrow {
                    self.place_arrow(start, end);
                } else {
                    let a = a.unwrap_or_else(|| self.doc.add_atom("C", start));
                    let b = b.unwrap_or_else(|| self.doc.add_atom("C", end));
                    if let Some(preset) = self.tool.bond_preset() {
                        preset.place(&mut self.doc, a, b);
                    } else {
                        let (order, display) = self.bond_style();
                        self.doc.add_bond(a, b, order, display);
                    }
                    self.selected = vec![b];
                }
            }
            Edit::Click(p) => {
                let hit = canvas::hit_object(&self.doc, p, 10.0 / self.camera.zoom);
                match self.tool {
                    Tool::Atom => {
                        if let Some(id) = self.doc.nearest(p, 10.0 / self.camera.zoom) {
                            self.doc.invalidate_chemistry(&[id]);
                            if let Some(a) = self.doc.atom_mut(id) {
                                a.element = self.element.clone();
                                a.display.variable = None;
                                a.explicit_h = 0;
                                a.no_implicit = false;
                                a.charge = 0;
                                a.isotope = 0;
                            }
                            self.selected = vec![id];
                        } else {
                            let id = self.doc.add_atom(&self.element, p);
                            self.selected = vec![id];
                        }
                    }
                    tool if tool.bond_preset() == Some(reshiki::bonds::BondPreset::Dotted) => {
                        self.status =
                            "Drag from a bonded explicit H to an existing acceptor".into();
                        self.error = true;
                        return;
                    }
                    Tool::Bond(_) | Tool::StyledBond(_) | Tool::Wedge | Tool::Hash | Tool::Wavy => {
                        let atom = self.doc.nearest(p, 10.0 / self.camera.zoom);
                        let bond =
                            self.doc
                                .bonds
                                .iter()
                                .find(|b| {
                                    self.doc.atom(b.a).zip(self.doc.atom(b.b)).is_some_and(
                                        |(a, z)| {
                                            canvas::distance_to_segment(p, a.position, z.position)
                                                < 7.0 / self.camera.zoom
                                        },
                                    )
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
                                    reshiki::scene::effective_double_position(&self.doc, &b)
                                        .cycled();
                                if let Some(bond) = self
                                    .doc
                                    .bonds
                                    .iter_mut()
                                    .find(|bond| bond.a == b.a && bond.b == b.b)
                                {
                                    bond.double_position = position;
                                }
                                self.selected = vec![b.a, b.b];
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
                                preset.place(&mut self.doc, b.a, b.b);
                            } else {
                                self.doc.add_bond(b.a, b.b, order, display);
                                self.apply_current_bond_preset(b.a, b.b);
                            }
                            if reverse
                                && let Some(bond) = self
                                    .doc
                                    .bonds
                                    .iter_mut()
                                    .find(|bond| bond.a == b.a && bond.b == b.b)
                            {
                                bond.reverse();
                            }
                            self.selected = vec![b.a, b.b];
                        } else {
                            let (order, display) = self.bond_style();
                            let a = atom.unwrap_or_else(|| self.doc.add_atom("C", p));
                            let Some(start) = self.doc.atom(a).map(|a| a.position) else {
                                self.status = "The bond's starting atom is unavailable".into();
                                self.error = true;
                                return;
                            };
                            let end = editing::bond_extension(&self.doc, start, Some(a), order);
                            let ratio = self.bond_drawing.length
                                / reshiki::style::DEFAULT.bond_length_world;
                            let end =
                                start.offset((end.x - start.x) * ratio, (end.y - start.y) * ratio);
                            let b = self.doc.add_atom("C", end);
                            self.doc.add_bond(a, b, order, display);
                            self.apply_current_bond_preset(a, b);
                            self.selected = vec![b];
                        }
                    }
                    Tool::Ring => {
                        self.selected = editing::ring(
                            &mut self.doc,
                            p,
                            self.ring_size,
                            self.aromatic_ring,
                            10.0 / self.camera.zoom,
                        );
                    }
                    Tool::Text => {
                        if let Some(label) =
                            hit.filter(|id| self.doc.annotations.iter().any(|a| a.id == *id))
                        {
                            self.selected = vec![label];
                            self.sync_typography();
                            return;
                        }
                        if !self.caption.trim().is_empty() {
                            let id = self.doc.next_id();
                            self.doc.annotations.push(Annotation {
                                id,
                                position: p,
                                text: self.caption.clone(),
                                format: self.caption_format.clone(),
                            });
                            self.selected = vec![id];
                            self.caption_target = Some(id);
                            self.tool = Tool::Select;
                        }
                    }
                    Tool::Arrow => {
                        if let Some(id) =
                            hit.filter(|id| self.doc.arrows.iter().any(|a| a.id == *id))
                        {
                            self.apply_arrow_tool(id);
                        } else {
                            let length = self.doc.drawing_style.bond_length_world * 2.;
                            self.place_arrow(p, p.offset(length, 0.));
                        }
                    }
                    Tool::Erase => {
                        reshiki::erasing::stroke(&mut self.doc, p, p, 7. / self.camera.zoom)
                    }
                    _ => self.selected = hit.into_iter().collect(),
                }
            }
        }
        self.changed(before);
    }
    fn place_arrow(&mut self, start: Point, end: Point) {
        let id = self.doc.next_id();
        self.doc.arrows.push(Arrow::new(
            id,
            start,
            end,
            self.arrow_style,
            self.arrows.style.clone(),
        ));
        self.selected = vec![id];
    }
    fn erase_segment(&mut self, from: Point, to: Point) {
        let before = self.doc.clone();
        reshiki::erasing::stroke(&mut self.doc, from, to, 7. / self.camera.zoom);
        if self.doc != before {
            self.selected.clear();
            let revision = self.revision;
            self.changed_continuing(before, self.erase_committed);
            self.erase_committed |= self.revision != revision;
        }
    }
    fn sync_bonds(&mut self) {
        if let Some(b) = self
            .doc
            .bonds
            .iter()
            .find(|b| self.selected.contains(&b.a) && self.selected.contains(&b.b))
        {
            let [r, g, b] = b.color;
            self.bond_color_input = format!("#{r:02X}{g:02X}{b:02X}");
        }
    }
    fn apply_current_bond_preset(&mut self, a: u64, b: u64) {
        if let Tool::StyledBond(preset) = self.tool
            && let Some(bond) = self
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
            self.with_assistant_image(self.with_atom_text(
                self.with_updates(self.with_help(self.with_palette(self.workspace()))),
            )),
            self.help_open,
            self.assistant.viewed_image.is_some(),
        )
    }
}

/// Hydrogen labels are a computed display cache, not unsaved drawing edits.
fn same_drawing(a: &Document, b: &Document) -> bool {
    a.version == b.version
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
            a.color = [0, 0, 0];
            b.color = [0, 0, 0];
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

fn platform_shortcut(macos: &'static str, other: &'static str) -> &'static str {
    if cfg!(target_os = "macos") {
        macos
    } else {
        other
    }
}

#[cfg(test)]
mod tests {
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
            app.doc = Document::default();
            let before = app.doc.clone();
            app.edit(Edit::RingPreset(
                preset,
                Point::default(),
                None,
                false,
                false,
            ));
            assert!(!app.error, "{}", app.status);
            let placed = app.doc.clone();
            assert!(placed.bonds.iter().all(|b| b.projection));
            let _ = app.update(Message::Undo);
            assert_eq!(app.doc, before);
            let _ = app.update(Message::Redo);
            assert_eq!(app.doc, placed);
        }
        for preset in [P::Wedge, P::HashedWedge, P::Bold, P::Single] {
            let (mut app, _) = App::new();
            app.doc =
                sugar_document(Sugar::Glucose, Anomer::Alpha, 42.).map_err(anyhow::Error::msg)?;
            app.doc.reconcile_molecule_groups();
            let source = app.doc.clone();
            let bond = source
                .bonds
                .iter()
                .find(|b| b.display == "bold")
                .context("Front edge")?;
            app.selected = vec![bond.a, bond.b];
            let _ = app.update(Message::ApplyBondPreset(preset));
            assert!(!app.error, "{}", app.status);
            assert_eq!(app.doc.atoms, source.atoms);
            assert!(!chemistry_changed(&source, &app.doc));
            if app.doc != source {
                let _ = app.update(Message::Undo);
                assert_eq!(app.doc, source);
            }
            let a = source.atom(bond.a).context("Front atom")?.position;
            let b = source.atom(bond.b).context("Front atom")?.position;
            app.tool = Tool::StyledBond(preset);
            app.edit(Edit::Click(Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.)));
            assert!(!app.error, "{}", app.status);
            assert_eq!(app.doc.atoms, source.atoms);
            assert!(!chemistry_changed(&source, &app.doc));
        }
        Ok(())
    }

    #[test]
    fn inspector_changes_reset_scrolling_but_normal_updates_keep_the_position() {
        let (mut app, _) = App::new();
        app.busy = false;
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
        app.busy = false;
        let idle = subscriptions(&app);
        assert_eq!(idle, 2); // Window close and keyboard/mouse events only.
        app.assistant.busy = true;
        assert_eq!(subscriptions(&app), idle + 1);
        app.assistant.busy = false;
        app.refresh_due = Some(std::time::Instant::now());
        assert_eq!(subscriptions(&app), idle + 1);
        app.busy = true;
        assert_eq!(subscriptions(&app), idle);
        app.busy = false;
        app.refresh_due = None;

        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        app.recovery = Some(Recovery::in_directory(directory.path())?);
        assert_eq!(subscriptions(&app), idle);
        app.doc.add_atom("O", Point::default());
        assert_eq!(subscriptions(&app), idle + 1);
        let _ = app.update(Message::Tick);
        assert_eq!(subscriptions(&app), idle);
        let path = app
            .recovery
            .as_ref()
            .ok_or("Missing recovery")?
            .session
            .clone();
        assert!(path.exists());
        app.saved = app.doc.clone();
        assert_eq!(subscriptions(&app), idle + 1);
        let _ = app.update(Message::Tick);
        assert!(!path.exists());
        assert_eq!(subscriptions(&app), idle);
        Ok(())
    }

    fn checked_labels(app: &mut App) {
        let mut checked = app.doc.clone();
        for atom in &mut checked.atoms {
            atom.label_h = 2;
        }
        // A check must not adopt the engine's normalized bond depiction.
        if let Some(bond) = checked.bonds.first_mut() {
            bond.order = 4;
        }
        let _ = app.update(Message::EngineDone {
            revision: app.revision,
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
        app.busy = false;
        app.tool = Tool::Bond(2);
        let c = app.doc.add_atom("C", Point::default());
        let o = app.doc.add_atom("O", Point::new(0., -42.));
        let methyl = app.doc.add_atom("C", Point::new(36.373, 21.));
        app.doc.add_bond(c, o, 2, "plain");
        app.doc.add_bond(c, methyl, 1, "plain");
        app.doc.bonds[0].color = [32, 80, 145];
        app.doc.atom_mut(c).unwrap().label_h = 1;
        let original = app.doc.clone();
        let mut scenes = std::collections::BTreeSet::new();
        for position in [P::Left, P::Right, P::Center] {
            app.edit(Edit::Click(Point::new(0., -21.)));
            let mut expected = original.clone();
            expected.bonds[0].double_position = position;
            assert_eq!(app.doc, expected);
            assert!(!chemistry_changed(&original, &app.doc));
            scenes.insert(reshiki::scene::svg(&app.doc));
        }
        assert_eq!(scenes.len(), 3);
        for _ in 0..3 {
            let _ = app.update(Message::Undo);
        }
        assert_eq!(app.doc, original);
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc.bonds[0].double_position, P::Left);
        app.doc.bonds[0].order = 1;
        app.edit(Edit::Click(Point::new(0., -21.)));
        assert_eq!(app.doc.bonds[0].order, 2);
    }

    #[test]
    fn cancelling_a_running_cleanup_refresh_prevents_late_preview_or_apply() {
        let (mut app, _) = App::new();
        app.busy = false;
        let a = app.doc.add_atom("C", Point::default());
        let b = app.doc.add_atom("C", Point::new(80., 0.));
        app.doc.add_bond(a, b, 1, "plain");
        app.selected = vec![b];
        let original = app.doc.clone();
        let _ = app.update(Message::Clean);
        let job = cleanup::CleanupJob {
            options: reshiki::cleanup::Options {
                scope: reshiki::cleanup::Scope::SelectedAtoms,
                ..Default::default()
            },
            selection: vec![b],
            serial: app.cleanup_serial,
            epoch: app.file_epoch,
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
            revision: app.revision,
            kind: Job::Clean(job.clone()),
            result: response(),
        });
        assert_eq!(
            app.cleanup.as_ref().unwrap().job.options.scope,
            reshiki::cleanup::Scope::SelectedAtoms
        );
        let _ = app.update(Message::CleanupScope(
            reshiki::cleanup::Scope::SelectedMolecules,
        ));
        assert!(app.busy);
        let mut pending = job;
        pending.serial = app.cleanup_serial;
        let _ = app.update(Message::ApplyCleanup);
        assert_eq!(app.doc, original);
        assert!(app.cleanup.is_some());
        let _ = app.update(Message::CancelCleanup);
        let _ = app.update(Message::EngineDone {
            revision: app.revision,
            kind: Job::Clean(pending),
            result: response(),
        });
        assert!(app.cleanup.is_none());
        assert_eq!(app.doc, original);
        assert!(!app.history.can_undo());
    }

    #[test]
    fn abbreviation_display_changes_are_unsaved_and_undoable() {
        let (mut app, _) = App::new();
        app.busy = false;
        let a = app.doc.add_atom("C", Point::default());
        let b = app.doc.add_atom("O", Point::new(42., 0.));
        let c = app.doc.add_atom("C", Point::new(63., 36.));
        app.doc.add_bond(a, b, 1, "plain");
        app.doc.add_bond(b, c, 1, "plain");
        app.doc.contract(&[b, c], "OMe", "MeO").unwrap();
        app.saved = app.doc.clone();
        let saved = app.doc.clone();
        let _ = app.update(Message::Abbreviations(abbreviations::Action::ExpandAll));
        assert!(app.dirty());
        assert!(app.title().contains('•'));
        assert!(app.doc.abbreviations.is_empty());
        let _ = app.update(Message::Undo);
        assert!(!app.dirty());
        assert_eq!(app.doc, saved);
        let _ = app.update(Message::Redo);
        assert!(app.dirty());
        assert!(app.doc.abbreviations.is_empty());
    }

    #[test]
    fn cleanup_requires_apply_can_cancel_and_rejects_stale_results() {
        let (mut app, _) = App::new();
        app.busy = false;
        let a = app.doc.add_atom("C", Point::default());
        let b = app.doc.add_atom("O", Point::new(70., 12.));
        app.doc.add_bond(a, b, 1, "plain");
        app.saved = app.doc.clone();
        app.selected = vec![a, b];
        let original = app.doc.clone();
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
            revision: app.revision,
            kind: Job::Clean(cleanup::CleanupJob {
                options: Default::default(),
                selection: vec![a, b],
                serial: app.cleanup_serial,
                epoch: app.file_epoch,
            }),
            result: response(),
        });
        assert_eq!(app.doc, original);
        assert_eq!(app.display_document(), &cleaned);
        assert!(!app.dirty());
        let _ = app.update(Message::Delete);
        assert_eq!(app.doc, original);
        let _ = app.update(Message::CleanupOriginal(true));
        assert_eq!(app.display_document(), &original);
        let _ = app.update(Message::CancelCleanup);
        assert_eq!(app.doc, original);
        assert!(!app.history.can_undo());
        let _ = app.update(Message::EngineDone {
            revision: app.revision,
            kind: Job::Clean(cleanup::CleanupJob {
                options: Default::default(),
                selection: vec![a, b],
                serial: app.cleanup_serial,
                epoch: app.file_epoch,
            }),
            result: response(),
        });
        let _ = app.update(Message::ApplyCleanup);
        assert_eq!(app.doc, cleaned);
        assert_eq!(app.selected, vec![a, b]);
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, original);
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, cleaned);
        let _ = app.update(Message::EngineDone {
            revision: app.revision.wrapping_sub(1),
            kind: Job::Clean(cleanup::CleanupJob {
                options: Default::default(),
                selection: vec![a, b],
                serial: app.cleanup_serial,
                epoch: app.file_epoch,
            }),
            result: response(),
        });
        assert!(app.cleanup.is_none());
    }

    #[test]
    fn label_edits_and_indicator_drags_are_atomic_and_do_not_change_chemistry() {
        use atom_labels::Action;
        use reshiki::atom_labels::{Carbons, Owner};
        let (mut app, _) = App::new();
        let _ = app.perform(Pending::New);
        let a = app.doc.add_atom("N", Point::default());
        app.history = History::default();
        let original = app.doc.clone();
        app.label_action(Action::Number);
        assert!(!chemistry_changed(&original, &app.doc));
        let numbered = app.doc.clone();
        app.edit(Edit::AtomIndicator(Owner::Number(a), Point::new(20., -30.)));
        assert_eq!(
            app.doc
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
        assert!(same_drawing(&app.doc, &numbered));
        let _ = app.update(Message::Undo);
        assert!(same_drawing(&app.doc, &original));
        let _ = app.update(Message::Redo);
        assert!(same_drawing(&app.doc, &numbered));
        app.label_action(Action::Carbons(Carbons::All));
        app.label_action(Action::Hydrogens(false));
        app.label_action(Action::Stereo(true));
        let _ = app.perform(Pending::New);
        assert_eq!(app.doc.atom_labels, Default::default());
        assert_eq!(app.current_text_style().size_pt, 10.);
        assert_eq!(app.current_text_style().family, "Arial");
    }

    #[test]
    fn stale_refresh_is_rescheduled_and_derived_labels_do_not_dirty_the_document() {
        let (mut app, _) = App::new();
        let _ = app.perform(Pending::New);
        let id = app.doc.add_atom("N", Point::default());
        app.saved = app.doc.clone();
        let mut computed = app.doc.clone();
        computed.atom_mut(id).unwrap().cip_label = Some("S".into());
        computed.atom_mut(id).unwrap().label_h = 3;
        let response = Response {
            document: Some(computed),
            analysis: None,
            output: None,
            engine_version: "test".into(),
            warnings: vec![],
        };
        let _ = app.update(Message::EngineDone {
            revision: app.revision.wrapping_sub(1),
            kind: Job::RefreshLabels,
            result: Box::new(Ok(response.clone())),
        });
        assert!(app.refresh_due.is_some());
        assert!(app.doc.atom(id).unwrap().cip_label.is_none());
        let _ = app.update(Message::EngineDone {
            revision: app.revision,
            kind: Job::RefreshLabels,
            result: Box::new(Ok(response)),
        });
        assert_eq!(app.doc.atom(id).unwrap().label_h, 3);
        assert!(!app.dirty());
    }

    #[test]
    fn invalid_edit_is_rolled_back_without_an_undo_entry() {
        let (mut app, _) = App::new();
        let _ = app.perform(Pending::New);
        app.doc.add_atom("C", Point::default());
        app.history = History::default();
        let before = app.doc.clone();
        app.doc.atoms[0].position.x = f32::NAN;
        app.changed(before.clone());
        assert_eq!(app.doc, before);
        assert!(app.error);
        assert!(!app.history.undo(&mut app.doc));
        for color in ["αβγ", "💚AB", "#GG0000", "12345", "1234567"] {
            assert!(graphics::parse_color(color).is_none());
        }
    }

    #[test]
    fn chemistry_check_keeps_placement_atomic_and_preserves_redo() {
        use reshiki::rings::Preset;
        let (mut app, _) = App::new();
        let _ = app.perform(Pending::New);
        app.edit(Edit::RingPreset(
            Preset::ChairUp,
            Point::default(),
            None,
            false,
            false,
        ));
        let first = app.doc.clone();
        let selected = app.selected.clone();
        let revision = app.revision;
        checked_labels(&mut app);
        assert_eq!(app.revision, revision);
        assert_eq!(app.selected, selected);
        assert!(same_drawing(&first, &app.doc));
        assert!(app.doc.atoms.iter().all(|atom| atom.label_h == 2));
        let checked_first = app.doc.clone();
        let _ = app.update(Message::Undo);
        assert!(app.doc.atoms.is_empty());
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, checked_first);
        app.edit(Edit::RingPreset(
            Preset::ChairDown,
            Point::new(300., 0.),
            None,
            false,
            false,
        ));
        let second = app.doc.clone();
        let _ = app.update(Message::Undo);
        checked_labels(&mut app);
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, second);
    }

    #[test]
    fn computed_hydrogen_labels_do_not_make_a_saved_drawing_dirty() {
        use reshiki::rings::Preset;
        let (mut app, _) = App::new();
        let _ = app.perform(Pending::New);
        app.doc = Preset::ChairUp.document(42., false);
        app.saved = app.doc.clone();
        checked_labels(&mut app);
        assert!(!app.dirty());
        assert!(!app.history.can_undo());
        assert_ne!(app.doc, app.saved);
        app.doc.atoms[0].charge = 1;
        assert!(app.dirty());
        app.doc = app.saved.clone();
        app.doc.atoms[0].position.x += 1.;
        assert!(app.dirty());
    }

    #[test]
    fn ring_presets_use_atomic_history_and_leave_invalid_hosts_untouched() {
        use reshiki::rings::Preset;
        let (mut app, _) = App::new();
        app.doc = Document::default();
        let before = app.doc.clone();
        app.edit(Edit::RingPreset(
            Preset::ChairUp,
            Point::default(),
            Some(Point::new(0., 80.)),
            false,
            false,
        ));
        let placed = app.doc.clone();
        assert_eq!(placed.atoms.len(), 6);
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, before);
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, placed);
        app.edit(Edit::RingPreset(
            Preset::ChairDown,
            Point::new(300., 0.),
            None,
            false,
            false,
        ));
        assert_eq!(app.doc.atoms.len(), 12);
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, placed);
        let _ = app.update(Message::Tool(Tool::RingPreset(Preset::ChairUp)));
        let _ = app.perform(Pending::New);
        assert_eq!(app.tool, Tool::Select);
        assert_eq!(app.bond_drawing.length, 42.);
        let c = app.doc.add_atom("C", Point::default());
        app.doc.atom_mut(c).unwrap().radical_electrons = 1;
        let before = app.doc.clone();
        app.edit(Edit::RingPreset(
            Preset::ChairUp,
            Point::default(),
            None,
            false,
            false,
        ));
        assert_eq!(app.doc, before);
        assert!(app.error);
    }

    #[test]
    fn arrow_click_places_a_fixed_rightward_arrow_and_repeated_click_reverses_it() {
        use reshiki::arrows::{ArrowStyle, Preset};
        use reshiki::graphics::LinePattern;
        for zoom in [0.5, 2.5] {
            let (mut app, _) = App::new();
            app.camera.zoom = zoom;
            let style = ArrowStyle {
                pattern: LinePattern::Dashed,
                ..ArrowStyle::preset(Preset::Forward)
            };
            let _ = app.update(Message::Palette(palettes::Action::ArrowVariant(
                Preset::Forward,
                style.clone(),
            )));
            let blank = app.doc.clone();
            let start = Point::new(-100., 30.);
            app.edit(Edit::Click(start));
            assert_eq!(app.doc.arrows.len(), 1);
            let arrow = app.doc.arrows[0].clone();
            assert_eq!(arrow.start, start);
            assert_eq!(
                arrow.end,
                start.offset(blank.drawing_style.bond_length_world * 2., 0.)
            );
            assert_eq!(arrow.appearance(), style);
            assert_eq!(app.selected, [arrow.id]);
            let placed = app.doc.clone();
            app.selected.clear();
            app.edit(Edit::Click(arrow.point(0.5)));
            assert_eq!(app.doc.arrows.len(), 1);
            assert_eq!(app.doc.arrows[0].start, arrow.end);
            assert_eq!(app.doc.arrows[0].end, arrow.start);
            assert_eq!(app.selected, [arrow.id]);
            let reversed = app.doc.clone();
            let _ = app.update(Message::Undo);
            assert_eq!(app.doc, placed);
            let _ = app.update(Message::Undo);
            assert_eq!(app.doc, blank);
            let _ = app.update(Message::Redo);
            assert_eq!(app.doc, placed);
            let _ = app.update(Message::Redo);
            assert_eq!(app.doc, reversed);
        }
    }

    #[test]
    fn arrow_tool_click_applies_variants_and_cycles_half_heads_without_adding_objects() {
        use reshiki::arrows::{ArrowStyle, Head, Preset};
        let (mut app, _) = App::new();
        let _ = app.update(Message::Tool(Tool::Arrow));
        app.edit(Edit::Click(Point::default()));
        let before = app.doc.clone();
        let style = ArrowStyle {
            head: Head::Left,
            ..ArrowStyle::default()
        };
        let _ = app.update(Message::Palette(palettes::Action::ArrowVariant(
            Preset::Forward,
            style.clone(),
        )));
        let midpoint = app.doc.arrows[0].point(0.5);
        app.edit(Edit::Click(midpoint));
        assert_eq!(app.doc.arrows.len(), 1);
        assert_eq!(app.doc.arrows[0].appearance(), style);
        app.edit(Edit::Click(midpoint));
        assert_eq!(app.doc.arrows[0].appearance().head, Head::Right);
        app.edit(Edit::Click(midpoint));
        assert_eq!(app.doc.arrows[0].appearance().head, Head::Left);
        for _ in 0..3 {
            let _ = app.update(Message::Undo);
        }
        assert_eq!(app.doc, before);
    }

    #[test]
    fn repeated_arrow_click_reverses_reaction_roles_and_undo_restores_them() {
        use reshiki::reactions::{Participant, Reaction};
        let (mut app, _) = App::new();
        let reactant = app.doc.add_atom("O", Point::new(-100., 0.));
        let product = app.doc.add_atom("N", Point::new(200., 0.));
        let _ = app.update(Message::Tool(Tool::Arrow));
        app.edit(Edit::Click(Point::default()));
        let arrow = &app.doc.arrows[0];
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
        app.doc.reactions.push(reaction);
        let before = app.doc.clone();
        app.edit(Edit::Click(midpoint));
        assert_eq!(app.doc.reactions[0].reactants, before.reactions[0].products);
        assert_eq!(app.doc.reactions[0].products, before.reactions[0].reactants);
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, before);
    }

    #[test]
    fn arrow_edits_keep_bend_history_and_new_resets_jacs_defaults() {
        use arrows::{Action, Field};
        use reshiki::arrows::{ArrowStyle, Head, Preset};
        let (mut app, _) = App::new();
        app.doc = Document::default();
        app.saved = app.doc.clone();
        let _ = app.update(Message::ArrowStyle(Preset::Fishhook));
        app.edit(Edit::Bond(
            Point::new(0., 0.),
            Point::new(120., 0.),
            None,
            None,
        ));
        let id = app.selected[0];
        app.edit(Edit::ArrowHandle(id, 2, Point::new(60., -50.)));
        let bent = app.doc.clone();
        app.arrow_action(Action::Number(Field::Line, "1.5".into()));
        app.arrow_action(Action::ApplyNumber(Field::Line));
        assert_eq!(app.doc.arrows[0].appearance().width_pt, 1.5);
        assert_eq!(app.doc.arrows[0].control, bent.arrows[0].control);
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, bent);
        let _ = app.update(Message::Redo);
        assert_eq!(app.arrows.style.width_pt, 1.5);
        app.arrow_action(Action::Tail(Head::Full));
        app.arrow_action(Action::Reverse);
        assert_eq!(app.doc.arrows[0].end, Point::default());
        app.arrow_action(Action::Number(Field::Length, "NaN".into()));
        let before = app.doc.clone();
        app.arrow_action(Action::ApplyNumber(Field::Length));
        assert_eq!(app.doc, before);
        assert!(app.error);
        let _ = app.perform(Pending::New);
        assert_eq!(app.arrows.style, ArrowStyle::default());
        assert_eq!(app.arrow_style, Preset::Forward);
        assert_eq!(app.caption_format.style.family, "Arial");
        assert_eq!(app.caption_format.style.size_pt, 10.);
        assert_eq!(
            app.bond_drawing.length,
            reshiki::style::DEFAULT.bond_length_world
        );
        assert!(app.doc.arrows.is_empty());
    }

    #[test]
    fn library_authoring_is_independent_of_drawing_history_and_repeat_placement_keeps_anchor() {
        use reshiki::templates::Anchor;
        use template_library::Action as A;
        let (mut app, _) = App::new();
        let a = app.doc.add_atom("C", Point::default());
        let b = app.doc.add_atom("O", Point::new(42., 0.));
        app.doc.add_bond(a, b, 1, "plain");
        app.selected = vec![a, b];
        let before = app.doc.clone();
        let revision = app.revision;
        let _ = app.update(Message::Templates(A::BeginSave));
        app.selected.clear();
        let _ = app.update(Message::Templates(A::Name("Methanol".into())));
        let _ = app.update(Message::Templates(A::SaveDetails));
        assert_eq!(app.templates.library.templates[0].document, before);
        assert_eq!(app.doc, before);
        assert_eq!(app.revision, revision);
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
        assert_eq!(app.doc.atoms.len(), 4);
        assert_eq!(app.doc.atoms[3].position, Point::new(250., 100.));
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, before);
        let _ = app.update(Message::Templates(A::Remove));
        assert!(app.templates.library.templates.is_empty());
        assert_eq!(app.doc, before);
        let _ = app.update(Message::Templates(A::Restore));
        assert_eq!(app.templates.library.templates.len(), 1);
        assert_eq!(app.templates.library.templates[0].anchor, Anchor::Atom(b));
        let caption = app.doc.next_id();
        app.doc.annotations.push(Annotation {
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
        app.doc = Document::default();
        let id = app.doc.add_atom("N", Point::default());
        let before = app.doc.clone();
        app.tool = Tool::Graphic(reshiki::graphics::GraphicKind::Symbol(
            SymbolKind::CirclePlus,
        ));
        app.edit(Edit::Graphic(Point::default(), Point::default(), false));
        assert_eq!(app.selected, vec![id]);
        assert_eq!(app.doc.atoms[0].charge, 1);
        assert_eq!(app.doc.atoms[0].marks[0].kind, MarkKind::CircledCharge);
        let attached = app.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, before);
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, attached);
        app.edit(Edit::AtomMark(id, 0, Point::new(-30., 20.)));
        assert_eq!(app.doc.atoms[0].marks[0].offset, Point::new(-30., 20.));
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, attached);
        let _ = app.update(Message::RemoveMark(id, 0));
        assert_eq!(app.doc.atoms[0].charge, 0);
        assert!(app.doc.atoms[0].marks.is_empty());
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, attached);
    }

    #[test]
    fn every_new_document_starts_with_jacs_drawing_and_typography_defaults() {
        let (mut app, _) = App::new();
        app.orbital_phase = reshiki::scientific::Phase::Shaded;
        app.phase_flipped = true;
        app.attach_symbols = false;
        app.graphic_style.width_pt = 3.;
        app.caption_format.style.family = "Times New Roman".into();
        app.caption_format.style.size_pt = 18.;
        app.caption_format.style.color = [190, 30, 40];
        app.caption_format.style.bold = true;
        let _ = app.update(Message::DrawingLength("30".into()));
        let _ = app.update(Message::FixedLength(false));
        let _ = app.update(Message::FixedAngles(false));
        let _ = app.update(Message::ChainAngle("90".into()));
        let _ = app.perform(Pending::New);
        assert_eq!(
            app.caption_format.style,
            reshiki::typography::TextStyle::default()
        );
        assert_eq!(app.caption_format.style.family, "Arial");
        assert_eq!(app.font_size_input, "10");
        assert_eq!(app.text_color_input, "#000000");
        assert_eq!(app.drawing_length_input, "14.4");
        assert_eq!(app.bond_drawing.length, 42.);
        assert_eq!(app.graphic_width_input, "0.6");
        assert_eq!(app.orbital_phase, reshiki::scientific::Phase::Solid);
        assert!(!app.phase_flipped && app.attach_symbols);
        assert_eq!(app.graphic_style.width_pt, 0.6);
        assert_eq!(app.chain_drawing.angle, 120.);
        assert!(app.bond_drawing.fixed_angles && app.bond_drawing.fixed_length);
        assert_eq!(app.tool, Tool::Select);
    }

    #[test]
    fn entire_chain_is_one_history_step_and_draw_settings_do_not_edit_the_document() {
        let (mut app, _) = App::new();
        app.doc = Document::default();
        let before = app.doc.clone();
        let _ = app.update(Message::ChainAtoms("8".into()));
        let _ = app.update(Message::DrawingLength("20".into()));
        let _ = app.update(Message::ChainAngle("110".into()));
        assert_eq!(app.doc, before);
        assert_eq!(app.revision, 0);
        let points = reshiki::chains::straight(
            Point::default(),
            Point::new(350., 0.),
            false,
            app.bond_drawing,
            app.chain_drawing,
            false,
        );
        app.tool = Tool::Chain(reshiki::chains::ChainMode::Straight);
        app.edit(Edit::Chain {
            points,
            source: None,
            target: None,
        });
        assert_eq!((app.doc.atoms.len(), app.doc.bonds.len()), (8, 7));
        let drawn = app.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, before);
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, drawn);
        app.edit(Edit::Chain {
            points: vec![Point::default(), Point::new(42., 0.)],
            source: None,
            target: None,
        });
        assert!(app.error);
        assert_eq!(app.doc, drawn);
        app.tool = Tool::Bond(1);
        let last = app.doc.atoms.last().unwrap().position;
        app.edit(Edit::Click(last));
        assert!(
            (app.doc.atoms.last().unwrap().position.distance(last)
                - reshiki::style::DEFAULT.world(20.))
            .abs()
                < 0.001
        );
        let drawing = app.doc.clone();
        let _ = app.update(Message::ResetBondDrawing);
        assert_eq!(
            app.bond_drawing.length,
            reshiki::style::DEFAULT.bond_length_world
        );
        assert_eq!(app.drawing_length_input, "14.4");
        assert_eq!(app.chain_drawing.angle, 120.);
        assert!(app.bond_drawing.fixed_length && app.bond_drawing.fixed_angles);
        assert_eq!(app.doc, drawing);
    }

    #[test]
    fn palette_keeps_text_range_formatting_and_recolors_graphics_only_in_all_scope() {
        use iced::widget::text_editor::{Action, Motion};
        use reshiki::typography::StyleChange;
        use typography::ColorScope;
        let (mut app, _) = App::new();
        app.doc = Document::default();
        app.doc.annotations.push(Annotation {
            id: 1,
            position: Point::default(),
            text: "AB CD".into(),
            format: Default::default(),
        });
        app.doc.graphics.push(Graphic::dragged(
            2,
            reshiki::graphics::GraphicKind::Rectangle,
            Point::new(0., 80.),
            Point::new(84., 120.),
            GraphicStyle {
                fill: Some([200, 200, 200]),
                ..Default::default()
            },
            Default::default(),
            false,
        ));
        app.selected = vec![1];
        app.sync_typography();
        app.caption_action(Action::Move(Motion::DocumentStart));
        app.caption_action(Action::Select(Motion::Right));
        app.caption_action(Action::Select(Motion::Right));
        assert_eq!(app.text_range(), Some(0..2));
        let original = app.doc.clone();
        let red = [180, 50, 55];
        let _ = app.update(Message::TextStyle(StyleChange::Color(red)));
        assert_eq!(app.doc.annotations[0].format.at(0).color, red);
        assert_eq!(app.doc.annotations[0].format.at(3).color, [0; 3]);
        assert_eq!(app.doc.graphics, original.graphics);
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, original);
        // A stale range in the inspector must not constrain Select All.
        let _ = app.update(Message::SelectAll);
        let _ = app.update(Message::ColorScope(ColorScope::Text));
        let _ = app.update(Message::TextStyle(StyleChange::Color(red)));
        assert_eq!(app.doc.graphics, original.graphics);
        assert_eq!(app.doc.annotations[0].format.at(3).color, red);
        let _ = app.update(Message::ColorScope(ColorScope::All));
        let _ = app.update(Message::TextStyle(StyleChange::Color(red)));
        assert_eq!(app.doc.graphics[0].style.stroke, red);
        assert_eq!(app.doc.graphics[0].style.fill, Some(red));
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc.graphics, original.graphics);
    }

    #[test]
    fn palette_scopes_recolor_selected_bonds_and_objects_in_one_undo() {
        use reshiki::typography::StyleChange;
        use typography::ColorScope;
        let (mut app, _) = App::new();
        app.doc = Document::default();
        let a = app.doc.add_atom("C", Point::default());
        let b = app.doc.add_atom("O", Point::new(42., 0.));
        let c = app.doc.add_atom("N", Point::new(84., 0.));
        app.doc.add_bond(a, b, 1, "plain");
        app.doc.add_bond(b, c, 2, "plain");
        app.doc.arrows.push(Arrow::new(
            4,
            Point::new(0., 80.),
            Point::new(84., 80.),
            Default::default(),
            Default::default(),
        ));
        app.doc.annotations.push(Annotation {
            id: 5,
            position: Point::new(0., 120.),
            text: "Label".into(),
            format: Default::default(),
        });
        app.doc.atom_mut(b).unwrap().display.number = Some(reshiki::atom_labels::Number {
            text: "2".into(),
            offset: None,
            style: reshiki::atom_labels::number_style(),
        });
        let original = app.doc.clone();
        let blue = [32, 80, 145];
        let red = [180, 50, 55];
        let _ = app.update(Message::SelectAll);
        let _ = app.update(Message::TextStyle(StyleChange::Color(blue)));
        assert!(
            app.doc
                .bonds
                .iter()
                .all(|b| b.color == blue && b.indicator.style.color == blue)
        );
        assert!(
            app.doc
                .atoms
                .iter()
                .all(|a| a.text_style.as_ref().unwrap().color == blue)
        );
        assert_eq!(
            app.doc
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
        assert_eq!(app.doc.arrows[0].appearance().color, blue);
        assert_eq!(app.doc.annotations[0].format.style.color, blue);
        let recolored = app.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, original);
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, recolored);
        app.selected = vec![a, b];
        let _ = app.update(Message::ColorScope(ColorScope::Bonds));
        let _ = app.update(Message::TextColor("#B43237".into()));
        let _ = app.update(Message::ApplyTextColor);
        assert_eq!(app.doc.bonds[0].color, red);
        assert_eq!(app.doc.bonds[1].color, blue);
        assert_eq!(app.doc.atoms[1].text_style.as_ref().unwrap().color, blue);
        assert_eq!(app.doc.arrows, recolored.arrows);
        let _ = app.update(Message::ColorScope(ColorScope::Text));
        let _ = app.update(Message::TextStyle(StyleChange::Color(red)));
        assert_eq!(app.doc.atoms[0].text_style.as_ref().unwrap().color, red);
        assert_eq!(app.doc.atoms[2], recolored.atoms[2]);
        assert_eq!(app.doc.bonds[1].color, blue);
        assert_eq!(app.doc.arrows, recolored.arrows);
        // Selecting only one end of a bond does not recolor that bond.
        app.selected = vec![c];
        let _ = app.update(Message::ColorScope(ColorScope::All));
        let _ = app.update(Message::TextStyle(StyleChange::Color(red)));
        assert_eq!(app.doc.bonds[1].color, blue);
    }

    #[test]
    fn bond_styles_position_color_and_direction_are_undoable() {
        use reshiki::bonds::{BondPreset, DoublePosition};
        let (mut app, _) = App::new();
        app.doc.add_atom("C", Point::new(0., 0.));
        app.doc.add_atom("C", Point::new(84., 0.));
        app.doc.add_atom("C", Point::new(168., 0.));
        app.doc.add_bond(1, 2, 2, "plain");
        app.doc.add_bond(2, 3, 1, "plain");
        app.selected = vec![1, 2];
        let other = app.doc.bonds[1].clone();
        let _ = app.update(Message::BondPosition(DoublePosition::Left));
        let _ = app.update(Message::BondColor("#205091".into()));
        let _ = app.update(Message::ApplyBondColor);
        assert_eq!(app.doc.bonds[0].color, [32, 80, 145]);
        assert_eq!(app.doc.bonds[1], other);
        let _ = app.update(Message::ApplyBondPreset(BondPreset::HollowWedge));
        assert_eq!(app.doc.bonds[0].display, "hollow_wedge");
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc.bonds[0].order, 2);
        let _ = app.update(Message::Redo);
        app.tool = Tool::StyledBond(BondPreset::HollowWedge);
        app.edit(Edit::Click(Point::new(42., 0.)));
        assert_eq!((app.doc.bonds[0].a, app.doc.bonds[0].b), (2, 1));
        let _ = app.update(Message::Undo);
        assert_eq!((app.doc.bonds[0].a, app.doc.bonds[0].b), (1, 2));
        let before = app.doc.clone();
        app.tool = Tool::StyledBond(BondPreset::Dotted);
        app.edit(Edit::Bond(
            Point::new(0., 0.),
            Point::new(168., 0.),
            Some(1),
            Some(3),
        ));
        assert!(app.error);
        assert_eq!(app.doc, before);
    }

    #[test]
    fn aromatic_bond_tools_preserve_circles_and_undo_direction_changes() -> anyhow::Result<()> {
        use anyhow::Context;
        use reshiki::bonds::BondPreset as P;
        for preset in [P::Wedge, P::HashedWedge, P::HollowWedge, P::Bold, P::Hashed] {
            let (mut app, _) = App::new();
            let ids = editing::ring(&mut app.doc, Point::default(), 6, true, 0.);
            reshiki::projection::tilt(&mut app.doc, &ids, 35., true);
            app.doc.reconcile_molecule_groups();
            let source = app.doc.clone();
            let bond = source.bonds.first().context("Missing ring edge")?;
            let a = source.atom(bond.a).context("Missing ring atom")?.position;
            let b = source.atom(bond.b).context("Missing ring atom")?.position;
            let midpoint = Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.);
            app.camera.zoom = 2.;
            app.tool = match preset {
                P::Wedge => Tool::Wedge,
                P::HashedWedge => Tool::Hash,
                p => Tool::StyledBond(p),
            };
            app.edit(Edit::Click(midpoint));
            assert!(!app.error, "{}", app.status);
            assert_eq!(app.doc.bonds[0].display, preset.parts().1);
            assert!(app.doc.bonds.iter().all(|b| b.order == 4));
            assert_eq!(reshiki::aromatic::circles(&app.doc).len(), 1);
            assert!(!chemistry_changed(&source, &app.doc));
            assert_eq!(app.doc.atoms, source.atoms);
            let styled = app.doc.clone();
            app.edit(Edit::Click(midpoint));
            assert_eq!((app.doc.bonds[0].a, app.doc.bonds[0].b), (bond.b, bond.a));
            assert_eq!(reshiki::aromatic::circles(&app.doc).len(), 1);
            assert!(!chemistry_changed(&source, &app.doc));
            let _ = app.update(Message::Undo);
            assert_eq!(app.doc, styled);
            let _ = app.update(Message::Undo);
            assert_eq!(app.doc, source);
            let _ = app.update(Message::Redo);
            assert_eq!(app.doc, styled);
            app.tool = Tool::Bond(1);
            app.edit(Edit::Click(midpoint));
            assert_eq!(app.doc, source, "Plain appearance retains aromatic order");
            app.tool = Tool::Bond(2);
            app.edit(Edit::Click(midpoint));
            assert_eq!(
                app.doc.bonds[0].order, 2,
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
            app.selected = editing::ring(&mut app.doc, Point::default(), 6, true, 0.);
            reshiki::projection::tilt(&mut app.doc, &app.selected, 65., true);
            app.doc.reconcile_molecule_groups();
            let source = app.doc.clone();
            let _ = app.update(Message::ApplyBondPreset(preset));
            assert!(!app.error, "{}", app.status);
            assert!(app.doc.bonds.iter().all(|b| b.order == 4));
            assert!(app.doc.bonds.iter().all(|b| b.display == preset.parts().1));
            assert_eq!(reshiki::aromatic::circles(&app.doc).len(), 1);
            assert!(!chemistry_changed(&source, &app.doc));
            assert_eq!(app.doc.atoms, source.atoms);
            if preset != P::Single {
                let _ = app.update(Message::Undo);
                assert_eq!(app.doc, source);
            }
            let bond = source.bonds.first().context("Missing ring bond")?;
            let a = source.atom(bond.a).context("Missing ring atom")?.position;
            let b = source.atom(bond.b).context("Missing ring atom")?.position;
            app.tool = Tool::StyledBond(preset);
            app.edit(Edit::Bond(b, a, Some(bond.b), Some(bond.a)));
            assert!(!app.error, "{}", app.status);
            assert_eq!((app.doc.bonds[0].a, app.doc.bonds[0].b), (bond.b, bond.a));
            assert!(app.doc.bonds.iter().all(|b| b.order == 4));
            assert_eq!(reshiki::aromatic::circles(&app.doc).len(), 1);
            assert!(!chemistry_changed(&source, &app.doc));
            let _ = app.update(Message::Undo);
            assert_eq!(app.doc, source);
        }
        Ok(())
    }

    #[test]
    fn group_frame_and_ungroup_are_individually_undoable() {
        let (mut app, _) = App::new();
        let a = app.doc.add_atom("C", Point::default());
        let b = app.doc.add_atom("O", Point::new(42., 0.));
        app.doc.add_bond(a, b, 1, "plain");
        app.selected = vec![a];
        let initial = app.doc.clone();
        let _ = app.update(Message::AddFrame(reshiki::graphics::GraphicKind::Brackets));
        assert_eq!(app.doc.groups.len(), 1);
        assert_eq!(app.doc.graphics.len(), 1);
        assert_eq!(app.selected.len(), 3);
        assert_eq!(app.doc.atoms, initial.atoms);
        let framed = app.doc.clone();
        let _ = app.update(Message::Ungroup);
        assert!(app.doc.groups.is_empty());
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, framed);
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, initial);
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, framed);
        assert_eq!(app.selected.len(), 3);
        let _ = app.update(Message::InvertSelection);
        assert!(app.selected.is_empty());
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
        app.doc = result.document.unwrap();
        app.analysis = result.analysis;
        let _ = app.update(Message::Tool(Tool::Graphic(GraphicKind::Curve)));
        app.edit(Edit::Graphic(
            Point::default(),
            Point::new(100., 40.),
            false,
        ));
        let id = app.doc.graphics[0].id;
        assert_eq!(app.tool, Tool::Select);
        app.apply_graphic_style(GraphicChange::Stroke([32, 80, 145]));
        let before = app.doc.clone();
        let _ = app.update(Message::Tool(Tool::EditPoints));
        app.edit(Edit::GraphicPoint(id, 1, Point::new(20., -50.)));
        assert_eq!(app.doc.graphics[0].kind, GraphicKind::Path);
        let after = app.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, before);
        assert_eq!(app.selected, vec![id]);
        assert_eq!(app.analysis.as_ref().unwrap().formula, "C2H6O");
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, after);
        assert_eq!(app.selected, vec![id]);
        assert!(app.analysis.is_some());
        let _ = app.update(Message::Tool(Tool::Graphic(GraphicKind::Ellipse)));
        assert!(app.selected.is_empty());
        app.apply_graphic_style(GraphicChange::Stroke([180, 50, 55]));
        assert_eq!(
            app.doc, after,
            "new drawing style must not change the previous object"
        );
    }

    #[test]
    fn partial_typography_edit_and_repeated_backspace_restore_with_undo() {
        use iced::widget::text_editor::{Action, Edit as TextEdit, Motion};
        use reshiki::typography::{StyleChange, TextFormat};
        let (mut app, _) = App::new();
        app.doc.annotations.push(Annotation {
            id: 1,
            position: Point::default(),
            text: "AAA".into(),
            format: TextFormat::default(),
        });
        app.selected = vec![1];
        app.sync_typography();
        app.caption_action(Action::Move(Motion::DocumentStart));
        app.caption_action(Action::Move(Motion::Right));
        app.caption_action(Action::Select(Motion::Right));
        assert_eq!(app.text_range(), Some(1..2));
        app.apply_text_style(StyleChange::Bold(true));
        assert!(!app.doc.annotations[0].format.at(0).bold);
        assert!(app.doc.annotations[0].format.at(1).bold);
        let styled = app.doc.clone();
        app.caption_action(Action::Move(Motion::DocumentStart));
        app.caption_action(Action::Move(Motion::Right));
        app.caption_action(Action::Edit(TextEdit::Backspace));
        assert_eq!(app.doc.annotations[0].text, "AA");
        assert!(app.doc.annotations[0].format.at(0).bold);
        assert!(!app.doc.annotations[0].format.at(1).bold);
        let edited = app.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, styled);
        assert_eq!(app.caption, "AAA");
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, edited);
        assert_eq!(app.caption, "AA");
    }

    #[test]
    fn one_off_template_choice_is_nonmutating_and_attachment_is_one_undo_step() {
        let (mut app, _) = App::new();
        app.busy = false;
        let a = app.doc.add_atom("C", Point::new(-30.0, 0.0));
        let b = app.doc.add_atom("C", Point::new(30.0, 0.0));
        app.doc.add_bond(a, b, 1, "plain");
        app.saved = app.doc.clone();
        let before = app.doc.clone();
        let index = reshiki::templates::LIBRARY
            .iter()
            .position(|t| t.name == "Cyclopentane")
            .unwrap();
        let _ = app.update(Message::InsertTemplate(index));
        assert_eq!(app.doc, before);
        assert!(!app.dirty());
        assert_eq!(app.tool, Tool::Template);
        let _ = app.update(Message::Templates(template_library::Action::Repeat(false)));
        app.templates.connection = reshiki::templates::Connection::FuseBond;
        app.edit(Edit::Template(Point::default(), None));
        assert_eq!(app.tool, Tool::Select);
        let placed = app.doc.clone();
        assert_eq!(placed.atoms.len(), 5);
        assert_eq!(placed.bonds.len(), 5);
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, before);
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, placed);
        let _ = app.update(Message::Tool(Tool::Select));
        app.edit(Edit::Template(Point::new(500.0, 500.0), None));
        assert_eq!(app.doc, placed);
    }

    #[test]
    fn templates_repeat_bond_fusion_by_default_until_cancelled() {
        use reshiki::templates::{Anchor, Connection};
        use template_library::Action as A;
        let (mut app, _) = App::new();
        app.busy = false;
        let a = app.doc.add_atom("C", Point::new(0., -21.));
        let b = app.doc.add_atom("C", Point::new(0., 21.));
        app.doc.add_bond(a, b, 1, "plain");
        let index = reshiki::templates::LIBRARY
            .iter()
            .position(|t| t.name == "Cyclohexane")
            .unwrap();
        let source = &reshiki::templates::LIBRARY[index].document.bonds[0];
        let anchor = Anchor::Bond(source.a, source.b);
        let _ = app.update(Message::InsertTemplate(index));
        let _ = app.update(Message::Templates(A::Anchor(anchor)));
        assert!(app.templates.repeat);
        let mut snapshots = vec![app.doc.clone()];
        for (atoms, bonds) in [(6, 6), (10, 11), (14, 16)] {
            let point = app
                .doc
                .bonds
                .iter()
                .map(|bond| {
                    let p = app.doc.atom(bond.a).unwrap().position;
                    let q = app.doc.atom(bond.b).unwrap().position;
                    Point::new((p.x + q.x) / 2., (p.y + q.y) / 2.)
                })
                .max_by(|p, q| p.x.total_cmp(&q.x))
                .unwrap();
            app.edit(Edit::Template(point, None));
            assert!(!app.error, "{}", app.status);
            assert_eq!((app.doc.atoms.len(), app.doc.bonds.len()), (atoms, bonds));
            assert_eq!(app.tool, Tool::Template);
            assert_eq!(app.templates.anchor, anchor);
            assert_eq!(app.templates.connection, Connection::FuseBond);
            app.doc.validate().unwrap();
            snapshots.push(app.doc.clone());
        }
        // A misplaced click must preserve both the drawing and placement mode.
        app.edit(Edit::Template(app.doc.atoms[0].position, None));
        assert!(app.error);
        assert_eq!(app.doc, snapshots[3]);
        assert_eq!(app.tool, Tool::Template);
        for document in snapshots[..3].iter().rev() {
            let _ = app.update(Message::Undo);
            assert_eq!(&app.doc, document);
            assert_eq!(app.tool, Tool::Template);
        }
        for document in &snapshots[1..] {
            let _ = app.update(Message::Redo);
            assert_eq!(&app.doc, document);
        }
        let _ = app.update(Message::Escape);
        assert_eq!(app.tool, Tool::Select);
        app.edit(Edit::Template(Point::new(500., 500.), None));
        assert_eq!(app.doc, snapshots[3]);
    }

    #[test]
    fn axis_resize_has_one_step_undo_and_ignores_invalid_scales() -> Result<(), String> {
        let (mut app, _) = App::new();
        let a = app.doc.add_atom("C", Point::new(-20., -10.));
        let b = app.doc.add_atom("C", Point::new(20., 10.));
        let remote = app.doc.add_atom("O", Point::new(150., 80.));
        app.doc.add_bond(a, b, 1, "wedge");
        let original = app.doc.clone();
        app.edit(Edit::ScaleAxes {
            ids: vec![a, b],
            pivot: Point::new(-20., -10.),
            x: 2.,
            y: 1.,
        });
        assert_eq!(
            app.doc.atom(b).ok_or("Atom")?.position,
            Point::new(60., 10.)
        );
        assert_eq!(app.doc.atom(remote), original.atom(remote));
        assert_eq!(app.doc.bonds, original.bonds);
        let resized = app.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, original);
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, resized);
        for (x, y) in [(1., 1.), (f32::NAN, 1.), (0., 1.), (1., -1.)] {
            app.edit(Edit::ScaleAxes {
                ids: vec![a, b],
                pivot: Point::default(),
                x,
                y,
            });
            assert_eq!(app.doc, resized);
        }
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, original, "No-op drags do not consume undo steps");
        Ok(())
    }

    #[test]
    fn selection_handle_transforms_preserve_other_objects_and_undo_in_one_step() {
        let (mut app, _) = App::new();
        let a = app.doc.add_atom("C", Point::new(-20.0, -10.0));
        let b = app.doc.add_atom("C", Point::new(20.0, 10.0));
        let other = app.doc.add_atom("O", Point::new(150.0, 80.0));
        app.doc.add_bond(a, b, 1, "wedge");
        let original = app.doc.clone();
        app.edit(Edit::Transform {
            ids: vec![a, b],
            pivot: Point::new(-20.0, -10.0),
            scale: 2.0,
            rotation: 0.0,
        });
        let resized = app.doc.clone();
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
        let rotated = app.doc.clone();
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
        assert_eq!(app.selected, vec![a, b]);
        for expected in [&resized, &original] {
            let _ = app.update(Message::Undo);
            assert_eq!(&app.doc, expected);
        }
        for expected in [&resized, &rotated] {
            let _ = app.update(Message::Redo);
            assert_eq!(&app.doc, expected);
        }
        app.edit(Edit::Transform {
            ids: vec![a, b],
            pivot: Point::default(),
            scale: 1.0,
            rotation: 0.0,
        });
        let _ = app.update(Message::Undo);
        assert_eq!(
            app.doc, resized,
            "clicking a handle without dragging adds no history"
        );
        app.tool = Tool::Ring;
        let _ = app.update(Message::SelectAll);
        assert_eq!(app.tool, Tool::Select);
        assert_eq!(app.selected, app.doc.all_ids());
    }

    #[test]
    fn snapping_a_ring_is_one_undoable_edit_with_original_atom_ids_restored() {
        let (mut app, _) = App::new();
        let a = app.doc.add_atom("C", Point::default());
        let b = app.doc.add_atom("C", Point::new(60.0, 0.0));
        app.doc.add_bond(a, b, 1, "plain");
        let ids = editing::ring(&mut app.doc, Point::new(200.0, 200.0), 5, false, 5.0);
        let p = app.doc.atom(ids[0]).unwrap().position;
        let q = app.doc.atom(ids[1]).unwrap().position;
        let before = app.doc.clone();
        app.edit(Edit::Move(
            ids,
            30.0 - (p.x + q.x) / 2.0,
            -(p.y + q.y) / 2.0,
        ));
        let snapped = app.doc.clone();
        assert_eq!((snapped.atoms.len(), snapped.bonds.len()), (5, 5));
        assert_eq!(app.selected.len(), 5);
        assert!(app.selected.contains(&a) && app.selected.contains(&b));
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, before);
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, snapped);
    }

    #[test]
    fn clicking_existing_bonds_cycles_order_and_can_be_undone() {
        for tool in [Tool::Bond(1), Tool::Bond(3)] {
            let (mut app, _) = App::new();
            app.tool = tool;
            let a = app.doc.add_atom("C", Point::default());
            let b = app.doc.add_atom("C", Point::new(42.0, 0.0));
            app.doc.add_bond(a, b, 1, "plain");
            let original = app.doc.clone();
            for order in [2, 3, 1] {
                app.edit(Edit::Click(Point::new(21.0, 0.0)));
                assert_eq!(app.doc.atoms, original.atoms);
                assert_eq!(app.doc.bonds.len(), 1);
                assert_eq!(app.doc.bonds[0].order, order);
                assert_eq!(app.doc.bonds[0].display, "plain");
            }
            assert_eq!(app.doc, original);
            for order in [3, 2, 1] {
                let _ = app.update(Message::Undo);
                assert_eq!(app.doc.bonds[0].order, order);
            }
            assert_eq!(app.doc, original);
            app.tool = Tool::Wedge;
            app.edit(Edit::Click(Point::new(21.0, 0.0)));
            assert_eq!(app.doc.bonds[0].order, 1);
            assert_eq!(app.doc.bonds[0].display, "wedge");
        }
    }

    #[tokio::test]
    async fn endpoint_clicks_grow_a_connected_zigzag_with_undo_and_redo() {
        let (mut app, _) = App::new();
        app.tool = Tool::Bond(1);
        app.edit(Edit::Click(Point::default()));
        for _ in 0..5 {
            let endpoint = app
                .doc
                .atom(*app.selected.first().unwrap())
                .unwrap()
                .position;
            app.edit(Edit::Click(endpoint));
        }
        assert_eq!((app.doc.atoms.len(), app.doc.bonds.len()), (7, 6));
        for three in app.doc.atoms.windows(3) {
            let a = three[0].position;
            let b = three[1].position;
            let c = three[2].position;
            let cosine = ((a.x - b.x) * (c.x - b.x) + (a.y - b.y) * (c.y - b.y))
                / (a.distance(b) * c.distance(b));
            assert!((cosine + 0.5).abs() < 0.001, "chain needs 120° junctions");
            assert!(c.x > b.x && b.x > a.x, "chain must keep extending forward");
            assert!((a.y - c.y).abs() < 0.001, "successive turns must alternate");
        }
        let complete = app.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!((app.doc.atoms.len(), app.doc.bonds.len()), (6, 5));
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, complete);
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
    fn bond_tools_grow_carbon_after_using_an_atom_label_and_still_edit_bonds() {
        let (mut app, _) = App::new();
        let _ = app.update(Message::Element("O".into()));
        app.edit(Edit::Click(Point::default()));
        let oxygen = app.doc.atoms[0].id;
        let _ = app.update(Message::Tool(Tool::Bond(1)));
        app.edit(Edit::Click(Point::default()));
        let carbon = app.doc.atoms[1].clone();
        assert_eq!(carbon.element, "C");
        assert_eq!(app.doc.atom(oxygen).unwrap().element, "O");
        app.edit(Edit::Bond(
            carbon.position,
            carbon.position.offset(36.373066, 21.0),
            Some(carbon.id),
            None,
        ));
        assert_eq!(app.doc.atoms[2].element, "C");
        app.tool = Tool::Bond(2);
        app.edit(Edit::Click(Point::new(
            carbon.position.x / 2.0,
            carbon.position.y / 2.0,
        )));
        assert_eq!((app.doc.atoms.len(), app.doc.bonds.len()), (3, 2));
        assert_eq!(app.doc.bonds[0].order, 2);
    }

    #[test]
    fn fit_uses_available_canvas_and_respects_manual_pan() {
        let (mut app, _) = App::new();
        app.doc.add_atom("C", Point::new(-250.0, -100.0));
        app.doc.add_atom("O", Point::new(250.0, 100.0));
        let document = app.doc.clone();
        let _ = app.update(Message::Viewport(iced::Size::new(600.0, 400.0)));
        let small_zoom = app.camera.zoom;
        let _ = app.update(Message::Viewport(iced::Size::new(1000.0, 700.0)));
        assert!(app.camera.zoom > small_zoom);
        app.edit(Edit::Pan(60.0, -20.0));
        let camera = app.camera;
        let _ = app.update(Message::Viewport(iced::Size::new(700.0, 500.0)));
        assert_eq!(app.camera.center, camera.center);
        assert_eq!(app.camera.zoom, camera.zoom);
        assert_eq!(app.doc, document);
    }

    #[test]
    fn view_aids_preserve_drawing_selection_history_and_manual_camera() {
        let (mut app, _) = App::new();
        let id = app.doc.add_atom("O", Point::new(50., 20.));
        app.selected = vec![id];
        app.edit(Edit::Pan(60., -20.));
        let document = app.doc.clone();
        let camera = app.camera;
        let revision = app.revision;
        let history = app.history.can_undo();
        let export = reshiki::export::drawing(&app.doc, "svg").expect("SVG before view change");
        for message in [
            Message::ToggleView,
            Message::Rulers(true),
            Message::Crosshair(true),
            Message::RulerUnit(canvas::guides::Unit::Inches),
            Message::Grid,
        ] {
            let _ = app.update(message);
        }
        assert_eq!(app.doc, document);
        assert_eq!(app.selected, [id]);
        assert_eq!(app.revision, revision);
        assert_eq!(app.history.can_undo(), history);
        assert_eq!(app.camera.center, camera.center);
        assert_eq!(app.camera.zoom, camera.zoom);
        assert_eq!(
            reshiki::export::drawing(&app.doc, "svg").expect("SVG after view change"),
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
            let _ = app.update(Message::Opened(Some((path.clone(), Ok(contents.clone())))));
            assert!(!app.error && !app.dirty(), "{extension}: {}", app.status);
            assert_eq!(app.doc, document);
            assert_eq!(app.path, Some(path));
            assert_eq!(app.status, "Document opened");
        }
    }

    #[test]
    fn late_save_does_not_mark_newer_edits_as_saved() {
        let (mut app, _) = App::new();
        let snapshot = app.doc.clone();
        app.doc.add_atom("O", Point::default());
        let _ = app.update(Message::Saved(
            0,
            Box::new(snapshot),
            Ok(Some("example.reshiki".into())),
        ));
        assert!(app.dirty());
    }

    #[test]
    fn late_save_does_not_retarget_another_document() {
        let (mut app, _) = App::new();
        let snapshot = app.doc.clone();
        let _ = app.perform(Pending::New);
        let _ = app.update(Message::Saved(
            0,
            Box::new(snapshot),
            Ok(Some("previous.reshiki".into())),
        ));
        assert!(app.path.is_none());
    }

    #[test]
    fn startup_is_a_blank_saved_canvas_without_an_import_job() {
        let (app, _) = App::new();
        assert_eq!(app.doc, Document::default());
        assert!(app.doc.all_ids().is_empty());
        assert!(!app.busy && !app.dirty() && !app.history.can_undo());
        assert!(app.analysis.is_none() && app.path.is_none());
        assert!(app.smiles.is_empty());
    }

    #[test]
    fn new_document_invalidates_inflight_import_even_when_empty() {
        let (mut app, _) = App::new();
        let revision = app.revision;
        let _ = app.perform(Pending::New);
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
        assert!(app.doc.atoms.is_empty());
    }
}
