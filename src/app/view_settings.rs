//! Window, camera and view-aid preferences.
use super::App;

/// View-aid commands, nested under `Message::View`.
/// Every View action passes both previews through `gates::preview_passthrough`, and none commits a draft.
#[derive(Debug, Clone)]
pub enum Action {
    /// Flips the grid regardless of the checkbox value.
    Grid,
    SmartGuides(bool),
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
            Action::Toggle => self.view_open = !self.view_open,
            Action::Rulers(enabled) => self.set_rulers(enabled),
            Action::Crosshair(enabled) => self.guides.crosshair = enabled,
            Action::RulerUnit(unit) => self.guides.unit = unit,
        }
    }
    pub(super) fn set_viewport(&mut self, size: iced::Size) {
        if self.viewport != size {
            self.viewport = size;
            if let Some(index) = self.tab.pages.fit {
                self.fit_pages(index);
            } else if self.tab.fit_to_view {
                self.fit();
            }
        }
    }
    pub(super) fn set_appearance(&mut self, mode: crate::appearance::Mode) {
        self.appearance.mode = mode;
        if let Err(error) = self.appearance.save() {
            self.status = format!("Appearance changed, but could not save preference: {error}");
            self.error = true;
        }
    }
    pub(super) fn set_smart_guides(&mut self, enabled: bool) {
        self.appearance.smart_guides = enabled;
        if let Err(error) = self.appearance.save() {
            self.status = format!("Could not save smart guides preference: {error}");
            self.error = true;
        }
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
