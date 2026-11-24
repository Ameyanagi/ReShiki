use super::*;
use std::fmt::Write;

#[test]
fn angstrom_conversion_matches_the_driver_bit_for_bit() {
    for xyz in [
        include_str!("../../../../tests/fixtures/xtb/water.xyz"),
        include_str!("../../../../tests/fixtures/xtb/caffeine.xyz"),
        include_str!("../../../../tests/fixtures/xtb/ferrocene.xyz"),
        &random_xyz(),
    ] {
        let angstrom: Vec<[f64; 3]> = xyz
            .lines()
            .skip(2)
            .map(|line| {
                let mut coordinates = line.split_whitespace().skip(1);
                std::array::from_fn(|_| coordinates.next().unwrap().parse().unwrap())
            })
            .collect();
        let method = axtb_driver::Method::Gfn2;
        let (_, expected) = axtb_driver::read_xyz(xyz, method, None).unwrap();
        let actual = to_bohr(angstrom, Units::Angstrom, angstrom_to_bohr(method).unwrap()).unwrap();
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert_eq!(actual.map(f64::to_bits), expected.map(f64::to_bits));
        }
    }
}

fn random_xyz() -> String {
    let mut xyz = String::from("100\nDeterministic random coordinates\n");
    let mut seed = 0x5209_ab8d_u32;
    let mut coordinate = || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (f64::from(seed) / f64::from(u32::MAX) - 0.5) * 1000.0
    };
    for _ in 0..100 {
        writeln!(xyz, "H {} {} {}", coordinate(), coordinate(), coordinate()).unwrap();
    }
    xyz
}
