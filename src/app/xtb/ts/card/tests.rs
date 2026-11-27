//! Reaction drawings and regression coverage for Predict’s reaction card.
use super::*;
#[cfg(not(target_arch = "wasm32"))]
use iced::advanced::{
    Layout, layout,
    renderer::Headless,
    widget::{Id, Operation, Tree, operation},
};
#[cfg(not(target_arch = "wasm32"))]
use iced::{Rectangle, Size};
use reshiki::document::{Arrow, Point};
use reshiki::reactions::ts_plan;
use reshiki::reactions::{Participant, Reaction};
use std::collections::BTreeMap;

/// Inspect the widgets a production view actually creates, including lazy cards.
/// Run opt-in renderer tests with `--test-threads=1` on headless GPU hosts.
#[cfg(not(target_arch = "wasm32"))]
pub(in crate::app::xtb::ts) struct ViewHarness {
    renderer: iced::Renderer,
    viewport: Rectangle,
}

#[cfg(not(target_arch = "wasm32"))]
pub(in crate::app::xtb::ts) struct ViewSnapshot {
    pub texts: Vec<String>,
    pub semantics: reshiki::accessibility::Snapshot,
}

#[cfg(not(target_arch = "wasm32"))]
impl ViewHarness {
    pub async fn new() -> Self {
        Self {
            renderer: <iced::Renderer as Headless>::new(
                iced::Font::with_name(reshiki::style::ui_font_family()),
                iced::Pixels(16.),
                None,
            )
            .await
            .expect("headless renderer"),
            viewport: Rectangle::with_size(Size::new(600., 2400.)),
        }
    }

    pub fn at_width(mut self, width: f32) -> Self {
        self.viewport.width = width;
        self
    }

    fn operate(&self, view: &mut Element<'_, Message>, operation: &mut dyn Operation) {
        let mut tree = Tree::new(view.as_widget());
        let node = view.as_widget_mut().layout(
            &mut tree,
            &self.renderer,
            &layout::Limits::new(Size::ZERO, self.viewport.size()),
        );
        view.as_widget_mut()
            .operate(&mut tree, Layout::new(&node), &self.renderer, operation);
    }

    pub fn inspect(&self, mut view: Element<'_, Message>) -> ViewSnapshot {
        #[derive(Default)]
        struct Texts(Vec<String>);
        impl Operation for Texts {
            fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
                operate(self);
            }

            fn text(&mut self, _: Option<&Id>, _: Rectangle, content: &str) {
                self.0.push(content.to_owned());
            }
        }

        let mut texts = Texts::default();
        self.operate(&mut view, &mut texts);
        let mut semantics = reshiki::accessibility::Collect::new(self.viewport);
        self.operate(&mut view, &mut operation::black_box(&mut semantics));
        ViewSnapshot {
            texts: texts.0,
            semantics: semantics.snapshot().clone(),
        }
    }

    pub fn activate(&self, mut view: Element<'_, Message>, id: &str) -> Option<Message> {
        let mut activate = reshiki::accessibility::Activate::<Message>::new(id);
        self.operate(&mut view, &mut operation::black_box(&mut activate));
        activate.message().cloned()
    }
}

fn molecule(
    doc: &mut Document,
    atoms: &[(&str, u32)],
    bonds: &[(u32, u32, u8)],
) -> BTreeMap<u32, u64> {
    let map: BTreeMap<_, _> = atoms
        .iter()
        .map(|&(element, number)| {
            let id = doc.add_atom(element, Point::new(doc.atoms.len() as f32 * 28., 0.));
            doc.atom_mut(id).unwrap().map_num = number;
            (number, id)
        })
        .collect();
    for &(a, b, order) in bonds {
        doc.add_bond(map[&a], map[&b], order, "plain");
    }
    map
}

fn assign(
    doc: &mut Document,
    reactants: &[&BTreeMap<u32, u64>],
    products: &[&BTreeMap<u32, u64>],
) -> u64 {
    let arrow = doc.next_id();
    doc.arrows.push(Arrow::new(
        arrow,
        Point::default(),
        Point::new(100., 0.),
        Default::default(),
        Default::default(),
    ));
    let participants = |molecules: &[&BTreeMap<u32, u64>]| {
        molecules
            .iter()
            .map(|molecule| Participant {
                atoms: molecule.values().copied().collect(),
                coefficient: 1,
            })
            .collect()
    };
    let mut reaction = Reaction::new(arrow);
    reaction.reactants = participants(reactants);
    reaction.products = participants(products);
    doc.reactions.push(reaction);
    doc.validate().unwrap();
    arrow
}

pub(in crate::app::xtb::ts) fn claisen() -> (Document, u64) {
    let mut doc = Document::default();
    let atoms = [("C", 1), ("C", 2), ("O", 3), ("C", 4), ("C", 5), ("C", 6)];
    let left = molecule(
        &mut doc,
        &atoms,
        &[(1, 2, 2), (2, 3, 1), (3, 4, 1), (4, 5, 1), (5, 6, 2)],
    );
    let right = molecule(
        &mut doc,
        &atoms,
        &[(2, 3, 2), (1, 2, 1), (1, 6, 1), (5, 6, 1), (4, 5, 2)],
    );
    let arrow = assign(&mut doc, &[&left], &[&right]);
    (doc, arrow)
}

#[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
pub(in crate::app::xtb::ts) fn hydrogen_transfer() -> (Document, u64) {
    let mut doc = Document::default();
    let atoms = [("C", 1), ("C", 2), ("O", 3), ("C", 4), ("H", 5)];
    let left = molecule(
        &mut doc,
        &atoms,
        &[(1, 4, 1), (2, 4, 1), (3, 4, 2), (2, 5, 1)],
    );
    let right = molecule(
        &mut doc,
        &atoms,
        &[(1, 4, 1), (2, 4, 2), (3, 4, 1), (2, 5, 1)],
    );
    let arrow = assign(&mut doc, &[&left], &[&right]);
    (doc, arrow)
}

fn app(doc: Document, arrow: u64) -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = doc;
    app.tab.selected = vec![arrow];
    app
}

fn pairs(bonds: &[[MappedAtom; 2]]) -> Vec<String> {
    bonds
        .iter()
        .map(|[a, b]| format!("{}{}–{}{}", a.element, a.number, b.element, b.number))
        .collect()
}

#[test]
fn barrier_method_is_remembered_independently_of_the_molecule_method() {
    let (doc, arrow) = claisen();
    let mut app = app(doc, arrow);
    assert_eq!(app.xtb.settings.barrier_method, XtbMethod::Gfn2Xtb);
    let _ = app.xtb_ts_card_action(Action::Method(XtbMethod::GXtb));
    assert_eq!(app.xtb.settings.barrier_method, XtbMethod::GXtb);
    assert_eq!(app.xtb.settings.method, XtbMethod::Gfn2Xtb);
    app.xtb.settings.method = XtbMethod::GfnFf;
    app.xtb.settings.retarget(Some([1, 0, 42]));
    assert_eq!(app.xtb.settings.barrier_method, XtbMethod::GXtb);
    // GFN-FF is never an admitted barrier-method choice.
    let _ = app.xtb_ts_card_action(Action::Method(XtbMethod::GfnFf));
    assert_eq!(app.xtb.settings.barrier_method, XtbMethod::GXtb);
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
#[ignore = "Opt-in renderer check: needs a graphics adapter"]
async fn barrier_method_controls_name_gxtb_and_its_size_note_at_narrow_width() {
    let (doc, arrow) = claisen();
    let mut app = app(doc, arrow);
    let ui = ViewHarness::new().await.at_width(330.);
    let initial = ui.inspect(app.xtb_reaction_card(arrow));
    assert!(initial.semantics.duplicate_ids.is_empty());
    for id in ["xtb-ts-method-gfn2", "xtb-ts-method-gxtb"] {
        assert_eq!(
            initial.semantics.nodes.iter().any(|node| node.id == id),
            xtb::available()
        );
    }
    if !xtb::available() {
        return;
    }
    let _ = app.xtb_ts_card_action(Action::Barrier);
    assert!(!app.tab.xtb.ts_card.barrier);
    let action = ui
        .activate(app.xtb_reaction_card(arrow), "xtb-ts-method-gxtb")
        .unwrap();
    let _ = app.update(action);
    assert_eq!(app.xtb.settings.barrier_method, XtbMethod::GXtb);
    let selected = ui.inspect(app.xtb_reaction_card(arrow));
    assert!(
        selected
            .texts
            .iter()
            .any(|text| text == "closer to DFT energies; slower for large molecules")
    );
    assert!(
        !selected
            .texts
            .iter()
            .any(|text| text.contains("about 5–10 s"))
    );
    assert!(selected.semantics.duplicate_ids.is_empty());
}

#[test]
fn reaction_label_uses_names_then_formulas() {
    let (mut doc, arrow) = claisen();
    let atoms = doc.reactions[0].reactants[0].atoms.clone();
    let identity = reshiki::naming::selected_identity(&doc, &atoms)
        .unwrap()
        .smiles;
    reshiki::molecule_names::show(
        &mut doc,
        &atoms,
        "allyl vinyl ether".into(),
        identity,
        Default::default(),
        false,
    )
    .unwrap();
    let app = app(doc, arrow);
    let label = app.xtb_reaction_label(arrow);
    assert_eq!(label.title(), "allyl vinyl ether → C5H8O");
    assert_eq!(label.first_reactant(), Some("allyl vinyl ether"));
    assert_eq!(label.formula(), "C5H8O → C5H8O");
}

#[test]
fn reaction_label_prefers_drawn_compound_labels_and_custom_names() {
    let (doc, arrow) = claisen();
    let mut app = app(doc, arrow);
    let atoms = app.tab.doc.reactions[0].reactants[0].atoms.clone();
    let smiles = reshiki::naming::selected_identity(&app.tab.doc, &atoms)
        .unwrap()
        .smiles;
    app.tab
        .naming
        .caption_names
        .insert(smiles, "allyl vinyl ether".into());
    assert_eq!(
        app.xtb_reaction_label(arrow).title(),
        "allyl vinyl ether → C5H8O"
    );
    reshiki::linked_properties::rename(
        &mut app.tab.doc,
        &atoms,
        "Starting ether",
        Default::default(),
        false,
    )
    .unwrap();
    assert_eq!(
        app.xtb_reaction_label(arrow).title(),
        "Starting ether → C5H8O"
    );
    reshiki::compound_numbers::number(&mut app.tab.doc, &atoms, Default::default()).unwrap();
    let annotation = reshiki::compound_numbers::label(&app.tab.doc, &atoms)
        .unwrap()
        .annotation;
    app.tab
        .doc
        .annotations
        .iter_mut()
        .find(|label| label.id == annotation)
        .unwrap()
        .text = "1a".into();
    app.tab.doc.reactions[0].reactants[0].coefficient = 2;
    let label = app.xtb_reaction_label(arrow);
    assert_eq!(label.title(), "2 1a → C5H8O");
    assert_eq!(
        label.formula(),
        "2 C5H8O → C5H8O",
        "the name line's tooltip keeps formulas and coefficients"
    );
}

fn show_reaction_card(app: &mut App) {
    app.inspector_open = true;
    app.inspector_tab = crate::app::InspectorTab::Predict;
    app.xtb.page = xtb::Page::Xtb;
}

#[test]
fn visible_reaction_names_arrive_from_the_caption_queue_without_editing_the_drawing() {
    use crate::app::naming::{Outcome, test_support::record};
    let (doc, arrow) = claisen();
    let mut app = app(doc, arrow);
    app.inspector_open = false;
    assert_eq!(app.start_caption_names().units(), 0);
    show_reaction_card(&mut app);
    let before = app.tab.doc.clone();
    let revision = app.tab.revision;
    assert_eq!(app.xtb_reaction_label(arrow).title(), "C5H8O → C5H8O");
    assert!(app.start_caption_names().units() > 0);
    let smiles = app.tab.naming.caption_pending.clone().unwrap();
    assert_eq!(
        app.start_caption_names().units(),
        0,
        "the pending request is shared"
    );
    let mut named = record(&smiles);
    named.systematic_name = Some("allyl vinyl ether".into());
    app.caption_named(&smiles, Ok(Outcome::Structure(named)));
    assert_eq!(
        app.xtb_reaction_label(arrow).title(),
        "allyl vinyl ether → C5H8O"
    );
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.revision, revision);
    assert!(
        app.start_caption_names().units() > 0,
        "the product follows in the same queue"
    );
    let product = app.tab.naming.caption_pending.clone().unwrap();
    assert_ne!(product, smiles);
    app.inspector_open = false;
    app.caption_named(
        &product,
        Err(reshiki::naming::Error::Rejected("not supported".into())),
    );
    show_reaction_card(&mut app);
    assert_eq!(
        app.start_caption_names().units(),
        0,
        "a failed identity is not requested again"
    );
    assert!(app.tab.naming.caption_failed.contains_key(&product));
    assert!(
        !app.error,
        "a card's unavailable name leaves its formula visible"
    );
    let tab = app.tab.id;
    app.add_tab();
    assert_eq!(
        app.in_tab(tab, |app| app.xtb_reaction_card_arrow()),
        Some(None)
    );
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn reaction_names_wait_for_the_review_sheet_to_close() {
    let (doc, arrow) = hydrogen_transfer();
    let mut app = app(doc, arrow);
    app.tab.selected.clear();
    app.inspector_open = false;
    let _ = app.update(Message::Xtb(XtbAction::Ts(TsAction::Review(
        review::Mode::Energy,
        None,
    ))));
    assert!(app.ts_review_visible(), "{}", app.status);
    assert_eq!(app.xtb_reaction_card_arrow(), Some(arrow));
    assert!(app.xtb_reaction_name_request().is_none());
    assert!(app.tab.naming.caption_pending.is_none());
    assert_eq!(app.start_caption_names().units(), 0);

    let _ = app.update(Message::Xtb(XtbAction::Ts(TsAction::ReviewSheet(
        review::Action::Cancel,
    ))));
    assert!(!app.ts_review_visible());
    assert!(
        app.tab.naming.caption_pending.is_some(),
        "closing the review resumes the visible card's naming queue"
    );
}

#[test]
fn identical_reaction_species_reuse_one_caption_name() {
    use crate::app::naming::{Outcome, test_support::record};
    let mut doc = Document::default();
    let left = molecule(&mut doc, &[("C", 1), ("C", 2)], &[(1, 2, 2)]);
    let right = molecule(&mut doc, &[("C", 1), ("C", 2)], &[(1, 2, 2)]);
    let arrow = assign(&mut doc, &[&left], &[&right]);
    let mut app = app(doc, arrow);
    show_reaction_card(&mut app);
    assert!(app.start_caption_names().units() > 0);
    let smiles = app.tab.naming.caption_pending.clone().unwrap();
    let mut named = record(&smiles);
    named.systematic_name = Some("ethene".into());
    app.caption_named(&smiles, Ok(Outcome::Structure(named)));
    assert_eq!(app.xtb_reaction_label(arrow).title(), "ethene → ethene");
    assert_eq!(app.start_caption_names().units(), 0);
}

#[test]
fn the_run_captures_the_drawn_map_numbers() {
    let (mut doc, arrow) = claisen();
    let plan = ts_plan::plan(&doc, arrow).unwrap();
    let bonds = BondChanges::from_plan(&doc, &plan);
    assert_eq!(pairs(&bonds.0.formed), ["C1–C6"]);
    assert_eq!(pairs(&bonds.0.broken), ["O3–C4"]);

    doc.atom_mut(plan.form[0].0).unwrap().map_num = 99;
    assert_eq!(
        pairs(&bonds.0.formed),
        ["C1–C6"],
        "edits must not change the run's map"
    );
}

#[test]
fn bond_marks_follow_the_visible_reaction_and_highlight_one_pair() {
    let (doc, arrow) = claisen();
    let [c1, _, o3, c4, _, c6] = doc.reactions[0].reactants[0].atoms[..] else {
        panic!("expected six mapped reactant atoms");
    };
    let mut app = app(doc, arrow);
    show_reaction_card(&mut app);
    let formed = BondChangeMark {
        kind: BondChangeKind::Form,
        atoms: [c1, c6],
        highlighted: false,
    };
    let broken = BondChangeMark {
        kind: BondChangeKind::Break,
        atoms: [o3, c4],
        highlighted: false,
    };
    assert_eq!(app.xtb_reaction_bond_marks(), [formed, broken]);

    app.view_open = true;
    let _ = app.update(Message::ReactionBondHover(
        arrow,
        BondChangeKind::Form,
        [1, 6],
        true,
    ));
    let _ = app.update(Message::ReactionBondHover(
        arrow,
        BondChangeKind::Break,
        [3, 4],
        true,
    ));
    let _ = app.update(Message::ReactionBondHover(
        arrow,
        BondChangeKind::Form,
        [1, 6],
        false,
    ));
    assert!(app.view_open, "hovering leaves open controls alone");
    app.view_open = false;
    assert_eq!(
        app.xtb_reaction_bond_marks(),
        [
            formed,
            BondChangeMark {
                highlighted: true,
                ..broken
            }
        ]
    );

    let _ = app.update(Message::Canvas(crate::canvas::Edit::Select(vec![c1])));
    assert!(app.xtb_reaction_bond_marks().is_empty());
    let _ = app.update(Message::Canvas(crate::canvas::Edit::Select(vec![arrow])));
    assert_eq!(app.inspector_tab, crate::app::InspectorTab::Properties);
    assert!(app.xtb_reaction_bond_marks().is_empty());
    show_reaction_card(&mut app);
    assert_eq!(app.xtb_reaction_bond_marks(), [formed, broken]);
    let _ = app.update(Message::ToggleInspector);
    assert!(app.xtb_reaction_bond_marks().is_empty());
}

#[test]
fn drawing_bond_marks_are_absent_from_the_saved_document() {
    let (doc, arrow) = claisen();
    let mut app = app(doc, arrow);
    let before = app.tab.doc.file_json().unwrap();
    show_reaction_card(&mut app);
    let _ = app.update(Message::ReactionBondHover(
        arrow,
        BondChangeKind::Form,
        [1, 6],
        true,
    ));
    assert!(app.xtb_reaction_bond_marks()[0].highlighted);
    assert_eq!(app.tab.doc.file_json().unwrap(), before);
}

#[test]
fn bond_change_colors_use_the_reaction_atoms_and_canvas_ink() {
    let (mut doc, arrow) = claisen();
    let reaction = reshiki::reactions::preview(&doc, arrow).unwrap();
    let plan = ts_plan::plan(&doc, arrow).unwrap();
    let bonds = BondChanges::from_plan(&doc, &plan);
    let label = &bonds.0.formed[0][0];
    let unrelated = doc.add_atom("C", Point::new(800., 0.));
    let atom = doc.atom_mut(unrelated).unwrap();
    atom.map_num = label.number;
    atom.display.mapping.style.color = Color::Custom([180, 50, 55]);
    assert_eq!(
        mapped_label_color(&doc, &reaction, label, Side::Product),
        Color::Ink,
        "an unrelated reaction's same-map color must not replace canvas Ink"
    );

    let product = reaction.products[0]
        .atoms
        .iter()
        .copied()
        .find(|&id| {
            doc.atom(id)
                .is_some_and(|atom| atom.map_num == label.number)
        })
        .unwrap();
    let product_color = Color::Custom([140, 65, 155]);
    doc.atom_mut(product).unwrap().display.mapping.style.color = product_color;
    assert_eq!(
        mapped_label_color(&doc, &reaction, label, Side::Product),
        product_color,
        "a formed bond uses the product's custom color"
    );

    let selected = reaction.reactants[0]
        .atoms
        .iter()
        .copied()
        .find(|&id| {
            doc.atom(id)
                .is_some_and(|atom| atom.map_num == label.number)
        })
        .unwrap();
    let custom = Color::Custom([17, 126, 108]);
    doc.atom_mut(selected).unwrap().display.mapping.style.color = custom;
    assert_eq!(
        mapped_label_color(&doc, &reaction, label, Side::Reactant),
        custom,
        "a selected reaction atom's custom color must match its canvas label"
    );
}

pub(in crate::app::xtb::ts) fn diels_alder() -> (Document, u64) {
    let mut doc = Document::default();
    let diene = molecule(
        &mut doc,
        &[("C", 1), ("C", 2), ("C", 3), ("C", 4)],
        &[(1, 2, 2), (2, 3, 1), (3, 4, 2)],
    );
    let ethylene = molecule(&mut doc, &[("C", 5), ("C", 6)], &[(5, 6, 2)]);
    let ring = molecule(
        &mut doc,
        &(1..=6).map(|number| ("C", number)).collect::<Vec<_>>(),
        &[
            (1, 2, 1),
            (2, 3, 2),
            (3, 4, 1),
            (4, 5, 1),
            (5, 6, 1),
            (6, 1, 1),
        ],
    );
    let arrow = assign(&mut doc, &[&diene, &ethylene], &[&ring]);
    (doc, arrow)
}

#[test]
fn diels_alder_lists_bonds_in_arrow_direction_when_search_starts_on_product() {
    let (doc, arrow) = diels_alder();
    let plan = ts_plan::plan(&doc, arrow).unwrap();
    assert_eq!(plan.start, Side::Product);
    let bonds = BondChanges::from_plan(&doc, &plan);
    assert_eq!(pairs(&bonds.0.formed), ["C1–C6", "C4–C5"]);
    assert!(bonds.0.broken.is_empty());
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[tokio::test]
#[ignore = "Opt-in renderer check: needs a graphics adapter"]
async fn drawn_product_stereo_adds_one_note_to_the_card_and_review() {
    let (mut doc, arrow) = claisen();
    let mut product_bond = (0, 0);
    for product in [false, true] {
        let side = if product {
            &doc.reactions[0].products[0]
        } else {
            &doc.reactions[0].reactants[0]
        };
        let mapped = |number| {
            side.atoms
                .iter()
                .copied()
                .find(|&id| doc.atom(id).unwrap().map_num == number)
                .unwrap()
        };
        let (center, next, far) = (mapped(1), mapped(2), mapped(6));
        let origin = doc.atom(center).unwrap().position;
        let fluorine = doc.add_atom("F", origin.offset(0., -28.));
        doc.atom_mut(fluorine).unwrap().map_num = 7;
        doc.add_bond(center, fluorine, 1, "plain");
        if product {
            doc.atom_mut(next).unwrap().position = origin.offset(-24., 14.);
            doc.atom_mut(far).unwrap().position = origin.offset(24., 14.);
            doc.reactions[0].products[0].atoms.push(fluorine);
            product_bond = (center, fluorine);
        } else {
            doc.reactions[0].reactants[0].atoms.push(fluorine);
        }
    }
    let ui = ViewHarness::new().await;
    let mut app = app(doc, arrow);
    let note = "Stereochemistry of the drawn product is not checked; compare it with the result.";
    for expected in [0, 1] {
        if expected == 1 {
            let before = app.tab.doc.clone();
            app.tab
                .doc
                .bonds
                .iter_mut()
                .find(|bond| (bond.a, bond.b) == product_bond)
                .unwrap()
                .display = "wedge".into();
            app.changed(before);
        }
        let card = ui.inspect(app.xtb_reaction_card(arrow));
        assert_eq!(
            card.texts.iter().filter(|text| *text == note).count(),
            expected
        );
        let _ = app.xtb_review_show(review::Mode::TransitionState, Some(arrow));
        assert!(app.ts_review_visible(), "{}", app.status);
        let sheet = ui.inspect(app.with_ts_review(text("").into()));
        assert_eq!(
            sheet.texts.iter().filter(|text| *text == note).count(),
            expected
        );
        let _ = app.xtb_review_action(review::Action::Cancel);
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
#[ignore = "Opt-in renderer check: needs a graphics adapter"]
async fn predict_view_switches_reaction_controls_with_the_selected_chemistry_arrow() {
    let (doc, arrow) = claisen();
    let atom = doc.atoms[0].id;
    let mut app = app(doc, arrow);
    app.xtb.page = xtb::Page::Xtb;
    let ui = ViewHarness::new().await.at_width(330.);
    let reaction = ui.inspect(app.predict_inspector());
    assert!(reaction.semantics.duplicate_ids.is_empty());
    for id in ["xtb-ts-review", "xtb-ts-calculate", "xtb-ts-barrier"] {
        assert_eq!(
            reaction.semantics.nodes.iter().any(|node| node.id == id),
            xtb::available(),
            "{id} must follow xTB availability"
        );
    }
    assert!(reaction.texts.iter().any(|text| text == "C5H8O → C5H8O"));
    for selection in [vec![atom], vec![arrow, atom], vec![]] {
        app.tab.selected = selection;
        let molecule = ui.inspect(app.predict_inspector());
        assert!(
            molecule
                .semantics
                .nodes
                .iter()
                .all(|node| !node.id.starts_with("xtb-ts-")),
            "a molecule or mixed selection must replace the reaction card"
        );
        assert!(molecule.texts.iter().all(|text| text != "C5H8O → C5H8O"));
    }
    app.tab.selected = vec![arrow];
    assert_eq!(
        ui.inspect(app.predict_inspector())
            .semantics
            .nodes
            .iter()
            .any(|node| node.id == "xtb-ts-calculate"),
        xtb::available(),
        "returning to the arrow restores the reaction controls"
    );
    app.tab.doc.arrows[0].kind = "resonance".into();
    assert!(
        ui.inspect(app.predict_inspector())
            .semantics
            .nodes
            .iter()
            .all(|node| !node.id.starts_with("xtb-ts-")),
        "a resonance arrow must not produce reaction calculation controls"
    );
}
