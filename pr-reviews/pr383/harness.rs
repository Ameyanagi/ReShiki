
// ---- Issue #380 evidence harness (appended for the capture, not committed) ----
#[tokio::test]
#[ignore = "Issue #380 evidence harness"]
async fn evidence_380_name_import_notice() {
    #[allow(unused_imports)]
    use iced::advanced::Widget;
    use iced::advanced::renderer::Headless;
    use iced::advanced::widget::Tree;
    use iced::advanced::layout;
    use iced::mouse;
    let out = std::path::PathBuf::from(std::env::var("EV380_OUT").unwrap());
    std::fs::create_dir_all(&out).unwrap();
    for (slug, name) in [
        ("sodium-chloride", "sodium chloride"),
        ("sodium-acetate", "sodium acetate"),
        ("nonsense", "reshiki-no-such-chemical-name-50"),
    ] {
        let mut app = ready();
        let _ = app.naming_action(Action::Name(name.into()));
        let _ = app.insert_input();
        let ticket = app.tab.naming.pending.unwrap().ticket();
        let result =
            resolve_preview(app.engine.clone(), name.into(), naming::Cancel::default()).await;
        let error = result.as_ref().err().cloned();
        let _ = app.naming_action(Action::Finished(ticket, Box::new(result)));
        app.tab.naming.details_open = true; // Details expanded, as after the user reopens it and the re-resolve finishes
        let notice = app.tab.naming.notice.as_ref().unwrap();
        println!("EV380 name: {name}");
        println!("EV380   naming::Error: {error:?}");
        println!("EV380   NoticeKind: {:?}", notice.kind);
        println!("EV380   Summary (visible): {}", notice.summary());
        println!("EV380   Details text: {}", notice.detail);
        println!("EV380   Drawing atoms after Insert: {}", app.tab.doc.atoms.len());
        let (width, height) = (320u32, 560u32);
        let mut renderer = <iced::Renderer as Headless>::new(
            iced::Font::with_name(reshiki::style::ui_font_family()),
            iced::Pixels(16.),
            None,
        )
        .await
        .unwrap();
        let size = iced::Size::new(width as f32, height as f32);
        let theme = app.theme();
        let background = theme.palette().background;
        let mut view: Element<'_, Message> = iced::widget::container(app.import_panel())
            .padding(16)
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
        let mut tree = Tree::new(view.as_widget());
        let node = view
            .as_widget_mut()
            .layout(&mut tree, &renderer, &layout::Limits::new(size, size));
        view.as_widget().draw(
            &tree,
            &mut renderer,
            &theme,
            &iced::advanced::renderer::Style::default(),
            iced::advanced::Layout::new(&node),
            mouse::Cursor::Unavailable,
            &iced::Rectangle::with_size(size),
        );
        let pixels = Headless::screenshot(
            &mut renderer,
            iced::Size::new(width, height),
            1.,
            background,
        );
        image::save_buffer(
            out.join(format!("{slug}.png")),
            &pixels,
            width,
            height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}
