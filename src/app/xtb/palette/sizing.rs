//! The bottom dock's window budget and short-window collapse behavior.
use super::super::results::View;
use crate::app::App;

pub(super) const MAX_HEIGHT_FRACTION: f32 = 0.4;
const SHORT_WINDOW_HEIGHT: f32 = 800.;
const BODY_HEIGHT: f32 = 150.;
const MIN_BODY_HEIGHT: f32 = 60.;
/// Header, method/caveat lines, footer, spacing and the dock's outer padding.
/// The outer scrollable also caps unusually tall warnings and wrapped notes.
const CHROME_HEIGHT_ESTIMATE: f32 = 150.;

pub(super) fn body_height(view: View, window_height: f32) -> f32 {
    let default = if view == View::Orbitals {
        super::super::orbitals::HEIGHT
    } else {
        BODY_HEIGHT
    };
    default.min((window_height * MAX_HEIGHT_FRACTION - CHROME_HEIGHT_ESTIMATE).max(MIN_BODY_HEIGHT))
}

impl App {
    /// Opening in a short window starts with the header; the inspector has
    /// its own scrollable area and keeps its usual expanded presentation.
    pub(in crate::app) fn xtb_palette_opened(&mut self) {
        if self.tabs.temporary_depth == 0
            && self.tab.xtb.palette.open
            && !self.xtb.dock.inspector
            && self.accessibility.viewport().height < SHORT_WINDOW_HEIGHT
        {
            self.xtb.dock.collapsed = true;
        }
    }

    /// Only crossing the threshold collapses an open bottom dock. A manual
    /// expansion below it survives subsequent short-window resize events.
    pub(in crate::app) fn xtb_palette_resized(&mut self, size: iced::Size) {
        if self.accessibility.viewport().height >= SHORT_WINDOW_HEIGHT
            && size.height < SHORT_WINDOW_HEIGHT
            && self.tab.xtb.palette.open
            && !self.xtb.dock.inspector
        {
            self.xtb.dock.collapsed = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Message;
    use crate::app::accessibility::Action as WindowAction;
    use crate::app::xtb::palette::Action;
    use crate::app::xtb::tests::{MOVED, START, event, optimized, water};
    use crate::app::xtb::{Action as XtbAction, CalcTask};
    use iced::{Size, window};

    fn window(app: &mut App, height: f32) -> window::Id {
        let id = window::Id::unique();
        let size = Size::new(1040., height);
        let _ = app.accessibility_action(WindowAction::Window(
            id,
            window::Event::Opened {
                position: None,
                size,
            },
        ));
        let _ = app.accessibility_action(WindowAction::Geometry(id, size, 1.));
        id
    }

    fn resize(app: &mut App, id: window::Id, height: f32) {
        let _ = app.accessibility_action(WindowAction::Window(
            id,
            window::Event::Resized(Size::new(1040., height)),
        ));
    }

    fn palette(app: &mut App, action: Action) {
        let _ = app.update(Message::Xtb(XtbAction::Palette(action)));
    }

    #[test]
    fn opening_a_bottom_palette_below_800_starts_collapsed() {
        for height in [712., 799., 800., 900.] {
            let (mut app, _) = water(CalcTask::Optimize);
            window(&mut app, height);
            event(&mut app, optimized(&START, &MOVED));
            assert!(app.tab.xtb.palette.open);
            assert_eq!(app.xtb.dock.collapsed, height < 800., "height {height}");
            palette(&mut app, Action::Collapse);
            palette(&mut app, Action::Close);
            palette(&mut app, Action::Show);
            if height < 800. {
                assert!(app.xtb.dock.collapsed, "reopening at height {height}");
            }
        }
    }

    #[test]
    fn only_crossing_below_800_collapses_a_manually_expanded_palette() {
        let (mut app, _) = water(CalcTask::Optimize);
        let id = window(&mut app, 900.);
        event(&mut app, optimized(&START, &MOVED));
        assert!(!app.xtb.dock.collapsed);
        resize(&mut app, id, 800.);
        assert!(!app.xtb.dock.collapsed);
        resize(&mut app, id, 799.);
        assert!(app.xtb.dock.collapsed);
        palette(&mut app, Action::Collapse);
        assert!(!app.xtb.dock.collapsed);
        for height in [712., 760., 799.] {
            resize(&mut app, id, height);
            assert!(!app.xtb.dock.collapsed, "manual expansion at {height}");
        }
        resize(&mut app, id, 820.);
        resize(&mut app, id, 712.);
        assert!(app.xtb.dock.collapsed, "a new threshold crossing collapses");
    }

    #[test]
    fn short_windows_leave_the_inspector_expanded_until_docked_at_bottom() {
        let (mut app, _) = water(CalcTask::Optimize);
        let id = window(&mut app, 900.);
        app.xtb.dock.inspector = true;
        event(&mut app, optimized(&START, &MOVED));
        resize(&mut app, id, 712.);
        assert!(!app.xtb.dock.collapsed);
        palette(&mut app, Action::Close);
        palette(&mut app, Action::Show);
        assert!(!app.xtb.dock.collapsed);
        palette(&mut app, Action::Dock);
        assert!(!app.xtb.dock.inspector);
        assert!(app.xtb.dock.collapsed);
    }

    #[test]
    fn background_results_leave_the_visible_windows_dock_choice_alone() {
        let (mut app, _) = water(CalcTask::Optimize);
        window(&mut app, 712.);
        app.tabs.temporary_depth = 1;
        event(&mut app, optimized(&START, &MOVED));
        assert!(app.tab.xtb.palette.open);
        assert!(
            !app.xtb.dock.collapsed,
            "background completion preserves the dock"
        );
        app.tabs.temporary_depth = 0;
        app.xtb_palette_opened();
        assert!(
            app.xtb.dock.collapsed,
            "the result collapses when made visible"
        );
        palette(&mut app, Action::Close);
        app.xtb.dock.collapsed = false;
        app.xtb_palette_opened();
        assert!(
            !app.xtb.dock.collapsed,
            "a closed palette preserves the dock"
        );
    }

    #[test]
    fn the_body_budget_keeps_the_default_height_and_a_usable_floor() {
        assert_eq!(body_height(View::Summary, 900.), BODY_HEIGHT);
        for view in [View::Summary, View::Orbitals] {
            let height = body_height(view, 712.);
            assert!(height >= MIN_BODY_HEIGHT);
            assert!(height + CHROME_HEIGHT_ESTIMATE <= 712. * MAX_HEIGHT_FRACTION);
            assert_eq!(body_height(view, 400.), MIN_BODY_HEIGHT);
        }
    }
}
