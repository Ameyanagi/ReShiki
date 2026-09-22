use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PictureSource {
    #[serde(serialize_with = "encoded")]
    pub data: Vec<u8>,
    pub format: String,
    pub opacity: f64,
}
fn encoded<S: serde::Serializer>(
    data: &[u8],
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    serializer.serialize_str(&STANDARD.encode(data))
}
impl PictureSource {
    pub fn into_document(
        self,
        budget: &mut crate::pictures::exchange::Budget,
    ) -> Result<crate::pictures::Picture> {
        crate::pictures::exchange::import(&self.data, &self.format, self.opacity, budget)
            .map_err(Error::Picture)
    }
}
fn hex(text: &str) -> Result<Vec<u8>> {
    let mut bytes = text.bytes();
    let mut data = Vec::with_capacity(text.len() / 2);
    while let Some(first) = bytes.find(|b| !b.is_ascii_whitespace()) {
        let a = char::from(first).to_digit(16);
        let b = bytes.next().and_then(|b| char::from(b).to_digit(16));
        let (Some(a), Some(b)) = (a, b) else {
            return Err(Error::Native("Invalid embedded picture hexadecimal data"));
        };
        data.push((16 * a + b) as u8);
    }
    Ok(data)
}
impl Reader<'_, '_> {
    pub(super) fn picture(&mut self, el: Node<'_, '_>) -> Result<NativeGraphic> {
        let (format,text) = ["PNG","TIFF","JPEG","GIF","BMP"].iter().find_map(|key| el.attribute(*key).filter(|v| !v.is_empty()).map(|v| (*key,v)))
            .ok_or(Error::Native("Embedded picture needs a PNG, TIFF, JPEG, GIF or BMP representation; vector/OLE-only pictures are not supported"))?;
        self.spend(text.len())?;
        if text.len() > crate::pictures::MAX_BYTES * 3 {
            return Err(Error::Native("Embedded picture data exceeds 16 MB"));
        }
        let data = hex(text)?;
        if data.is_empty() || data.len() > crate::pictures::MAX_BYTES {
            return Err(Error::Native(
                "Embedded pictures must contain at most 16 MB",
            ));
        }
        let opacity = float(el, "alpha", "1")?;
        if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
            return Err(Error::Native("Invalid embedded picture opacity"));
        }
        let mut bounds = [0.; 4];
        let mut count = 0usize;
        let mut finite = true;
        let text = el.attribute("BoundingBox").unwrap_or("");
        self.spend(text.len())?;
        for value in text.split_whitespace() {
            let value = numeric::float(value)? * self.scale;
            finite &= value.is_finite();
            if let Some(slot) = bounds.get_mut(count) {
                *slot = value;
            }
            count += 1;
        }
        let angle = float(el, "RotationAngle", "0")? / 65536.;
        if count != 4 {
            return Err(Error::Native("Invalid embedded picture bounds or rotation"));
        }
        let [left, top, right, bottom] = bounds;
        if !angle.is_finite() || !finite {
            return Err(Error::Native("Invalid embedded picture bounds or rotation"));
        }
        let (width, height) = (right - left, bottom - top);
        if !(0.01..=1_000_000.).contains(&width.abs())
            || !(0.01..=1_000_000.).contains(&height.abs())
        {
            return Err(Error::Native(
                "Embedded picture dimensions are outside the supported range",
            ));
        }
        let radians = angle * (std::f64::consts::PI / 180.0);
        let (c, s) = (radians.cos(), radians.sin());
        let x = NativePoint::new(c * width, s * width);
        let y = NativePoint::new(-s * height, c * height);
        let origin = NativePoint::new(
            (left + right - x.x - y.x) / 2.,
            (top + bottom - x.y - y.y) / 2.,
        );
        Ok(NativeGraphic {
            id: 0,
            kind: GraphicKind::Picture,
            origin,
            axis_x: x,
            axis_y: y,
            style: None,
            sides: None,
            phase: None,
            phase_flipped: None,
            layer: self.layer(el)?,
            path: None,
            picture_source: Some(PictureSource {
                data,
                format: format.into(),
                opacity,
            }),
        })
    }
}
