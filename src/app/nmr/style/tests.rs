use super::*;
use iced::{Color, Vector};

#[test]
fn nmr_surfaces_keep_the_platform_shadow_policy_and_theme() {
    for theme in [Theme::Light, Theme::Dark] {
        let style = surface(&theme);
        let dark = crate::appearance::is_dark(&theme);
        assert_eq!(
            style.shadow,
            if cfg!(windows) {
                Shadow::default()
            } else {
                Shadow {
                    color: Color::from_rgba8(20, 40, 35, 0.18),
                    offset: Vector::new(0., 5.),
                    blur_radius: 18.,
                }
            }
        );
        assert_eq!(style.text_color, Some(theme.palette().text));
        assert_eq!(style.border.width, 1.);
        assert_eq!(style.border.radius, 10.into());
        assert_eq!(
            style.border.color,
            crate::appearance::themed(&theme, Color::from_rgb8(192, 204, 201))
        );
        assert_eq!(
            style.background,
            Some(
                if dark {
                    Color::from_rgb8(24, 28, 34)
                } else {
                    Color::WHITE
                }
                .into()
            )
        );
        let table = table_surface(&theme);
        assert_eq!(table.shadow, Shadow::default());
        assert_eq!(table.background, style.background);
        assert_eq!(table.border.color, style.border.color);
        assert_eq!(table.border.radius, 6.into());
    }
}
