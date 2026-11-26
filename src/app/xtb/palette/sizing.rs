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
        if self.tab.xtb.palette.open
            && !self.xtb.dock.inspector
            && self.xtb_failed().is_none()
            && self.window_size.height < SHORT_WINDOW_HEIGHT
        {
            self.tab.xtb.palette.collapsed = true;
        }
    }

    /// Only crossing the threshold collapses an open bottom dock. A manual
    /// expansion below it survives subsequent short-window resize events.
    pub(in crate::app) fn xtb_palette_resized(&mut self, size: iced::Size) {
        if self.window_size.height >= SHORT_WINDOW_HEIGHT
            && size.height < SHORT_WINDOW_HEIGHT
            && self.tab.xtb.palette.open
            && !self.xtb.dock.inspector
            && self.xtb_failed().is_none()
        {
            self.tab.xtb.palette.collapsed = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Message;
    use crate::app::xtb::palette::Action;
    use crate::app::xtb::tests::{MOVED, START, event, optimized, water};
    use crate::app::xtb::{Action as XtbAction, CalcTask, Failure, RunEvent};
    use iced::{Size, window};

    fn window_event(app: &mut App, event: window::Event) {
        let viewport = app.viewport;
        let message = crate::app::window_size_event(&event).expect("logical window size message");
        let Message::WindowSize(size) = message else {
            unreachable!()
        };
        let _ = app.update(message);
        assert_eq!(app.window_size, size);
        assert_eq!(
            app.viewport, viewport,
            "the drawing viewport is independent"
        );
    }

    fn window(app: &mut App, height: f32) {
        window_event(
            app,
            window::Event::Opened {
                position: None,
                size: Size::new(1040., height),
            },
        );
    }

    fn resize(app: &mut App, height: f32) {
        window_event(app, window::Event::Resized(Size::new(1040., height)));
    }

    fn palette(app: &mut App, action: Action) {
        let _ = app.update(Message::Xtb(XtbAction::Palette(action)));
    }

    #[test]
    fn first_resize_tracks_logical_size_before_editor_gates() {
        let (mut app, _) = App::new();
        app.tab.busy = true;
        app.help_open = true;
        resize(&mut app, 712.);
        assert_eq!(app.window_size, Size::new(1040., 712.));
    }

    #[test]
    fn opening_a_bottom_palette_below_800_starts_collapsed() {
        for height in [712., 799., 800., 900.] {
            let (mut app, _) = water(CalcTask::Optimize);
            window(&mut app, height);
            event(&mut app, optimized(&START, &MOVED));
            assert!(app.tab.xtb.palette.open);
            assert_eq!(
                app.tab.xtb.palette.collapsed,
                height < 800.,
                "height {height}"
            );
            palette(&mut app, Action::Collapse);
            palette(&mut app, Action::Close);
            palette(&mut app, Action::Show);
            if height < 800. {
                assert!(
                    app.tab.xtb.palette.collapsed,
                    "reopening at height {height}"
                );
            }
        }
    }

    #[test]
    fn only_crossing_below_800_collapses_a_manually_expanded_palette() {
        let (mut app, _) = water(CalcTask::Optimize);
        window(&mut app, 900.);
        event(&mut app, optimized(&START, &MOVED));
        assert!(!app.tab.xtb.palette.collapsed);
        resize(&mut app, 800.);
        assert!(!app.tab.xtb.palette.collapsed);
        resize(&mut app, 799.);
        assert!(app.tab.xtb.palette.collapsed);
        palette(&mut app, Action::Collapse);
        assert!(!app.tab.xtb.palette.collapsed);
        for height in [712., 760., 799.] {
            resize(&mut app, height);
            assert!(
                !app.tab.xtb.palette.collapsed,
                "manual expansion at {height}"
            );
        }
        resize(&mut app, 820.);
        resize(&mut app, 712.);
        assert!(
            app.tab.xtb.palette.collapsed,
            "a new threshold crossing collapses"
        );
    }

    #[test]
    fn short_windows_leave_the_inspector_expanded_until_docked_at_bottom() {
        let (mut app, _) = water(CalcTask::Optimize);
        window(&mut app, 900.);
        app.xtb.dock.inspector = true;
        event(&mut app, optimized(&START, &MOVED));
        resize(&mut app, 712.);
        assert!(!app.tab.xtb.palette.collapsed);
        palette(&mut app, Action::Close);
        palette(&mut app, Action::Show);
        assert!(!app.tab.xtb.palette.collapsed);
        palette(&mut app, Action::Dock);
        assert!(!app.xtb.dock.inspector);
        assert!(app.tab.xtb.palette.collapsed);
    }

    #[test]
    fn background_results_initialize_their_own_collapsed_state() {
        let (mut app, _) = water(CalcTask::Optimize);
        window(&mut app, 712.);
        let background = app.tab.id;
        let serial = app.tab.xtb.run.as_ref().expect("a run in flight").serial;
        app.add_tab();
        let foreground = app.tab.id;
        assert!(!app.tab.xtb.palette.collapsed);
        let _ = app.update(Message::Tab(
            background,
            Box::new(Message::Xtb(XtbAction::Event(
                serial,
                optimized(&START, &MOVED),
            ))),
        ));
        assert_eq!(app.tab.id, foreground);
        assert!(
            !app.tab.xtb.palette.collapsed,
            "the foreground stays expanded"
        );
        assert!(!app.tab.xtb.palette.open);
        let completed = app
            .strip()
            .find(|tab| tab.id == background)
            .expect("background tab");
        assert!(completed.xtb.palette.open);
        assert!(
            completed.xtb.palette.collapsed,
            "the background result starts collapsed"
        );
        app.select_tab(0);
        assert_eq!(app.tab.id, background);
        assert!(app.tab.xtb.palette.collapsed);
        palette(&mut app, Action::Close);
        app.tab.xtb.palette.collapsed = false;
        app.xtb_palette_opened();
        assert!(
            !app.tab.xtb.palette.collapsed,
            "a closed palette preserves its state"
        );
    }

    #[test]
    fn resizing_or_bottom_docking_keeps_failure_recovery_expanded() {
        for inspector in [false, true] {
            let (mut app, _) = water(CalcTask::Optimize);
            window(&mut app, 900.);
            app.xtb.dock.inspector = inspector;
            event(
                &mut app,
                RunEvent::Done(Err(Failure::Other("deliberate sizing failure".into()))),
            );
            assert!(!app.tab.xtb.palette.collapsed);
            resize(&mut app, 712.);
            assert!(
                !app.tab.xtb.palette.collapsed,
                "failure remains visible after resize"
            );
            if inspector {
                palette(&mut app, Action::Dock);
                assert!(!app.xtb.dock.inspector);
                assert!(
                    !app.tab.xtb.palette.collapsed,
                    "failure remains visible after docking"
                );
            }
            palette(&mut app, Action::Collapse);
            assert!(
                app.tab.xtb.palette.collapsed,
                "manual collapse remains available"
            );
        }
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
