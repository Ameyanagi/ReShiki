use super::*;
pub(super) fn parse_color(s: &str) -> Option<[u8; 3]> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 || !s.is_ascii() {
        return None;
    }
    Some([
        u8::from_str_radix(s.get(0..2)?, 16).ok()?,
        u8::from_str_radix(s.get(2..4)?, 16).ok()?,
        u8::from_str_radix(s.get(4..6)?, 16).ok()?,
    ])
}
impl App {
    pub(super) fn sync_graphics(&mut self) {
        self.sync_pictures();
        if let Some(g) = self
            .doc
            .graphics
            .iter()
            .find(|g| self.selected.contains(&g.id))
        {
            self.graphic_style = g.style.clone();
            self.orbital_phase = g.phase;
            self.phase_flipped = g.phase_flipped;
            self.bracket_sides = g.sides;
            if !matches!(
                self.inspector_tab,
                InspectorTab::Templates
                    | InspectorTab::Reactions
                    | InspectorTab::Assistant
                    | InspectorTab::Pages
            ) {
                self.inspector_open = true;
                self.inspector_tab = InspectorTab::Properties;
            }
        }
        self.graphic_width_input = self.graphic_style.width_pt.to_string();
        let hex = |c: [u8; 3]| format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2]);
        self.graphic_stroke_input = hex(self.graphic_style.stroke);
        if let Some(c) = self.graphic_style.fill {
            self.graphic_fill_input = hex(c);
        }
    }
    pub(super) fn apply_graphic_style(&mut self, change: GraphicChange) {
        change.apply(&mut self.graphic_style);
        let before = self.doc.clone();
        for g in &mut self.doc.graphics {
            if self.selected.contains(&g.id) && g.picture.is_none() {
                change.apply(&mut g.style);
            }
        }
        self.changed(before);
        self.sync_graphics();
    }
}
