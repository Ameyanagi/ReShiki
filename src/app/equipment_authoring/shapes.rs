//! Per-shape controls for the detached piece draft.
use super::*;
use reshiki::equipment::authoring::{ShapeStyle, closed_shape};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Choice(pub usize);

impl std::fmt::Display for Choice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Shape {}", self.0 + 1)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Named<T>(T, &'static str);

impl<T> std::fmt::Display for Named<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.1)
    }
}

const LAYERS: [Named<Layer>; 4] = [
    Named(Layer::Back, "Back"),
    Named(Layer::Body, "Body"),
    Named(Layer::Inner, "Inner"),
    Named(Layer::Front, "Front"),
];
const MATERIALS: [Named<Material>; 13] = [
    Named(Material::Glass, "Glass"),
    Named(Material::Frosted, "Frosted"),
    Named(Material::Ptfe, "PTFE"),
    Named(Material::Rubber, "Rubber"),
    Named(Material::Plastic, "Plastic"),
    Named(Material::Metal, "Metal"),
    Named(Material::Porcelain, "Porcelain"),
    Named(Material::Cork, "Cork"),
    Named(Material::Paper, "Paper"),
    Named(Material::Silica, "Silica"),
    Named(Material::Sand, "Sand"),
    Named(Material::Liquid, "Liquid"),
    Named(Material::Ink, "Ink"),
];
const ROLES: [Named<Role>; 4] = [
    Named(Role::Glass, "Joined glass"),
    Named(Role::Solid, "Solid"),
    Named(Role::Detail, "Detail"),
    Named(Role::Outline, "Outline"),
];

impl State {
    pub(super) fn refresh_shape(&mut self) {
        self.shape_outline = self
            .current
            .draft
            .definition
            .shapes
            .get(self.selected_shape)
            .and_then(|s| self.current.draft.shape_path(&s.id).ok())
            .unwrap_or_default();
    }

    pub(super) fn edit_shape(&mut self, action: Action) -> Result<(), String> {
        // A pending joint owns a candidate definition. Finish or cancel it
        // before changing the geometry on which that candidate was checked.
        if self.current.joint.is_some() {
            return Err("Finish or cancel the joint before changing a shape.".into());
        }
        let shape = self
            .current
            .draft
            .definition
            .shapes
            .get(self.selected_shape)
            .ok_or("Select a shape in the equipment draft.")?;
        let mut style = ShapeStyle::from(shape);
        match action {
            Action::ShapeLayer(layer) => style.layer = layer,
            Action::ShapeMaterial(material) => {
                style.material = material;
                if style.role == Role::Glass && material != Material::Glass {
                    style.role = Role::Solid;
                }
            }
            Action::ShapeRole(role) => style.role = role,
            _ => return Ok(()),
        }
        if style == ShapeStyle::from(shape) {
            return Ok(());
        }
        let mut draft = (*self.current.draft).clone();
        draft.set_shape_style(&shape.id, style)?;
        self.checkpoint();
        self.current.draft = Arc::new(draft);
        Ok(())
    }

    pub(super) fn shapes_panel(&self) -> Element<'_, Message> {
        let shapes = &self.current.draft.definition.shapes;
        let Some(shape) = shapes.get(self.selected_shape) else {
            return column![].into();
        };
        let mut body = column![
            section("Shapes"),
            crate::appearance::pick_list(
                (0..shapes.len()).map(Choice).collect::<Vec<_>>(),
                Some(Choice(self.selected_shape)),
                |choice| Message::EquipmentAuthoring(Action::SelectShape(choice)),
            )
            .text_size(12),
        ]
        .spacing(7);
        if self.current.joint.is_some() {
            return body
                .push(text("Finish or cancel the joint to edit a shape.").size(12))
                .into();
        }
        body = body
            .push(text("Layer").size(12))
            .push(
                crate::appearance::pick_list(
                    LAYERS.as_slice(),
                    LAYERS.iter().find(|v| v.0 == shape.layer).copied(),
                    |value| Message::EquipmentAuthoring(Action::ShapeLayer(value.0)),
                )
                .text_size(12),
            )
            .push(text("Material").size(12))
            .push(
                crate::appearance::pick_list(
                    MATERIALS.as_slice(),
                    MATERIALS.iter().find(|v| v.0 == shape.material).copied(),
                    |value| Message::EquipmentAuthoring(Action::ShapeMaterial(value.0)),
                )
                .text_size(12),
            );
        let closed = closed_shape(shape).unwrap_or(false);
        let roles: Vec<_> = ROLES
            .into_iter()
            .filter(|v| {
                (!v.0.filled() || closed)
                    && (v.0 != Role::Glass || shape.material == Material::Glass)
            })
            .collect();
        body = body
            .push(text("Draw as").size(12))
            .push(
                crate::appearance::pick_list(
                    roles,
                    ROLES.iter().find(|v| v.0 == shape.role).copied(),
                    |value| Message::EquipmentAuthoring(Action::ShapeRole(value.0)),
                )
                .text_size(12),
            )
            .push(
                text(if closed {
                    "Joined glass merges Body outlines. Solid keeps its own outline."
                } else {
                    "Open path: Detail is a fine stroke; Outline uses the full line weight."
                })
                .size(12),
            );
        if !self.is_measuring() && self.trial.is_none() {
            body = body.push(text("Click a shape in the drawing to select it.").size(12));
        }
        body.into()
    }
}
