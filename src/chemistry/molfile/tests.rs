use super::*;
use crate::{
    allocation_metrics,
    attachments::{Attachment, Kind},
};
use std::{collections::HashSet, hint::black_box, time::Instant};

fn molecule() -> Molecule {
    let mut state = crate::chemistry::smiles::prepare("*C.C.C.C").unwrap().state;
    state.rings.kind = RingKind::Symmetric;
    Molecule {
        rdkit_version: RDKIT_VERSION,
        ids: vec![9_007_199_254_740_993, 81, 12, 99, 73],
        positions: vec![crate::chemistry::stereo::Point3::default(); 5],
        state,
    }
}

#[test]
fn attachment_indices_keep_member_order_and_missing_target_errors() {
    let molecule = molecule();
    for (kind, mode) in [(Kind::MultiCenter, "ALL"), (Kind::Variable, "ANY")] {
        let attachment = Attachment {
            id: molecule.ids[0],
            kind,
            members: vec![73, 99, 12],
        };
        let block = write_part(
            &molecule,
            Options { force_v3000: true },
            true,
            std::slice::from_ref(&attachment),
            false,
        )
        .unwrap();
        assert!(
            block.contains(&format!("ENDPTS=(3 5 4 3) ATTACH={mode}")),
            "{block}"
        );
        let missing = Attachment {
            members: vec![99, 404],
            ..attachment
        };
        assert_eq!(
            write_part(
                &molecule,
                Options { force_v3000: true },
                true,
                &[missing],
                false
            )
            .unwrap_err()
            .to_string(),
            "Invalid MOL output: Missing MOL attachment target"
        );
    }
    let mut duplicate = molecule.clone();
    duplicate.ids[2] = duplicate.ids[1];
    assert_eq!(
        write(&duplicate, Options::default())
            .unwrap_err()
            .to_string(),
        "Invalid MOL output: Molecule dimensions or annotations changed"
    );
}

#[test]
#[ignore = "isolated requested-Rust-allocation and lookup timing measurement"]
fn measure_attachment_index_tradeoff() {
    for (atoms, members) in [(32, 0), (4096, 0), (4096, 300), (4096, 30_000)] {
        let ids = (0..atoms)
            .map(|i| 9_007_199_254_740_993 + (i * 7) as u64)
            .collect::<Vec<_>>();
        let targets = (0..members)
            .map(|i| ids[atoms - 1 - i % atoms])
            .collect::<Vec<_>>();
        for indexed in [false, true] {
            let baseline = allocation_metrics::reset();
            let start = Instant::now();
            let indices = if indexed {
                let mut map = HashMap::new();
                map.extend(ids.iter().enumerate().map(|(i, &id)| (id, i)));
                assert_eq!(map.len(), atoms);
                Some(map)
            } else {
                assert_eq!(ids.iter().collect::<HashSet<_>>().len(), atoms);
                None
            };
            // Model the writer's output buffer overlap with the validation map.
            let output = vec![0u8; atoms * 80];
            let mut checksum = 0;
            for id in &targets {
                checksum += if let Some(indices) = &indices {
                    *indices.get(black_box(id)).unwrap()
                } else {
                    ids.iter().position(|n| n == black_box(id)).unwrap()
                };
            }
            black_box((&output, checksum));
            let elapsed = start.elapsed();
            let snapshot = allocation_metrics::snapshot();
            drop(indices);
            drop(output);
            let after_drop = allocation_metrics::snapshot();
            println!(
                "indexed={indexed} atoms={atoms} members={members} elapsed={elapsed:?} allocations={} allocated={} peak_extra={} live_during_serialization={} live_after_drop={} checksum={checksum}",
                snapshot.allocation_count,
                snapshot.allocated_bytes,
                snapshot.peak_bytes.saturating_sub(baseline),
                snapshot.live_bytes.saturating_sub(baseline),
                after_drop.live_bytes.saturating_sub(baseline)
            );
        }
    }
}
