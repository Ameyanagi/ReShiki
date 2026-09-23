//! The Reaction Cycle mark shared by the desktop app and release packages.

use iced::{
    Alignment, Element,
    widget::{image, row, text},
};
use std::sync::OnceLock;

const MARK: &[u8] = include_bytes!("../assets/branding/runtime/mark.png");
const ICON: &[u8] = include_bytes!("../assets/branding/runtime/icon.png");

pub fn wordmark<'a, Message: 'a>(size: f32) -> Element<'a, Message> {
    static MARK_HANDLE: OnceLock<Option<image::Handle>> = OnceLock::new();
    let handle = MARK_HANDLE.get_or_init(|| {
        let pixels = ::image::load_from_memory(MARK).ok()?.into_rgba8();
        Some(image::Handle::from_rgba(
            pixels.width(),
            pixels.height(),
            pixels.into_raw(),
        ))
    });
    let mut brand = row![];
    if let Some(handle) = handle {
        brand = brand.push(image(handle.clone()).width(size * 1.5).height(size * 1.5));
    }
    brand
        .push(text("ReShiki").size(size))
        .spacing(4)
        .align_y(Alignment::Center)
        .into()
}

pub fn window_icon() -> Option<iced::window::Icon> {
    let pixels = ::image::load_from_memory(ICON).ok()?.into_rgba8();
    let (width, height) = pixels.dimensions();
    iced::window::icon::from_rgba(pixels.into_raw(), width, height).ok()
}
