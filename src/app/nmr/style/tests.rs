use super::*;

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
                    color: Color::BLACK.scale_alpha(if dark { 0.2 } else { 0.08 }),
                    offset: Vector::new(0., 3.),
                    blur_radius: 12.,
                }
            }
        );
        assert_eq!(style.text_color, Some(theme.palette().text));
        assert_eq!(style.border.width, 1.);
        assert_eq!(style.border.radius, 6.into());
        assert_eq!(
            style.background,
            Some(
                if dark {
                    theme.extended_palette().background.base.color
                } else {
                    Color::WHITE
                }
                .into()
            )
        );
        assert_eq!(table_surface(&theme).shadow, Shadow::default());
    }
}
