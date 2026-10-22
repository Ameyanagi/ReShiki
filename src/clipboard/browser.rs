//! The browser's clipboard. A page may write only text/plain, text/html,
//! image/png and, in Chromium, image/svg+xml, and read the same back, so PDF
//! and CDX stay with the desktop app. The editable drawing travels in the
//! HTML flavor's picture, which other apps show as a plain image.
use super::{NATIVE, Representation};
use base64::{Engine as _, engine::general_purpose::STANDARD};

const ATTRIBUTE: &str = "data-reshiki";

/// The web types for prepared clipboard representations, in writing order.
pub(super) fn web_items(
    representations: &[Representation],
) -> Result<Vec<(&'static str, Vec<u8>)>, String> {
    let find = |kind: &str| {
        representations
            .iter()
            .find(|item| item.kind == kind)
            .map(Representation::bytes)
            .transpose()
    };
    let mut items = Vec::new();
    if let Some(png) = find("public.png")? {
        items.push((
            "text/html",
            html(&png, find(NATIVE)?.as_deref()).into_bytes(),
        ));
        items.push(("image/png", png));
    }
    if let Some(svg) = find("public.svg-image")? {
        items.push(("image/svg+xml", svg));
    }
    if let Some(text) = find("public.utf8-plain-text")? {
        items.push(("text/plain", text));
    }
    Ok(items)
}

/// A picture for rich-text apps, sized by its resolution, that carries the
/// drawing for ReShiki.
fn html(png: &[u8], drawing: Option<&[u8]>) -> String {
    let size = png::Decoder::new(std::io::Cursor::new(png))
        .read_info()
        .ok()
        .and_then(|reader| {
            let info = reader.info();
            let dims = info
                .pixel_dims
                .filter(|d| d.unit == png::Unit::Meter && d.xppu > 0 && d.yppu > 0)?;
            // CSS pixels are 96 per inch.
            let css = |pixels: u32, per_meter: u32| {
                (f64::from(pixels) * 96. / (f64::from(per_meter) * 0.0254)).round()
            };
            Some(format!(
                r#" width="{}" height="{}""#,
                css(info.width, dims.xppu),
                css(info.height, dims.yppu)
            ))
        })
        .unwrap_or_default();
    let data = drawing
        .map(|drawing| format!(r#" {ATTRIBUTE}="{}""#, STANDARD.encode(drawing)))
        .unwrap_or_default();
    format!(
        r#"<img src="data:image/png;base64,{}" alt="Chemical drawing"{size}{data}>"#,
        STANDARD.encode(png)
    )
}

/// The ReShiki drawing a copy from the editor put in its HTML.
fn drawing(html: &str) -> Option<Vec<u8>> {
    let start = html.find(&format!(r#"{ATTRIBUTE}=""#))? + ATTRIBUTE.len() + 2;
    let end = start + html[start..].find('"')?;
    STANDARD.decode(&html[start..end]).ok()
}

/// The first of `formats`, in their order, that the clipboard's web types hold.
pub(super) fn pick(formats: &[String], items: &[(String, Vec<u8>)]) -> Option<Representation> {
    let web = |mime: &str| {
        items
            .iter()
            .find(|(kind, _)| kind == mime)
            .map(|(_, data)| data.as_slice())
    };
    formats.iter().find_map(|kind| {
        let data = match kind.as_str() {
            NATIVE => drawing(std::str::from_utf8(web("text/html")?).ok()?)?,
            "public.utf8-plain-text" => web("text/plain")?.to_vec(),
            "public.png" => web("image/png")?.to_vec(),
            "public.svg-image" => web("image/svg+xml")?.to_vec(),
            _ => return None,
        };
        Some(Representation::new(kind, &data))
    })
}

/// The browser reads the clipboard again for each fallback, so a token of its
/// contents lets read_candidates notice when it changed in between.
pub(super) fn generation(items: &[(String, Vec<u8>)]) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    items.hash(&mut hasher);
    format!("browser-{:016x}", hasher.finish())
}

/// What to tell the user when the browser refuses with this DOMException name.
pub(super) fn refusal(name: &str, reading: bool) -> String {
    match name {
        "NotAllowedError" => {
            "The browser blocked clipboard access; allow Clipboard for this site in its address bar"
                .into()
        }
        "NotFoundError" | "DataError" if reading => "The clipboard is empty".into(),
        "SecurityError" => "Clipboard access needs a secure (https) page".into(),
        _ if reading => "The browser could not read the clipboard".into(),
        _ => "The browser could not write to the clipboard".into(),
    }
}

#[cfg(target_arch = "wasm32")]
pub(super) use transport::{read, write};

#[cfg(target_arch = "wasm32")]
mod transport {
    use super::{super::Packet, Representation, refusal};
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::JsFuture;

    const UNAVAILABLE: &str = "This page has no clipboard access: open it over https, and an embedding page must allow clipboard-read and clipboard-write";
    /// The web types `pick` reads.
    const READ: [&str; 4] = ["text/html", "text/plain", "image/png", "image/svg+xml"];

    fn get(target: &JsValue, name: &str) -> Option<JsValue> {
        js_sys::Reflect::get(target, &name.into())
            .ok()
            .filter(|value| !value.is_undefined() && !value.is_null())
    }

    fn clipboard() -> Result<web_sys::Clipboard, String> {
        let window = web_sys::window().ok_or(UNAVAILABLE)?;
        get(&window.navigator(), "clipboard")
            .map(JsCast::unchecked_into)
            .ok_or_else(|| UNAVAILABLE.into())
    }

    fn refused(error: JsValue, reading: bool) -> String {
        let name = get(&error, "name").and_then(|name| name.as_string());
        web_sys::console::warn_2(&"ReShiki clipboard:".into(), &error);
        refusal(name.as_deref().unwrap_or_default(), reading)
    }

    /// Whether this browser lets a page write `mime`.
    fn supports(class: &JsValue, mime: &str) -> bool {
        match get(class, "supports").and_then(|f| f.dyn_into::<js_sys::Function>().ok()) {
            Some(supports) => supports
                .call1(class, &mime.into())
                .is_ok_and(|yes| yes.is_truthy()),
            // Browsers before ClipboardItem.supports write these three.
            None => matches!(mime, "text/plain" | "text/html" | "image/png"),
        }
    }

    pub(in crate::clipboard) async fn write(
        representations: &[Representation],
    ) -> Result<Packet, String> {
        let items = super::web_items(representations)?;
        let clipboard = clipboard()?;
        let class = web_sys::window()
            .and_then(|window| get(&window, "ClipboardItem"))
            .filter(|class| class.is_function());
        let promise = match (class, items.as_slice()) {
            (Some(class), _) => {
                let record = js_sys::Object::new();
                for (mime, data) in items.iter().filter(|(mime, _)| supports(&class, mime)) {
                    let options = web_sys::BlobPropertyBag::new();
                    options.set_type(mime);
                    let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(data.as_slice()));
                    let blob =
                        web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options)
                            .map_err(|error| refused(error, false))?;
                    js_sys::Reflect::set(&record, &(*mime).into(), &blob)
                        .map_err(|error| refused(error, false))?;
                }
                let item =
                    web_sys::ClipboardItem::new_with_record_from_str_to_blob_promise(&record)
                        .map_err(|error| refused(error, false))?;
                clipboard.write(&js_sys::Array::of1(&item))
            }
            // Older browsers write text only.
            (None, [("text/plain", text)]) => clipboard.write_text(&String::from_utf8_lossy(text)),
            (None, _) => {
                return Err("This browser cannot put pictures on the clipboard; use Export".into());
            }
        };
        JsFuture::from(promise)
            .await
            .map_err(|error| refused(error, false))?;
        Ok(Packet {
            representations: vec![],
            generation: None,
        })
    }

    async fn bytes(item: &web_sys::ClipboardItem, mime: &str) -> Result<Vec<u8>, String> {
        let blob: web_sys::Blob = JsFuture::from(item.get_type(mime))
            .await
            .map_err(|error| refused(error, true))?
            .unchecked_into();
        let buffer = JsFuture::from(blob.array_buffer())
            .await
            .map_err(|error| refused(error, true))?;
        Ok(js_sys::Uint8Array::new(&buffer).to_vec())
    }

    /// The first of `formats` on the clipboard, read again for each request.
    pub(in crate::clipboard) async fn read(formats: &[String]) -> Result<Packet, String> {
        let clipboard = clipboard()?;
        if get(&clipboard, "read").is_none() {
            return Err(
                "This browser cannot read the clipboard; paste text with Import → Structure".into(),
            );
        }
        let found = JsFuture::from(clipboard.read())
            .await
            .map_err(|error| refused(error, true))?;
        let mut items = Vec::new();
        for item in js_sys::Array::from(&found).iter() {
            let item: web_sys::ClipboardItem = item.unchecked_into();
            for mime in item.types().iter().filter_map(|mime| mime.as_string()) {
                if READ.contains(&mime.as_str()) && !items.iter().any(|(kind, _)| *kind == mime) {
                    let data = bytes(&item, &mime).await?;
                    items.push((mime, data));
                }
            }
        }
        Ok(Packet {
            representations: super::pick(formats, &items).into_iter().collect(),
            generation: Some(super::generation(&items)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(pixels_per_meter: Option<u32>) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, 300, 150);
        encoder.set_color(png::ColorType::Rgba);
        if let Some(ppu) = pixels_per_meter {
            encoder.set_pixel_dims(Some(png::PixelDimensions {
                xppu: ppu,
                yppu: ppu,
                unit: png::Unit::Meter,
            }));
        }
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&vec![0; 300 * 150 * 4]).unwrap();
        writer.finish().unwrap();
        bytes
    }

    #[test]
    fn a_copy_writes_html_png_svg_and_text_and_reads_the_drawing_back() {
        let png = png(Some(11811)); // 300 dpi
        let representations = [
            Representation::new(NATIVE, br#"{"atoms":[]}"#),
            Representation::new("com.adobe.pdf", b"%PDF"),
            Representation::new("public.png", &png),
            Representation::new("public.svg-image", b"<svg/>"),
            Representation::new("public.utf8-plain-text", b"CCO"),
        ];
        let items = web_items(&representations).unwrap();
        assert_eq!(
            items.iter().map(|(mime, _)| *mime).collect::<Vec<_>>(),
            ["text/html", "image/png", "image/svg+xml", "text/plain"]
        );
        let html = String::from_utf8(items[0].1.clone()).unwrap();
        // 300 px at 300 dpi is one inch, 96 CSS pixels.
        assert!(html.contains(r#"width="96" height="48""#), "{html}");
        assert!(html.starts_with(r#"<img src="data:image/png;base64,"#));
        let read: Vec<_> = items
            .into_iter()
            .map(|(mime, data)| (mime.to_owned(), data))
            .collect();
        let formats = super::super::priority::read_formats(&[], false)
            .unwrap()
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let first = pick(&formats, &read).unwrap();
        assert_eq!(first.kind, NATIVE);
        assert_eq!(first.bytes().unwrap(), br#"{"atoms":[]}"#);
        // Without the HTML, text comes before pictures.
        let first = pick(&formats, &read[1..]).unwrap();
        assert_eq!(first.kind, "public.utf8-plain-text");
        // Paste picture asks for pictures only.
        let pictures = vec!["public.png".to_owned()];
        assert_eq!(pick(&pictures, &read).unwrap().bytes().unwrap(), png);
    }

    #[test]
    fn an_image_copy_has_no_drawing_and_html_from_elsewhere_is_not_a_drawing() {
        let png = png(None);
        let items = web_items(&[Representation::new("public.png", &png)]).unwrap();
        let html = String::from_utf8(items[0].1.clone()).unwrap();
        assert!(
            !html.contains(ATTRIBUTE) && !html.contains("width="),
            "{html}"
        );
        let other = vec![("text/html".to_owned(), b"<b>CCO</b>".to_vec())];
        assert!(pick(&[NATIVE.to_owned()], &other).is_none());
    }

    #[test]
    fn the_generation_follows_the_clipboard_contents() {
        let text = |t: &str| vec![("text/plain".to_owned(), t.as_bytes().to_vec())];
        assert_eq!(generation(&text("CCO")), generation(&text("CCO")));
        assert_ne!(generation(&text("CCO")), generation(&text("CCN")));
        assert!(
            super::super::priority::validate_generation(Some(&generation(&text("CCO")))).is_ok()
        );
    }

    #[test]
    fn refusals_say_what_to_do() {
        assert!(refusal("NotAllowedError", true).contains("allow Clipboard"));
        assert_eq!(refusal("NotFoundError", true), "The clipboard is empty");
        assert_eq!(
            refusal("NotFoundError", false),
            "The browser could not write to the clipboard"
        );
    }
}
