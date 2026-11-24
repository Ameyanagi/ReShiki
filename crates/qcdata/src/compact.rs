//! Compact encodings for floating-point arrays in their field's declared units.

use base64::{Engine, engine::general_purpose::STANDARD};
use miniz_oxide::{deflate::compress_to_vec, inflate::decompress_to_vec_with_limit};

use crate::{MAX_COMPACT_EXPANSION, MAX_COMPACT_VALUES};

const Q16_SCALE: f64 = i16::MAX as f64;

/// Encoded string length in bytes for single-precision values, or zero on encoding failure.
pub fn encoded_len_f32(values: &[f64]) -> usize {
    encode_f32(values).map_or(0, |encoded| encoded.len())
}

/// Encoded string length in bytes for unitless signed fractions, or zero on encoding failure.
pub fn encoded_len_q16(values: &[f64]) -> usize {
    encode_q16(values).map_or(0, |encoded| encoded.len())
}

fn encode_f32(values: &[f64]) -> Result<String, &'static str> {
    encode("f32:", values, |value| {
        let value = value as f32;
        if !value.is_finite() {
            return Err("compact f32 values must be finite at single precision");
        }
        Ok(value.to_le_bytes())
    })
}

fn encode_q16(values: &[f64]) -> Result<String, &'static str> {
    encode("q16:", values, |value| {
        if !(-1.0..=1.0).contains(&value) {
            return Err("compact q16 values must be finite and in [-1, 1]");
        }
        Ok(((value * Q16_SCALE).round() as i16).to_le_bytes())
    })
}

fn encode<const WIDTH: usize>(
    tag: &str,
    values: &[f64],
    to_bytes: impl Fn(f64) -> Result<[u8; WIDTH], &'static str>,
) -> Result<String, &'static str> {
    if values.is_empty() {
        return Ok(String::new());
    }
    let count = values.len();
    if count > MAX_COMPACT_VALUES {
        return Err("too many compact array values");
    }
    let mut shuffled = vec![0; count * WIDTH];
    for (index, &value) in values.iter().enumerate() {
        for (plane, byte) in to_bytes(value)?.into_iter().enumerate() {
            shuffled[plane * count + index] = byte;
        }
    }
    let mut encoded = tag.to_owned();
    STANDARD.encode_string(compress_to_vec(&shuffled, 6), &mut encoded);
    // Highly repetitive data (a mostly zero Hessian) can compress beyond the
    // decoder's expansion limit; store it uncompressed so it always reloads.
    if shuffled.len() > encoded.len() * MAX_COMPACT_EXPANSION {
        encoded.truncate(tag.len());
        STANDARD.encode_string(compress_to_vec(&shuffled, 0), &mut encoded);
    }
    Ok(encoded)
}

fn decode<const WIDTH: usize>(
    tag: &str,
    encoded: &str,
    from_bytes: impl Fn([u8; WIDTH]) -> Result<f64, &'static str>,
) -> Result<Vec<f64>, &'static str> {
    if encoded.is_empty() {
        return Ok(Vec::new());
    }
    let payload = encoded
        .strip_prefix(tag)
        .ok_or("unexpected compact array tag")?;
    if encoded.len() > tag.len() + 2 * MAX_COMPACT_VALUES * WIDTH {
        return Err("compact array encoded size limit exceeded");
    }
    let compressed = STANDARD
        .decode(payload)
        .map_err(|_| "invalid compact array base64")?;
    let limit =
        (MAX_COMPACT_VALUES * WIDTH).min(encoded.len().saturating_mul(MAX_COMPACT_EXPANSION));
    let shuffled = decompress_to_vec_with_limit(&compressed, limit)
        .map_err(|_| "invalid compact array deflate data or size limit exceeded")?;
    if !shuffled.len().is_multiple_of(WIDTH) {
        return Err("incorrect compact array byte count");
    }
    let count = shuffled.len() / WIDTH;
    (0..count)
        .map(|index| from_bytes(std::array::from_fn(|plane| shuffled[plane * count + index])))
        .collect()
}

/// Serde adapter for single-precision values in the field's declared units.
pub mod f32 {
    use serde::{Deserialize, Deserializer, Serializer, de::Error as _, ser::Error as _};

    /// Serializes values as a tagged compact single-precision string.
    pub fn serialize<S: Serializer>(values: &[f64], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&super::encode_f32(values).map_err(S::Error::custom)?)
    }

    /// Deserializes finite single-precision values into the field's declared units.
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<f64>, D::Error> {
        super::decode("f32:", &String::deserialize(deserializer)?, |bytes| {
            let value = f32::from_le_bytes(bytes);
            if !value.is_finite() {
                return Err("non-finite compact f32 value");
            }
            Ok(f64::from(value))
        })
        .map_err(D::Error::custom)
    }
}

/// Serde adapter for unitless signed fractions in [-1, 1].
pub mod q16 {
    use serde::{Deserialize, Deserializer, Serializer, de::Error as _, ser::Error as _};

    /// Serializes unitless fractions as a tagged compact 16-bit string.
    pub fn serialize<S: Serializer>(values: &[f64], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&super::encode_q16(values).map_err(S::Error::custom)?)
    }

    /// Deserializes signed 16-bit values scaled by 1/32767.
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<f64>, D::Error> {
        super::decode(
            "q16:",
            &String::deserialize(deserializer)?,
            |bytes| match i16::from_le_bytes(bytes) {
                i16::MIN => Err("compact q16 value below -1"),
                value => Ok(f64::from(value) / super::Q16_SCALE),
            },
        )
        .map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_COMPACT_EXPANSION, MAX_COMPACT_VALUES, Q16_SCALE, STANDARD, compress_to_vec,
        encode_f32, encode_q16, encoded_len_f32, encoded_len_q16,
    };
    use base64::Engine;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct F32(#[serde(with = "super::f32")] Vec<f64>);

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Q16(#[serde(with = "super::q16")] Vec<f64>);

    #[test]
    fn f32_round_trip() {
        let samples = F32(vec![-100.125, 0.0, 1.0 / 3.0, 1.0e-20, 1.0e10]);
        let json = serde_json::to_string(&samples).unwrap();
        assert_eq!(encoded_len_f32(&samples.0), json.len() - 2);
        let decoded: F32 = serde_json::from_str(&json).unwrap();
        assert_eq!(
            decoded.0,
            samples
                .0
                .iter()
                .map(|&x| f64::from(x as f32))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn q16_round_trip() {
        let samples = Q16(vec![-1.0, -0.125, 0.0, 1.0 / 3.0, 1.0]);
        let json = serde_json::to_string(&samples).unwrap();
        assert_eq!(encoded_len_q16(&samples.0), json.len() - 2);
        let decoded: Q16 = serde_json::from_str(&json).unwrap();
        for (expected, observed) in samples.0.iter().zip(decoded.0) {
            assert!((expected - observed).abs() <= 1.0 / Q16_SCALE);
        }
    }

    #[test]
    fn exact_wire_representation() {
        let a = f64::from(f32::from_bits(0x3f34_5678));
        let b = f64::from(f32::from_bits(0xbf98_7654));
        let expected = [0x78, 0x54, 0x56, 0x76, 0x34, 0x98, 0x3f, 0xbf];
        assert_eq!(
            encode_f32(&[a, b]).unwrap(),
            format!("f32:{}", STANDARD.encode(compress_to_vec(&expected, 6)))
        );
        let expected = [0xff, 0x01, 0x00, 0x7f, 0x80, 0x40];
        assert_eq!(
            encode_q16(&[1.0, -1.0, 0.5]).unwrap(),
            format!("q16:{}", STANDARD.encode(compress_to_vec(&expected, 6)))
        );
    }

    #[test]
    fn empty_arrays_are_empty_strings() {
        assert_eq!(serde_json::to_string(&F32(vec![])).unwrap(), r#""""#);
        assert_eq!(serde_json::to_string(&Q16(vec![])).unwrap(), r#""""#);
        assert_eq!(serde_json::from_str::<F32>(r#""""#).unwrap(), F32(vec![]));
        assert_eq!(serde_json::from_str::<Q16>(r#""""#).unwrap(), Q16(vec![]));
    }

    #[test]
    fn unknown_and_mismatched_tags_are_rejected() {
        assert!(serde_json::from_str::<F32>(r#""f64:AQ==""#).is_err());
        let encoded = serde_json::to_string(&Q16(vec![0.1])).unwrap();
        assert!(serde_json::from_str::<F32>(&encoded).is_err());
    }

    #[test]
    fn truncated_base64_is_rejected() {
        let mut encoded = encode_f32(&[0.0, 1.0, 2.0]).unwrap();
        encoded.pop();
        let json = serde_json::to_string(&encoded).unwrap();
        assert!(serde_json::from_str::<F32>(&json).is_err());
    }

    #[test]
    fn wrong_byte_counts_are_rejected() {
        let f32_json = format!(r#""f32:{}""#, STANDARD.encode(compress_to_vec(&[0, 0], 6)));
        let q16_json = format!(r#""q16:{}""#, STANDARD.encode(compress_to_vec(&[0], 6)));
        assert!(serde_json::from_str::<F32>(&f32_json).is_err());
        assert!(serde_json::from_str::<Q16>(&q16_json).is_err());
    }

    #[test]
    fn oversized_encoded_payload_is_rejected() {
        let tag = "q16:";
        let mut compressed = compress_to_vec(&[0, 0], 6);
        // Pad the deflate stream with trailing bytes that the inflater ignores.
        compressed.resize(3 * MAX_COMPACT_VALUES, 0);
        let mut encoded = format!("{tag}{}", STANDARD.encode(compressed));
        assert_eq!(encoded.len(), tag.len() + 2 * MAX_COMPACT_VALUES * 2);
        assert_eq!(
            super::decode::<2>(tag, &encoded, |_| Ok(0.0)),
            Ok(vec![0.0])
        );

        encoded.push_str("AAAA");
        assert_eq!(
            super::decode::<2>(tag, &encoded, |_| Ok(0.0)),
            Err("compact array encoded size limit exceeded")
        );
    }

    #[test]
    fn expansion_limit_rejects_deflate_bomb() {
        let zeros = vec![0; 10_000_000];
        let encoded = format!("f32:{}", STANDARD.encode(compress_to_vec(&zeros, 6)));
        assert!(zeros.len() < MAX_COMPACT_VALUES * 4);
        assert!(zeros.len() > encoded.len() * MAX_COMPACT_EXPANSION);
        let json = serde_json::to_string(&encoded).unwrap();
        assert!(serde_json::from_str::<F32>(&json).is_err());
    }

    #[test]
    fn repetitive_arrays_reload() {
        let zeros = F32(vec![0.0; 1_000_000]);
        let json = serde_json::to_string(&zeros).unwrap();
        assert_eq!(serde_json::from_str::<F32>(&json).unwrap(), zeros);
        let modes = Q16(vec![0.0; 1_000_000]);
        let json = serde_json::to_string(&modes).unwrap();
        assert_eq!(serde_json::from_str::<Q16>(&json).unwrap(), modes);
        let below = format!(
            r#""q16:{}""#,
            STANDARD.encode(compress_to_vec(&[0x00, 0x80], 6))
        );
        assert!(serde_json::from_str::<Q16>(&below).is_err());
    }

    #[test]
    fn non_finite_f32_is_rejected() {
        for value in [f32::INFINITY, f32::NAN] {
            let data = compress_to_vec(&value.to_le_bytes(), 6);
            let json = format!(r#""f32:{}""#, STANDARD.encode(data));
            assert!(serde_json::from_str::<F32>(&json).is_err());
        }
        assert!(serde_json::to_string(&F32(vec![f64::MAX])).is_err());
    }

    #[test]
    fn invalid_q16_is_rejected() {
        for value in [1.5, -1.5, f64::INFINITY, f64::NAN] {
            assert!(serde_json::to_string(&Q16(vec![value])).is_err());
        }
    }
}
