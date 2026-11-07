//! #777 records a curve's explicit "no head" CDXML arrowhead attributes as
//! empty path heads; the original importer has no heads field.
use serde_json::Value;

pub fn without(mut value: Value) -> Value {
    fn strip(value: &mut Value) {
        match value {
            Value::Object(map) => {
                if map
                    .get("heads")
                    .is_some_and(|h| h["head"] == "none" && h["tail"] == "none")
                {
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
