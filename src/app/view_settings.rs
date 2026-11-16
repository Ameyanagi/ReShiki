//! Window, camera and view-aid preferences.
use super::App;

/// View-aid commands, nested under `Message::View`.
/// Every View action passes the 3D preview through `gates::preview_passthrough`, and none commits a draft.
#[derive(Debug, Clone)]
pub enum Action {
    /// Flips the grid regardless of the checkbox value.
    Grid,
    SmartGuides(bool),
    TwoDecimalWeight(bool),
    NameTypography(bool),
    /// Put the editable drawing in copied and exported pictures by default.
    ExportEditableDrawing(bool),
    /// Keep text editable in plain SVG exports.
    ExportSvgLiveText(bool),
    Rulers(bool),
    Crosshair(bool),
    RulerUnit(crate::canvas::guides::Unit),
    /// Shows or hides the View options bar (was `ToggleView`).
    Toggle,
}

impl App {
    pub(super) fn view_action(&mut self, action: Action) {
        match action {
            Action::Grid => self.grid = !self.grid,
            Action::SmartGuides(enabled) => self.set_smart_guides(enabled),
            Action::TwoDecimalWeight(enabled) => self.set_two_decimal_weight(enabled),
            Action::NameTypography(enabled) => {
                self.appearance.name_typography = enabled;
                self.save_preference(
                    super::settings_page::Section::Names,
                    "chemical name typography",
                );
            }
            Action::ExportEditableDrawing(enabled) => {
                self.appearance.export_editable_drawing = enabled;
                // The Export tab follows the new default.
                self.export_tab_editable = None;
                self.save_preference(super::settings_page::Section::CopyExport, "export");
            }
            Action::ExportSvgLiveText(enabled) => {
                self.appearance.export_svg_live_text = enabled;
                self.save_preference(super::settings_page::Section::CopyExport, "export");
            }
            Action::Toggle => self.view_open = !self.view_open,
            Action::Rulers(enabled) => self.set_rulers(enabled),
            Action::Crosshair(enabled) => self.guides.crosshair = enabled,
            Action::RulerUnit(unit) => {
                self.guides.unit = unit;
                self.sync_arrow_label_gap_input();
            }
        }
    }
    pub(super) fn set_viewport(&mut self, size: iced::Size) {
        // Inspector width changes, such as docking NMR, do not request fitting
        // the drawing. Only suppress the expected layout transition; later
        // genuine window resizes keep their ordinary fit behavior.
        let preserve_camera = self.tab.inspector_ui.take_expected_viewport(size);
        if self.viewport != size {
            self.viewport = size;
            if !preserve_camera {
                if let Some(index) = self.tab.pages.fit {
                    self.fit_pages(index);
                } else if self.tab.fit_to_view {
                    self.fit();
                }
            }
        }
    }
    pub(super) fn set_appearance(&mut self, mode: crate::appearance::Mode) {
        self.appearance.mode = mode;
        self.save_preference(super::settings_page::Section::Appearance, "appearance");
    }
    pub(super) fn set_smart_guides(&mut self, enabled: bool) {
        self.appearance.smart_guides = enabled;
        self.save_preference(super::settings_page::Section::Drawing, "smart guides");
    }
    fn set_two_decimal_weight(&mut self, enabled: bool) {
        self.appearance.two_decimal_weight = enabled;
        // Mol. Wt. captions in every tab change at once, in one Undo step each.
        // During a preview or a text draft they change with the next edit.
        // Syncing a selected caption's text box leaves the inspector as shown.
        let inspector = (self.inspector_open, self.inspector_tab);
        self.each_tab(|app| {
            let tab = &app.tab;
            if !tab.busy
                && !tab.drawing_edit_active()
                && tab
                    .doc
                    .property_captions
                    .iter()
                    .any(|link| link.two_decimal_weight != enabled)
            {
                let before = app.tab.doc.clone();
                app.changed(before);
                app.sync_typography();
            }
        });
        (self.inspector_open, self.inspector_tab) = inspector;
        self.save_preference(
            super::settings_page::Section::Names,
            "molecular weight display",
        );
    }
    pub(super) fn set_rulers(&mut self, enabled: bool) {
        self.guides.rulers = enabled;
        if self.tab.fit_to_view {
            self.fit();
        }
    }
    pub(super) fn zoom_by(&mut self, factor: f32) {
        self.tab.pages.fit = None;
        self.tab.fit_to_view = false;
        self.tab.camera.zoom = (self.tab.camera.zoom * factor).clamp(0.005, 5.0);
    }
}
