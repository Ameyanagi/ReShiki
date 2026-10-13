//! Snapshot scenes for the UI surfaces added after the maintainability
//! refactor (40c8d82a): NMR, names, atom maps, generic structures, Join and
//! coordination, pen paths, mechanism arrows, reference geometry, rear
//! opacity, porphine and the Assistant setup guide.
//!
//! Unlike [`super::STATES`], a scene may finish background work (NMR
//! prediction, local naming, auto-mapping, expansion) the way the event loop
//! would, activate a control by its accessibility id, and scroll the
//! inspector. Naming scenes need `RESHIKI_NAMING_HELPER` and
//! `RESHIKI_INCHI_HELPER`, as the naming tests do; set `RESHIKI_DATA_DIR` to
//! an empty directory so saved templates and drafts do not appear.
use super::*;
use crate::app::{
    assistant, depth_appearance, generic, graphics, import, inspector, joining, naming, nmr,
    reaction_mapping, shortcuts, template_library,
};
use reshiki::nmr::Nucleus;
use std::cell::Cell;

/// What a scene may ask of the harness besides messages.
pub(in crate::app::workspace) struct Driver<'a> {
    renderer: &'a iced::Renderer,
    /// Wheel lines to scroll the inspector by before the screenshot.
    scroll: Cell<Option<f32>>,
}

impl<'a> Driver<'a> {
    pub(in crate::app::workspace) fn new(renderer: &'a iced::Renderer) -> Self {
        Self {
            renderer,
            scroll: Cell::new(None),
        }
    }

    pub(in crate::app::workspace) fn scroll(&self) -> Option<f32> {
        self.scroll.get()
    }

    /// Activates the enabled control with this accessibility id, as a click
    /// would, and finishes the work it starts.
    fn activate(&self, app: &mut App, id: &str) {
        let size = Size::new(1280., 820.);
        let message = {
            let mut view = app.view();
            let mut tree = Tree::new(view.as_widget());
            let node = view.as_widget_mut().layout(
                &mut tree,
                self.renderer,
                &layout::Limits::new(size, size),
            );
            let mut activate = reshiki::accessibility::Activate::<Message>::new(id);
            view.as_widget_mut().operate(
                &mut tree,
                Layout::new(&node),
                self.renderer,
                &mut iced::advanced::widget::operation::black_box(&mut activate),
            );
            activate.message().cloned()
        };
        settle(
            app,
            message.unwrap_or_else(|| panic!("No enabled control {id}")),
        );
    }

    /// Scrolls the inspector down by this many wheel lines.
    fn scroll_inspector(&self, lines: f32) {
        self.scroll.set(Some(lines));
    }
}

pub(in crate::app::workspace) type Scene = fn(&mut App, &Driver<'_>);

/// Updates the app and runs the work it starts to completion, as the event
/// loop would, feeding results back in order.
fn settle(app: &mut App, message: Message) {
    let mut queue = std::collections::VecDeque::from([message]);
    for _ in 0..64 {
        let Some(message) = queue.pop_front() else {
            return;
        };
        let task = app.update(message);
        queue.extend(outputs(task));
    }
}

/// The messages a task publishes, run on a runtime of its own. Other runtime
/// actions (focus, clipboard, window, widget operations) are dropped, which
/// ends the streams that wait for their replies.
fn outputs(task: iced::Task<Message>) -> Vec<Message> {
    use iced::futures::StreamExt;
    let Some(mut stream) = iced_runtime::task::into_stream(task) else {
        return vec![];
    };
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async move {
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
            let mut messages = vec![];
            loop {
                match tokio::time::timeout_at(deadline, stream.next()).await {
                    Ok(Some(iced_runtime::Action::Output(message))) => messages.push(message),
                    Ok(Some(_)) => {}
                    Ok(None) => break,
                    Err(_) => {
                        eprintln!("A snapshot task did not finish within 30 s");
                        break;
                    }
                }
            }
            messages
        })
    })
    .join()
    .unwrap()
}

fn fixture(bytes: &[u8]) -> Document {
    Document::from_native_file(bytes).unwrap()
}

/// Opens a drawing, refreshes its derived labels and centers it at `zoom`,
/// shifted right by `dx` screen pixels.
fn show(app: &mut App, doc: Document, zoom: f32, dx: f32) {
    open(app, doc);
    app.tab.labels_dirty = true;
    for message in outputs(app.start_label_refresh()) {
        settle(app, message);
    }
    let (lo, hi) = app.tab.doc.bounds();
    app.tab.camera.zoom = zoom;
    app.tab.camera.center = World::new((lo.x + hi.x) / 2. - dx / zoom, (lo.y + hi.y) / 2.);
}

fn select(app: &mut App, ids: Vec<u64>) {
    let _ = app.update(Message::Canvas(Edit::Select(ids)));
}

/// Opens the right-click menu at a canvas point for this selection and hit.
fn context_menu(app: &mut App, at: iced::Point, selected: Vec<u64>, hit: Vec<u64>) {
    let _ = app.update(Message::Canvas(Edit::ContextMenu {
        position: at,
        selected,
        hit,
    }));
}

fn section(app: &mut App, section: inspector::Section, expanded: bool) {
    let _ = app.update(Message::InspectorAction(inspector::Action::Section(
        section, expanded,
    )));
}

// NMR prediction.

fn ethyl_acetate(app: &mut App) {
    show(
        app,
        fixture(include_bytes!(
            "../../../../tests/fixtures/nmr/ethyl-acetate.rsk"
        )),
        2.,
        -60.,
    );
    select_all(app);
}

/// Ethyl acetate with its ¹H prediction in the floating palette.
fn nmr_proton(app: &mut App, _: &Driver<'_>) {
    predicted(app);
    // Choosing a row selects its carbon and attached hydrogens.
    let _ = app.update(Message::Nmr(nmr::Action::SelectRow(1)));
}

fn predicted(app: &mut App) {
    ethyl_acetate(app);
    settle(app, Message::Nmr(nmr::Action::Open));
}

/// The ¹³C prediction with atom-linked shift labels on the drawing.
fn nmr_carbon_labels(app: &mut App, _: &Driver<'_>) {
    predicted(app);
    settle(app, Message::Nmr(nmr::Action::Predict(Nucleus::C13)));
    for action in [
        nmr::Action::ToggleLabels,
        nmr::Action::LabelMode(crate::canvas::nmr::LabelMode::Both),
    ] {
        let _ = app.update(Message::Nmr(action));
    }
    select(app, vec![]);
}

/// The label format menu of the floating palette.
fn nmr_label_menu(app: &mut App, _: &Driver<'_>) {
    predicted(app);
    for action in [nmr::Action::ToggleLabels, nmr::Action::LabelMenu(true)] {
        let _ = app.update(Message::Nmr(action));
    }
}

/// The ¹H report docked in the inspector, with reference details shown.
fn nmr_docked(app: &mut App, _: &Driver<'_>) {
    predicted(app);
    for action in [nmr::Action::Dock, nmr::Action::ToggleDetails] {
        settle(app, Message::Nmr(action));
    }
}

/// The NMR inspector tab before any prediction.
fn nmr_tab(app: &mut App, _: &Driver<'_>) {
    let _ = app.update(Message::Inspector(InspectorTab::Predict));
}

// Chemical names.

fn name_dock(app: &mut App) {
    use import::{Action, InputKind};
    let _ = app.update(Message::Inspector(InspectorTab::Import));
    let _ = app.update(Message::Imports(Action::InputKind(InputKind::Name)));
    let _ = app.update(Message::Naming(naming::Action::Name(
        "ethyl acetate".into(),
    )));
}

/// The Import dock in Name mode with the optional editable preview open.
fn name_import_preview(app: &mut App, _: &Driver<'_>) {
    name_dock(app);
    settle(app, Message::Naming(naming::Action::PreviewOpen(true)));
}

/// Insert from the Name dock: the structure with its linked name caption.
fn name_import_inserted(app: &mut App, _: &Driver<'_>) {
    name_dock(app);
    settle(app, Message::InsertInput);
    app.tab.camera.zoom = 2.5;
}

/// Import → Name details for ethanol: its curated aliases and the generated name of the selection.
fn name_import_details(app: &mut App, _: &Driver<'_>) {
    use import::{Action, InputKind};
    show(
        app,
        fixture(include_bytes!(
            "../../../../tests/fixtures/chemical-naming-rust/ethanol-rust-native.rsk"
        )),
        2.5,
        -60.,
    );
    select_all(app);
    let _ = app.update(Message::Inspector(InspectorTab::Import));
    let _ = app.update(Message::Imports(Action::InputKind(InputKind::Name)));
    let _ = app.update(Message::Naming(naming::Action::Name("ethanol".into())));
    settle(app, Message::Naming(naming::Action::DetailsOpen(true)));
    settle(app, Message::Naming(naming::Action::Generate));
}

/// The entry point of the Chemical names inspector: the right-click menu
/// on a selected molecule.
fn names_menu(app: &mut App, _: &Driver<'_>) {
    ethyl_acetate(app);
    let atom = app.tab.doc.atoms[2].id;
    let selected = app.tab.selected.clone();
    context_menu(app, iced::Point::new(520., 110.), selected, vec![atom]);
}

/// The Chemical names inspector with a parsed name's editable preview.
fn names_panel(app: &mut App, d: &Driver<'_>) {
    names(app, d);
    for action in [naming::Action::Resolve, naming::Action::Generate] {
        settle(app, Message::Naming(action));
    }
}

/// The Chemical names inspector with the generated name of the selection.
/// Without a parsed preview it fits unscrolled; a wheel over the preview
/// would zoom the preview instead of scrolling the panel.
fn names_generated(app: &mut App, d: &Driver<'_>) {
    names(app, d);
    settle(app, Message::Naming(naming::Action::Generate));
}

/// Opens the inspector from the molecule menu and enters a name.
fn names(app: &mut App, d: &Driver<'_>) {
    names_menu(app, d);
    d.activate(app, "menu-chemical-names");
    let _ = app.update(Message::Naming(naming::Action::Name(
        "ethyl acetate".into(),
    )));
}

/// The right-click menu on one atom: Show chemical name, atom maps, Join.
fn molecule_menu(app: &mut App, _: &Driver<'_>) {
    ethyl_acetate(app);
    let atom = app.tab.doc.atoms[2].id;
    context_menu(app, iced::Point::new(520., 110.), vec![atom], vec![atom]);
}

/// Show chemical name: a generated name caption linked to its molecule.
fn molecule_caption(app: &mut App, _: &Driver<'_>) {
    ethyl_acetate(app);
    let atoms = reshiki::molecule_names::component(&app.tab.doc, app.tab.doc.atoms[0].id);
    settle(
        app,
        Message::Naming(naming::Action::ShowMoleculeName(atoms)),
    );
    select(app, vec![]);
}

// Reaction atom maps.

fn oxidation(app: &mut App) {
    show(
        app,
        fixture(include_bytes!(
            "../../../../tests/fixtures/reaction-mapping/oxidation-before.rsk"
        )),
        1.6,
        30.,
    );
    let _ = app.update(Message::Mapping(reaction_mapping::Action::Open));
}

/// Auto-map's proposal waiting for review at the end of the Atom maps
/// section, which follows the role list.
fn reaction_auto_map(app: &mut App, d: &Driver<'_>) {
    auto_mapped(app);
    d.scroll_inspector(-60.);
}

fn auto_mapped(app: &mut App) {
    oxidation(app);
    settle(app, Message::Mapping(reaction_mapping::Action::Auto));
}

/// The applied proposal with map numbers shown on the drawing, and the
/// Atom maps section at the end of the Reactions inspector.
fn reaction_maps(app: &mut App, d: &Driver<'_>) {
    d.scroll_inspector(-60.);
    auto_mapped(app);
    for action in [
        reaction_mapping::Action::ApplyProposal,
        reaction_mapping::Action::Show(true),
    ] {
        settle(app, Message::Mapping(action));
    }
}

// Generic structures.

fn methoxytoluene(app: &mut App) {
    show(
        app,
        fixture(include_bytes!(
            "../../../../tests/fixtures/generic-structures/variable-methoxytoluene-v26.rsk"
        )),
        2.,
        0.,
    );
}

/// A variable attachment site, with the Chemical abbreviations page open.
fn generic_variable(app: &mut App, _: &Driver<'_>) {
    methoxytoluene(app);
    let _ = app.update(Message::Inspector(InspectorTab::Abbreviations));
}

/// The ortho, meta and para products expanded into a new tab.
fn generic_expanded(app: &mut App, _: &Driver<'_>) {
    methoxytoluene(app);
    settle(app, Message::Generic(generic::Action::Expand));
}

/// A [Cl,Br,I] atom alternative, with Expand in the right-click menu.
fn generic_choice(app: &mut App, _: &Driver<'_>) {
    let mut doc = benzene();
    let top = doc
        .atoms
        .iter()
        .min_by(|a, b| a.position.y.total_cmp(&b.position.y))
        .unwrap()
        .clone();
    let halogen = doc.add_atom("Cl", top.position.offset(0., -42.));
    doc.add_bond(top.id, halogen, 1, "plain");
    let doc = reshiki::atom_text::apply(&doc, halogen, "[Cl,Br,I]", reshiki::atom_text::Mode::Auto)
        .unwrap();
    show(app, doc, 2., -100.);
    context_menu(
        app,
        iced::Point::new(420., 200.),
        vec![top.id],
        vec![top.id],
    );
}

/// A bracketed repeating unit with its Repeating unit section.
fn generic_repeat(app: &mut App, _: &Driver<'_>) {
    let mut doc = Document::default();
    let atoms: Vec<u64> = [(0., 0.), (36.373, -21.), (72.746, 0.), (109.119, -21.)]
        .into_iter()
        .map(|(x, y)| doc.add_atom("C", World::new(x, y)))
        .collect();
    for pair in atoms.windows(2) {
        doc.add_bond(pair[0], pair[1], 1, "plain");
    }
    let bracket = doc.next_id();
    doc.graphics.push(Graphic::dragged(
        bracket,
        GraphicKind::Brackets,
        World::new(18., -40.),
        World::new(91., 19.),
        Default::default(),
        Default::default(),
        false,
    ));
    show(app, doc, 2.5, 0.);
    select(app, vec![bracket]);
    settle(
        app,
        Message::Generic(generic::Action::DefineRepeat(bracket)),
    );
}

// Join and coordination.

/// Three atoms on three fragments selected, with Join and Merge in the menu.
fn join_three_atoms(app: &mut App, _: &Driver<'_>) {
    show(
        app,
        fixture(include_bytes!(
            "../../../../tests/fixtures/join-three-atoms-before.rsk"
        )),
        2.,
        -120.,
    );
    select(app, vec![1, 3, 5]);
    context_menu(app, iced::Point::new(480., 110.), vec![1, 3, 5], vec![1]);
}

fn co_en3(app: &mut App) {
    show(
        app,
        fixture(include_bytes!(
            "../../../../tests/fixtures/coordination/co-en3-before.rsk"
        )),
        2.,
        0.,
    );
}

/// Coordinate in place: the donor preview and the canvas bar.
fn coordinate_in_place(app: &mut App, _: &Driver<'_>) {
    co_en3(app);
    select(app, vec![2, 3, 4, 5]);
    settle(app, Message::Join(joining::Action::BeginCoordination));
}

/// Co(en)₃ after ⌘J on each donor–metal pair: six donor → Co contacts.
fn coordination_contacts(app: &mut App, _: &Driver<'_>) {
    co_en3(app);
    for donor in [2, 5, 6, 9, 10, 13] {
        select(app, vec![donor, 1]);
        settle(app, Message::Shortcut(shortcuts::Action::Join));
    }
    select(app, vec![]);
}

// Pen paths and mechanism arrows.

fn five_node_path(app: &mut App) -> u64 {
    show(
        app,
        fixture(include_bytes!(
            "../../../../tests/fixtures/tunable-pen-lines-67/pen-final-five-nodes-fresh-reopened.rsk"
        )),
        1.5,
        0.,
    );
    let path = app.tab.doc.graphics[0].id;
    select(app, vec![path]);
    path
}

/// Edit Points on a five-node pen path, its third node selected.
fn pen_path_nodes(app: &mut App, _: &Driver<'_>) {
    use graphics::{Action, path::Action as Path};
    let path = five_node_path(app);
    let _ = app.update(Message::Tool(Tool::EditPoints));
    select(app, vec![path]);
    let node = app.tab.doc.graphics[0]
        .path_handles()
        .unwrap()
        .into_iter()
        .filter(|handle| handle.node)
        .nth(2)
        .unwrap()
        .index;
    let _ = app.update(Message::Graphics(Action::Path(Path::Node(node))));
}

/// Continue drawing the open path with the pen tool.
fn pen_tool(app: &mut App, _: &Driver<'_>) {
    use graphics::{Action, path::Action as Path};
    five_node_path(app);
    let _ = app.update(Message::Graphics(Action::Path(Path::Continue)));
}

fn methoxide_carbonyl(app: &mut App) {
    show(
        app,
        fixture(include_bytes!(
            "../../../../tests/fixtures/mechanism-curvature-91/after.rsk"
        )),
        1.7,
        0.,
    );
}

/// A selected cubic mechanism arrow: diamond bend and square tangent handles.
fn mechanism_tangents(app: &mut App, _: &Driver<'_>) {
    methoxide_carbonyl(app);
    select(app, vec![101]);
}

/// Methoxide adding to a carbonyl: only the upper pair, without arrows.
fn upper_pair(app: &mut App) {
    let mut doc = fixture(include_bytes!(
        "../../../../tests/fixtures/mechanism-curvature-91/before.rsk"
    ));
    doc.atoms.retain(|atom| atom.id < 10);
    doc.bonds.retain(|bond| bond.a < 10 && bond.b < 10);
    doc.arrows.clear();
    show(app, doc, 3., 0.);
    let _ = app.update(Message::ArrowStyle(reshiki::arrows::Preset::Curved));
}

/// Attach ends to atoms and bonds: the first click picked the methoxide oxygen.
fn mechanism_source(app: &mut App, _: &Driver<'_>) {
    upper_pair(app);
    let _ = app.update(Message::Canvas(Edit::ArrowTarget(
        World::new(36., 30.),
        false,
    )));
}

/// Two attached arrows (O⁻ → C, C=O → O), the first selected.
fn mechanism_attached(app: &mut App, _: &Driver<'_>) {
    upper_pair(app);
    for point in [
        World::new(36., 30.),
        World::new(146., 30.),
        World::new(164., 40.5),
        World::new(182., 51.),
    ] {
        let _ = app.update(Message::Canvas(Edit::ArrowTarget(point, false)));
    }
    let _ = app.update(Message::Tool(Tool::Select));
    let arrow = app.tab.doc.arrows[0].id;
    select(app, vec![arrow]);
}

// Reference geometry.

/// Biphenyl with its bridge bond selected and the Transform section open.
fn biphenyl_bridge(app: &mut App, d: &Driver<'_>) {
    // Two pointed-top rings, the lower one a ring diameter plus a bond below.
    let mut doc = benzene();
    let lowest = |doc: &Document, ids: &[u64], top: bool| {
        let key = |id: &&u64| doc.atom(**id).unwrap().position.y;
        let found = if top {
            ids.iter().min_by(|a, b| key(a).total_cmp(&key(b)))
        } else {
            ids.iter().max_by(|a, b| key(a).total_cmp(&key(b)))
        };
        *found.unwrap()
    };
    let upper = lowest(&doc, &doc.all_ids(), false);
    let ids = reshiki::editing::append(&mut doc, &benzene(), World::new(0., 126.));
    let lower = lowest(&doc, &ids, true);
    doc.add_bond(upper, lower, 1, "plain");
    show(app, doc, 2., 0.);
    select(app, vec![upper, lower]);
    for (name, expanded) in [
        (inspector::Section::Molecule, false),
        (inspector::Section::Atoms, false),
        (inspector::Section::Bonds, false),
        (inspector::Section::Groups, false),
        (inspector::Section::Transform, true),
    ] {
        section(app, name, expanded);
    }
    d.activate(app, "reference-controls");
}

/// Reference: align / stretch, opened for the bridge bond.
fn reference_controls(app: &mut App, d: &Driver<'_>) {
    biphenyl_bridge(app, d);
    d.scroll_inspector(-7.);
}

/// Drag to stretch: the bridge-stretch tool with its Done control.
fn stretch_tool(app: &mut App, d: &Driver<'_>) {
    biphenyl_bridge(app, d);
    d.activate(app, "reference-stretch-drag");
}

// Drawing tools, appearance and templates.

/// The orbital tool's Snap to atoms option over a p orbital snapped to O.
fn orbital_snap(app: &mut App, _: &Driver<'_>) {
    show(
        app,
        fixture(include_bytes!(
            "../../../../tests/fixtures/drawing-tool-defaults/orbital-methanol-P-Solid-layer1-Light.rsk"
        )),
        3.,
        0.,
    );
    let _ = app.update(Message::Tool(Tool::Graphic(GraphicKind::Orbital(
        reshiki::scientific::OrbitalKind::P,
    ))));
}

/// C60 at 25% rear opacity with the 3D appearance section.
fn rear_opacity(app: &mut App, _: &Driver<'_>) {
    show(
        app,
        fixture(include_bytes!(
            "../../../../tests/fixtures/rear-opacity/c60-rear-opacity-25.rsk"
        )),
        1.6,
        0.,
    );
    section(app, inspector::Section::Molecule, false);
    let ids = app.depth_ids();
    let _ = app.update(Message::DepthAppearance(
        depth_appearance::Action::RearInput(depth_appearance::Draft {
            ids,
            revision: app.tab.revision,
            file_epoch: app.tab.file_epoch,
            value: "25".into(),
        }),
    ));
}

/// Free-base porphine placed, and found by searching the template library.
fn porphine(app: &mut App, _: &Driver<'_>) {
    let template = reshiki::templates::LIBRARY
        .iter()
        .find(|template| template.name.starts_with("Porphine"))
        .unwrap();
    show(app, template.document.clone(), 1.5, 0.);
    let _ = app.update(Message::Inspector(InspectorTab::Templates));
    let _ = app.update(Message::Templates(template_library::Action::Search(
        "porphine".into(),
    )));
}

/// The figure format menu with SVG for Office chosen.
fn export_office_svg(app: &mut App, _: &Driver<'_>) {
    ethyl_acetate(app);
    select(app, vec![]);
    let _ = app.update(Message::Inspector(InspectorTab::Export));
    for action in [
        inspector::Action::Figure(inspector::FigureFormat::SvgOffice),
        inspector::Action::FigureMenu(true),
    ] {
        let _ = app.update(Message::InspectorAction(action));
    }
}

// Assistant setup.

/// Opens the Assistant and answers its connection check with `result`.
fn assistant_checked(
    app: &mut App,
    result: Result<reshiki::assistant::codex::Account, reshiki::assistant::codex::ConnectionError>,
) {
    // Opening starts the first check (serial 1); its task is not run.
    let _ = app.update(Message::Assistant(assistant::Action::Open));
    let _ = app.update(Message::Assistant(assistant::Action::Connected(1, result)));
}

/// Codex missing: the installation step of the setup guide.
fn assistant_install(app: &mut App, _: &Driver<'_>) {
    assistant_checked(
        app,
        Err(reshiki::assistant::codex::ConnectionError::MissingInstallation),
    );
}

/// Codex installed but signed out: the sign-in step, with the commands in
/// a monospace font.
fn assistant_sign_in(app: &mut App, _: &Driver<'_>) {
    assistant_checked(
        app,
        Ok(reshiki::assistant::codex::Account {
            connected: false,
            models: vec![],
        }),
    );
}

/// Connected: the guided ethanol image exercise attached, before Send.
fn assistant_example(app: &mut App, d: &Driver<'_>) {
    d.scroll_inspector(-6.);
    assistant_checked(
        app,
        Ok(reshiki::assistant::codex::Account {
            connected: true,
            models: vec![],
        }),
    );
    let _ = app.update(Message::Assistant(assistant::Action::ImageExample));
}

pub(in crate::app::workspace) const STATES: [(&str, Scene); 36] = [
    ("nmr-proton", nmr_proton),
    ("nmr-carbon-labels", nmr_carbon_labels),
    ("nmr-label-menu", nmr_label_menu),
    ("nmr-docked", nmr_docked),
    ("nmr-tab", nmr_tab),
    ("name-import-preview", name_import_preview),
    ("name-import-inserted", name_import_inserted),
    ("name-import-details", name_import_details),
    ("names-menu", names_menu),
    ("names-panel", names_panel),
    ("names-generated", names_generated),
    ("molecule-menu", molecule_menu),
    ("molecule-caption", molecule_caption),
    ("reaction-auto-map", reaction_auto_map),
    ("reaction-maps", reaction_maps),
    ("generic-variable", generic_variable),
    ("generic-expanded", generic_expanded),
    ("generic-choice", generic_choice),
    ("generic-repeat", generic_repeat),
    ("join-three-atoms", join_three_atoms),
    ("coordinate-in-place", coordinate_in_place),
    ("coordination-contacts", coordination_contacts),
    ("pen-path-nodes", pen_path_nodes),
    ("pen-tool", pen_tool),
    ("mechanism-tangents", mechanism_tangents),
    ("mechanism-source", mechanism_source),
    ("mechanism-attached", mechanism_attached),
    ("reference-controls", reference_controls),
    ("stretch-tool", stretch_tool),
    ("orbital-snap", orbital_snap),
    ("rear-opacity", rear_opacity),
    ("porphine", porphine),
    ("export-office-svg", export_office_svg),
    ("assistant-install", assistant_install),
    ("assistant-sign-in", assistant_sign_in),
    ("assistant-example", assistant_example),
];

pub(in crate::app::workspace) fn needs_naming(name: &str) -> bool {
    name.starts_with("name-import-") || name.starts_with("names-") || name == "molecule-caption"
}

/// Settles the view as the window first shows, plays the scene, lets the
/// view settle again, scrolls the inspector if the scene asked, then saves
/// the screenshot. The tree persists throughout, so first-show sensors fire
/// before the scene and cannot close the menus it opens.
pub(super) fn capture(
    app: &mut App,
    renderer: &mut iced::Renderer,
    size: Size,
    scene: Scene,
    output: &Path,
) {
    let mut tree = Tree::empty();
    settle_view(app, renderer, &mut tree, size);
    let driver = Driver::new(renderer);
    scene(app, &driver);
    let scroll = driver.scroll();
    settle_view(app, renderer, &mut tree, size);
    if let Some(lines) = scroll {
        let at = iced::Point::new(size.width - app.inspector_width() / 2., size.height / 2.);
        let mut messages = Vec::new();
        {
            let mut view = app.view();
            tree.diff(view.as_widget());
            let node =
                view.as_widget_mut()
                    .layout(&mut tree, renderer, &layout::Limits::new(size, size));
            view.as_widget_mut().update(
                &mut tree,
                &Event::Mouse(mouse::Event::WheelScrolled {
                    delta: mouse::ScrollDelta::Lines { x: 0., y: lines },
                }),
                Layout::new(&node),
                mouse::Cursor::Available(at),
                renderer,
                &mut iced::advanced::clipboard::Null,
                &mut iced::advanced::Shell::new(&mut messages),
                &Rectangle::with_size(size),
            );
        }
        for message in messages {
            let _ = app.update(message);
        }
    }
    pass(app, renderer, &mut tree, size, Some(output));
}

/// Up to three layout passes, applying what they publish (viewport sizes,
/// sensors), as in [`super::snapshot`].
pub(in crate::app::workspace) fn settle_view(
    app: &mut App,
    renderer: &mut iced::Renderer,
    tree: &mut Tree,
    size: Size,
) {
    for _ in 0..3 {
        let messages = pass(app, renderer, tree, size, None);
        if messages.is_empty() {
            break;
        }
        for message in messages {
            let _ = app.update(message);
        }
    }
}
