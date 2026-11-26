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
use reshiki::reactions::{Participant, Reaction};
use std::collections::BTreeMap;

/// Inspect the widgets a production view actually creates, including lazy cards.
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

#[test]
fn diels_alder_lists_bonds_in_arrow_direction_when_search_starts_on_product() {
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
    let plan = ts_plan::plan(&doc, arrow).unwrap();
    assert_eq!(plan.start, Side::Product);
    let bonds = BondChanges::from_plan(&doc, &plan);
    assert_eq!(pairs(&bonds.0.formed), ["C1–C6", "C4–C5"]);
    assert!(bonds.0.broken.is_empty());
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn predict_view_switches_reaction_controls_with_the_selected_chemistry_arrow() {
    let (doc, arrow) = claisen();
    let atom = doc.atoms[0].id;
    let mut app = app(doc, arrow);
    app.xtb.page = xtb::Page::Xtb;
    let ui = ViewHarness::new().await;
    let reaction = ui.inspect(app.predict_inspector());
    assert!(reaction.semantics.duplicate_ids.is_empty());
    for id in ["xtb-ts-review", "xtb-ts-find", "xtb-ts-export-input"] {
        assert!(
            reaction.semantics.nodes.iter().any(|node| node.id == id),
            "the selected reaction must produce {id} in Predict"
        );
    }
    assert!(
        reaction
            .texts
            .iter()
            .any(|text| text.starts_with("Reaction (xTB) · "))
    );
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
        assert!(
            molecule
                .texts
                .iter()
                .all(|text| !text.starts_with("Reaction (xTB) · "))
        );
    }
    app.tab.selected = vec![arrow];
    assert!(
        ui.inspect(app.predict_inspector())
            .semantics
            .nodes
            .iter()
            .any(|node| node.id == "xtb-ts-find"),
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
