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
        self.sync_arc();
        if let Some(g) = self
            .tab
            .doc
            .graphics
            .iter()
            .find(|g| self.tab.selected.contains(&g.id))
        {
            self.tab.graphic_style = g.style.clone();
            self.tab.orbital_phase = g.phase;
            self.tab.phase_flipped = g.phase_flipped;
            self.tab.bracket_sides = g.sides;
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
        self.tab.graphic_width_input = self.tab.graphic_style.width_pt.to_string();
        let palette = reshiki::palette::Palette::of(&self.tab.doc);
        let hex = |c| reshiki::palette::hex(palette.rgb(c));
        self.tab.graphic_stroke_input = hex(self.tab.graphic_style.stroke);
        if let Some(c) = self.tab.graphic_style.fill {
            self.tab.graphic_fill_input = hex(c);
        }
    }
    pub(super) fn apply_graphic_style(&mut self, change: GraphicChange) {
        change.apply(&mut self.tab.graphic_style);
        let before = self.tab.doc.clone();
        for g in &mut self.tab.doc.graphics {
            if self.tab.selected.contains(&g.id) && g.picture.is_none() {
                change.apply(&mut g.style);
            }
        }
        if let GraphicChange::Stroke(color) | GraphicChange::Fill(Some(color)) = change {
            self.remember_custom(Some(color), &before);
        }
        self.changed(before);
        self.sync_graphics();
    }
}
