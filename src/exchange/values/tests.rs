use super::*;
use crate::allocation_metrics;
use std::{hint::black_box, time::Instant};

// The former allocating implementation is a differential measurement baseline.
fn allocating_number(kind: &str, n: f64) -> Result<Vec<u8>> {
    if !n.is_finite() {
        return Err("Non-finite drawing value".into());
    }
    if kind == "FLOAT64" {
        return Ok(n.to_le_bytes().to_vec());
    }
    let n = n.round_ties_even();
    macro_rules! pack {
        ($t:ty) => {{
            if n < <$t>::MIN as f64 || n > <$t>::MAX as f64 {
                return Err("Drawing value exceeds the binary format range".into());
            }
            Ok((n as $t).to_le_bytes().to_vec())
        }};
    }
    match kind {
        "INT8" => pack!(i8),
        "UINT8" => pack!(u8),
        "INT16" => pack!(i16),
        "UINT16" => pack!(u16),
        "INT32" | "CDXCoordinate" => pack!(i32),
        "UINT32" | "CDXObjectID" => pack!(u32),
        _ => Err("Unsupported binary number type".into()),
    }
}

#[test]
fn numeric_append_preserves_bytes_errors_and_rounding() {
    for kind in [
        "INT8",
        "UINT8",
        "INT16",
        "UINT16",
        "INT32",
        "UINT32",
        "CDXCoordinate",
        "CDXObjectID",
        "FLOAT64",
        "unsupported",
    ] {
        for n in [
            f64::NEG_INFINITY,
            f64::NAN,
            f64::INFINITY,
            -4_294_967_296.,
            -2_147_483_649.,
            -2_147_483_648.,
            -32769.,
            -32768.,
            -129.,
            -128.,
            -2.5,
            -1.5,
            -0.5,
            -0.,
            0.,
            0.5,
            1.5,
            2.5,
            127.,
            128.,
            255.,
            256.,
            32767.,
            32768.,
            65535.,
            65536.,
            2_147_483_647.,
            2_147_483_648.,
            4_294_967_295.,
            4_294_967_296.,
        ] {
            let expected = allocating_number(kind, n);
            let mut actual = vec![0xa5];
            let result = pack_number(kind, n, |bytes| crate::exchange::append(&mut actual, bytes));
            match expected {
                Ok(bytes) => {
                    result.unwrap();
                    assert_eq!(&actual[1..], bytes, "{kind}: {n}");
                }
                Err(error) => {
                    assert_eq!(result.unwrap_err(), error, "{kind}: {n}");
                    assert_eq!(actual, [0xa5]);
                }
            }
        }
    }
    let mut data = vec![];
    for (kind, n) in [("INT16", 2.5), ("INT16", 3.5), ("FLOAT64", -0.)] {
        pack_number(kind, n, |bytes| crate::exchange::append(&mut data, bytes)).unwrap();
    }
    assert_eq!(
        data,
        [
            2i16.to_le_bytes().as_slice(),
            4i16.to_le_bytes().as_slice(),
            (-0f64).to_le_bytes().as_slice()
        ]
        .concat()
    );
}

#[test]
fn numeric_validation_precedes_output_limit_without_partial_append() {
    let mut data = vec![0; crate::exchange::LIMIT];
    for (kind, value, error) in [
        ("UINT16", f64::NAN, "Non-finite drawing value"),
        (
            "UINT16",
            65536.,
            "Drawing value exceeds the binary format range",
        ),
        ("invalid", 1., "Unsupported binary number type"),
        ("UINT16", 1., "Binary drawing exceeds the 16 MB limit"),
    ] {
        assert_eq!(
            pack_number(kind, value, |bytes| crate::exchange::append(
                &mut data, bytes
            ))
            .unwrap_err(),
            error
        );
        assert_eq!(data.len(), crate::exchange::LIMIT);
    }
    assert_eq!(
        encode_coordinates("0 0 0 bad", "CDXPoint2D").unwrap_err(),
        "Invalid drawing number"
    );
    assert_eq!(
        encode_coordinates("0 0 0", "CDXPoint2D").unwrap_err(),
        "Invalid drawing coordinates"
    );
    assert_eq!(
        encode_coordinates("bad", "invalid").unwrap_err(),
        "Invalid coordinate type"
    );
    assert_eq!(
        encode_coordinates("1 -2", "CDXPoint2D").unwrap(),
        [(-131072i32).to_le_bytes(), 65536i32.to_le_bytes()].concat()
    );
}

#[test]
#[ignore = "isolated requested-Rust-allocation and timing measurement"]
fn measure_numeric_append() {
    const COUNT: usize = 100_000;
    for direct in [false, true] {
        let mut out = Vec::with_capacity(COUNT * 2);
        let baseline = allocation_metrics::reset();
        let start = Instant::now();
        for i in 0..COUNT {
            let value = black_box((i % 65536) as f64);
            if direct {
                pack_number("UINT16", value, |bytes| {
                    crate::exchange::append(&mut out, bytes)
                })
                .unwrap();
            } else {
                crate::exchange::append(&mut out, &allocating_number("UINT16", value).unwrap())
                    .unwrap();
            }
        }
        let elapsed = start.elapsed();
        let snapshot = allocation_metrics::snapshot();
        black_box(&out);
        println!(
            "direct={direct} count={COUNT} elapsed={elapsed:?} allocations={} allocated={} peak_extra={} retained_extra={}",
            snapshot.allocation_count,
            snapshot.allocated_bytes,
            snapshot.peak_bytes.saturating_sub(baseline),
            snapshot.live_bytes.saturating_sub(baseline)
        );
    }
}
