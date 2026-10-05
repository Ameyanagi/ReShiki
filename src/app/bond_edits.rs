//! Bond drawing settings and edits to selected bonds.
use super::App;
use crate::canvas::Tool;

impl App {
    pub(super) fn reset_bond_drawing(&mut self) {
        self.tab.bond_drawing = Default::default();
        self.tab.bond_drawing.length = self.tab.doc.drawing_style.bond_length_world;
        self.tab.drawing_length_input = self.tab.doc.drawing_style.bond_length_pt.to_string();
        self.tab.chain_drawing.angle = 120.;
        self.tab.chain_angle_input = "120".into();
        self.status = format!(
            "{} bond defaults · {} pt length · 120° chain angle",
            self.tab.doc.drawing_style.name, self.tab.doc.drawing_style.bond_length_pt
        );
        self.error = false;
    }
    pub(super) fn set_drawing_length(&mut self, value: String) {
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
    pub(super) fn set_chain_atoms(&mut self, value: String) {
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
            self.status = "Enter 1–512 chain atoms, or clear the field for automatic length".into();
            self.error = true;
        }
    }
    pub(super) fn set_chain_angle(&mut self, value: String) {
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
    pub(super) fn apply_bond_preset(&mut self, preset: reshiki::bonds::BondPreset) {
        if preset == reshiki::bonds::BondPreset::Dotted
            && self.tab.doc.bonds.iter().any(|b| {
                self.tab.selected.contains(&b.a)
                    && self.tab.selected.contains(&b.b)
                    && !reshiki::bonds::hydrogen_endpoints(&self.tab.doc, b.a, b.b)
            })
        {
            self.status = "Hydrogen bonds need a bonded explicit H and an acceptor".into();
            self.error = true;
            return;
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
    pub(super) fn set_double_position(&mut self, position: reshiki::bonds::DoublePosition) {
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
    pub(super) fn apply_bond_color(&mut self) {
        if let Some(rgb) = super::graphics::parse_color(&self.tab.bond_color_input) {
            let color = reshiki::palette::Color::Custom(rgb);
            let before = self.tab.doc.clone();
            for bond in &mut self.tab.doc.bonds {
                if self.tab.selected.contains(&bond.a) && self.tab.selected.contains(&bond.b) {
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
    pub(super) fn reverse_selected_bonds(&mut self) {
        let before = self.tab.doc.clone();
        self.tab.doc.invalidate_chemistry(&self.tab.selected);
        for b in &mut self.tab.doc.bonds {
            if self.tab.selected.contains(&b.a) && self.tab.selected.contains(&b.b) {
                b.reverse();
            }
        }
        self.changed(before);
    }
    pub(super) fn sync_bonds(&mut self) {
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
    pub(super) fn apply_current_bond_preset(&mut self, a: u64, b: u64) {
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
    pub(super) fn bond_style(&self) -> (u8, &'static str) {
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
}
