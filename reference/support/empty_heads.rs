//! #777 records a curve's explicit "no head" CDXML arrowhead attributes as
//! empty path heads (no head or tail, default solid 10/2.5/0 geometry); the
//! original importer has no heads field. Any other heads are still compared.
use serde_json::Value;

pub fn without(mut value: Value) -> Value {
    fn strip(value: &mut Value) {
        match value {
            Value::Object(map) => {
                if map.get("heads").is_some_and(|h| {
                    h["head"] == "none"
                        && h["tail"] == "none"
                        && h["shape"] == "solid"
                        && h["head_length_pt"] == 10.0
                        && h["head_width_pt"] == 2.5
                        && h["head_notch"] == 0.0
                }) {
                    map.remove("heads");
                }
                map.values_mut().for_each(strip);
            }
            Value::Array(items) => items.iter_mut().for_each(strip),
            _ => {}
        }
    }
    strip(&mut value);
    value
}
