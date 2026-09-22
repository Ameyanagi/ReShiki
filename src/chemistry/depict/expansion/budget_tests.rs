use super::*;
use crate::chemistry::{
    depict::{collision, finalize},
    electronic::Hybridization,
    graph::{Atom, Bond},
    kekulize::Direction,
    stereo::perception::{self, Properties, RingKind},
};

type TestResult = anyhow::Result<()>;

fn chain(count: usize) -> anyhow::Result<State> {
    let graph = Graph {
        atoms: vec![
            Atom {
                atomic_number: 6,
                ..Atom::default()
            };
            count
        ],
        bonds: (1..count)
            .map(|b| Bond {
                a: b - 1,
                b,
                order: 1,
                aromatic: false,
            })
            .collect(),
    };
    Ok(State {
        metadata: Metadata::unspecified(&graph),
        directions: vec![Direction::None; graph.bonds.len()],
        valences: graph.provisional_valences().map_err(anyhow::Error::msg)?,
        conjugated: vec![false; graph.bonds.len()],
        hybridizations: vec![Hybridization::Sp3; graph.atoms.len()],
        rings: RingCache::default(),
        properties: Properties::unspecified(&graph),
        graph,
    })
}

fn prepared(source: &State, remaining: &mut usize) -> Result<Initial> {
    compute_initial_with_work(
        source,
        &vec![None; source.graph.atoms.len()],
        None,
        Options::default(),
        remaining,
    )
}

fn repair(initial: &Initial, remaining: &mut usize) -> anyhow::Result<Vec<Fragment>> {
    let input = collision::Input::new_with_work(
        &initial.state.graph,
        &initial.state.metadata,
        &initial.state.rings,
        remaining,
    )?;
    let mut fragments = initial.fragments.clone();
    for stage in [
        collision::Stage::BondAndSpiroFlip,
        collision::Stage::OpenAngles,
        collision::Stage::ShortenBonds,
    ] {
        for fragment in &mut fragments {
            *fragment = input.apply_with_work(fragment, stage, remaining)?;
        }
    }
    Ok(fragments)
}

#[test]
fn shared_work_is_cumulative_across_preparation_collision_and_finalization() -> TestResult {
    let source = chain(6)?;
    let before = serde_json::to_value(&source)?;
    let mut remaining = MAX_WORK;
    let initial = prepared(&source, &mut remaining)?;
    let initial_cost = MAX_WORK - remaining;
    let fragments = repair(&initial, &mut remaining)?;
    let collision_cost = MAX_WORK - remaining - initial_cost;
    let expected = finalize::finish_with_work(
        6,
        &fragments,
        None,
        finalize::Options::default(),
        &mut remaining,
    )?;
    let total = MAX_WORK - remaining;
    let final_cost = total - initial_cost - collision_cost;
    assert!(initial_cost > 0 && collision_cost > 0 && final_cost > 0);

    // Precisely the measured total succeeds; no stage receives a fresh budget.
    let mut remaining = total;
    let replay = prepared(&source, &mut remaining)?;
    let fragments = repair(&replay, &mut remaining)?;
    let actual = finalize::finish_with_work(
        6,
        &fragments,
        None,
        finalize::Options::default(),
        &mut remaining,
    )?;
    assert_eq!(remaining, 0);
    assert_eq!(actual.fragments, expected.fragments);
    assert_eq!(actual.conformer_id, expected.conformer_id);
    assert_eq!(
        serde_json::to_value(actual.conformer)?,
        serde_json::to_value(expected.conformer)?
    );

    // The same allowance suffices for each stage alone, but not their sum.
    let mut remaining = total - 1;
    let replay = prepared(&source, &mut remaining)?;
    let fragments = repair(&replay, &mut remaining)?;
    assert!(matches!(
        finalize::finish_with_work(
            6,
            &fragments,
            None,
            finalize::Options::default(),
            &mut remaining
        ),
        Err(finalize::Error::Limit)
    ));
    assert!(remaining < final_cost);
    assert!(repair(&initial, &mut { total - 1 }).is_ok());
    assert!(
        finalize::finish_with_work(6, &fragments, None, finalize::Options::default(), &mut {
            total - 1
        })
        .is_ok()
    );
    assert_eq!(serde_json::to_value(&source)?, before);
    Ok(())
}

#[test]
fn preparation_uses_caller_budget_and_preserves_public_output() -> TestResult {
    let source = chain(8)?;
    let before = serde_json::to_value(&source)?;
    let public = compute_initial(&source, &[None; 8], None, Options::default())?;
    let mut remaining = MAX_WORK;
    let shared = prepared(&source, &mut remaining)?;
    assert_eq!(
        serde_json::to_value(&public)?,
        serde_json::to_value(&shared)?
    );
    let used = MAX_WORK - remaining;
    let mut excess = usize::MAX;
    prepared(&source, &mut excess)?;
    assert_eq!(excess, usize::MAX - used);
    let mut insufficient = used - 1;
    assert!(prepared(&source, &mut insufficient).is_err());
    assert!(insufficient < used - 1);
    let mut empty = 0;
    assert!(matches!(
        prepared(&source, &mut empty),
        Err(Error::Attachment(attachment::Error::Limit))
    ));
    assert_eq!(empty, 0);
    assert_eq!(serde_json::to_value(&source)?, before);
    Ok(())
}

#[test]
fn prepared_stereo_charges_early_return_and_rejects_unprepared_cache() -> TestResult {
    let mut source = chain(3)?;
    let options = perception::Options {
        clean: false,
        force: false,
        flag_possible: false,
    };
    let mut remaining = 1000;
    assert!(perception::perceive_prepared_with_work(&source, options, &mut remaining).is_err());
    assert_eq!(remaining, 1000);
    source.rings.kind = RingKind::Symmetric;
    source.properties.done = Some(false);
    let before = serde_json::to_value(&source)?;
    let value = perception::perceive_prepared_with_work(&source, options, &mut remaining)
        .map_err(anyhow::Error::msg)?;
    assert!(remaining < 1000);
    assert_eq!(serde_json::to_value(value)?, before);
    assert!(perception::perceive_prepared_with_work(&source, options, &mut 0).is_err());
    assert_eq!(serde_json::to_value(&source)?, before);
    Ok(())
}

#[test]
fn nested_caps_preserve_excess_and_ring_continuations_never_refill() -> TestResult {
    let mut source = chain(3)?;
    source.graph.bonds.push(Bond {
        a: 2,
        b: 0,
        order: 1,
        aromatic: false,
    });
    source.metadata = Metadata::unspecified(&source.graph);
    source.rings = RingCache {
        kind: RingKind::Symmetric,
        atoms: vec![vec![0, 1, 2]],
    };
    let data = vec![
        AtomData {
            hybridization: Hybridization::Sp3,
            cip_rank: None,
            chiral_rank: None
        };
        3
    ];
    let attachment = attachment::Input::new(&source.graph, &data)?.with_work_limit(1000);
    let mut local = 1000;
    let fragment = attachment.single_atom_with_budget(0, &mut local)?;
    let mut excess = usize::MAX;
    assert_eq!(
        attachment.single_atom_with_budget(0, &mut excess)?,
        fragment
    );
    assert_eq!(excess, usize::MAX - (1000 - local));

    let rings = rings::Input::new(&source.graph, &source.metadata, &source.rings, &[0])?
        .with_work_limit(1000);
    let mut local = 1000;
    let reference = rings.embed_with_budget(1.5, &mut local)?;
    let used = 1000 - local;
    let mut excess = usize::MAX;
    let construction = rings.begin_with_budget(1.5, &mut excess)?;
    excess -= 17; // Other template work runs between the constructor halves.
    assert_eq!(
        construction.finish_with_budget(None, &mut excess)?,
        reference
    );
    assert_eq!(excess, usize::MAX - used - 17);
    let mut remaining = 1000;
    let construction = rings.begin_with_budget(1.5, &mut remaining)?;
    remaining = 0;
    assert!(matches!(
        construction.finish_with_budget(None, &mut remaining),
        Err(rings::Error::Limit)
    ));
    assert_eq!(remaining, 0);

    let mut excess = usize::MAX;
    assert!(rings.begin_with_budget(f64::NAN, &mut excess).is_err());
    assert!(excess < usize::MAX && excess > usize::MAX - 1000);
    let mut excess = usize::MAX;
    let construction = rings.begin_with_budget(1.5, &mut excess)?;
    let prior = excess;
    assert!(
        construction
            .finish_with_budget(Some((fragment, vec![])), &mut excess)
            .is_err()
    );
    assert_eq!(excess, prior);
    Ok(())
}

#[test]
fn collision_cap_and_failed_entry_charge_only_completed_work() -> TestResult {
    let source = chain(4)?;
    let initial = prepared(&source, &mut { MAX_WORK })?;
    let collision = collision::Input::new(
        &initial.state.graph,
        &initial.state.metadata,
        &initial.state.rings,
    )?
    .with_work_limit(1000);
    let fragment = initial
        .fragments
        .first()
        .ok_or_else(|| anyhow::anyhow!("missing fragment"))?;
    let before = fragment.clone();
    let mut local = 1000;
    let expected = collision.repair_with_work(fragment, &mut local)?;
    let mut excess = usize::MAX;
    assert_eq!(collision.repair_with_work(fragment, &mut excess)?, expected);
    assert_eq!(excess, usize::MAX - (1000 - local));
    let mut empty = 0;
    assert!(matches!(
        collision.repair_with_work(fragment, &mut empty),
        Err(collision::Error::Limit)
    ));
    assert_eq!(fragment, &before);
    let mut insufficient = source.graph.atoms.len();
    assert!(matches!(
        collision::Input::new_with_work(
            &source.graph,
            &source.metadata,
            &source.rings,
            &mut insufficient
        ),
        Err(collision::Error::Limit)
    ));
    assert_eq!(insufficient, 0);
    Ok(())
}

#[test]
fn ring_preparation_preserves_order_and_debits_failed_search() -> TestResult {
    let mut source = chain(6)?;
    source.graph.bonds.push(Bond {
        a: 5,
        b: 0,
        order: 1,
        aromatic: false,
    });
    let options = crate::chemistry::rings::Options::default();
    let native_order = crate::chemistry::rings::perceive(&source.graph, options)?;
    let mut remaining = MAX_WORK;
    let shared =
        crate::chemistry::rings::perceive_with_work(&source.graph, options, &mut remaining)?;
    assert_eq!(shared.atoms, native_order.atoms);
    assert_eq!(shared.bonds, native_order.bonds);
    assert_eq!(shared.basis_count, native_order.basis_count);
    assert_eq!(shared.approximate, native_order.approximate);
    let used = MAX_WORK - remaining;
    let entry = source.graph.atoms.len() + source.graph.bonds.len();
    assert!(used > entry);
    let mut excess = usize::MAX;
    crate::chemistry::rings::perceive_with_work(&source.graph, options, &mut excess)?;
    assert_eq!(excess, usize::MAX - used);
    let mut insufficient = used - 1;
    assert!(
        crate::chemistry::rings::perceive_with_work(&source.graph, options, &mut insufficient)
            .is_err()
    );
    assert!(insufficient < used - 1);
    Ok(())
}
