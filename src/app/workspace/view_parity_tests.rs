//! Opt-in before/after parity of the workspace view builders.
//!
//! Capture the unchanged view once, then compare a candidate on the same
//! machine, renderer, fonts and empty data directory. metadata.json holds the
//! renderer, font, sizes, ordered case names and filter, and compare mode
//! requires them to be equal. The test is modelled on
//! `canvas::render_parity_tests`.
//!
//! **Pinned**, per case and window size, after the case script:
//! - layout bounds of the base and of the nested overlay;
//! - operation records: containers, scrollables, focusables, text inputs,
//!   text and custom widgets, with their ids, bounds and traverse depth;
//! - accessibility nodes and duplicate ids;
//! - the message that activating each accessible control resolves to;
//! - allocations of view, diff and layout (the minimum of 5 warm runs);
//! - pixels of the base and the overlay;
//! - probes of the case's region: the tooltip each target opens after its
//!   delay (bounds and a screenshot hash), the messages of a click, of each
//!   option of a menu that the click opens, and of typing `7` then Enter.
//!
//! **Not visible to operations**, so covered by pixels and probes only:
//! inert subtrees (`reshiki::accessibility::inert` skips `operate`),
//! canvases, pick-list values, rich text (help key spans, and hover-hint
//! labels that contain shortcut symbols), tooltip labels and menu overlays.
//!
//! **Normalisations in force:** none. Message lines are never sorted, dropped
//! or deduplicated. Only consecutive MENU samples with identical messages
//! share one line, which names their y range.
//!
//! Environment:
//! - `RESHIKI_VIEW_PARITY` (required): the artifact directory.
//! - `RESHIKI_VIEW_PARITY_CAPTURE=1`: write the baseline; otherwise compare.
//! - `RESHIKI_DATA_DIR` (required): recreate it empty before every run.
//!   `App::new` reads recovery drafts and the template library from it.
//! - `RESHIKI_PERF_RENDERER` (optional): the headless backend.
//! - `RESHIKI_VIEW_PARITY_ONLY` (optional, development only): a case-name
//!   prefix. Compare mode refuses it, and refuses a baseline captured with it.
//!
//! On macOS the only headless renderer is wgpu/Metal; TinySkia is enabled
//! only on Windows.
use super::layout_snapshots::{
    STATES, atom, benzene, canvas_bounds, mixed, molecule, open, pass, select_all,
};
use super::{App, InspectorTab, Message, PALETTE_WIDTH};
use crate::canvas::{Edit, Tool};
use iced::advanced::Renderer as _;
use iced::advanced::widget::operation::{self, Focusable, TextInput, scrollable::AbsoluteOffset};
use iced::advanced::widget::{Id, Operation, Tree, tree};
use iced::advanced::{
    Clipboard, Layout, Shell, Widget, clipboard, layout, mouse, overlay, renderer,
    renderer::Headless,
};
use iced::keyboard::{
    self, Key, Modifiers,
    key::{Code, Named, Physical},
};
use iced::{Element, Event, Length, Point, Rectangle, Size, Theme, Vector};
use iced_runtime::{UserInterface, user_interface::Cache};
use reshiki::accessibility::{Activate, Collect};
use reshiki::document::{Arrow, Document, Point as World};
use std::{any::Any, cell::RefCell, fmt::Write as _, path::Path, rc::Rc};

type Renderer = iced::Renderer;

/// Input replayed on every tree and cache the harness creates, before anything else.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Script {
    /// Scrolls by lines at a window point; a negative `lines` scrolls down.
    Wheel { x: f32, y: f32, lines: f32 },
    /// Scrolls the scrollable with this id to an absolute vertical offset.
    ScrollTo { id: &'static str, y: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Probe {
    None,
    /// The context bar above the canvas.
    ContextBar,
    /// The inspector's scrolling content, page by page.
    Inspector,
    /// Every leaf of the window except the canvas.
    Window,
}

struct Case {
    name: String,
    setup: Box<dyn Fn(&mut App)>,
    sizes: &'static [(u32, u32)],
    probe: Probe,
    script: &'static [Script],
}

const BOTH: &[(u32, u32)] = &[(1280, 820), (1040, 680)];
const MINIMUM: &[(u32, u32)] = &[(1040, 680)];
const PALETTE_SCROLL: &[Script] = &[Script::Wheel {
    x: PALETTE_WIDTH / 2.,
    y: 400.,
    lines: -10.,
}];
const INSPECTOR: &str = "inspector-content";
/// Wheel scripts at the centre of the minimum window, over the help body.
const HELP_MID: &[Script] = &[Script::Wheel {
    x: 520.,
    y: 340.,
    lines: -15.,
}];
const HELP_END: &[Script] = &[Script::Wheel {
    x: 520.,
    y: 340.,
    lines: -200.,
}];
/// Where the right-click menu cases open, as in the `copy-as` state.
const MENU_AT: Point = Point::new(36., 24.);

fn case(
    name: impl Into<String>,
    setup: impl Fn(&mut App) + 'static,
    sizes: &'static [(u32, u32)],
    probe: Probe,
) -> Case {
    Case {
        name: name.into(),
        setup: Box::new(setup),
        sizes,
        probe,
        script: &[],
    }
}

/// Every tool, as `every_context_row_fits_the_minimum_window` lists them.
fn tools() -> Vec<Tool> {
    use reshiki::graphics::GraphicKind;
    use reshiki::{bonds::BondPreset, chains::ChainMode, rings::Preset, scientific};
    let mut tools = vec![
        Tool::Select,
        Tool::Lasso,
        Tool::Tilt,
        Tool::Wedge,
        Tool::Hash,
        Tool::Wavy,
        Tool::Atom,
        Tool::Ring,
        Tool::Template,
        Tool::Arrow,
        Tool::Text,
        Tool::Erase,
        Tool::EditPoints,
        Tool::Graphic(GraphicKind::Symbol(scientific::SymbolKind::ALL[0])),
        Tool::Graphic(GraphicKind::Orbital(scientific::OrbitalKind::ALL[0])),
    ];
    tools.extend([ChainMode::Straight, ChainMode::Snaking].map(Tool::Chain));
    tools.extend((1..=3).map(Tool::Bond));
    tools.extend(BondPreset::ALL.map(Tool::StyledBond));
    tools.extend(Preset::ALL.iter().map(|&p| Tool::RingPreset(p)));
    tools.extend(GraphicKind::DRAWABLE.map(Tool::Graphic));
    tools
}

/// `Graphic(Symbol(Plus))` becomes `Graphic-Symbol-Plus`.
fn slug(tool: Tool) -> String {
    format!("{tool:?}")
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Benzene, selected, under a cleanup preview that moves every atom by (20, 10).
fn cleanup(app: &mut App) {
    open(app, benzene());
    select_all(app);
    app.tool = Tool::Ring;
    app.tab.busy = false;
    let mut document = benzene();
    for atom in &mut document.atoms {
        atom.position = atom.position.offset(20., 10.);
    }
    app.tab.cleanup = Some(crate::app::CleanupPreview {
        job: crate::app::cleanup::CleanupJob {
            options: Default::default(),
            selection: app.tab.selected.clone(),
            serial: app.tab.cleanup_serial,
            epoch: app.tab.file_epoch,
        },
        warnings: vec![
            "Two bonds keep their crossing".into(),
            "Ring orientation follows the selection".into(),
        ],
        document,
        analysis: None,
        revision: app.tab.revision,
        epoch: app.tab.file_epoch,
        original: false,
    });
}

fn first_collection(app: &App) -> String {
    let template = app.templates.library.get(0).expect("A built-in template");
    crate::app::template_library::category(template).to_owned()
}

/// The first built-in template, chosen for placing.
fn active_template(app: &mut App) {
    app.template_index = 0;
    app.templates.active = true;
    app.tool = Tool::Template;
}

fn labels_tab(app: &mut App) {
    let _ = app.update(Message::Inspector(InspectorTab::Labels));
}

fn cases() -> Vec<Case> {
    use crate::app::template_library::{Action as T, Filter};
    let mut cases: Vec<Case> = STATES
        .into_iter()
        .map(|(name, setup)| case(name, setup, BOTH, Probe::None))
        .collect();
    // The tool, element and Help buttons and their tooltips, with no popup open.
    cases.push(case("default-window", |_| {}, BOTH, Probe::Window));
    cases.push(case("molecule-window", molecule, BOTH, Probe::Window));
    cases.push(Case {
        script: PALETTE_SCROLL,
        ..case("palette-scrolled", |_| {}, MINIMUM, Probe::Window)
    });

    // Tool context bars.
    for tool in tools() {
        cases.push(case(
            format!("tool-{}", slug(tool)),
            move |app: &mut App| {
                open(app, benzene());
                app.tool = tool;
            },
            BOTH,
            Probe::ContextBar,
        ));
    }
    let context = |name: &str, setup: fn(&mut App)| {
        case(format!("tool-{name}"), setup, BOTH, Probe::ContextBar)
    };
    cases.extend([
        context("Select-all", |app| {
            open(app, benzene());
            select_all(app);
            app.tool = Tool::Select;
        }),
        context("Lasso-all", |app| {
            open(app, benzene());
            select_all(app);
            app.tool = Tool::Lasso;
        }),
        context("Ring-aromatic", |app| {
            open(app, benzene());
            app.tool = Tool::Ring;
            app.aromatic_ring = true;
        }),
        context("Select-keyboard", |app| {
            open(app, benzene());
            app.tool = Tool::Select;
            let _ = app.update(Message::KeyboardDrawing(
                crate::app::keyboard_drawing::Action::Toggle,
            ));
        }),
        context("Select-bonded", |app| {
            atom(app);
            assert!(app.moving_bonded_selection());
        }),
        context("Tilt-all", |app| {
            open(app, benzene());
            select_all(app);
            app.tool = Tool::Tilt;
        }),
        context("Template-first", |app| {
            open(app, benzene());
            app.tool = Tool::Template;
            app.template_index = 0;
        }),
    ]);

    // The Templates tab.
    let templates = |name: &str, setup: fn(&mut App)| {
        case(
            format!("templates-{name}"),
            move |app: &mut App| {
                app.inspector_open = true;
                app.inspector_tab = InspectorTab::Templates;
                setup(app);
            },
            BOTH,
            Probe::Inspector,
        )
    };
    cases.extend([
        templates("categories", |_| {}),
        templates("query", |app| app.templates.query = "benz".into()),
        // Both the Categories button (any query) and the category list
        // (a blank trimmed query) show.
        templates("query-blank", |app| app.templates.query = "   ".into()),
        templates("favorites", |app| app.templates.filter = Filter::Favorites),
        templates("collection", |app| {
            app.templates.collection = first_collection(app);
        }),
        templates("forward", |app| {
            let collection = first_collection(app);
            let _ = app.update(Message::Templates(T::Collection(collection)));
            let _ = app.update(Message::Templates(T::Browse));
            assert!(app.templates.can_forward());
        }),
        templates("new", |app| {
            app.templates.editing = true;
            app.templates.draft = Some(benzene());
        }),
        templates("edit", |app| app.templates.editing = true),
        templates("notice", |app| {
            app.templates.notice = Some("The template library could not be saved".into());
        }),
        templates("undo", |app| {
            app.templates.undo = Some(app.templates.library.clone());
        }),
        templates("active-repeat", |app| {
            active_template(app);
            app.templates.repeat = true;
        }),
        templates("active-once", |app| {
            active_template(app);
            app.templates.repeat = false;
        }),
        templates("active-user", |app| {
            let mut doc = Document::default();
            doc.add_atom("O", World::default());
            let index = app
                .templates
                .library
                .add(
                    "Hydroxyl",
                    "My templates",
                    doc,
                    reshiki::templates::Anchor::Auto,
                )
                .unwrap();
            app.template_index = index;
            app.templates.active = true;
            app.tool = Tool::Template;
        }),
    ]);

    // The Labels tab, always opened through its message.
    let labels = |name: &str, setup: fn(&mut App)| {
        case(
            format!("labels-{name}"),
            move |app: &mut App| {
                open(app, benzene());
                setup(app);
            },
            BOTH,
            Probe::Inspector,
        )
    };
    cases.extend([
        labels("none", labels_tab),
        labels("one", |app| {
            let id = app.tab.doc.atoms[0].id;
            let _ = app.update(Message::Canvas(Edit::Select(vec![id])));
            labels_tab(app);
            assert_eq!(app.label_ids().len(), 1);
        }),
        labels("all", |app| {
            select_all(app);
            labels_tab(app);
        }),
        labels("empty-scope", |app| {
            use crate::app::atom_labels::{Action, Scope};
            labels_tab(app);
            let _ = app.update(Message::Labels(Action::Scope(Scope::Selection)));
            assert!(app.label_ids().is_empty());
        }),
        labels("notice", |app| {
            app.tab.chemistry_notice = Some("Stereo labels are out of date".into());
            labels_tab(app);
        }),
    ]);

    // The cleanup bar, and the canvas inputs of other sessions.
    let preview = |name: &str, setup: fn(&mut App)| {
        case(
            format!("cleanup-{name}"),
            move |app: &mut App| {
                cleanup(app);
                setup(app);
            },
            BOTH,
            Probe::ContextBar,
        )
    };
    cases.extend([
        preview("current", |_| {}),
        preview("busy", |app| app.tab.busy = true),
        preview("original", |app| {
            if let Some(preview) = &mut app.tab.cleanup {
                preview.original = true;
            }
        }),
        preview("stale", |app| {
            if let Some(preview) = &mut app.tab.cleanup {
                preview.revision += 1;
            }
        }),
        // Stays in Preparing: the worker task is dropped.
        case(
            "optimization",
            |app: &mut App| {
                open(app, benzene());
                select_all(app);
                let _ = app.update(Message::Optimization(
                    crate::app::optimization::Action::Begin,
                ));
                assert!(app.tab.optimization.is_some());
            },
            BOTH,
            Probe::None,
        ),
        case(
            "joining",
            |app: &mut App| {
                use crate::app::joining::Action;
                app.tab.busy = false;
                app.tab.doc = Document::default();
                app.tab.doc.add_atom("C", World::default());
                let source = app.tab.doc.add_atom("C", World::new(180., 0.));
                let end = app.tab.doc.add_atom("C", World::new(222., 0.));
                app.tab.doc.add_bond(source, end, 1, "plain");
                app.tab.selected = vec![source];
                let _ = app.update(Message::Join(Action::Begin));
                let _ = app.update(Message::Join(Action::Mode(
                    reshiki::templates::Connection::ShareAtom,
                )));
                assert!(app.tab.joining.is_some());
            },
            BOTH,
            Probe::None,
        ),
        case(
            "inline-annotation",
            |app: &mut App| {
                use reshiki::document::Annotation;
                use reshiki::typography::TextFormat;
                app.tab.busy = false;
                app.tab.doc = Document::default();
                app.tab.doc.annotations = vec![
                    Annotation {
                        id: 1,
                        position: World::new(20., 30.),
                        text: "First label".into(),
                        format: TextFormat {
                            width_pt: Some(120.),
                            line_spacing: 1.0,
                            ..Default::default()
                        },
                    },
                    Annotation {
                        id: 2,
                        position: World::new(200., 30.),
                        text: "Other label".into(),
                        format: Default::default(),
                    },
                ];
                app.tab.saved = app.tab.doc.clone();
                app.tab.selected = vec![1];
                app.sync_typography();
                let _ = app.update(Message::InlineText(crate::app::inline_text::Action::Begin(
                    Some(1),
                    World::default(),
                )));
                assert!(app.tab.inline_text.is_some());
            },
            BOTH,
            Probe::None,
        ),
    ]);
    cases.extend(palette_cases());
    cases.extend(help_cases());
    cases.extend(menu_cases());
    cases.extend(selection_cases());
    cases
}

/// Every toolbar flyout, and flyouts with a variant active.
fn palette_cases() -> Vec<Case> {
    use crate::app::palettes::{Action, Family, GraphicOption};
    use reshiki::arrows::{ArrowStyle, Preset as ArrowPreset};
    use reshiki::graphics::{BracketSides, GraphicKind, LinePattern};
    use reshiki::{rings::Preset, scientific::SymbolKind};
    let palette = |name: &str, family: Family, setup: fn(&mut App)| {
        case(
            format!("palette-{name}"),
            move |app: &mut App| {
                setup(app);
                app.palette = Some(family);
            },
            MINIMUM,
            Probe::Window,
        )
    };
    let mut cases: Vec<Case> = [
        Family::Atoms,
        Family::Bonds,
        Family::Rings,
        Family::Arrows,
        Family::Rectangles,
        Family::Ellipses,
        Family::Brackets,
        Family::Symbols,
        Family::Orbitals,
    ]
    .into_iter()
    .map(|family| palette(&format!("{family:?}"), family, |_| {}))
    .collect();
    // Choosing an option closes the flyout, so each variant reopens it.
    cases.extend([
        palette("Bonds-Wedge", Family::Bonds, |app| app.tool = Tool::Wedge),
        palette("Rings-Benzene", Family::Rings, |app| {
            app.tool = Tool::RingPreset(Preset::Benzene);
        }),
        palette("Rings-aromatic", Family::Rings, |app| {
            app.tool = Tool::Ring;
            app.ring_size = 6;
            app.aromatic_ring = true;
        }),
        palette("Arrows-Dashed", Family::Arrows, |app| {
            let dashed = ArrowStyle {
                pattern: LinePattern::Dashed,
                ..ArrowStyle::default()
            };
            let _ = app.update(Message::Palette(Action::ArrowVariant(
                ArrowPreset::Forward,
                dashed,
            )));
        }),
        palette("Brackets-Left", Family::Brackets, |app| {
            // The second option of the private `graphic_options(Family::Brackets)`.
            let option = GraphicOption {
                kind: GraphicKind::Brackets,
                style: Default::default(),
                sides: BracketSides::Left,
                constrain: false,
            };
            let _ = app.update(Message::Palette(Action::Graphic(option)));
        }),
        palette("Symbols-second", Family::Symbols, |app| {
            app.tool = Tool::Graphic(GraphicKind::Symbol(SymbolKind::ALL[1]));
        }),
    ]);
    cases
}

/// Help, at the top, scrolled part way and scrolled to the end. The STATES
/// case `help` already covers both sizes without probes.
fn help_cases() -> Vec<Case> {
    let help = |name: &str, script: &'static [Script]| Case {
        script,
        ..case(
            format!("help-{name}"),
            |app: &mut App| app.help_open = true,
            MINIMUM,
            Probe::Window,
        )
    };
    vec![
        help("window", &[]),
        help("scrolled-mid", HELP_MID),
        help("scrolled-end", HELP_END),
    ]
}

/// Benzene with a filled ring, all selected.
fn ring_fill(app: &mut App) {
    use reshiki::palette::{Color, Hue, Row};
    let mut doc = benzene();
    let ids = doc.all_ids();
    let fill = Some(Color::Palette(Hue::Blue, Row::Strong));
    assert_eq!(reshiki::ring_fills::apply(&mut doc, &ids, fill), 1);
    open(app, doc);
    select_all(app);
}

/// Right-click menus and context row menus; the cascade tests own their input.
fn menu_cases() -> Vec<Case> {
    use crate::app::context_menu::{Action, Page, State};
    let menu =
        |name: &str, setup: fn(&mut App)| case(format!("menu-{name}"), setup, BOTH, Probe::None);
    // The right-click menu with one submenu open, as its row opens it.
    fn submenu(app: &mut App, page: Page) {
        app.context_menu = Some(State::new(MENU_AT, Page::Main));
        let _ = app.context_action(Action::Page(page));
    }
    fn row(app: &mut App, page: Page) {
        mixed(app);
        let _ = app.context_action(Action::Open(page, 200.));
        assert!(app.context_menu.is_some());
    }
    vec![
        menu("main-empty", |app| {
            open(app, benzene());
            app.context_menu = Some(State::new(MENU_AT, Page::Main));
        }),
        menu("main-selected", |app| {
            molecule(app);
            app.context_menu = Some(State::new(MENU_AT, Page::Main));
        }),
        menu("align", |app| {
            mixed(app);
            assert!(app.alignment_count() >= 2);
            submenu(app, Page::Align);
        }),
        menu("bonds-ring-fill", |app| {
            ring_fill(app);
            submenu(app, Page::Bonds);
        }),
        menu("tilt", |app| {
            molecule(app);
            submenu(app, Page::Tilt);
        }),
        menu("attachments", |app| {
            molecule(app);
            submenu(app, Page::Attachments);
        }),
        menu("row-AlignObjects", |app| row(app, Page::AlignObjects)),
        menu("row-Distribute", |app| row(app, Page::Distribute)),
        menu("row-Order", |app| row(app, Page::Order)),
        menu("row-Arrange", |app| row(app, Page::Arrange)),
        menu("row-More", |app| {
            mixed(app);
            let folded = app.context_commands().len();
            row(app, Page::More(folded));
        }),
    ]
}

/// Unbonded atoms carrying every kind of positioned mark, all selected.
fn marks(app: &mut App) {
    use reshiki::scientific::{SymbolKind as S, attach};
    let mut doc = Document::default();
    for (element, x, kinds) in [
        ("C", 0., &[S::Radical, S::LonePair, S::LonePairBar][..]),
        ("O", 60., &[S::CirclePlus]),
        ("N", 120., &[S::RadicalAnion]),
        ("S", 180., &[S::Minus]),
    ] {
        let id = doc.add_atom(element, World::new(x, 0.));
        for (i, &kind) in kinds.iter().enumerate() {
            let atom = doc.atom_mut(id).unwrap();
            attach(atom, kind, World::new(10., -10. + 8. * i as f32)).unwrap();
        }
    }
    open(app, doc);
    select_all(app);
}

/// The Properties panel for each kind of selection.
fn selection_cases() -> Vec<Case> {
    use crate::app::inspector::{Action, Section};
    let selection = |name: &str, setup: fn(&mut App)| {
        case(
            format!("selection-{name}"),
            move |app: &mut App| {
                setup(app);
                app.inspector_open = true;
                app.inspector_tab = InspectorTab::Properties;
            },
            BOTH,
            Probe::Inspector,
        )
    };
    fn section(app: &mut App, section: Section) {
        let _ = app.update(Message::InspectorAction(Action::Section(section, true)));
    }
    vec![
        selection("double-bond", |app| {
            let mut doc = Document::default();
            let a = doc.add_atom("C", World::default());
            let b = doc.add_atom("C", World::new(42., 0.));
            doc.add_bond(a, b, 2, "plain");
            open(app, doc);
            select_all(app);
        }),
        selection("chain", |app| {
            let mut doc = Document::default();
            let a = doc.add_atom("C", World::default());
            let b = doc.add_atom("C", World::new(36., 21.));
            let c = doc.add_atom("C", World::new(72., 0.));
            doc.add_bond(a, b, 1, "plain");
            doc.add_bond(b, c, 1, "plain");
            open(app, doc);
            select_all(app);
        }),
        selection("ring", molecule),
        selection("marks-select", |app| {
            marks(app);
            app.tool = Tool::Select;
        }),
        selection("marks-edit-points", |app| {
            marks(app);
            app.tool = Tool::EditPoints;
        }),
        selection("radicals-mixed", |app| {
            let mut doc = Document::default();
            let a = doc.add_atom("C", World::default());
            doc.add_atom("C", World::new(60., 0.));
            doc.atom_mut(a).unwrap().radical_electrons = 1;
            open(app, doc);
            select_all(app);
        }),
        selection("atom-text", |app| {
            open(app, benzene());
            let id = app.tab.doc.atoms[0].id;
            let _ = app.update(Message::Canvas(Edit::Select(vec![id])));
            assert!(app.atom_text_target().is_some());
        }),
        selection("group-integral", |app| {
            molecule(app);
            let _ = app.update(Message::Group);
            let _ = app.update(Message::IntegralGroup(true));
            section(app, Section::Groups);
            assert!(app.tab.doc.groups.iter().any(|group| group.integral));
        }),
        selection("arrow", |app| {
            let mut doc = Document::default();
            doc.arrows.push(Arrow::new(
                doc.next_id(),
                World::default(),
                World::new(90., 0.),
                Default::default(),
                Default::default(),
            ));
            open(app, doc);
            select_all(app);
        }),
        selection("bonds-atoms", |app| {
            molecule(app);
            section(app, Section::Bonds);
            section(app, Section::Atoms);
        }),
        // Crossings & direction is collapsed by default.
        selection("crossings", |app| {
            open(app, benzene());
            let bond = &app.tab.doc.bonds[0];
            let ids = vec![bond.a, bond.b];
            let _ = app.update(Message::Canvas(Edit::Select(ids)));
            section(app, Section::BondDirection);
        }),
    ]
}

fn rect(bounds: Rectangle) -> String {
    format!(
        "{:?},{:?},{:?},{:?}",
        bounds.x, bounds.y, bounds.width, bounds.height
    )
}

/// Every node, depth first, with its depth.
fn nodes(layout: Layout<'_>, depth: usize, found: &mut Vec<(usize, Rectangle)>) {
    found.push((depth, layout.bounds()));
    for child in layout.children() {
        nodes(child, depth + 1, found);
    }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn wheel(lines: f32) -> Event {
    Event::Mouse(mouse::Event::WheelScrolled {
        delta: mouse::ScrollDelta::Lines { x: 0., y: lines },
    })
}

fn scroll_to(id: &'static str, y: f32) -> impl Operation {
    operation::scrollable::scroll_to(
        Id::new(id),
        AbsoluteOffset {
            x: Some(0.),
            y: Some(y),
        },
    )
}

fn click(point: Point) -> [(&'static str, Event); 3] {
    [
        (
            "move",
            Event::Mouse(mouse::Event::CursorMoved { position: point }),
        ),
        (
            "press",
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        ),
        (
            "release",
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        ),
    ]
}

/// As `press` in shortcut_focus_tests.
fn press(key: Key, code: Code, text: Option<&str>) -> Event {
    Event::Keyboard(keyboard::Event::KeyPressed {
        modified_key: key.clone(),
        key,
        physical_key: Physical::Code(code),
        location: keyboard::Location::Standard,
        modifiers: Modifiers::empty(),
        text: text.map(Into::into),
        repeat: false,
    })
}

fn keys() -> [(&'static str, Event); 2] {
    [
        (
            "7",
            press(Key::Character("7".into()), Code::Digit7, Some("7")),
        ),
        ("enter", press(Key::Named(Named::Enter), Code::Enter, None)),
    ]
}

/// A scrollable as its operation reports it.
#[derive(Clone)]
struct Scroll {
    id: Option<Id>,
    bounds: Rectangle,
    content: Rectangle,
    translation: Vector,
}

/// Records every operation call, prefixed with its traverse depth.
#[derive(Default)]
struct Record {
    depth: usize,
    lines: String,
    scrolls: Vec<Scroll>,
}

impl Operation for Record {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        self.depth += 1;
        operate(self);
        self.depth -= 1;
    }
    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        let _ = writeln!(
            self.lines,
            "OP,{},container,{id:?},{}",
            self.depth,
            rect(bounds)
        );
    }
    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        content: Rectangle,
        translation: Vector,
        _: &mut dyn operation::Scrollable,
    ) {
        let _ = writeln!(
            self.lines,
            "OP,{},scrollable,{id:?},{},{},{:?},{:?}",
            self.depth,
            rect(bounds),
            rect(content),
            translation.x,
            translation.y
        );
        self.scrolls.push(Scroll {
            id: id.cloned(),
            bounds,
            content,
            translation,
        });
    }
    fn focusable(&mut self, id: Option<&Id>, bounds: Rectangle, state: &mut dyn Focusable) {
        let _ = writeln!(
            self.lines,
            "OP,{},focusable,{id:?},{},{}",
            self.depth,
            rect(bounds),
            state.is_focused()
        );
    }
    fn text_input(&mut self, id: Option<&Id>, bounds: Rectangle, state: &mut dyn TextInput) {
        let _ = writeln!(
            self.lines,
            "OP,{},text_input,{id:?},{},{:?}",
            self.depth,
            rect(bounds),
            state.text()
        );
    }
    fn text(&mut self, id: Option<&Id>, bounds: Rectangle, text: &str) {
        let _ = writeln!(
            self.lines,
            "OP,{},text,{id:?},{},{text:?}",
            self.depth,
            rect(bounds)
        );
    }
    fn custom(&mut self, id: Option<&Id>, bounds: Rectangle, _: &mut dyn Any) {
        let _ = writeln!(
            self.lines,
            "OP,{},custom,{id:?},{}",
            self.depth,
            rect(bounds)
        );
    }
}

/// Visits nothing; operating still lays out the runtime's overlay.
struct Noop;

impl Operation for Noop {
    fn traverse(&mut self, _: &mut dyn FnMut(&mut dyn Operation)) {}
}

/// The nodes of each nested overlay level, as the runtime last laid them out.
type Sink = Rc<RefCell<Vec<Vec<(usize, Rectangle)>>>>;

/// A transparent root: the same tree, layout, events and drawing as the view,
/// with every overlay level reporting its layout to the sink.
struct Spy<'a> {
    inner: Element<'a, Message>,
    sink: Sink,
}

impl Widget<Message, Theme, Renderer> for Spy<'_> {
    fn size(&self) -> Size<Length> {
        self.inner.as_widget().size()
    }
    fn size_hint(&self) -> Size<Length> {
        self.inner.as_widget().size_hint()
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.inner.as_widget_mut().layout(tree, renderer, limits)
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.inner
            .as_widget()
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }
    fn tag(&self) -> tree::Tag {
        self.inner.as_widget().tag()
    }
    fn state(&self) -> tree::State {
        self.inner.as_widget().state()
    }
    fn children(&self) -> Vec<Tree> {
        self.inner.as_widget().children()
    }
    fn diff(&self, tree: &mut Tree) {
        self.inner.as_widget().diff(tree);
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.inner
            .as_widget_mut()
            .operate(tree, layout, renderer, operation);
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.inner.as_widget_mut().update(
            tree, event, layout, cursor, renderer, clipboard, shell, viewport,
        );
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.inner
            .as_widget()
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let sink = self.sink.clone();
        self.inner
            .as_widget_mut()
            .overlay(tree, layout, renderer, viewport, translation)
            .map(|inner| {
                overlay::Element::new(Box::new(Recorder {
                    inner,
                    sink,
                    depth: 0,
                }))
            })
    }
}

/// Delegates to an overlay and records the layout of its nested level.
struct Recorder<'a> {
    inner: overlay::Element<'a, Message, Theme, Renderer>,
    sink: Sink,
    depth: usize,
}

impl overlay::Overlay<Message, Theme, Renderer> for Recorder<'_> {
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        let node = self.inner.as_overlay_mut().layout(renderer, bounds);
        let mut found = Vec::new();
        nodes(Layout::new(&node), 0, &mut found);
        let mut sink = self.sink.borrow_mut();
        sink.truncate(self.depth);
        sink.push(found);
        node
    }
    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        self.inner
            .as_overlay()
            .draw(renderer, theme, style, layout, cursor);
    }
    fn operate(&mut self, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn Operation) {
        self.inner
            .as_overlay_mut()
            .operate(layout, renderer, operation);
    }
    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        self.inner
            .as_overlay_mut()
            .update(event, layout, cursor, renderer, clipboard, shell);
    }
    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.inner
            .as_overlay()
            .mouse_interaction(layout, cursor, renderer)
    }
    fn overlay<'b>(
        &'b mut self,
        layout: Layout<'b>,
        renderer: &Renderer,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let (sink, depth) = (self.sink.clone(), self.depth + 1);
        self.inner
            .as_overlay_mut()
            .overlay(layout, renderer)
            .map(|inner| overlay::Element::new(Box::new(Recorder { inner, sink, depth })))
    }
    fn index(&self) -> f32 {
        self.inner.as_overlay().index()
    }
}

/// The outermost overlay nodes smaller than the window: popups rather than
/// the window-sized groups holding them.
fn popups(levels: &[Vec<(usize, Rectangle)>], window: Rectangle) -> Vec<Rectangle> {
    let mut found = Vec::new();
    for level in levels {
        let mut taken: Option<usize> = None;
        for &(depth, bounds) in level {
            if taken.is_some_and(|taken| depth > taken) {
                continue;
            }
            taken = None;
            if bounds != window {
                found.push(bounds);
                taken = Some(depth);
            }
        }
    }
    found
}

/// The base and overlay layouts and operation records of a manual tree.
struct Frame {
    base: layout::Node,
    overlay: Option<layout::Node>,
    record: Record,
}

#[derive(Clone)]
struct Target {
    point: Point,
    script: Vec<Script>,
}

/// Walks the layout for probe targets, translating and clipping through
/// scrollables as `reshiki::accessibility::Collect` does.
struct Walk<'a> {
    scrolls: &'a [Scroll],
    probe: Probe,
    canvas: Option<Rectangle>,
    context: Option<Vec<usize>>,
    /// Untranslated centres and window points.
    leaves: Vec<(Point, Point)>,
}

impl Walk<'_> {
    fn visit(
        &mut self,
        layout: Layout<'_>,
        path: &mut Vec<usize>,
        translation: Vector,
        clip: Option<Rectangle>,
        region: Option<Rectangle>,
    ) {
        let bounds = layout.bounds();
        let region = if self.context.as_deref() == Some(path.as_slice()) {
            Some(bounds + translation)
        } else {
            region
        };
        let first = layout.children().next();
        let scroll = self.scrolls.iter().find(|scroll| {
            scroll.bounds == bounds && first.is_some_and(|child| child.bounds() == scroll.content)
        });
        let (translation, clip, region) = if let Some(scroll) = scroll {
            let visible = bounds + translation;
            let region = if self.probe == Probe::Inspector && scroll.id == Some(Id::new(INSPECTOR))
            {
                Some(visible)
            } else {
                region
            };
            (
                translation - scroll.translation,
                clip.and_then(|clip| clip.intersection(&visible)),
                region,
            )
        } else {
            (translation, clip, region)
        };
        if first.is_none() {
            let point = bounds.center() + translation;
            if bounds.width > 0.
                && bounds.height > 0.
                && Some(bounds) != self.canvas
                && region.is_some_and(|region| region.contains(point))
                && clip.is_some_and(|clip| clip.contains(point))
            {
                self.leaves.push((bounds.center(), point));
            }
            return;
        }
        for (index, child) in layout.children().enumerate() {
            path.push(index);
            self.visit(child, path, translation, clip, region);
            path.pop();
        }
    }
}

/// The path to the context bar: the node right before the sibling holding the
/// canvas, with the same x and width, as `column![background(context), paper]`
/// lays them out. The outer column of style bar and body matches the same
/// rule, so the deepest match is taken.
fn context_bar(layout: Layout<'_>, canvas: Size) -> Option<Vec<usize>> {
    let mut path = Vec::new();
    let mut found = None;
    let mut layout = layout;
    loop {
        let children: Vec<_> = layout.children().collect();
        let Some(index) = children
            .iter()
            .position(|child| canvas_bounds(*child, canvas).is_some())
        else {
            return found;
        };
        if index > 0 {
            let (above, below) = (children[index - 1].bounds(), children[index].bounds());
            if above.x == below.x && above.width == below.width {
                let mut context = path.clone();
                context.push(index - 1);
                found = Some(context);
            }
        }
        if children[index].children().next().is_none() {
            return found;
        }
        path.push(index);
        layout = children[index];
    }
}

/// Drives one settled app at one window size. The manual tree (`pass`) gives
/// layout dumps and probe targets; the runtime interface delivers every event
/// and operation in the runtime's overlay-before-base order.
struct Driver<'a> {
    app: &'a App,
    size: Size,
    sink: Sink,
}

impl<'a> Driver<'a> {
    fn viewport(&self) -> Rectangle {
        Rectangle::with_size(self.size)
    }

    fn limits(&self) -> layout::Limits {
        layout::Limits::new(self.size, self.size)
    }

    /// Delivers one event to a manual tree: the overlay first, then the base
    /// unless the overlay captured it.
    fn deliver(
        &self,
        renderer: &Renderer,
        tree: &mut Tree,
        event: &Event,
        cursor: mouse::Cursor,
    ) -> Vec<Message> {
        let viewport = self.viewport();
        let mut messages = Vec::new();
        let mut view = self.app.view();
        tree.diff(view.as_widget());
        let node = view.as_widget_mut().layout(tree, renderer, &self.limits());
        let mut base_cursor = cursor;
        let mut captured = false;
        if let Some(element) = view.as_widget_mut().overlay(
            tree,
            Layout::new(&node),
            renderer,
            &viewport,
            Vector::ZERO,
        ) {
            let mut nested = overlay::Nested::new(element);
            let layout = nested.layout(renderer, self.size);
            let mut shell = Shell::new(&mut messages);
            nested.update(
                event,
                Layout::new(&layout),
                cursor,
                renderer,
                &mut clipboard::Null,
                &mut shell,
            );
            captured = shell.event_status() == iced::event::Status::Captured;
            if let Some(position) = cursor.position()
                && nested.mouse_interaction(
                    Layout::new(&layout),
                    mouse::Cursor::Available(position),
                    renderer,
                ) != mouse::Interaction::None
            {
                base_cursor = mouse::Cursor::Unavailable;
            }
        }
        if !captured {
            view.as_widget_mut().update(
                tree,
                event,
                Layout::new(&node),
                base_cursor,
                renderer,
                &mut clipboard::Null,
                &mut Shell::new(&mut messages),
                &viewport,
            );
        }
        messages
    }

    /// Operates on the base, then the nested overlay, as the runtime does;
    /// returns their layouts.
    fn operate(
        &self,
        renderer: &Renderer,
        tree: &mut Tree,
        operation: &mut dyn Operation,
    ) -> (layout::Node, Option<layout::Node>) {
        let mut view = self.app.view();
        tree.diff(view.as_widget());
        let base = view.as_widget_mut().layout(tree, renderer, &self.limits());
        view.as_widget_mut()
            .operate(tree, Layout::new(&base), renderer, operation);
        let overlay = view
            .as_widget_mut()
            .overlay(
                tree,
                Layout::new(&base),
                renderer,
                &self.viewport(),
                Vector::ZERO,
            )
            .map(|element| {
                let mut nested = overlay::Nested::new(element);
                let node = nested.layout(renderer, self.size);
                nested.operate(Layout::new(&node), renderer, operation);
                node
            });
        (base, overlay)
    }

    fn frame(&self, renderer: &Renderer, tree: &mut Tree) -> Frame {
        let mut record = Record::default();
        let (base, overlay) = self.operate(renderer, tree, &mut record);
        Frame {
            base,
            overlay,
            record,
        }
    }

    fn replay_tree(
        &self,
        renderer: &Renderer,
        tree: &mut Tree,
        script: &[Script],
        out: &mut String,
    ) {
        for step in script {
            match *step {
                Script::Wheel { x, y, lines } => {
                    let cursor = mouse::Cursor::Available(Point::new(x, y));
                    for message in self.deliver(renderer, tree, &wheel(lines), cursor) {
                        let _ = writeln!(out, "SCRIPT,{message:?}");
                    }
                }
                Script::ScrollTo { id, y } => {
                    let _ = self.operate(renderer, tree, &mut scroll_to(id, y));
                }
            }
        }
    }

    fn interface(
        &self,
        renderer: &mut Renderer,
        cache: Cache,
    ) -> UserInterface<'a, Message, Theme, Renderer> {
        let spy = Spy {
            inner: self.app.view(),
            sink: self.sink.clone(),
        };
        UserInterface::build(Element::new(spy), self.size, cache, renderer)
    }

    fn send(
        &self,
        renderer: &mut Renderer,
        cache: Cache,
        event: Event,
        cursor: mouse::Cursor,
    ) -> (Cache, Vec<Message>) {
        let mut ui = self.interface(renderer, cache);
        let mut messages = Vec::new();
        let _ = ui.update(
            std::slice::from_ref(&event),
            cursor,
            renderer,
            &mut clipboard::Null,
            &mut messages,
        );
        (ui.into_cache(), messages)
    }

    fn operate_cache(
        &self,
        renderer: &mut Renderer,
        cache: Cache,
        operation: &mut dyn Operation,
    ) -> Cache {
        let mut ui = self.interface(renderer, cache);
        ui.operate(renderer, operation);
        ui.into_cache()
    }

    /// A fresh runtime cache with the script replayed, and its messages.
    fn replay_cache(&self, renderer: &mut Renderer, script: &[Script]) -> (Cache, Vec<Message>) {
        let mut cache = Cache::new();
        let mut messages = Vec::new();
        for step in script {
            cache = match *step {
                Script::Wheel { x, y, lines } => {
                    let cursor = mouse::Cursor::Available(Point::new(x, y));
                    let (cache, emitted) = self.send(renderer, cache, wheel(lines), cursor);
                    messages.extend(emitted);
                    cache
                }
                Script::ScrollTo { id, y } => {
                    self.operate_cache(renderer, cache, &mut scroll_to(id, y))
                }
            };
        }
        (cache, messages)
    }

    /// The popups of the runtime's overlay.
    fn popups(&self, renderer: &mut Renderer, cache: Cache) -> (Cache, Vec<Rectangle>) {
        self.sink.borrow_mut().clear();
        let cache = self.operate_cache(renderer, cache, &mut Noop);
        let levels = self.sink.borrow().clone();
        (cache, popups(&levels, self.viewport()))
    }

    /// The minimum allocation count and bytes of 5 warm view, diff and layout runs.
    fn allocations(&self, renderer: &Renderer, tree: &mut Tree) -> (usize, usize) {
        let viewport = self.viewport();
        let mut run = || {
            let mut view = self.app.view();
            tree.diff(view.as_widget());
            let node = view.as_widget_mut().layout(tree, renderer, &self.limits());
            if let Some(element) = view.as_widget_mut().overlay(
                tree,
                Layout::new(&node),
                renderer,
                &viewport,
                Vector::ZERO,
            ) {
                let _ = overlay::Nested::new(element).layout(renderer, self.size);
            }
            crate::allocation_metrics::snapshot()
        };
        let _ = run();
        let mut minimum = (usize::MAX, usize::MAX);
        for _ in 0..5 {
            crate::allocation_metrics::reset();
            let measured = run();
            minimum = (
                minimum.0.min(measured.allocation_count),
                minimum.1.min(measured.allocated_bytes),
            );
        }
        minimum
    }

    /// Draws the base and the overlay, as `pass` does, and reads the pixels back.
    fn screenshot(
        &self,
        renderer: &mut Renderer,
        tree: &mut Tree,
        cursor: mouse::Cursor,
    ) -> Vec<u8> {
        let viewport = self.viewport();
        let theme = self.app.theme();
        let style = renderer::Style {
            text_color: theme.palette().text,
        };
        let mut view = self.app.view();
        tree.diff(view.as_widget());
        let node = view.as_widget_mut().layout(tree, renderer, &self.limits());
        renderer.reset(viewport);
        view.as_widget().draw(
            tree,
            renderer,
            &theme,
            &style,
            Layout::new(&node),
            cursor,
            &viewport,
        );
        if let Some(element) = view.as_widget_mut().overlay(
            tree,
            Layout::new(&node),
            renderer,
            &viewport,
            Vector::ZERO,
        ) {
            let mut nested = overlay::Nested::new(element);
            let node = nested.layout(renderer, self.size);
            nested.draw(renderer, &theme, &style, Layout::new(&node), cursor);
        }
        Headless::screenshot(
            renderer,
            Size::new(self.size.width as u32, self.size.height as u32),
            1.,
            theme.palette().background,
        )
    }

    /// The probe targets of a frame not already probed at another offset.
    fn targets(
        &self,
        frame: &Frame,
        probe: Probe,
        script: &[Script],
        probed: &mut Vec<Point>,
        targets: &mut Vec<Target>,
    ) {
        let base = Layout::new(&frame.base);
        let window = self.viewport();
        let mut walk = Walk {
            scrolls: &frame.record.scrolls,
            probe,
            canvas: canvas_bounds(base, self.app.viewport),
            context: if probe == Probe::ContextBar {
                context_bar(base, self.app.viewport)
            } else {
                None
            },
            leaves: Vec::new(),
        };
        let region = (probe == Probe::Window).then_some(window);
        walk.visit(base, &mut Vec::new(), Vector::ZERO, Some(window), region);
        walk.context = None;
        if let Some(overlay) = &frame.overlay {
            walk.visit(
                Layout::new(overlay),
                &mut Vec::new(),
                Vector::ZERO,
                Some(window),
                region,
            );
        }
        let mut points = Vec::new();
        let mut centres = Vec::new();
        for (centre, point) in walk.leaves {
            if probed.contains(&centre) {
                continue;
            }
            centres.push(centre);
            if points.contains(&point) {
                continue;
            }
            points.push(point);
            targets.push(Target {
                point,
                script: script.to_vec(),
            });
        }
        probed.extend(centres);
    }

    /// Probes every target: tooltips, clicks, menus the clicks open, and keys.
    fn probes(
        &self,
        renderer: &mut Renderer,
        tree: &mut Tree,
        case: &Case,
        frame: &Frame,
    ) -> String {
        let mut out = String::new();
        let mut targets = Vec::new();
        let mut probed = Vec::new();
        self.targets(frame, case.probe, case.script, &mut probed, &mut targets);
        // Page through the inspector, at multiples of its viewport height.
        let inspector = frame
            .record
            .scrolls
            .iter()
            .find(|scroll| scroll.id == Some(Id::new(INSPECTOR)));
        if case.probe == Probe::Inspector
            && let Some(scroll) = inspector
            && scroll.bounds.height > 0.
        {
            let page = scroll.bounds.height;
            let last = (scroll.content.height - page).max(0.);
            let mut offset = 0.;
            let mut k = 1.;
            while offset < last {
                offset = (k * page).min(last);
                k += 1.;
                let step = Script::ScrollTo {
                    id: INSPECTOR,
                    y: offset,
                };
                let _ = self.operate(renderer, tree, &mut scroll_to(INSPECTOR, offset));
                let frame = self.frame(renderer, tree);
                let mut script = case.script.to_vec();
                script.push(step);
                self.targets(&frame, case.probe, &script, &mut probed, &mut targets);
            }
        }
        for (i, target) in targets.iter().enumerate() {
            let _ = writeln!(
                out,
                "TARGET,{i},{:?},{:?},{:?}",
                target.point.x, target.point.y, target.script
            );
        }

        // TIP: hover every target on its own tree, wait once for the hint
        // delay (500 ms of wall-clock time), then redraw each tree.
        let mut tips = Vec::with_capacity(targets.len());
        for (i, target) in targets.iter().enumerate() {
            let mut tree = Tree::empty();
            self.replay_tree(renderer, &mut tree, &target.script, &mut out);
            let moved = Event::Mouse(mouse::Event::CursorMoved {
                position: target.point,
            });
            let cursor = mouse::Cursor::Available(target.point);
            let messages = self.deliver(renderer, &mut tree, &moved, cursor);
            let _ = writeln!(out, "HOVER,{i},{messages:?}");
            tips.push(tree);
        }
        std::thread::sleep(std::time::Duration::from_millis(600));
        for (i, (target, mut tree)) in targets.iter().zip(tips).enumerate() {
            let cursor = mouse::Cursor::Available(target.point);
            let redraw = Event::Window(iced::window::Event::RedrawRequested(
                std::time::Instant::now(),
            ));
            let messages = self.deliver(renderer, &mut tree, &redraw, cursor);
            let _ = writeln!(out, "TIP,{i},{messages:?}");
            let (_, overlay) = self.operate(renderer, &mut tree, &mut Noop);
            if let Some(overlay) = overlay {
                let mut found = Vec::new();
                nodes(Layout::new(&overlay), 0, &mut found);
                for (depth, bounds) in found {
                    let _ = writeln!(out, "TIP-OVERLAY,{i},{depth},{}", rect(bounds));
                }
                let pixels = self.screenshot(renderer, &mut tree, cursor);
                let _ = writeln!(out, "TIP-HASH,{i},{:016x}", fnv1a(&pixels));
            }
        }

        // The popups before any click, once per script.
        let mut before: Vec<(Vec<Script>, Vec<Rectangle>)> = Vec::new();
        for (i, target) in targets.iter().enumerate() {
            if !before.iter().any(|(script, _)| *script == target.script) {
                let (cache, messages) = self.replay_cache(renderer, &target.script);
                for message in messages {
                    let _ = writeln!(out, "SCRIPT,{message:?}");
                }
                let (_, popups) = self.popups(renderer, cache);
                for popup in &popups {
                    let _ = writeln!(out, "BEFORE,{i},{}", rect(*popup));
                }
                before.push((target.script.clone(), popups));
            }
        }

        for (i, target) in targets.iter().enumerate() {
            let cursor = mouse::Cursor::Available(target.point);
            // CLICK
            let (mut cache, messages) = self.replay_cache(renderer, &target.script);
            for message in messages {
                let _ = writeln!(out, "SCRIPT,{message:?}");
            }
            for (name, event) in click(target.point) {
                let (next, messages) = self.send(renderer, cache, event, cursor);
                cache = next;
                let _ = writeln!(out, "CLICK,{i},{name},{messages:?}");
            }
            // MENU
            let (next, popups) = self.popups(renderer, cache);
            cache = next;
            let previous = before
                .iter()
                .find(|(script, _)| *script == target.script)
                .map(|(_, popups)| popups.as_slice())
                .unwrap_or_default();
            let opened = popups
                .into_iter()
                .filter(|popup| !previous.contains(popup))
                .reduce(|a, b| a.union(&b));
            if let Some(menu) = opened {
                let _ = writeln!(out, "MENU,{i},bounds,{}", rect(menu));
                self.menu(renderer, i, target, menu, &mut out);
            }
            // KEYS
            for (name, event) in keys() {
                let (next, messages) = self.send(renderer, cache, event, cursor);
                cache = next;
                let _ = writeln!(out, "KEYS,{i},{name},{messages:?}");
            }
        }
        out
    }

    /// Selects each option of a menu that a click on the target opened: the
    /// menu selects on a press after a move has set the hovered option.
    fn menu(
        &self,
        renderer: &mut Renderer,
        i: usize,
        target: &Target,
        menu: Rectangle,
        out: &mut String,
    ) {
        let x = menu.center_x();
        let mut runs: Vec<(f32, f32, String)> = Vec::new();
        let mut y = menu.y + 2.;
        while y <= menu.y + menu.height - 2. {
            let (mut cache, mut messages) = self.replay_cache(renderer, &target.script);
            let at = Point::new(x, y);
            let events = click(target.point)
                .into_iter()
                .map(|(_, event)| (event, target.point))
                .chain([
                    (Event::Mouse(mouse::Event::CursorMoved { position: at }), at),
                    (
                        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                        at,
                    ),
                ]);
            for (event, point) in events {
                let (next, emitted) =
                    self.send(renderer, cache, event, mouse::Cursor::Available(point));
                cache = next;
                messages.extend(emitted);
            }
            let messages = format!("{messages:?}");
            match runs.last_mut() {
                Some((_, to, last)) if *last == messages => *to = y,
                _ => runs.push((y, y, messages)),
            }
            y += 4.;
        }
        for (from, to, messages) in runs {
            let _ = writeln!(out, "MENU,{i},{from:?}..={to:?},{messages}");
        }
    }
}

/// The fingerprint and probes of one case at one window size.
struct Run {
    tree: String,
    probes: Option<String>,
    pixels: Vec<u8>,
}

fn run(case: &Case, size: Size, renderer: &mut Renderer) -> Run {
    let (mut app, _) = App::new();
    app.appearance.mode = crate::appearance::Mode::Light;
    (case.setup)(&mut app);
    // Settle exactly as `snapshot` does; the app is never updated again.
    let mut tree = Tree::empty();
    for _ in 0..3 {
        let messages = pass(&app, renderer, &mut tree, size, None);
        if messages.is_empty() {
            break;
        }
        for message in messages {
            let _ = app.update(message);
        }
    }
    let driver = Driver {
        app: &app,
        size,
        sink: Sink::default(),
    };
    let mut out = String::new();
    driver.replay_tree(renderer, &mut tree, case.script, &mut out);

    // (a) LAYOUT and (b) OP, on the manual tree.
    let frame = driver.frame(renderer, &mut tree);
    let mut found = Vec::new();
    nodes(Layout::new(&frame.base), 0, &mut found);
    for (depth, bounds) in found {
        let _ = writeln!(out, "LAYOUT,{depth},{}", rect(bounds));
    }
    if let Some(overlay) = &frame.overlay {
        let mut found = Vec::new();
        nodes(Layout::new(overlay), 0, &mut found);
        for (depth, bounds) in found {
            let _ = writeln!(out, "OVERLAY,{depth},{}", rect(bounds));
        }
    }
    out.push_str(&frame.record.lines);
    // Self-check of the help scroll scripts, not part of the baseline: the
    // only scrollable that Help leaves operable is its body.
    if let Some(position) = case.name.strip_prefix("help-scrolled-") {
        let [help] = frame.record.scrolls.as_slice() else {
            panic!("{}: one help scrollable expected", case.name);
        };
        let y = help.translation.y;
        let end = (help.content.height - help.bounds.height).max(0.).round();
        assert!(y > 0., "{}: translation {y}", case.name);
        if position == "end" {
            assert_eq!(y, end, "{}: translation", case.name);
        }
    }

    // (c) A11Y and (d) ACT, through the runtime.
    let (cache, messages) = driver.replay_cache(renderer, case.script);
    for message in messages {
        let _ = writeln!(out, "SCRIPT,{message:?}");
    }
    let mut ui = driver.interface(renderer, cache);
    let mut collect = Collect::new(Rectangle::with_size(size));
    ui.operate(renderer, &mut operation::black_box(&mut collect));
    let snapshot = collect.snapshot().clone();
    for node in &snapshot.nodes {
        let _ = writeln!(out, "A11Y,{node:?}");
    }
    let _ = writeln!(out, "A11Y-DUPLICATES,{:?}", snapshot.duplicate_ids);
    let (mut help_done, mut tool_select) = (false, false);
    for node in &snapshot.nodes {
        let mut activate = Activate::<Message>::new(node.id.clone());
        ui.operate(renderer, &mut operation::black_box(&mut activate));
        let message = activate.message();
        let _ = writeln!(out, "ACT,{},{message:?}", node.id);
        // Self-checks of the harness, not part of the baseline.
        if case.name == "help" && node.id == "help-done" {
            assert!(matches!(message, Some(Message::ToggleHelp)), "{message:?}");
            help_done = true;
        }
        if case.name == "default-window" && node.id == "tool-Select" {
            assert!(
                matches!(message, Some(Message::Tool(Tool::Select))),
                "{message:?}"
            );
            tool_select = true;
        }
    }
    drop(ui);
    assert!(help_done || case.name != "help", "help-done must activate");
    assert!(
        tool_select || case.name != "default-window",
        "tool-Select must activate"
    );

    // (e) ALLOC and (f) pixels, on the manual tree.
    let (count, bytes) = driver.allocations(renderer, &mut tree);
    let _ = writeln!(out, "ALLOC,{count},{bytes}");
    let pixels = driver.screenshot(renderer, &mut tree, mouse::Cursor::Unavailable);

    let probes =
        (case.probe != Probe::None).then(|| driver.probes(renderer, &mut tree, case, &frame));
    Run {
        tree: out,
        probes,
        pixels,
    }
}

fn check_text(directory: &Path, file: &str, actual: &str, capture: bool) -> bool {
    let path = directory.join(file);
    if capture {
        std::fs::write(&path, actual).unwrap();
        return true;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("Read baseline {}: {error}", path.display()));
    if expected == actual {
        return true;
    }
    let (mut expected_lines, mut actual_lines) = (expected.lines(), actual.lines());
    for line in 1.. {
        match (expected_lines.next(), actual_lines.next()) {
            (Some(expected), Some(actual)) if expected == actual => {}
            (expected, actual) => {
                println!(
                    "MISMATCH,{file},line {line}\n  expected: {expected:?}\n  actual:   {actual:?}"
                );
                break;
            }
        }
    }
    false
}

fn check_pixels(directory: &Path, file: &str, pixels: &[u8], width: u32, capture: bool) -> bool {
    let path = directory.join(file);
    if capture {
        std::fs::write(&path, pixels).unwrap();
        return true;
    }
    let expected = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("Read baseline {}: {error}", path.display()));
    if expected.len() != pixels.len() {
        println!(
            "MISMATCH,{file},{} bytes, expected {}",
            pixels.len(),
            expected.len()
        );
        return false;
    }
    let Some(first) = pixels.iter().zip(&expected).position(|(a, b)| a != b) else {
        return true;
    };
    let pixel = first / 4;
    let width = width as usize;
    println!(
        "MISMATCH,{file},first different pixel ({}, {}), actual {:?}, expected {:?}",
        pixel % width,
        pixel / width,
        &pixels[pixel * 4..pixel * 4 + 4],
        &expected[pixel * 4..pixel * 4 + 4]
    );
    false
}

#[tokio::test]
#[ignore = "Baseline/candidate view parity; requires a headless renderer, RESHIKI_VIEW_PARITY and RESHIKI_DATA_DIR"]
async fn views_match_captured_baseline() {
    let directory = std::path::PathBuf::from(
        std::env::var("RESHIKI_VIEW_PARITY")
            .expect("Set RESHIKI_VIEW_PARITY to the baseline artifact directory"),
    );
    let capture = std::env::var("RESHIKI_VIEW_PARITY_CAPTURE").as_deref() == Ok("1");
    std::env::var_os("RESHIKI_DATA_DIR")
        .expect("Set RESHIKI_DATA_DIR to a directory recreated empty before every run");
    let only = std::env::var("RESHIKI_VIEW_PARITY_ONLY").ok();
    let backend = std::env::var("RESHIKI_PERF_RENDERER").ok();
    let mut renderer = <Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        backend.as_deref(),
    )
    .await
    .expect("Headless renderer");
    let cases: Vec<Case> = cases()
        .into_iter()
        .filter(|case| {
            only.as_deref()
                .is_none_or(|only| case.name.starts_with(only))
        })
        .collect();
    let mut names = Vec::new();
    for case in &cases {
        for (width, _) in case.sizes {
            let name = format!("{}-{width}", case.name);
            assert!(!names.contains(&name), "Duplicate case {name}");
            names.push(name);
        }
    }
    let metadata = serde_json::json!({
        "renderer": renderer.name(),
        "font": reshiki::style::ui_font_family(),
        "sizes": BOTH,
        "cases": names,
        "filter": only,
    });
    if capture {
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("metadata.json"),
            serde_json::to_vec_pretty(&metadata).unwrap(),
        )
        .unwrap();
    } else {
        assert!(
            only.is_none(),
            "RESHIKI_VIEW_PARITY_ONLY is only for captures during development"
        );
        let expected: serde_json::Value =
            serde_json::from_slice(&std::fs::read(directory.join("metadata.json")).unwrap())
                .unwrap();
        assert!(
            expected["filter"].is_null(),
            "The baseline was captured with RESHIKI_VIEW_PARITY_ONLY; capture every case"
        );
        assert_eq!(
            metadata, expected,
            "Use the same renderer, fonts and cases as the baseline"
        );
    }
    let mut mismatches = Vec::new();
    for case in &cases {
        for &(width, height) in case.sizes {
            let name = format!("{}-{width}", case.name);
            let run = run(case, Size::new(width as f32, height as f32), &mut renderer);
            let mut matched =
                check_text(&directory, &format!("{name}.tree.txt"), &run.tree, capture);
            if let Some(probes) = &run.probes {
                matched &= check_text(&directory, &format!("{name}.probes.txt"), probes, capture);
            }
            matched &= check_pixels(
                &directory,
                &format!("{name}.rgba"),
                &run.pixels,
                width,
                capture,
            );
            let outcome = if capture {
                "captured"
            } else if matched {
                "matched"
            } else {
                "MISMATCH"
            };
            println!("VIEW,{name},{outcome}");
            if !matched {
                mismatches.push(name);
            }
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} views differ from the baseline: {mismatches:?}",
        mismatches.len()
    );
}
