//! Window, camera and view-aid preferences.
use super::App;

impl App {
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
