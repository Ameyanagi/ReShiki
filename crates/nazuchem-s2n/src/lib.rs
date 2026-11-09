// SPDX-License-Identifier: MIT AND (MIT OR Apache-2.0)
// Origin: mixed
// Source: openclatura | src/openclatura/namer.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | src/openclatura/engine.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: ReShiki | src/naming/openclatura/mod.rs | 5f559fd482e237307a589dfcb7ea333cee0a6e13
// Source: openclatura | src/openclatura/component_namer.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | src/openclatura/locant_elision.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | src/openclatura/fusion/charges.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/unit/test_analysis.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/roundtrip/test_stereo.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/fixtures/diverse_corpus.csv | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/fixtures/diverse_corpus.golden.json | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/unit/test_chalcogen_functional_groups.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/unit/test_fused_bonds.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/roundtrip/test_replacement_parents.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/data/parser_xml_resources/simpleGroups.json | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/unit/test_polyoxide_chains.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/unit/test_audit_reconstruction.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/unit/test_all_chalcogen_acyl_regressions.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/roundtrip/test_oxyacids.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/unit/test_locant_elision.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/roundtrip/test_polycycles.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/roundtrip/test_role_models.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/roundtrip/test_principal_group_breadth.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/exploratory/test_group14_ether_ligand_parent_selection.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/data/parser_xml_resources/arylGroups.json | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/data/parser_xml_resources/nonCarboxylicAcids.json | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/unit/test_chalcogen_citation_compatibility.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/data/parser_xml_resources/functionalTerms.json | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/data/fusion_pin_cases.json | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/roundtrip/test_alkenes_alkynes.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/unit/test_chalcogen_structural_roles.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/data/parser_xml_resources/multipliers.json | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/unit/test_von_baeyer_candidate_audit.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/roundtrip/test_retained_fused_production.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/roundtrip/test_fused_parents.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/unit/test_retained_bridge_h_relocation.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/unit/test_rules_tables.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/integration/test_systematic_fusion.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/unit/test_fusion_wrappers.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/roundtrip/test_p13_fusion.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/roundtrip/test_fused_hydrocarbon_retained.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Copyright (c) 2026 lamalab-org
// Copyright (c) 2026 Ameyanagi and ReShiki contributors
// Changes: Port entry sequence surrounded by local naming routes, assessment, result types and alternatives.
// Local contributions: MIT OR Apache-2.0; see NOTICE and PROVENANCE-nazuchem.toml.

//! Plain-name pipeline that began as a port of openclatura 0.4.3, extended in ReShiki.
//! Attribution and license terms are in `licenses/openclatura/`.
//!
//! Supports chains, monocycles, retained and systematic fused parents, pure
//! spiro and von Baeyer ring systems, with independent reconstruction proofs.
//!
//! Callers supply a [`nazuchem_kernel::budget::Budget`]; `Budget::default()` is
//! unbounded. The library performs no I/O. Worst-case time on adversarial input
//! is unbounded, so hosts needing hard limits should supervise requests externally.
// Direct graph/path indexing preserves upstream invariants and panic behavior;
// JSON Value indexing preserves null for missing optional fields.
#![cfg_attr(not(test), allow(clippy::indexing_slicing))]
#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented
    )
)]

mod identity;
pub use identity::{canonical_smiles, verify_alternative_identity, verify_identity};

mod additive;
mod amine_oxides;
mod assembler;
mod assembly_charge;
mod assembly_parent;
mod assembly_parts;
mod assembly_prefixes;
mod assembly_spiro;
mod assembly_utils;
mod chains;
mod component_group_rules;
mod component_modifiers;
mod component_namer;
mod endo_exo_stereo;
mod formatting;
mod functional_prefixes;
mod fused_ion_templates;
mod fusion;
mod graph_io;
mod graph_kernel;
mod heteroatom_subgraphs;
mod heterospiro;
#[cfg(test)]
mod heterospiro_tests;
mod identity_validation;
mod ionic_naming;
mod isotope_naming;
mod radical_ions;
pub use radical_ions::recover as recover_radical_ion;
mod locant_elision;
mod locants;
mod molecule;
mod namer;
mod nomenclature;
mod numbering;
mod parent_pipeline;
mod parent_selection;
mod perception;
mod phane;
mod polycycle_topology;
mod principal_groups;
mod principal_suffixes;
mod python_int_set;
mod radical_naming;
mod retained_derivative_hydrogen;
mod retained_fused_production;
mod retained_fused_templates;
mod retained_graph_model;
mod retained_name_policy;
mod retained_specs;
mod ring_assembly;
mod ring_parent;
mod ring_renderer;
mod rules;
mod small_ring_stereo;
mod special_cases;
mod species;
mod spelling;
mod spiro_subgraph;
mod spirobi;
#[cfg(test)]
mod spirobi_tests;
mod subgraph_tools;
mod unknown_stereo;
mod von_baeyer;

#[cfg(test)]
mod amine_oxide_tests;
#[cfg(test)]
mod anion_parent_tests;
#[cfg(test)]
mod anion_spelling_tests;
#[cfg(test)]
mod ion_spelling_tests;
#[cfg(test)]
mod salt_anion_spelling_tests;

/// Engine identifier for callers that pin naming output.
pub const PROFILE: &str = "openclatura 0.4.3 Rust port";

/// A generated name and how it relates to the preferred IUPAC name.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Name {
    /// The generated chemical name.
    pub text: String,
    /// Whether the name is preferred, with a reason when it is not.
    pub status: nazuchem_ir::NameStatus,
    /// Other generated names, each with its own preference status.
    pub alternatives: Vec<nazuchem_ir::AlternativeName>,
    /// Locants of unknown stereochemistry omitted from the main name.
    pub omitted_stereo: Vec<String>,
    /// Number of unknown stereochemical units omitted from the main name.
    pub omitted_units: usize,
    /// A recovered neutral target for verifying a descriptive radical-ion name.
    #[serde(default)]
    pub verification: Option<Verification>,
}

/// Recovered neutral target used to verify a descriptive radical-ion name.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Verification {
    /// The generated name of the recovered neutral structure.
    pub name: String,
    /// SMILES of the recovered neutral structure to compare with the parsed name.
    pub smiles: String,
}

/// Generate a name from molecular SMILES using IUPAC isotope notation.
///
/// Molecular perception uses the caller's budget. Returns an error when the
/// input is unsupported, cannot be named, or exceeds the supplied budget.
// Provenance: fn name_smiles | adapted
// Source: openclatura | src/openclatura/namer.py::name_smiles
// Source: openclatura | src/openclatura/engine.py::Namer.name_smiles
pub fn name_smiles(smiles: &str, budget: &nazuchem_kernel::budget::Budget) -> Result<Name, String> {
    name_smiles_with_style(smiles, nazuchem_ir::IsotopeStyle::Iupac, budget)
}

/// Generate a name from molecular SMILES using the requested isotope notation.
///
/// Molecular perception uses the caller's budget. Returns an error when the
/// input is unsupported, cannot be named, or exceeds the supplied budget.
// Provenance: fn name_smiles_with_style | original
// Source: ReShiki | src/naming/openclatura/mod.rs::name_smiles_with_style
pub fn name_smiles_with_style(
    smiles: &str,
    style: nazuchem_ir::IsotopeStyle,
    budget: &nazuchem_kernel::budget::Budget,
) -> Result<Name, String> {
    let mut name = name_routes(smiles, style, budget)?;
    // A radical-ion name is verified through its neutral parent only.
    if name.verification.is_some() {
        return Ok(name);
    }
    // P-93.5.7.1: the general name cites each M/P axis as Ra/Sa.
    let general: Vec<_> = std::iter::once(&name.text)
        .chain(
            name.alternatives
                .iter()
                .map(|alternative| &alternative.name),
        )
        .filter_map(|text| general_axial_name(text))
        .collect();
    for text in general {
        if !name
            .alternatives
            .iter()
            .any(|existing| existing.name == text)
        {
            name.alternatives.push(nazuchem_ir::AlternativeName {
                name: text,
                status: nazuchem_ir::NameStatus::NotPreferred(
                    nazuchem_ir::NameReason::AxialAlternative,
                ),
            });
        }
    }
    Ok(name)
}

/// The name with Ra/Sa for M/P in every locanted stereodescriptor group, such
/// as (1M,2R) -> (1Ra,2R); descriptor groups are innermost parentheses.
// Provenance: fn general_axial_name | original
// Source: ReShiki | src/naming/openclatura/mod.rs::general_axial_name
fn general_axial_name(text: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len() + 4);
    let mut rest = text;
    while let Some(open) = rest.find('(') {
        out.push_str(&rest[..=open]);
        rest = &rest[open + 1..];
        // A stereodescriptor group is followed by a hyphen; a nuclide group
        // such as (31P) is not.
        let Some(close) = rest
            .find(['(', ')'])
            .filter(|&i| rest[i..].starts_with(")-"))
        else {
            continue;
        };
        let terms: Option<Vec<_>> = rest[..close]
            .split(',')
            .map(|term| {
                let descriptor = term.chars().last()?;
                let locant = &term[..term.len() - descriptor.len_utf8()];
                (locant.starts_with(|c: char| c.is_ascii_digit())
                    && "RSrsEZMPΞξ".contains(descriptor))
                .then_some((locant, descriptor))
            })
            .collect();
        if let Some(terms) = terms.filter(|terms| terms.iter().any(|(_, d)| matches!(d, 'M' | 'P')))
        {
            let terms: Vec<_> = terms
                .into_iter()
                .map(|(locant, descriptor)| match descriptor {
                    'M' => format!("{locant}Ra"),
                    'P' => format!("{locant}Sa"),
                    _ => format!("{locant}{descriptor}"),
                })
                .collect();
            out.push_str(&terms.join(","));
            rest = &rest[close..];
        }
    }
    out.push_str(rest);
    (out != text).then_some(out)
}

// Provenance: fn name_routes | adapted
// Source: openclatura | src/openclatura/namer.py::name_smiles
// Source: openclatura | src/openclatura/namer.py::name_component
fn name_routes(
    smiles: &str,
    style: nazuchem_ir::IsotopeStyle,
    budget: &nazuchem_kernel::budget::Budget,
) -> Result<Name, String> {
    if let Some(name) = unknown_stereo::name_smiles(smiles, style, budget)? {
        return Ok(name);
    }
    let canonical = canonical_smiles(smiles, budget)?;
    if canonical.contains('.') {
        return species::name_salt(&canonical, style, budget);
    }
    if let Some(name) = species::monatomic_cation(&canonical, false, budget)? {
        return Ok(name);
    }
    if let Some((neutral, suffix)) = radical_ions::recover(&canonical, budget)? {
        let name = name_smiles_with_style(&neutral, style, budget)?;
        return Ok(Name {
            text: format!("{} {suffix}", name.text),
            status: nazuchem_ir::NameStatus::NotPreferred(nazuchem_ir::NameReason::RadicalIon),
            alternatives: vec![],
            omitted_stereo: vec![],
            omitted_units: 0,
            verification: Some(Verification {
                name: name.text,
                smiles: neutral,
            }),
        });
    }
    let mol = graph_io::read_smiles(smiles, budget)?;
    if mol.atoms.values().any(|atom| atom.isotope.is_some()) {
        return isotope_naming::name_smiles(smiles, style, budget);
    }
    // Match upstream require_validated_chalcogens before any shortcut can consume Po.
    if mol.atoms.values().any(|atom| atom.symbol == "Po") {
        return Err("openclatura: polonium nomenclature is not validated".into());
    }
    if let Some(name) = radical_naming::name_radical(&mol)? {
        return Ok(name);
    }
    validate_parent(&mol)?;
    let components = graph_io::get_connected_components(&mol);
    if components.len() != 1 {
        return Err("openclatura: expected one connected component".into());
    }
    let oxide = amine_oxides::name(&mol, &components[0])?;
    let phane = phane::name(&mol);
    let component =
        match component_namer::name_component_with_citation(&mol, &components[0], None, None) {
            Ok(component) => component,
            Err(error) => {
                if let Some(name) = oxide {
                    return Ok(name);
                }
                return phane.map_or(Err(error), |text| {
                    Ok(Name {
                        text,
                        status: nazuchem_ir::NameStatus::Unverified(nazuchem_ir::NameReason::Phane),
                        alternatives: vec![],
                        omitted_stereo: vec![],
                        omitted_units: 0,
                        verification: None,
                    })
                });
            }
        };
    assess_derivation(&mol, &component.derivation);
    // Preserve already certified fused oxide templates, including isoquinoline 2-oxide.
    if mol.name_status() != nazuchem_ir::NameStatus::Preferred
        && let Some(name) = oxide
    {
        return Ok(name);
    }
    let text = component.text;
    if phane.is_some() {
        mol.mark_name(nazuchem_ir::NameStatus::NotPreferred(
            nazuchem_ir::NameReason::Phane,
        ));
    }
    let status = mol.name_status();
    let mut alternatives = vec![];
    if mol.has_higher_order_fusion_alternative() || mol.has_benzo_fusion_alternative() {
        // FR-3.5 / P-25.3.5: retain the pre-regrouping citation for general use.
        // Cloning discards fusion plans cached under the preferred component policy.
        let higher_order = mol.clone();
        higher_order.set_benzoheterocycle_names(false);
        let alternative = component_namer::name_component(&higher_order, &components[0]);
        mol.set_benzoheterocycle_names(true);
        if let Ok(alternative) = alternative
            && alternative != text
        {
            alternatives.push(nazuchem_ir::AlternativeName {
                name: nest_enclosing_marks(&alternative),
                status: nazuchem_ir::NameStatus::NotPreferred(
                    if mol.has_benzo_fusion_alternative() {
                        nazuchem_ir::NameReason::BenzoFusion
                    } else {
                        nazuchem_ir::NameReason::HigherOrderFusion
                    },
                ),
            });
        }
    }
    if mol.has_relative_ring_alternative() {
        mol.set_relative_ring_names(true);
        if let Ok(relative) = component_namer::name_component(&mol, &components[0])
            && relative != text
        {
            alternatives.push(nazuchem_ir::AlternativeName {
                name: nest_enclosing_marks(&relative),
                status: nazuchem_ir::NameStatus::NotPreferred(
                    nazuchem_ir::NameReason::RelativeRingStereo,
                ),
            });
        }
        mol.set_relative_ring_names(false);
    }
    if mol.has_endo_exo_alternative() {
        mol.set_endo_exo_names(true);
        if let Ok(relative) = component_namer::name_component(&mol, &components[0])
            && relative != text
        {
            alternatives.push(nazuchem_ir::AlternativeName {
                name: nest_enclosing_marks(&relative),
                status: nazuchem_ir::NameStatus::NotPreferred(
                    nazuchem_ir::NameReason::EndoExoStereo,
                ),
            });
        }
    }
    alternatives.extend(phane.map(|name| nazuchem_ir::AlternativeName {
        name,
        status: nazuchem_ir::NameStatus::Unverified(nazuchem_ir::NameReason::Phane),
    }));
    Ok(Name {
        text: nest_enclosing_marks(&text),
        status,
        alternatives,
        omitted_stereo: vec![],
        omitted_units: 0,
        verification: None,
    })
}

/// Keep the isotope parent on the ordinary engine's existing validation path.
// Provenance: fn validate_parent | original
// Source: ReShiki | src/naming/openclatura/mod.rs::validate_parent
pub(crate) fn validate_parent(mol: &molecule::Molecule) -> Result<(), String> {
    ionic_naming::validate_heteroatom_oxo_charge_states(mol)?;
    // Upstream divergence: nitrogen stereocentres and ring stereocentres with a
    // lone pair (for example three single bonds on P, As, S+) read back through OPSIN as
    // another stereoisomer, or lose their descriptor.
    let cyclic = chains::get_cyclic_atoms(mol, &molecule::AtomSet::new());
    if mol.atoms.values().any(|atom| {
        atom.has_stereo()
            && (atom.symbol == "N"
                || !atom.is_carbon()
                    && cyclic.contains(&atom.idx)
                    && mol.degree(atom.idx) == 3
                    && has_nonbonding_pair(atom, 3)
                    && mol
                        .get_neighbors(atom.idx)
                        .iter()
                        .all(|&n| mol.bond_order(atom.idx, n) == 1))
    }) {
        return Err("openclatura: N and ring lone-pair stereocentres are not validated".into());
    }
    Ok(())
}

// Provenance: fn assess_derivation | original
// Source: ReShiki | src/naming/openclatura/mod.rs::assess_derivation
fn assess_derivation(mol: &molecule::Molecule, derivation: &component_namer::Derivation) {
    species::assess_anion_parent(mol, derivation);
    species::assess_cation_parent(mol, derivation);
    // P-51.3.2 / P-52.2.5.3: mandatory phane nomenclature takes precedence.
    if phane::has_linear_skeleton(mol, derivation.component_splits()) {
        mol.mark_name(nazuchem_ir::NameStatus::NotPreferred(
            nazuchem_ir::NameReason::LinearPhane,
        ));
    } else if has_ring_assembly(mol) || has_anionic_ring_assembly(mol) {
        mol.mark_name(nazuchem_ir::NameStatus::NotPreferred(
            nazuchem_ir::NameReason::RingAssembly,
        ));
    } else if has_multiplicative_pin(mol, derivation) {
        mol.mark_name(nazuchem_ir::NameStatus::NotPreferred(
            nazuchem_ir::NameReason::Multiplicative,
        ));
    }
    if requires_skeletal_replacement(mol, derivation) {
        mol.mark_name(nazuchem_ir::NameStatus::Unverified(
            nazuchem_ir::NameReason::UnauditedParent,
        ));
    }
    if matches!(
        derivation,
        component_namer::Derivation::Plan { .. }
            | component_namer::Derivation::Shortcut {
                certified: true,
                ..
            }
    ) {
        mol.certify();
    }
}

/// Rewrite the namer's enclosing parentheses in the IUPAC nesting order
/// `{[({[( )]})]}` (P-16.5.4), choosing each mark from what it encloses.
///
/// Square brackets (fusion, von Baeyer, spiro descriptors, locanted ring
/// assemblies), the braces of superscripts such as `1^{3,7}`, the parentheses
/// of a ring assembly's parent hydride and of indicated or added hydrogen are
/// part of the parent name and do not count (P-16.5.4.1.1,
/// P-16.5.4.1.2). Any other parenthesis, including stereodescriptors and
/// compound locants, counts as a level (P-16.5.4.1.3), and a mark that would
/// directly follow or precede one of the same kind moves up a level
/// (P-16.5.4.1.5).
///
/// Public so downstream regression harnesses, including ReShiki's, can compare
/// parentheses-only reference names with the engine's generated spelling.
// Provenance: fn nest_enclosing_marks | original
// Source: ReShiki | src/naming/openclatura/mod.rs::nest_enclosing_marks
pub fn nest_enclosing_marks(name: &str) -> String {
    // Per open group: its start, the deepest counted level inside it, and the
    // levels of counted groups that start right after it opens and end right
    // before it closes.
    struct Group {
        open: usize,
        inner: usize,
        first: Option<usize>,
        last: Option<(usize, usize)>,
    }
    let mut out = name.to_owned();
    let mut stack: Vec<Group> = Vec::new();
    for (i, byte) in name.bytes().enumerate() {
        match byte {
            b'(' | b'[' => stack.push(Group {
                open: i,
                inner: 0,
                first: None,
                last: None,
            }),
            close @ (b')' | b']') => {
                let Some(group) = stack.pop() else { continue };
                // Indicated or added hydrogen such as `(4aH,9H)` and the
                // multiplied parent of a ring assembly do not count
                // (P-16.5.4.1.1, P-16.5.4.1.2).
                let counted = close == b')'
                    && !["-bi", "-ter", "-quater", "-quinque", "-sexi"]
                        .iter()
                        .any(|multiplier| name[..group.open].ends_with(multiplier))
                    && !name[group.open + 1..i].split(',').all(|locant| {
                        locant.strip_suffix('H').is_some_and(|locant| {
                            let digits = locant
                                .trim_end_matches(|c: char| c == '\'' || c.is_ascii_lowercase());
                            !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
                        })
                    });
                let mut level = group.inner + usize::from(counted);
                if counted {
                    let last = group.last.filter(|&(end, _)| end + 1 == i).map(|(_, l)| l);
                    while [group.first, last]
                        .into_iter()
                        .flatten()
                        .any(|l| l % 3 == level % 3)
                    {
                        level += 1;
                    }
                    let mark = (level - 1) % 3;
                    out.replace_range(group.open..=group.open, ["(", "[", "{"][mark]);
                    out.replace_range(i..=i, [")", "]", "}"][mark]);
                }
                if let Some(parent) = stack.last_mut() {
                    parent.inner = parent.inner.max(level);
                    if counted {
                        if group.open == parent.open + 1 {
                            parent.first = Some(level);
                        }
                        parent.last = Some((i, level));
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// P-51.4.1 / P-15.4.3.2: four heterounits on an acyclic path require
/// skeletal replacement, including in substituents (with imine N or
/// oxo-substituted P in the chain, P-51.4.1.1). Each skeletal heteroatom
/// counts as a unit, which can only overcount heterounits such as -SiH2-O-SiH2-;
/// terminal B, Al, Ga, In, Tl, Si, Ge, Sn, Pb, P, As, Sb and Bi count too (P-51.4.1.4). Only the cited
/// principal group's atoms are excluded.
// Provenance: fn requires_skeletal_replacement | original
// Source: ReShiki | src/naming/openclatura/mod.rs::requires_skeletal_replacement
fn requires_skeletal_replacement(
    mol: &molecule::Molecule,
    derivation: &component_namer::Derivation,
) -> bool {
    let principal_atoms = match derivation {
        component_namer::Derivation::Plan {
            principal_group, ..
        } => principal_group.as_ref().map(|(_, atoms)| atoms),
        component_namer::Derivation::Shortcut { .. } => None,
    };
    let cyclic = chains::get_cyclic_atoms(mol, &molecule::AtomSet::new());
    let skeletal: molecule::AtomSet = mol
        .atoms
        .values()
        .filter(|atom| {
            !atom.is_carbon()
                && atom.symbol != "H"
                && atom.charge == 0
                && !cyclic.contains(&atom.idx)
                && principal_atoms.is_none_or(|atoms| !atoms.contains(&atom.idx))
                && (mol
                    .get_neighbors(atom.idx)
                    .iter()
                    .filter(|n| mol.atoms[n].symbol != "H")
                    .count()
                    >= 2
                    || matches!(
                        atom.symbol.as_str(),
                        "B" | "Al"
                            | "Ga"
                            | "In"
                            | "Tl"
                            | "Si"
                            | "Ge"
                            | "Sn"
                            | "Pb"
                            | "P"
                            | "As"
                            | "Sb"
                            | "Bi"
                    ))
        })
        .map(|atom| atom.idx)
        .collect();
    skeletal.iter().any(|&start| {
        let mut pending = vec![(start, start, 1)];
        while let Some((previous, current, units)) = pending.pop() {
            if units >= 4 {
                return true;
            }
            for &next in mol.get_neighbors(current) {
                if next != previous && !cyclic.contains(&next) && mol.atoms[&next].symbol != "H" {
                    pending.push((current, next, units + usize::from(skeletal.contains(&next))));
                }
            }
        }
        false
    })
}

/// P-51.1.5 / P-51.3.1: multiply identical senior parents, including all
/// substituents, suffixes and their attachment positions. P-15.3 permits an
/// asymmetric central group, but its concatenated arms must be identical.
// Provenance: fn has_multiplicative_pin | adapted
// Source: openclatura | src/openclatura/component_namer.py::name_component
// Source: openclatura | src/openclatura/component_namer.py::select_component_parent
fn has_multiplicative_pin(
    mol: &molecule::Molecule,
    derivation: &component_namer::Derivation,
) -> bool {
    if matches!(derivation, component_namer::Derivation::Shortcut { .. }) {
        return false;
    }
    use molecule::AtomSet;
    let cyclic = chains::get_cyclic_atoms(mol, &AtomSet::new());
    let mut groups = perception::perceive_groups(mol);
    let key = principal_groups::component_principal_key(mol, &groups, false);
    let mut excluded = AtomSet::new();
    component_group_rules::retarget_external_carbonyl_groups(
        mol,
        &mut groups,
        key.as_deref(),
        &mut excluded,
        &cyclic,
    );
    let (principal, _) =
        principal_groups::partition_principal_and_prefix_groups(&groups, key.as_deref());
    component_group_rules::exclude_nonparent_group_atoms(mol, &groups, &mut excluded, &cyclic);
    let (Ok(paths), Ok(rings)) = (
        chains::find_all_carbon_paths(mol, &excluded),
        chains::find_ring_systems(mol, &excluded),
    ) else {
        return false;
    };
    let parents: Vec<_> = parent_selection::senior_parent_candidates(
        mol, &paths, &rings, &principal, &groups, &excluded,
    )
    .into_iter()
    // P-15.3.1.1 / P-15.3.4.2 exclude acyclic hydrocarbons without suffixes.
    .filter(|p| p.is_ring || principal.iter().any(|a| p.primary_path().contains(a)))
    .map(|p| {
        let atoms = p.atom_set();
        let mut complete = atoms.clone();
        complete.extend(component_group_rules::principal_involved_atoms(
            mol,
            &groups,
            key.as_deref(),
            p.primary_path(),
        ));
        (p.primary_path()[0], atoms, complete)
    })
    .collect();
    if parents.len() < 2 {
        return false;
    }
    // Simple central groups use a chain, cyclic parent (including an assembly),
    // or heteroatom. Side substituents belong to the central group and need
    // not be symmetric; the branches from it towards the parents must be.
    let mut cores: Vec<AtomSet> = paths.iter().map(|p| p.iter().copied().collect()).collect();
    cores.extend(rings.iter().map(|r| r.atoms.clone()));
    cores.extend(
        ring_assembly::discover_ring_assemblies(mol, &rings)
            .into_iter()
            .map(|a| a.locant_maps[0].keys().copied().collect()),
    );
    cores.extend(
        mol.atoms
            .values()
            .filter(|a| !a.is_carbon() && a.symbol != "H")
            .map(|a| AtomSet::from([a.idx])),
    );
    for (i, (root_a, a, required_a)) in parents.iter().enumerate() {
        for (root_b, b, required_b) in &parents[i + 1..] {
            if !required_a.is_disjoint(required_b) {
                continue;
            }
            for core in &cores {
                if !core.is_disjoint(required_a) || !core.is_disjoint(required_b) {
                    continue;
                }
                let left = subgraph_tools::subgraph_component(mol, *root_a, core);
                if !required_a.is_subset(&left) || !left.is_disjoint(required_b) {
                    continue;
                }
                let right = subgraph_tools::subgraph_component(mol, *root_b, core);
                if !required_b.is_subset(&right) || !left.is_disjoint(&right) {
                    continue;
                }
                let attachments = |branch: &AtomSet| {
                    branch
                        .iter()
                        .flat_map(|&u| {
                            mol.get_neighbors(u)
                                .iter()
                                .filter(|v| core.contains(v))
                                .map(move |&v| (u, v))
                        })
                        .collect::<Vec<_>>()
                };
                let (left_edges, right_edges) = (attachments(&left), attachments(&right));
                let ([(u, x)], [(v, y)]) = (&left_edges[..], &right_edges[..]) else {
                    continue;
                };
                if multiplicative_bond(mol, *u, *x) == multiplicative_bond(mol, *v, *y)
                    && identical_multiplicative_branches(mol, &left, &right, a, b, *u, *v)
                {
                    return true;
                }
            }
        }
    }
    false
}

// Provenance: fn multiplicative_bond | original
// Source: ReShiki | src/naming/openclatura/mod.rs::multiplicative_bond
fn multiplicative_bond(
    mol: &molecule::Molecule,
    u: usize,
    v: usize,
) -> Option<(usize, Option<&str>)> {
    mol.get_bond(u, v)
        .map(|b| (if b.is_aromatic { 4 } else { b.order }, b.stereo.as_deref()))
}

/// A rooted graph proof preserves complete parent units and configuration
/// (P-51.3.1, P-45.6.2), rather than comparing unrooted hydrides or global ranks.
// Provenance: fn identical_multiplicative_branches | adapted
// Source: openclatura | src/openclatura/locant_elision.py::_isomorphic
// Source: openclatura | src/openclatura/locant_elision.py::_has_parent_set_automorphism_mapping
fn identical_multiplicative_branches(
    mol: &molecule::Molecule,
    left: &molecule::AtomSet,
    right: &molecule::AtomSet,
    parent_left: &molecule::AtomSet,
    parent_right: &molecule::AtomSet,
    root_left: usize,
    root_right: usize,
) -> bool {
    use std::collections::BTreeMap;
    if left.len() != right.len() {
        return false;
    }
    let ranks_left = chains::canonical_ranks(&mol.subgraph(left), None);
    let ranks_right = chains::canonical_ranks(&mol.subgraph(right), None);
    let atom_key = |u, atoms: &molecule::AtomSet, parent: &molecule::AtomSet, root| {
        let a = &mol.atoms[&u];
        (
            a.symbol.as_str(),
            a.charge,
            a.isotope,
            a.is_aromatic,
            a.total_h_count,
            a.radical_electrons,
            a.stereo.as_deref(),
            mol.get_neighbors(u)
                .iter()
                .filter(|n| atoms.contains(n))
                .count(),
            parent.contains(&u),
            u == root,
        )
    };
    let candidates: BTreeMap<_, Vec<_>> = left
        .iter()
        .map(|&u| {
            (
                u,
                right
                    .iter()
                    .copied()
                    .filter(|&v| {
                        ranks_left[&u] == ranks_right[&v]
                            && atom_key(u, left, parent_left, root_left)
                                == atom_key(v, right, parent_right, root_right)
                    })
                    .collect(),
            )
        })
        .collect();
    // Provenance: fn identical_multiplicative_branches::matches | adapted
    // Source: openclatura | src/openclatura/locant_elision.py::_isomorphic
    fn matches(
        mol: &molecule::Molecule,
        candidates: &BTreeMap<usize, Vec<usize>>,
        mapping: &mut BTreeMap<usize, usize>,
    ) -> bool {
        let Some((&u, options)) = candidates
            .iter()
            .filter(|(u, _)| !mapping.contains_key(u))
            .min_by_key(|(_, options)| {
                options
                    .iter()
                    .filter(|v| !mapping.values().any(|x| x == *v))
                    .count()
            })
        else {
            return true;
        };
        for &v in options {
            if mapping.values().any(|&x| x == v)
                || mapping.iter().any(|(&a, &b)| {
                    multiplicative_bond(mol, u, a) != multiplicative_bond(mol, v, b)
                })
            {
                continue;
            }
            mapping.insert(u, v);
            if matches(mol, candidates, mapping) {
                return true;
            }
            mapping.remove(&u);
        }
        false
    }
    matches(mol, &candidates, &mut BTreeMap::new())
}

/// Whether two identical ring systems are joined directly by a single or
/// double bond that the rendered name does not cite as a ring assembly
/// (P-28, P-52.2.7): a double-bond junction or units the assembly parent
/// cannot number. Systems compare as parent hydrides, so
/// substituents count as hydrogen; different systems keep the substitutive
/// PIN, cyclohexylbenzene (P-61.2.2).
// Provenance: fn has_ring_assembly | original
// Source: ReShiki | src/naming/openclatura/mod.rs::has_ring_assembly
fn has_ring_assembly(mol: &molecule::Molecule) -> bool {
    ring_assembly::assembly_junctions(mol)
        .iter()
        .any(|&junction| !mol.ring_assembly_is_named(junction))
}

/// Removing or adding a hydron does not change the parent hydride, so a
/// pyrrolide joined to a pyrrole still has a ring assembly parent (P-28,
/// P-72.2.2.1, P-73.1).
// Provenance: fn has_anionic_ring_assembly | adapted
// Source: openclatura | src/openclatura/fusion/charges.py::fusion_component_charge_parent
fn has_anionic_ring_assembly(mol: &molecule::Molecule) -> bool {
    if mol.atoms.values().all(|atom| atom.charge != -1) {
        return false;
    }
    let mut neutral = mol.clone();
    for atom in neutral.atoms.values_mut() {
        if atom.charge == -1 {
            atom.charge = 0;
            atom.total_h_count += 1;
            atom.explicit_h_count += 1;
        } else if atom.charge == 1 && atom.total_h_count > 0 {
            atom.charge = 0;
            atom.total_h_count -= 1;
            atom.explicit_h_count = atom.explicit_h_count.saturating_sub(1);
        }
    }
    ring_assembly::assembly_junctions(&neutral)
        .iter()
        .any(|&junction| !mol.ring_assembly_is_named(junction))
}

/// Count nonbonding electrons on the main-group elements that can carry
/// tetrahedral tags. Attached hydrogens also consume bonding electrons:
/// a three-neighbour Si–H centre has four bonds and no lone pair.
// Provenance: fn has_nonbonding_pair | original
// Source: ReShiki | src/naming/openclatura/mod.rs::has_nonbonding_pair
fn has_nonbonding_pair(atom: &molecule::Atom, bond_order_sum: usize) -> bool {
    let outer_electrons = match atom.symbol.as_str() {
        "C" | "Si" | "Ge" | "Sn" | "Pb" => 4,
        "N" | "P" | "As" | "Sb" | "Bi" => 5,
        "O" | "S" | "Se" | "Te" | "Po" => 6,
        "F" | "Cl" | "Br" | "I" | "At" => 7,
        _ => return false,
    };
    outer_electrons
        - atom.charge
        - (bond_order_sum + atom.total_h_count + atom.radical_electrons) as i32
        >= 2
}

#[cfg(test)]
mod test_support;

#[cfg(test)]
// Provenance: mod tests | original
// Source: ReShiki | src/naming/openclatura/mod.rs::tests
mod tests {
    use super::{name_smiles, nest_enclosing_marks};
    use std::io::Write;

    #[test]
    // Provenance: fn tests::naming_routes_use_the_supplied_reservation_policy | adapted
    // Source: ReShiki | src/naming/openclatura/mod.rs::tests::naming_routes_use_the_supplied_reservation_policy
    // Source: openclatura | tests/unit/test_analysis.py::test_public_api_golden_names
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn naming_routes_use_the_supplied_reservation_policy() {
        let budget =
            nazuchem_kernel::budget::Budget::new(usize::MAX, usize::MAX, None, usize::MAX, |_| {
                None
            });
        for smiles in ["CCO", "[13CH3]CO", "[Na+].[Cl-]", "C[N+](C)C"] {
            let error = name_smiles(smiles, &budget).unwrap_err();
            assert!(error.contains("resources are busy"), "{smiles}: {error}");
        }
    }

    #[test]
    fn symmetric_azine_ylidene_spelling_keeps_unaudited_status() {
        use nazuchem_ir::{NameReason, NameStatus};

        for (smiles, expected) in [
            ("C/C=N/N=C/C", "(1E,2E)-diethylidenehydrazine"),
            ("CC/C=N/N=C/CC", "(1E,2E)-dipropylidenehydrazine"),
            (
                "C/C(/CC)=N\\N=C(/C)\\CC",
                "(1E,2E)-di(butan-2-ylidene)hydrazine",
            ),
            ("CC(C)=NN=C(C)C", "di(propan-2-ylidene)hydrazine"),
            ("CCCCCCCCCC=NN=CCCCCCCCCC", "di(decylidene)hydrazine"),
        ] {
            let name = name_smiles(smiles, &Default::default()).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(
                name.status,
                NameStatus::Unverified(NameReason::UnauditedPath),
                "{smiles}"
            );
        }
        assert!(name_smiles("C/C=N/N=C\\C", &Default::default()).is_err());
    }

    #[test]
    // Provenance: fn tests::azine_spelling_preserves_alkene_control | adapted
    // Source: ReShiki | src/naming/openclatura/mod.rs::tests::azine_spelling_preserves_alkene_control
    // Source: openclatura | tests/roundtrip/test_stereo.py::test_roundtrip
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn azine_spelling_preserves_alkene_control() {
        let name = name_smiles("C/C=C/C", &Default::default()).unwrap();
        assert_eq!(name.text, "(2E)-but-2-ene");
        assert_eq!(name.status, nazuchem_ir::NameStatus::Preferred);
    }

    #[test]
    // Provenance: fn tests::alkoxy_contractions_preserve_parent_components | adapted
    // Source: ReShiki | src/naming/openclatura/mod.rs::tests::alkoxy_contractions_preserve_parent_components
    // Source: openclatura | tests/fixtures/diverse_corpus.csv::<fixture data>
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn alkoxy_contractions_preserve_parent_components() {
        for (smiles, expected) in [
            ("OC(=O)COC(C)C", "[(propan-2-yl)oxy]acetic acid"),
            ("OC(=O)COCC(C)C", "(2-methylpropoxy)acetic acid"),
            ("OC(=O)COCCCCCCl", "[(5-chloropentyl)oxy]acetic acid"),
            (
                "OC(=O)COC(C)(C)CCl",
                "[(1-chloro-2-methylpropan-2-yl)oxy]acetic acid",
            ),
            (
                "OC(=O)COc1ccc(-c2ccccc2)cc1",
                "[([1,1'-biphenyl]-4-yl)oxy]acetic acid",
            ),
            ("OC(=O)COCC1CCCCC1", "(cyclohexylmethoxy)acetic acid"),
            ("OC(=O)COCc1ccccc1", "(benzyloxy)acetic acid"),
            ("OC(=O)COC(C)CCl", "[(1-chloropropan-2-yl)oxy]acetic acid"),
            (
                "OC(=O)CSc1ccc(Cl)cc1",
                "[(4-chlorophenyl)sulfanyl]acetic acid",
            ),
            ("OC(=O)COCCO", "(2-hydroxyethoxy)acetic acid"),
            ("OC(=O)COC(C)(C)C", "tert-butoxyacetic acid"),
            ("ClCCOC(=O)c1ccccc1", "2-chloroethyl benzoate"),
            ("COc1ccccc1", "anisole"),
            ("OC(=O)COC(C)CC", "[(butan-2-yl)oxy]acetic acid"),
            ("OC(=O)C(OCCO)OCCO", "bis(2-hydroxyethoxy)acetic acid"),
            (
                "ClCOc1ccc(CCl)cc1",
                "1-(chloromethoxy)-4-(chloromethyl)benzene",
            ),
            ("ClCCOC(=O)CC(=O)O", "(2-chloroethoxycarbonyl)acetic acid"),
            ("CC(C)CO[SiH2]C", "methyl(2-methylpropoxy)silane"),
        ] {
            assert_eq!(
                name_smiles(smiles, &Default::default()).unwrap().text,
                expected,
                "{smiles}"
            );
        }
    }

    #[test]
    fn skeletal_replacement_counts_heterounits_and_the_cited_group() {
        for (smiles, required) in [
            ("COCOCOC", false),
            ("COCOCOCOC", true),
            ("CNNOOC", true),
            ("CO[SiH2]OCCOCC(=O)O", true),
            ("COCCOCCOC[SiH3]", true),
            ("COCCOCCOC[PH2]", true),
            ("COCCOCC=NCCOCC(=O)O", true),
            ("COCOCOCC(=O)NC", false),
            ("COCOCOCC(=O)NCC(=O)O", true),
            ("CC(=O)NCC(=O)NCC(=O)NCC(=O)NCC(=O)O", true),
        ] {
            let mol = super::graph_io::read_smiles(smiles, &Default::default()).unwrap();
            let atoms = mol.atoms.keys().copied().collect();
            let component =
                super::component_namer::name_component_with_citation(&mol, &atoms, None, None)
                    .unwrap();
            assert_eq!(
                super::requires_skeletal_replacement(&mol, &component.derivation),
                required,
                "{smiles}"
            );
        }
    }

    #[test]
    // Provenance: fn tests::allow_list_status_matrix | adapted
    // Source: ReShiki | src/naming/openclatura/mod.rs::tests::allow_list_status_matrix
    // Source: openclatura | tests/fixtures/diverse_corpus.golden.json::<fixture data>
    // Source: openclatura | tests/unit/test_chalcogen_functional_groups.py::test_anhydride_bridges_and_acyl_sites_are_independent
    // Source: openclatura | tests/unit/test_fused_bonds.py::test_fused_bonds_match_the_cycle_count
    // Source: openclatura | tests/roundtrip/test_replacement_parents.py::test_roundtrip
    // Source: openclatura | tests/data/parser_xml_resources/simpleGroups.json::<fixture data>
    // Source: openclatura | tests/unit/test_analysis.py::test_public_api_golden_names
    // Source: openclatura | tests/unit/test_analysis.py::test_group_13_14_central_atoms_have_a_parent_hydride
    // Source: openclatura | tests/unit/test_polyoxide_chains.py::test_substituted_oxygen_chains_use_the_oxidane_parent
    // Source: openclatura | tests/unit/test_audit_reconstruction.py::test_self_audit_confirms_modelled_names
    // Source: openclatura | tests/unit/test_chalcogen_functional_groups.py::test_ether_analogues_reuse_generic_heteroatom_subgraph_naming
    // Source: openclatura | tests/unit/test_all_chalcogen_acyl_regressions.py::test_reported_name_regressions
    // Source: openclatura | tests/unit/test_analysis.py::test_nitrogen_chain_roles_make_cyclic_hydrazines_prefixes
    // Source: openclatura | tests/unit/test_analysis.py::test_component_shortcuts_are_final_binding_audited
    // Source: openclatura | tests/unit/test_analysis.py::test_anhydride_shortcut_binds_acid_halves_and_core_separately
    // Source: openclatura | tests/unit/test_analysis.py::test_a_zwitterion_keeps_the_anion_as_the_suffix
    // Source: openclatura | tests/roundtrip/test_oxyacids.py::test_roundtrip
    // Source: openclatura | tests/unit/test_locant_elision.py::test_nonconstitutional_and_unsupported_parent_locants_are_unchanged
    // Source: openclatura | tests/roundtrip/test_polycycles.py::test_roundtrip
    // Source: openclatura | tests/roundtrip/test_role_models.py::test_roundtrip
    // Source: openclatura | tests/roundtrip/test_principal_group_breadth.py::test_roundtrip
    // Source: openclatura | tests/exploratory/test_group14_ether_ligand_parent_selection.py::test_ring_ligand_does_not_outrank_the_mononuclear_hydride_parent
    // Source: openclatura | tests/exploratory/test_group14_ether_ligand_parent_selection.py::test_a_genuine_principal_group_elsewhere_still_blocks_the_hydride_parent
    // Source: openclatura | tests/data/parser_xml_resources/arylGroups.json::<fixture data>
    // Source: openclatura | tests/data/parser_xml_resources/nonCarboxylicAcids.json::<fixture data>
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn allow_list_status_matrix() {
        use nazuchem_ir::{NameReason, NameStatus};
        for (smiles, expected_name, expected_status) in [
            ("CCO", "ethanol", NameStatus::Preferred),
            ("CC(=O)O", "acetic acid", NameStatus::Preferred),
            ("Oc1ccccc1", "phenol", NameStatus::Preferred),
            ("OC1CCCCC1", "cyclohexanol", NameStatus::Preferred),
            ("c1ccncc1", "pyridine", NameStatus::Preferred),
            ("c1ccc2ccccc2c1", "naphthalene", NameStatus::Preferred),
            (
                "c1ccc2c(c1)C=CC=CO2",
                "1-benzoxepine",
                NameStatus::Preferred,
            ),
            (
                "C1CCC2CCCCC2C1",
                "decahydronaphthalene",
                NameStatus::Preferred,
            ),
            (
                "c1ccc(cc1)-c1ccccc1",
                "1,1'-biphenyl",
                NameStatus::Preferred,
            ),
            (
                "C1CC2CCC1C2",
                "bicyclo[2.2.1]heptane",
                NameStatus::Preferred,
            ),
            (
                "O[C@@H]1C[C@H]2CC[C@@H]1C2",
                "(1R,2R,4S)-bicyclo[2.2.1]heptan-2-ol",
                NameStatus::Preferred,
            ),
            (
                "C1CCC2(CC1)CCCC2",
                "spiro[4.5]decane",
                NameStatus::Preferred,
            ),
            (
                "C1CCC2(CC1)OCCO2",
                "1,4-dioxaspiro[4.5]decane",
                NameStatus::Preferred,
            ),
            ("CCOC(C)=O", "ethyl acetate", NameStatus::Preferred),
            ("CC(=O)OC(C)=O", "acetic anhydride", NameStatus::Preferred),
            (
                "CC(=O)OC(=O)CCC(=O)OC(C)=O",
                "4-(acetyloxy)-4-oxobutanoic acetic anhydride",
                NameStatus::Unverified(NameReason::UnauditedPath),
            ),
            (
                "c1ccc(cc1)[Si](c1ccccc1)(c1ccccc1)c1ccccc1",
                "tetraphenylsilane",
                NameStatus::Preferred,
            ),
            (
                "CP(C)(C)=O",
                "trimethyl-lambda5-phosphanone",
                NameStatus::Preferred,
            ),
            (
                "C[P+](C)(C)[O-]",
                "trimethyl-lambda5-phosphanone",
                NameStatus::Preferred,
            ),
            (
                "o1ccc2oc3ccoc3c12",
                "difuro[3,2-b:2',3'-d]furan",
                NameStatus::Unverified(NameReason::FusionMultiparent),
            ),
            (
                "O1CCC2CCCCC2C1",
                "octahydro-1H-2-benzopyran",
                NameStatus::Preferred,
            ),
            (
                "O=C(Oc1ccc(Oc2ccccc2)cc1)Oc1ccc(Oc2ccccc2)cc1",
                "bis(4-phenoxyphenyl) carbonate",
                NameStatus::Preferred,
            ),
            ("CCO[N+](=O)[O-]", "ethyl nitrate", NameStatus::Preferred),
            ("[Na+].CC(=O)[O-]", "sodium acetate", NameStatus::Preferred),
            (
                "O=C([O-])c1cc[n-]c1",
                "1H-pyrrol-1-ide-3-carboxylate",
                NameStatus::Preferred,
            ),
            (
                "C1CC2C3CCC1C2C3",
                "tricyclo[4.3.0.0^{3,9}]nonane",
                NameStatus::Unverified(NameReason::UnauditedParent),
            ),
            (
                "C1CCCCCOCCCCC1",
                "1-oxacyclododecane",
                NameStatus::Unverified(NameReason::UnauditedParent),
            ),
            (
                "COOC",
                "1,2-dimethyldioxidane",
                NameStatus::NotPreferred(NameReason::HydrideParent),
            ),
            (
                "C[Te]C",
                "dimethyltellane",
                NameStatus::NotPreferred(NameReason::HydrideParent),
            ),
            (
                "NN",
                "hydrazine",
                NameStatus::Unverified(NameReason::UnauditedPath),
            ),
            (
                "O[N+](=O)[O-]",
                "nitric acid",
                NameStatus::Unverified(NameReason::UnauditedPath),
            ),
            (
                "[SiH4]",
                "silane",
                NameStatus::Unverified(NameReason::UnauditedPath),
            ),
            ("CCB(O)O", "ethylboronic acid", NameStatus::Preferred),
            (
                "C12(CC3CCC1C3)C1CCC2CC1",
                "2,7'-spirobi[bicyclo[2.2.1]heptane]",
                NameStatus::Preferred,
            ),
            (
                "CC(=O)[NH-]",
                "acetamide",
                NameStatus::Unverified(NameReason::AnionParent),
            ),
            (
                "c1ccccc1C1=CC2(C3=CC=CC=C13)C=CC1=CC=CC=C12",
                "3-phenyl-1,1'-spirobi[indene]",
                NameStatus::Preferred,
            ),
            (
                "COP(=O)(OC)OCC(=O)O",
                "carboxymethyl dimethyl phosphate",
                NameStatus::Unverified(NameReason::UnauditedPath),
            ),
            (
                "CC1=CC2(C3=CC=CC=C13)C=CC1=CC=CC=C12",
                "3-methyl-1,1'-spirobi[indene]",
                NameStatus::Preferred,
            ),
            (
                "[O-]C(=O)c1cc(C(=O)[O-])ccc1Cc1cc(S(=O)(=O)[O-])[n-]c1",
                "4-{[2,4-bis(carboxylato)phenyl]methyl}-1H-pyrrol-1-ide-2-sulfonate",
                NameStatus::Preferred,
            ),
            (
                "C1C=Cc2cc3sccc3n12",
                "5H-thieno[2,3-b]pyrrolizine",
                NameStatus::Preferred,
            ),
            (
                "C[SH2]C",
                "dimethyl-lambda4-sulfane",
                NameStatus::Unverified(NameReason::UnauditedPath),
            ),
            (
                "C1=CC2(C3=CC=CC=C13)C=CC1=CC=CC=C12",
                "1,1'-spirobi[indene]",
                NameStatus::Preferred,
            ),
            (
                "CC(=O)OOC(C)=O",
                "acetic peroxyanhydride",
                NameStatus::Preferred,
            ),
            (
                "CC(=O)[Se]C(C)=O",
                "acetic selenoanhydride",
                NameStatus::Preferred,
            ),
            (
                "CN(C)C(=O)OP(=O)(O)O",
                "dimethylcarbamoyl dihydrogen phosphate",
                NameStatus::Unverified(NameReason::UnauditedPath),
            ),
            (
                "[Na+].[Cl-]",
                "sodium chloride",
                NameStatus::Unverified(NameReason::UnauditedPath),
            ),
            ("C[O-]", "methoxide", NameStatus::Preferred),
            (
                "[Na+].c1ccc2[n-]cc(C(=O)O)c2c1",
                "sodium 3-carboxy-1H-indol-1-ide",
                NameStatus::Preferred,
            ),
            (
                "[O-]C(=O)c1ccccc1-c1[n-]cc(C(=O)[O-])c1",
                "5-(2-carboxylatophenyl)-1H-pyrrol-1-ide-3-carboxylate",
                NameStatus::Preferred,
            ),
            (
                "c1ccc(Oc2ccccc2)cc1",
                "phenoxybenzene",
                NameStatus::NotPreferred(NameReason::Multiplicative),
            ),
            (
                "Oc1ccc(Oc2ccc(Oc3ccc(Oc4ccccc4)cc3)cc2)cc1",
                "4-[4-(4-phenoxyphenoxy)phenoxy]phenol",
                NameStatus::NotPreferred(NameReason::LinearPhane),
            ),
            (
                "C1C2CC3CC1CC2C3",
                "octahydro-2,5-methanopentalene",
                NameStatus::Unverified(NameReason::BridgedFusedParent),
            ),
            (
                "CC(=O)OOC(=O)CC",
                "acetic propanoic peroxyanhydride",
                NameStatus::Unverified(NameReason::UnauditedPath),
            ),
            (
                "CC(=O)OC(=O)CC",
                "acetic propanoic anhydride",
                NameStatus::Preferred,
            ),
            (
                "CP(C)Cl",
                "chlorodi(methyl)phosphane",
                NameStatus::Unverified(NameReason::UnauditedPath),
            ),
            (
                "COP(C)C",
                "methoxydi(methyl)phosphane",
                NameStatus::Unverified(NameReason::UnauditedPath),
            ),
            (
                "C[Al](C)C",
                "trimethylalumane",
                NameStatus::Unverified(NameReason::UnauditedPath),
            ),
            (
                "CC[Sn](CC)(CC)CC",
                "tetraethylstannane",
                NameStatus::Preferred,
            ),
            (
                "c1ccc(cc1)P(c1ccccc1)c1ccccc1",
                "triphenylphosphane",
                NameStatus::Preferred,
            ),
            (
                "CC(=S)OP(=O)(OC)OC",
                "ethanethioyl dimethyl phosphate",
                NameStatus::Unverified(NameReason::UnauditedPath),
            ),
            (
                "COCCOCCOCCOC",
                "1-methoxy-2-[2-(2-methoxyethoxy)ethoxy]ethane",
                NameStatus::Unverified(NameReason::UnauditedParent),
            ),
            (
                "CC(=O)NCCOCCOCCOCCOC",
                "N-(2-{2-[2-(2-methoxyethoxy)ethoxy]ethoxy}ethyl)acetamide",
                NameStatus::Unverified(NameReason::UnauditedParent),
            ),
            ("CCOCCOCC", "1,2-diethoxyethane", NameStatus::Preferred),
            (
                "CC(=O)NCC(=O)NCC(=O)NCC(=O)NCC(=O)O",
                "{2-[2-(2-acetamidoacetamido)acetamido]acetamido}acetic acid",
                NameStatus::Unverified(NameReason::UnauditedParent),
            ),
            (
                "C[N+](C)(C)CC(=O)[O-]",
                "(trimethylammonio)acetate",
                NameStatus::Unverified(NameReason::CationPrefix),
            ),
            (
                "C1CC2C3CCC1C2C3C4CC5C6CCC4C5C6",
                "2,7'-bi(tricyclo[4.3.0.0^{3,9}]nonane)",
                NameStatus::Unverified(NameReason::UnauditedParent),
            ),
            (
                "[O-]C(=O)c1ccccc1Cc1cc[n-]c1",
                "3-[(2-carboxylatophenyl)methyl]-1H-pyrrol-1-ide",
                NameStatus::Preferred,
            ),
            (
                "[O-]C(=O)c1ccccc1[CH2-]",
                "(2-carboxylatophenyl)methanide",
                NameStatus::Preferred,
            ),
            (
                "[Na+].[Na+].[O-]c1ccccc1Cc1ccccc1C(=O)[O-]",
                "disodium 2-[(2-oxidophenyl)methyl]benzoate",
                NameStatus::Preferred,
            ),
            (
                "c1ccccc1Oc2ccc(Oc3ccccc3)cc2",
                "1,4-diphenoxybenzene",
                NameStatus::NotPreferred(NameReason::Multiplicative),
            ),
            (
                "[Na+]",
                "sodium(1+)",
                NameStatus::Unverified(NameReason::UnauditedPath),
            ),
            (
                "OCC[NH3+]",
                "2-hydroxyethan-1-aminium",
                NameStatus::Preferred,
            ),
            ("[O-]c1ccccc1", "phenoxide", NameStatus::Preferred),
        ] {
            let name = name_smiles(smiles, &Default::default())
                .unwrap_or_else(|error| panic!("{smiles}: {error}"));
            assert_eq!(name.text, expected_name, "{smiles}");
            assert_eq!(name.status, expected_status, "{smiles}: {}", name.text);
        }
    }

    #[test]
    fn axial_general_names_leave_nuclide_groups_alone() {
        assert_eq!(
            super::general_axial_name("(31P)phosphanium (2M)-2-(2-hydroxyphenyl)benzen-1-olate")
                .as_deref(),
            Some("(31P)phosphanium (2Ra)-2-(2-hydroxyphenyl)benzen-1-olate")
        );
        assert_eq!(super::general_axial_name("(31P)phosphane"), None);
        assert_eq!(
            super::general_axial_name("4-[(1P,2R)-x]-(3M)-y").as_deref(),
            Some("4-[(1Sa,2R)-x]-(3Ra)-y")
        );
    }

    #[test]
    fn axial_general_alternatives_cover_ordinary_unknown_isotope_and_salt_paths() {
        use nazuchem_ir::{NameReason, NameStatus};
        for (marker, descriptor, general) in [(900_004, 'M', "Ra"), (900_008, 'P', "Sa")] {
            for (input, unknown) in [
                (
                    format!("Oc1ccc2ccccc2[c:{marker}]1-[c:{marker}]1c(O)ccc2ccccc12"),
                    false,
                ),
                (
                    format!("O[13c]1ccc2ccccc2[c:{marker}]1-[c:{marker}]1c(O)ccc2ccccc12"),
                    false,
                ),
                (
                    format!("Oc1ccc(C[C:900001](F)(Cl)Br)c[c:{marker}]1-[c:{marker}]1ccccc1O"),
                    true,
                ),
                (
                    format!("Oc1cccc[c:{marker}]1-[c:{marker}]1ccccc1[O-].[Na+]"),
                    false,
                ),
            ] {
                let name = name_smiles(&input, &Default::default())
                    .unwrap_or_else(|e| panic!("{input}: {e}"));
                assert!(name.text.contains(descriptor), "{input}: {}", name.text);
                let expected = name.text.replace(descriptor, general);
                let alternative = name
                    .alternatives
                    .iter()
                    .find(|a| a.name == expected)
                    .unwrap_or_else(|| {
                        panic!("{input}: missing {expected}: {:?}", name.alternatives)
                    });
                assert_eq!(
                    alternative.status,
                    NameStatus::NotPreferred(NameReason::AxialAlternative)
                );
                assert!(alternative.status.detail().contains("P-93.5.7.1"));
                if unknown {
                    assert_eq!(name.omitted_units, 1);
                    assert!(
                        name.alternatives
                            .iter()
                            .any(|a| a.name.contains('Ξ') && a.name.contains(general))
                    );
                }
            }
        }
    }

    #[test]
    fn complete_senior_parents_need_multiplicative_pin_status_and_round_trip() {
        use nazuchem_ir::{NameReason, NameStatus};
        // P-16.5.4 Fig. 1.3(f) and P-15.3.2.1: the first central group is
        // asymmetric; the second has a cyclic assembly between equal arms.
        for (smiles, expected) in [
            (
                "N#Cc1ccc(-c2ccc(OCCCCCOC(=O)C(Cc3ccc(C(=O)O)cc3)c3ccc(C(=O)O)cc3)cc2)cc1",
                "4-[2-(4-carboxyphenyl)-3-({5-[(4'-cyano[1,1'-biphenyl]-4-yl)oxy]pentyl}oxy)-3-oxopropyl]benzoic acid",
            ),
            (
                "O=C(O)CCCCCCCCCOc1ccc(-c2ccc(OCCCCCCCCCC(=O)O)cc2)cc1",
                "10-({4'-[(9-carboxynonyl)oxy][1,1'-biphenyl]-4-yl}oxy)decanoic acid",
            ),
            (
                "P(CC(=O)O)(CC(=O)O)CC(=O)O",
                "[bis(carboxymethyl)phosphanyl]acetic acid",
            ),
            // P-15.3 / P-51.3.1: a more substituted core does not remove
            // identical terminal parents from multiplicative eligibility.
            ("c1ccccc1Oc2ccc(Oc3ccccc3)cc2", "1,4-diphenoxybenzene"),
            (
                "c1ccccc1Oc2cc(Oc3ccccc3)cc(Oc4ccccc4)c2",
                "1,3,5-triphenoxybenzene",
            ),
            (
                "c1ccccc1Oc2ccc(Oc3ccccc3)c(C)c2",
                "2-methyl-1,4-diphenoxybenzene",
            ),
            ("c1ccccc1Cc2ccc(Cc3ccccc3)cc2", "1,4-dibenzylbenzene"),
        ] {
            let name = name_smiles(smiles, &Default::default()).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(
                name.status,
                NameStatus::NotPreferred(NameReason::Multiplicative),
                "{smiles}"
            );
            let parsed = crate::test_support::parse_strict(&name.text).unwrap();
            assert_eq!(
                crate::test_support::canonical_smiles(smiles).unwrap(),
                crate::test_support::canonical_smiles(&parsed).unwrap(),
                "{}",
                name.text,
            );
        }
    }

    #[test]
    fn multiplicative_pin_accepts_simple_cores_and_identical_concatenated_arms() {
        for smiles in [
            "c1ccccc1Oc1ccccc1",
            "O=S(c1ccccc1)c1ccccc1",
            "O=[Se](c1ccccc1)c1ccccc1",
            "O=[Te](=O)(c1ccccc1)c1ccccc1",
            "S(CC(=O)O)CC(=O)O",
            // Principal acids outrank the central phosphane (P-51.3.1).
            "P(CC(=O)O)(CC(=O)O)CC(=O)O",
            "CC(c1ccccc1)Cc1ccccc1",
            "O=C(O)c1ccc(OCCOc2ccc(C(=O)O)cc2)cc1",
        ] {
            let mol = super::graph_io::read_smiles(smiles, &Default::default()).unwrap();
            let atoms = mol.atoms.keys().copied().collect();
            let name =
                super::component_namer::name_component_with_citation(&mol, &atoms, None, None)
                    .unwrap();
            assert!(
                super::has_multiplicative_pin(&mol, &name.derivation),
                "{smiles}"
            );
        }
    }

    #[test]
    // Provenance: fn tests::senior_structural_hydrides_keep_preferred_names | adapted
    // Source: ReShiki | src/naming/openclatura/mod.rs::tests::senior_structural_hydrides_keep_preferred_names
    // Source: openclatura | tests/exploratory/test_group14_ether_ligand_parent_selection.py::test_ring_ligand_does_not_outrank_the_mononuclear_hydride_parent
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn senior_structural_hydrides_keep_preferred_names() {
        for (smiles, expected) in [
            ("P(c1ccccc1)(c1ccccc1)c1ccccc1", "triphenylphosphane"),
            (
                "[Si](c1ccccc1)(c1ccccc1)(c1ccccc1)c1ccccc1",
                "tetraphenylsilane",
            ),
            ("B(c1ccccc1)(c1ccccc1)c1ccccc1", "triphenylborane"),
            ("N(c1ccccc1)c1ccccc1", "N-phenylaniline"),
        ] {
            let name = name_smiles(smiles, &Default::default()).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(name.status, nazuchem_ir::NameStatus::Preferred, "{smiles}");
        }
    }

    #[test]
    // Provenance: fn tests::multiplicative_pin_requires_identical_complete_rooted_senior_units | adapted
    // Source: ReShiki | src/naming/openclatura/mod.rs::tests::multiplicative_pin_requires_identical_complete_rooted_senior_units
    // Source: openclatura | tests/unit/test_analysis.py::test_anhydride_shortcut_binds_acid_halves_and_core_separately
    // Source: openclatura | tests/unit/test_polyoxide_chains.py::test_lone_oxygen_atoms_are_untouched
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn multiplicative_pin_requires_identical_complete_rooted_senior_units() {
        for smiles in [
            // Equal parents, different attachment locants (P-51.3.1(3)).
            "O=C(O)c1ccc(Cc2cccc(C(=O)O)c2)cc1",
            // Different substituted parent units and configurations.
            "Brc1cccc(Cc2cccc(Cl)c2)c1",
            "CC[C@@H](C)c1ccc(Sc2ccc(cc2)[C@@H](C)CC)cc1",
            r"O=C(O)/C=C/COC/C=C\C(=O)O",
            // Unequal concatenated arms (P-15.3.4.1.1, P-15.3.4.1.3).
            "Oc1ccc(CNc2ccc(O)cc2)cc1",
            "O=C(O)c1ccc(CC(Cl)OCCc2ccc(C(=O)O)cc2)cc1",
            // A more senior chain contains the principal group.
            "O=C(O)CC(c1ccccc1)c1ccccc1",
            // Unfunctionalized acyclic hydrocarbons and anhydrides.
            "CCOCC",
            "CC(=O)OC(C)=O",
            // Direct ring junctions are assemblies, including their suffixes.
            "Oc1ccccc1-c1ccccc1O",
            "O=C1CCC(C2CCC(=O)CC2)CC1",
            "c1ccc(-c2ccc(-c3ccccc3)cc2)cc1",
        ] {
            let mol = super::graph_io::read_smiles(smiles, &Default::default()).unwrap();
            let atoms = mol.atoms.keys().copied().collect();
            let name =
                super::component_namer::name_component_with_citation(&mol, &atoms, None, None)
                    .unwrap();
            assert!(
                !super::has_multiplicative_pin(&mol, &name.derivation),
                "{smiles}"
            );
        }
    }

    #[test]
    fn unequal_or_junior_multiplicative_units_keep_preferred_names_and_round_trip() {
        for (smiles, expected) in [
            ("Oc1ccc(Oc2ccccc2)cc1", "4-phenoxyphenol"),
            ("Cc1ccc(Oc2ccccc2)cc1", "1-methyl-4-phenoxybenzene"),
            ("c1ccccc1Oc2ccc(Oc3ccccc3)nc2", "2,5-diphenoxypyridine"),
            ("C1CCC(CC1)c1ccccc1", "cyclohexylbenzene"),
        ] {
            let name = name_smiles(smiles, &Default::default()).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(name.status, nazuchem_ir::NameStatus::Preferred, "{smiles}");
            let parsed = crate::test_support::parse_strict(&name.text).unwrap();
            assert_eq!(
                crate::test_support::canonical_smiles(smiles).unwrap(),
                crate::test_support::canonical_smiles(&parsed).unwrap(),
                "{}",
                name.text
            );
        }
    }

    #[test]
    // Provenance: fn tests::anhydride_names_need_acid_halves | adapted
    // Source: ReShiki | src/naming/openclatura/mod.rs::tests::anhydride_names_need_acid_halves
    // Source: openclatura | tests/unit/test_analysis.py::test_anhydride_shortcut_binds_acid_halves_and_core_separately
    // Source: openclatura | tests/unit/test_chalcogen_citation_compatibility.py::test_mixed_site_anhydrides_keep_both_acyl_ends
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn anhydride_names_need_acid_halves() {
        // Upstream names this "2-(((hydroxy)(thioxo)methyl)sulfanyl)acetonitrile
        // 4-methylbenzoic thioanhydride", which OPSIN reads as two compounds.
        let name = name_smiles("Cc1ccc(C(=O)SC(=S)SCC#N)cc1", &Default::default());
        assert!(
            !matches!(&name, Ok(name) if name.text.contains("anhydride")),
            "{name:?}"
        );
    }

    #[test]
    // Provenance: fn tests::enclosing_marks_nest_by_what_they_enclose | adapted
    // Source: ReShiki | src/naming/openclatura/mod.rs::tests::enclosing_marks_nest_by_what_they_enclose
    // Source: openclatura | tests/unit/test_analysis.py::test_public_api_golden_names
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn enclosing_marks_nest_by_what_they_enclose() {
        for (smiles, expected) in [
            (
                "CC(C)CC1=CC=C(C=C1)C(C)C(=O)O",
                "2-[4-(2-methylpropyl)phenyl]propanoic acid",
            ),
            (
                "OCCOCCOCCOCCO",
                "2-{2-[2-(2-hydroxyethoxy)ethoxy]ethoxy}ethan-1-ol",
            ),
            (
                "OCCOCCOCCOCCOCCO",
                "2-(2-{2-[2-(2-hydroxyethoxy)ethoxy]ethoxy}ethoxy)ethan-1-ol",
            ),
            (
                "CN(CC(=O)O)Nc1cnc(CO)cn1",
                "({[5-(hydroxymethyl)pyrazin-2-yl]amino}(methyl)amino)acetic acid",
            ),
            (
                "C[S@@](=O)CSCNN",
                "1-[({[(R)-methanesulfinyl]methyl}sulfanyl)methyl]hydrazine",
            ),
            // Stereodescriptors and compound locants count as a level; fusion
            // and von Baeyer brackets do not (P-16.5.4.1.2, P-16.5.4.1.3).
            ("C/C=C/Cc1ccc(O)cc1", "4-[(2E)-but-2-en-1-yl]phenol"),
            (
                "CN(CCO)[C@H]1CCCN1",
                "2-{methyl[(2S)-pyrrolidin-2-yl]amino}ethan-1-ol",
            ),
            (
                "OC(=O)CC1CC2CCC1C2",
                "(bicyclo[2.2.1]heptan-2-yl)acetic acid",
            ),
            (
                "CC(=O)NC12C=C1BNC2",
                "N-[3-aza-2-borabicyclo[3.1.0]hex-1(6)-en-5-yl]acetamide",
            ),
            (
                "OC(=O)Cc1ccc(cc1)-c1c[nH]c2ncccc12",
                "[4-(1H-pyrrolo[2,3-b]pyridin-3-yl)phenyl]acetic acid",
            ),
            (
                "CC(=O)N[C@@H]1CCN2C=CN[C@H]12",
                "N-[(7R,7aS)-5,6,7,7a-tetrahydro-1H-pyrrolo[1,2-a]imidazol-7-yl]acetamide",
            ),
        ] {
            let name = name_smiles(smiles, &Default::default()).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            if expected.ends_with("acetic acid") {
                let parsed = crate::test_support::parse_strict(&name.text).unwrap();
                assert_eq!(
                    crate::test_support::canonical_smiles(smiles).unwrap(),
                    crate::test_support::canonical_smiles(&parsed).unwrap(),
                    "{}",
                    name.text
                );
            }
        }
    }

    /// The P-16.5.4 examples, given as the namer writes them before nesting.
    #[test]
    fn enclosing_marks_follow_the_blue_book_examples() {
        for (enclosed, expected) in [
            (
                "4,4'-(3-((5-((4'-cyano[1,1'-biphenyl]-4-yl)oxy)pentyl)oxy)-3-oxopropane-1,2-diyl)dibenzoic acid",
                "4,4'-[3-({5-[(4'-cyano[1,1'-biphenyl]-4-yl)oxy]pentyl}oxy)-3-oxopropane-1,2-diyl]dibenzoic acid",
            ),
            (
                "1-(3,4-dihydroquinolin-1(2H)-yl)ethan-1-one",
                "1-(3,4-dihydroquinolin-1(2H)-yl)ethan-1-one",
            ),
            (
                "10,10'-([1,1'-biphenyl]-4,4'-diylbis(oxy))di(decanoic acid)",
                "10,10'-[[1,1'-biphenyl]-4,4'-diylbis(oxy)]di(decanoic acid)",
            ),
            (
                "4-(4-([1,1'-bi(cyclohexan)]-4-yl)phenyl)-4'-phenyl-1,1'-bi(cyclohexane)",
                "4-[4-([1,1'-bi(cyclohexan)]-4-yl)phenyl]-4'-phenyl-1,1'-bi(cyclohexane)",
            ),
            (
                "1-(tricyclo[3.3.1.1^{3,7}]decan-1-yl)ethan-1-one",
                "1-(tricyclo[3.3.1.1^{3,7}]decan-1-yl)ethan-1-one",
            ),
            (
                "2-(bicyclo[6.6.1]pentadeca-8(15)-en-1-yl)ethan-1-ol",
                "2-[bicyclo[6.6.1]pentadeca-8(15)-en-1-yl]ethan-1-ol",
            ),
            // P-16.5.4.1.5: `((2S)-` would put two parentheses together.
            (
                "(3S)-2-((2S)-2-(((2S)-1-ethoxy-1-oxo-4-phenylbutan-2-yl)amino)propanoyl)-1,2,3,4-tetrahydroisoquinoline-3-carboxylic acid",
                "(3S)-2-[(2S)-2-{[(2S)-1-ethoxy-1-oxo-4-phenylbutan-2-yl]amino}propanoyl]-1,2,3,4-tetrahydroisoquinoline-3-carboxylic acid",
            ),
        ] {
            assert_eq!(nest_enclosing_marks(enclosed), expected);
        }
    }

    #[test]
    // Provenance: fn tests::controls_keep_their_names_and_preference_status | adapted
    // Source: ReShiki | src/naming/openclatura/mod.rs::tests::controls_keep_their_names_and_preference_status
    // Source: openclatura | tests/fixtures/diverse_corpus.golden.json::<fixture data>
    // Source: openclatura | tests/data/parser_xml_resources/functionalTerms.json::<fixture data>
    // Source: openclatura | tests/unit/test_polyoxide_chains.py::test_the_other_heteroatom_chains_are_unchanged
    // Source: openclatura | tests/unit/test_audit_reconstruction.py::test_self_audit_confirms_modelled_names
    // Source: openclatura | tests/data/fusion_pin_cases.json::<fixture data>
    // Source: openclatura | tests/fixtures/diverse_corpus.csv::<fixture data>
    // Source: openclatura | tests/unit/test_analysis.py::test_sulfur_ylide_resonance_compare_accepts_lambda_fallback_graph
    // Source: openclatura | tests/unit/test_analysis.py::test_anhydride_shortcut_binds_acid_halves_and_core_separately
    // Source: openclatura | tests/unit/test_polyoxide_chains.py::test_oxygen_chains_do_not_displace_a_senior_heteroatom_chain
    // Source: openclatura | tests/roundtrip/test_alkenes_alkynes.py::test_roundtrip
    // Source: openclatura | tests/roundtrip/test_replacement_parents.py::test_roundtrip
    // Source: openclatura | tests/unit/test_chalcogen_structural_roles.py::test_acyl_pseudohalide_owns_complete_leaving_unit
    // Source: openclatura | tests/unit/test_chalcogen_functional_groups.py::test_nitrile_chalcogenides_use_zwitterion_priority_and_class_names
    // Source: openclatura | tests/unit/test_chalcogen_functional_groups.py::test_dynamic_chalcogen_rules_compose_with_existing_parent_and_modifier_paths
    // Source: openclatura | tests/data/parser_xml_resources/simpleGroups.json::<fixture data>
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn controls_keep_their_names_and_preference_status() {
        use nazuchem_ir::NameStatus;
        for (smiles, expected) in [
            ("CCO", "ethanol"),
            ("O=C(O)c1ccccc1", "benzoic acid"),
            (
                "CC(C)CC1=CC=C(C=C1)C(C)C(=O)O",
                "2-[4-(2-methylpropyl)phenyl]propanoic acid",
            ),
            ("c1ccc2[nH]ccc2c1", "1H-indole"),
            // Double bonds take the lower locant once multiple bonds tie
            // (P-31.1.1.1), whichever end the input starts from.
            ("C=CCC#C", "pent-1-en-4-yne"),
            ("C#CCC=C", "pent-1-en-4-yne"),
            ("C#CCCC=C", "hex-1-en-5-yne"),
            ("C#CCC(C)(C)CC=C", "4,4-dimethylhept-1-en-6-yne"),
            ("SS", "disulfane"),
            ("SSS", "trisulfane"),
            ("CC(=O)C#N", "acetyl cyanide"),
            ("COC(=O)Nc1ccccc1", "methyl phenylcarbamate"),
            ("CCC#[N+][O-]", "propanenitrile oxide"),
            // Substitutive sulfoxide (P-63.6) and -one (P-68.3.2.3.1) names.
            ("CS(C)=O", "(methanesulfinyl)methane"),
            ("CC(C)S(C)=O", "2-(methanesulfinyl)propane"),
            ("CP(C)(C)=O", "trimethyl-lambda5-phosphanone"),
            ("C[P+](C)(C)[O-]", "trimethyl-lambda5-phosphanone"),
            ("C[As](C)(C)=O", "trimethyl-lambda5-arsanone"),
            ("CP=O", "methylphosphanone"),
            ("c1ccccc1[As]=S", "phenylarsanethione"),
            ("C[As](C)(C)=[Te]", "trimethyl-lambda5-arsanetellone"),
            ("CP(C)(C)=S", "trimethyl-lambda5-phosphanethione"),
            // Monosubstituted ethane and ethene cite no locants (P-14.3.4).
            ("CCS(C)=O", "(methanesulfinyl)ethane"),
            ("CCSC", "(methylsulfanyl)ethane"),
            ("C=CSS", "ethenedithioperoxol"),
            ("C=CS(=O)(=O)O", "ethenesulfonic acid"),
            (
                "COS(=O)(=O)c1ccc(C(=O)O)cc1",
                "4-(methoxysulfonyl)benzoic acid",
            ),
            ("COS(=O)(=O)NCC(=O)O", "methoxysulfonylaminoacetic acid"),
            // Sulfonyl and sulfinyl prefixes are named from the acid (P-65.3.2.2.2).
            ("CS(=O)(=O)c1ccccc1", "(methanesulfonyl)benzene"),
            ("C[S@](=O)c1ccccc1", "[(S)-methanesulfinyl]benzene"),
            ("CS(=O)(=O)N1CCCC1", "1-(methanesulfonyl)pyrrolidine"),
            (
                "O=C(O)c1ccc(cc1)S(=O)(=O)N1CCN(C)CC1",
                "4-(4-methylpiperazine-1-sulfonyl)benzoic acid",
            ),
            (
                "O=C(O)C1CCN(C1)S(=O)(=O)c1ccc(C)cc1",
                "1-(4-methylbenzene-1-sulfonyl)pyrrolidine-3-carboxylic acid",
            ),
            (
                "CC(NS(=O)(=O)c1ccc(C)cc1)C(=O)O",
                "2-(4-methylbenzene-1-sulfonamido)propanoic acid",
            ),
            (
                "CS(=O)(=O)OCCC(=O)O",
                "3-[(methanesulfonyl)oxy]propanoic acid",
            ),
            ("OC(=O)CS(=O)(=O)c1ccccc1", "(benzenesulfonyl)acetic acid"),
            ("OS(=O)(=O)ON=C1CCCCC1", "N-(sulfooxy)cyclohexanimine"),
            // Substituted anisoles take systematic PINs (P-63.2.3).
            ("COc1ccc(Cl)cc1", "1-chloro-4-methoxybenzene"),
            ("COc1ccccc1OC", "1,2-dimethoxybenzene"),
            ("O=CCl", "formyl chloride"),
            ("CNC=O", "N-methylformamide"),
            ("COC=O", "methyl formate"),
            // Chalcogen analogues of oxo and formyl (P-64.6.1, P-66.6.3),
            // multiplied with bis to avoid ambiguity (P-16.3.6).
            ("O=C(O)CC(=S)C", "3-sulfanylidenebutanoic acid"),
            ("O=C1CSC(=S)N1", "2-sulfanylidene-1,3-thiazolidin-4-one"),
            ("CC(=[Te])CC(=[Se])C", "4-tellanylidenepentane-2-selone"),
            (
                "CC(=[Se])CCc1ccc(cc1)C(=O)O",
                "4-(3-selanylidenebutyl)benzoic acid",
            ),
            ("OC(=O)CC(=S)CC(=S)C", "3,5-bis(sulfanylidene)hexanoic acid"),
            ("O=C(O)c1ccc(C=S)cc1", "4-(methanethioyl)benzoic acid"),
            ("O=C(O)c1ccc(C=[Se])cc1", "4-(methaneselenoyl)benzoic acid"),
            ("O=C(O)c1ccc(C=[Te])cc1", "4-(methanetelluroyl)benzoic acid"),
            ("O=C(O)c1ccc(C(=S)C)cc1", "4-(ethanethioyl)benzoic acid"),
            (
                "CC(=S)C(C(C)=S)C(C)=S",
                "3-(ethanethioyl)pentane-2,4-dithione",
            ),
            (
                "O=C(O)c1ccc(C(=S)c2ccccc2)cc1",
                "4-(benzenecarbothioyl)benzoic acid",
            ),
            // Amines, sulfides and hydrodisulfides are named substitutively
            // (P-62.2.2.1, P-63.2.5, P-63.4.2.1).
            ("CNC", "N-methylmethanamine"),
            ("CN(C)C", "N,N-dimethylmethanamine"),
            ("CSC", "(methylsulfanyl)methane"),
            ("SSCCOC", "2-methoxyethane-1-dithioperoxol"),
            ("SSc1ccccn1", "pyridine-2-dithioperoxol"),
            // Alkoxy prefixes other than the retained contractions (P-63.2.2.2).
            ("CC(C)Oc1ccccc1", "[(propan-2-yl)oxy]benzene"),
            ("CCC(C)Oc1ccccc1", "[(butan-2-yl)oxy]benzene"),
            ("CC(C)COc1ccccc1", "(2-methylpropoxy)benzene"),
            ("CC(C)(C)Oc1ccccc1", "tert-butoxybenzene"),
            ("CC(C)Oc1ccccc1OC(C)C", "1,2-bis[(propan-2-yl)oxy]benzene"),
            ("CC(C)COc1ccccc1OCC(C)C", "1,2-bis(2-methylpropoxy)benzene"),
            ("CC(C)OC(=O)c1ccccc1", "propan-2-yl benzoate"),
            // Retained names for general nomenclature only (P-65.1.1.2.2,
            // P-66.5.1.2.3); HCN is formonitrile (P-66.5.1.2.1).
            ("CCC#N", "propanenitrile"),
            ("CC(C)C#N", "2-methylpropanenitrile"),
            ("O=CCCC#N", "4-oxobutanenitrile"),
            ("O=C(O)CC(=O)O", "propanedioic acid"),
            ("O=C(O)CCC(=O)O", "butanedioic acid"),
            ("O=C(O)CCCC(=O)O", "pentanedioic acid"),
            ("O=C(O)CCCCC(=O)O", "hexanedioic acid"),
            ("C#N", "formonitrile"),
            // Retained PINs (P-66.6.1.2.1, P-34.1.1.4, P-66.1.1.1.2.1, P-66.5.4.1).
            ("C=O", "formaldehyde"),
            ("CC=O", "acetaldehyde"),
            ("COc1ccccc1", "anisole"),
            ("NC(=O)C(N)=O", "oxamide"),
            ("C#[N+][O-]", "formonitrile oxide"),
            ("CC#[N+][O-]", "acetonitrile oxide"),
            ("ClCC#[N+][O-]", "chloroacetonitrile oxide"),
            // Anilino (P-62.2.1.1.1) and carbamimidoylamino (P-66.4.1.2.1.3)
            // prefixes; contractions still read anilino as phenylamino.
            ("O=C(O)CNc1ccccc1", "anilinoacetic acid"),
            (
                "OC(=O)c1ccc(N(c2ccccc2)c2ccccc2)cc1",
                "4-(N-phenylanilino)benzoic acid",
            ),
            ("NC(=N)NCC(=O)O", "(carbamimidoylamino)acetic acid"),
            (
                "CN(C(=N)N)CC(=O)O",
                "[carbamimidoyl(methyl)amino]acetic acid",
            ),
            ("COC(=O)Nc1ccccc1", "methyl phenylcarbamate"),
            (
                "OC(=O)c1ccc(C(=O)Nc2ccccc2)cc1",
                "4-(phenylcarbamoyl)benzoic acid",
            ),
            // Oxazole and thiazole fusion prefixes cite their Hantzsch-Widman
            // locants (P-25.3.2.2.3).
            ("O1C=NC2=NC=CC=C21", "[1,3]oxazolo[4,5-b]pyridine"),
            ("c1cnc2scnc2c1", "[1,3]thiazolo[5,4-b]pyridine"),
            ("c1cnc2oncc2c1", "[1,2]oxazolo[5,4-b]pyridine"),
            ("c1cnc2sncc2c1", "[1,2]thiazolo[5,4-b]pyridine"),
            // Differently substituted rings stay substitutive (P-51.3.1), and
            // the retained acyl and formyl prefixes need no ring acid name.
            (
                "Brc1cccc(Cc2cccc(Cl)c2)c1",
                "1-bromo-3-[(3-chlorophenyl)methyl]benzene",
            ),
            ("O=C(O)c1ccc(cc1)C(=O)c1ccccc1", "4-benzoylbenzoic acid"),
            ("O=C(O)c1ccc(cc1)C=O", "4-formylbenzoic acid"),
            // A chalcogen or peroxy prefix encloses a ligand that names a
            // parent hydride or is complex (P-16.5.1.4, P-16.5.1.1).
            (
                "CS(=O)(=O)SCC(=O)O",
                "[(methanesulfonyl)sulfanyl]acetic acid",
            ),
            ("CC(C)OOCC(=O)O", "[(propan-2-yl)peroxy]acetic acid"),
            ("CC(=S)SCC(=O)O", "[(ethanethioyl)sulfanyl]acetic acid"),
            ("CC(=S)OOCC(=O)O", "[(ethanethioyl)peroxy]acetic acid"),
            // Prefixes are numbered and cited from their first Roman letter;
            // bracketed initial locants only break ties (P-14.5, P-14.5.4).
            (
                "OC(=O)c1cc(C)cc(c2nc3ncccc3o2)c1",
                "3-methyl-5-([1,3]oxazolo[4,5-b]pyridin-2-yl)benzoic acid",
            ),
            (
                "O1N=CC2=NC=C(C=C21)CC(CO)C=2C=C1C(=NC2)N=CO1",
                "3-([1,2]oxazolo[4,5-b]pyridin-6-yl)-2-([1,3]oxazolo[4,5-b]pyridin-6-yl)propan-1-ol",
            ),
            // Units that differ in configuration keep the substitutive PIN
            // (P-45.6.2), as does an anhydride (P-65.7.1).
            (
                "CC[C@@H](C)c1ccc(Sc2ccc(cc2)[C@@H](C)CC)cc1",
                "1-[(2R)-butan-2-yl]-4-({4-[(2S)-butan-2-yl]phenyl}sulfanyl)benzene",
            ),
            ("CC(=O)OC(C)=O", "acetic anhydride"),
            // Biphenyl is the ring assembly name itself (P-28.2.1); different
            // rings are not an assembly (P-28.7, P-61.2.2).
            ("c1ccc(cc1)-c1ccccc1", "1,1'-biphenyl"),
            ("C1CCC(CC1)c1ccccc1", "cyclohexylbenzene"),
        ] {
            let name = name_smiles(smiles, &Default::default()).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            let status = match smiles {
                "SS" | "SSS" => NameStatus::Unverified(nazuchem_ir::NameReason::UnauditedPath),
                _ => NameStatus::Preferred,
            };
            assert_eq!(name.status, status, "{smiles}");
            if expected.ends_with("acetic acid") || expected == "chloroacetonitrile oxide" {
                let parsed = crate::test_support::parse_strict(&name.text).unwrap();
                assert_eq!(
                    crate::test_support::canonical_smiles(smiles).unwrap(),
                    crate::test_support::canonical_smiles(&parsed).unwrap(),
                    "{}",
                    name.text
                );
            }
        }
    }

    #[test]
    // Provenance: fn tests::ring_assemblies_are_preferred_numbered_parents_and_round_trip | adapted
    // Source: ReShiki | src/naming/openclatura/mod.rs::tests::ring_assemblies_are_preferred_numbered_parents_and_round_trip
    // Source: openclatura | tests/unit/test_analysis.py::test_biphenyl_is_graph_selected_not_exact_name_rewritten
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn ring_assemblies_are_preferred_numbered_parents_and_round_trip() {
        use nazuchem_ir::NameStatus;
        for (smiles, expected) in [
            (
                "Oc1ccc2ccccc2c1-c1c(O)ccc2ccccc12",
                "[1,1'-binaphthalene]-2,2'-diol",
            ),
            ("Oc1ccccc1-c1ccccc1O", "[1,1'-biphenyl]-2,2'-diol"),
            ("Oc1ccc(cc1)-c1ccccc1", "[1,1'-biphenyl]-4-ol"),
            (
                "OC(=O)c1ccc(cc1)-c1ccccc1",
                "[1,1'-biphenyl]-4-carboxylic acid",
            ),
            ("c1ccc(cc1)-c1ccccc1", "1,1'-biphenyl"),
            ("Cc1ccc(cc1)-c1ccc(C)cc1", "4,4'-dimethyl-1,1'-biphenyl"),
            ("Cc1ccc(cc1)-c1ccccc1", "4-methyl-1,1'-biphenyl"),
            ("c1ccc(nc1)-c1ccccn1", "2,2'-bipyridine"),
            ("c1ccncc1-c1cccnc1", "3,3'-bipyridine"),
            ("c1ccncc1-c1ccncc1", "3,4'-bipyridine"),
            ("c1ccc(nc1)-c1cccnc1", "2,3'-bipyridine"),
            ("c1cnccc1-c1ccncc1", "4,4'-bipyridine"),
            ("c1ccc(s1)-c1cccs1", "2,2'-bithiophene"),
            ("C1CC1C1CC1", "1,1'-bi(cyclopropane)"),
            ("CC1(CC1)C1CC1", "1-methyl-1,1'-bi(cyclopropane)"),
            ("C1CCC(CC1)C1CCCCC1", "1,1'-bi(cyclohexane)"),
            (
                "O=C1CCC(C2CCC(=O)CC2)CC1",
                "[1,1'-bi(cyclohexane)]-4,4'-dione",
            ),
            ("c1ccc2ccccc2c1-c1ccc2ccccc2c1", "1,2'-binaphthalene"),
            ("c1ccc2ccccc2c1-c1cccc2ccccc12", "1,1'-binaphthalene"),
            (
                "c1ccc(cc1)-c1ccc(cc1)-c1ccccc1",
                "1^1,2^1:2^4,3^1-terphenyl",
            ),
            (
                "c1ccc(cc1)-c1cccc(-c2ccccc2)c1",
                "1^1,2^1:2^3,3^1-terphenyl",
            ),
            (
                "Oc1ccc(cc1)-c1ccc(cc1)-c1ccccc1",
                "[1^1,2^1:2^4,3^1-terphenyl]-1^4-ol",
            ),
            (
                "c1(-c2ccccc2)cc(-c2ccccc2)cc(-c2ccccc2)c1",
                "2^5-phenyl-1^1,2^1:2^3,3^1-terphenyl",
            ),
            // P-28.6: seven units, but only three in a row.
            (
                "c1(-c2ccccc2)c(-c2ccccc2)c(-c2ccccc2)c(-c2ccccc2)c(-c2ccccc2)c1-c1ccccc1",
                "2^3,2^4,2^5,2^6-tetraphenyl-1^1,2^1:2^2,3^1-terphenyl",
            ),
            // P-31.1.4.3.4: r before s, whichever traversal the input uses.
            (
                "C[C@H]1CC[C@@H]([C@H]2CC[C@H](C)CC2)CC1",
                "(1r,1's,4r,4's)-4,4'-dimethyl-1,1'-bi(cyclohexane)",
            ),
            (
                "C[C@H]1CC[C@H]([C@H]2CC[C@@H](C)CC2)CC1",
                "(1r,1's,4r,4's)-4,4'-dimethyl-1,1'-bi(cyclohexane)",
            ),
            ("c1c[nH]c(c1)-c1ccc[nH]1", "1H,1'H-2,2'-bipyrrole"),
            ("c1ccn(c1)-n1cccc1", "1,1'-bipyrrole"),
            (
                "OC(=O)Cc1ccc(cc1)-c1ccccc1",
                "([1,1'-biphenyl]-4-yl)acetic acid",
            ),
            (
                "C[C@H]1C[C@@H]1[C@H]1C[C@@H]1C",
                "(1S,1'S,2S,2'S)-2,2'-dimethyl-1,1'-bi(cyclopropane)",
            ),
            ("C1CCC(CC1)c1ccccc1", "cyclohexylbenzene"),
            ("c1ccc(cc1)-c1ccncc1", "4-phenylpyridine"),
        ] {
            let name = name_smiles(smiles, &Default::default()).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(name.status, NameStatus::Preferred, "{smiles}");
            let parsed = crate::test_support::parse_strict(&name.text).unwrap();
            assert_eq!(
                crate::test_support::canonical_smiles(smiles).unwrap(),
                crate::test_support::canonical_smiles(&parsed).unwrap(),
                "{}",
                name.text
            );
        }
    }

    #[test]
    fn linear_phane_assemblies_keep_nonpreferred_status_and_round_trip() {
        use nazuchem_ir::{NameReason, NameStatus};
        for (smiles, expected, status) in [
            (
                "c1ccccc1-c1ccc(-c2ccc(Oc3cccc(-c4cccc(-c5ccccn5)n4)n3)cc2)cc1",
                "1^6-[([1^1,2^1:2^4,3^1-terphenyl]-1^4-yl)oxy]-1^2,2^2:2^6,3^2-terpyridine",
                NameStatus::NotPreferred(NameReason::LinearPhane),
            ),
            // P-51.3.2: even identical halves require the linear phane,
            // rather than a multiplicative name, for seven skeletal nodes.
            (
                "c1ccccc1-c1ccc(-c2ccc(Oc3ccc(-c4ccc(-c5ccccc5)cc4)cc3)cc2)cc1",
                "1^4-[([1^1,2^1:2^4,3^1-terphenyl]-1^4-yl)oxy]-1^1,2^1:2^4,3^1-terphenyl",
                NameStatus::NotPreferred(NameReason::LinearPhane),
            ),
            // P-65.6.3.3.7.2.1: the carbamate separates two two-ring
            // components; its named biphenyl junction needs no demotion.
            (
                "CN(CCCCCCCCCN1CCC(CC1)OC(=O)NC2=CC=CC=C2C3=CC=CC=C3)C(=O)C4=CC(=C(C=C4)O)Cl",
                "1-[9-(3-chloro-4-hydroxy-N-methylbenzamido)nonyl]piperidin-4-yl ([1,1'-biphenyl]-2-yl)carbamate",
                NameStatus::Preferred,
            ),
            // P-52.2.5.3 applies without identical-unit ring junctions.
            (
                "Oc1ccc(Oc2ccc(Oc3ccc(Oc4ccccc4)cc3)cc2)cc1",
                "4-[4-(4-phenoxyphenoxy)phenoxy]phenol",
                NameStatus::NotPreferred(NameReason::LinearPhane),
            ),
            (
                "Oc1ccc(Cc2ccc(Cc3ccc(Cc4ccccc4)cc3)cc2)cc1",
                "4-({4-[(4-benzylphenyl)methyl]phenyl}methyl)phenol",
                NameStatus::NotPreferred(NameReason::LinearPhane),
            ),
            (
                "Oc1ccc(Oc2ccc(Sc3ccc(Oc4ccccc4)cc3)cc2)cc1",
                "4-{4-[(4-phenoxyphenyl)sulfanyl]phenoxy}phenol",
                NameStatus::NotPreferred(NameReason::LinearPhane),
            ),
            (
                "Oc1ccc(-c2ccnc(-c3ccc(-c4ccnc(-c5ccc(-c6ccnc(-c7ccccc7)c6)cc5)c4)cc3)c2)cc1",
                "4-[2-(4-{2-[4-(2-phenylpyridin-4-yl)phenyl]pyridin-4-yl}phenyl)pyridin-4-yl]phenol",
                NameStatus::NotPreferred(NameReason::LinearPhane),
            ),
            // A linear phane outranks the multiplicative name (P-51.3.2).
            (
                "c1ccccc1Oc2ccc(Oc3ccc(Oc4ccccc4)cc3)cc2",
                "1-phenoxy-4-(4-phenoxyphenoxy)benzene",
                NameStatus::NotPreferred(NameReason::LinearPhane),
            ),
            (
                "c1ccccc1-c1ccc(-c2ccc(-c3ccc(-c4ccc(-c5ccccc5)cc4)cc3)cc2)cc1",
                "1^1,2^1:2^4,3^1:3^4,4^1:4^4,5^1:5^4,6^1-sexiphenyl",
                NameStatus::Preferred,
            ),
            (
                "c1(-c2ccccc2)c(-c2ccccc2)c(-c2ccccc2)c(-c2ccccc2)c(-c2ccccc2)c1-c1ccccc1",
                "2^3,2^4,2^5,2^6-tetraphenyl-1^1,2^1:2^2,3^1-terphenyl",
                NameStatus::Preferred,
            ),
        ] {
            let name = name_smiles(smiles, &Default::default()).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(name.status, status, "{smiles}");
            let parsed = crate::test_support::parse_strict(&name.text).unwrap();
            assert_eq!(
                crate::test_support::canonical_smiles(smiles).unwrap(),
                crate::test_support::canonical_smiles(&parsed).unwrap(),
                "{}",
                name.text
            );
        }
    }

    #[test]
    fn linear_phane_status_respects_functional_class_components_and_round_trips() {
        use nazuchem_ir::{NameReason, NameStatus};
        for (smiles, expected, status) in [
            // P-65.6.3.2.1 / P-65.6.3.3.1: each ester component has two rings.
            (
                "O=C(OCc1ccc(Oc2ccccc2)cc1)c1ccc(Oc2ccccc2)cc1",
                "(4-phenoxyphenyl)methyl 4-phenoxybenzoate",
                NameStatus::Preferred,
            ),
            (
                "O=C(OCc1ccc(-c2ccccc2)cc1)c1ccc(-c2ccccc2)cc1",
                "([1,1'-biphenyl]-4-yl)methyl [1,1'-biphenyl]-4-carboxylate",
                NameStatus::Preferred,
            ),
            // P-65.6.3.3.7.2.1: carbamic acid esters use the same split.
            (
                "O=C(Oc1ccc(Oc2ccccc2)cc1)Nc1ccc(Oc2ccccc2)cc1",
                "4-phenoxyphenyl (4-phenoxyphenyl)carbamate",
                NameStatus::Preferred,
            ),
            // P-65.7.1: anhydrides cite separate acid components.
            (
                "O=C(OC(=O)c1ccc(Oc2ccccc2)cc1)c1ccc(Oc2ccccc2)cc1",
                "4-phenoxybenzoic anhydride",
                NameStatus::Preferred,
            ),
            // P-65.6.3.3.4.4: a phane within either component still counts.
            (
                "COC(=O)c1ccc(Oc2ccc(Oc3ccc(Oc4ccccc4)cc3)cc2)cc1",
                "methyl 4-[4-(4-phenoxyphenoxy)phenoxy]benzoate",
                NameStatus::NotPreferred(NameReason::LinearPhane),
            ),
            (
                "CC(=O)Oc1ccc(Oc2ccc(Oc3ccc(Oc4ccccc4)cc3)cc2)cc1",
                "4-[4-(4-phenoxyphenoxy)phenoxy]phenyl acetate",
                NameStatus::NotPreferred(NameReason::LinearPhane),
            ),
            // P-65.6.3.3.4: an ester prefix does not split the acid parent.
            (
                "OC(=O)c1ccc(Oc2ccc(C(=O)Oc3ccc(Oc4ccccc4)cc3)cc2)cc1",
                "4-{4-[(4-phenoxyphenoxy)carbonyl]phenoxy}benzoic acid",
                NameStatus::NotPreferred(NameReason::LinearPhane),
            ),
            // A same-key ester outside the selected parent remains a prefix.
            (
                "COC(=O)c1cc(C(=O)OC)cc(Oc2ccc(C(=O)Oc3ccc(Oc4ccccc4)cc3)cc2)c1",
                "dimethyl 5-{4-[(4-phenoxyphenoxy)carbonyl]phenoxy}benzene-1,3-dicarboxylate",
                NameStatus::NotPreferred(NameReason::LinearPhane),
            ),
        ] {
            let name = name_smiles(smiles, &Default::default()).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(name.status, status, "{smiles}");
            let parsed = crate::test_support::parse_strict(&name.text).unwrap();
            assert_eq!(
                crate::test_support::canonical_smiles(smiles).unwrap(),
                crate::test_support::canonical_smiles(&parsed).unwrap(),
                "{}",
                name.text
            );
        }
    }

    #[test]
    fn linear_phane_status_respects_oxoacid_ester_components_and_round_trips() {
        use nazuchem_ir::{NameReason, NameStatus};
        for (smiles, expected, status) in [
            // P-65.2.3 / P-65.6.3.2.1: ester organyl groups are separate components.
            (
                "O=C(Oc1ccc(Oc2ccccc2)cc1)Oc1ccc(Oc2ccccc2)cc1",
                "bis(4-phenoxyphenyl) carbonate",
                NameStatus::Preferred,
            ),
            (
                "O=P(Oc1ccc(Oc2ccccc2)cc1)(Oc1ccccc1)Oc1ccc(Oc2ccccc2)cc1",
                "bis(4-phenoxyphenyl) phenyl phosphate",
                NameStatus::Preferred,
            ),
            (
                "O=S(=O)(Oc1ccc(Oc2ccccc2)cc1)Oc1ccc(Oc2ccccc2)cc1",
                "bis(4-phenoxyphenyl) sulfate",
                NameStatus::Preferred,
            ),
            (
                "O=S(=O)(Oc1ccc(Oc2ccccc2)cc1)c1ccc(Oc2ccccc2)cc1",
                "4-phenoxyphenyl 4-phenoxybenzene-1-sulfonate",
                NameStatus::Preferred,
            ),
            (
                "CP(=O)(Oc1ccc(Oc2ccccc2)cc1)Oc1ccc(Oc2ccccc2)cc1",
                "bis(4-phenoxyphenyl) methylphosphonate",
                NameStatus::Unverified(NameReason::UnauditedPath),
            ),
            (
                "COP(=O)(Oc1ccc(Oc2ccccc2)cc1)c1ccc(Oc2ccccc2)cc1",
                "methyl 4-phenoxyphenyl 4-phenoxyphenylphosphonate",
                NameStatus::Unverified(NameReason::UnauditedPath),
            ),
            (
                "OP(=O)(Oc1ccc(Oc2ccccc2)cc1)c1ccc(Oc2ccccc2)cc1",
                "4-phenoxyphenyl hydrogen 4-phenoxyphenylphosphonate",
                NameStatus::Unverified(NameReason::UnauditedPath),
            ),
            (
                "[PH](=O)(Oc1ccc(Oc2ccccc2)cc1)c1ccc(Oc2ccccc2)cc1",
                "4-phenoxyphenyl (4-phenoxyphenyl)phosphinate",
                NameStatus::Unverified(NameReason::UnauditedPath),
            ),
            // P-52.2.5.3: a four-ring path within one component still counts.
            (
                "COC(=O)Oc1ccc(Oc2ccc(Oc3ccc(Oc4ccccc4)cc3)cc2)cc1",
                "methyl 4-[4-(4-phenoxyphenoxy)phenoxy]phenyl carbonate",
                NameStatus::NotPreferred(NameReason::LinearPhane),
            ),
            (
                "COP(=O)(c1ccc(Oc2ccccc2)cc1)c1ccc(Oc2ccccc2)cc1",
                "methyl bis(4-phenoxyphenyl)phosphinate",
                NameStatus::NotPreferred(NameReason::LinearPhane),
            ),
            (
                "OP(=O)(c1ccc(Oc2ccccc2)cc1)c1ccc(Oc2ccccc2)cc1",
                "bis(4-phenoxyphenyl)phosphinic acid",
                NameStatus::NotPreferred(NameReason::LinearPhane),
            ),
            // A phosphonate prefix under a senior nitrile keeps the full skeleton.
            (
                "N#Cc1ccc(Oc2ccc(P(=O)(OC)Oc3ccc(Oc4ccccc4)cc3)cc2)cc1",
                "4-{4-[methoxy(4-phenoxyphenoxy)phosphoryl]phenoxy}benzonitrile",
                NameStatus::NotPreferred(NameReason::LinearPhane),
            ),
        ] {
            let name = name_smiles(smiles, &Default::default()).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(name.status, status, "{smiles}");
            let parsed = crate::test_support::parse_strict(&name.text).unwrap();
            assert_eq!(
                crate::test_support::canonical_smiles(smiles).unwrap(),
                crate::test_support::canonical_smiles(&parsed).unwrap(),
                "{}",
                name.text
            );
        }
    }

    #[test]
    fn substituent_amine_sulfide_is_cited_as_a_prefix() {
        use nazuchem_ir::{NameReason, NameStatus};
        let name = name_smiles(
            "O=P(Oc1ccc(Oc2ccccc2)cc1)(Oc1ccccc1)Oc1ccc(Oc2ccc([N+](C)(C)[S-])cc2)cc1",
            &Default::default(),
        )
        .unwrap();
        // P-62.5 names only a parent amine; this N-sulfide is a prefix (#947).
        assert_eq!(
            name.text,
            "4-{4-[dimethyl(sulfido)ammonio]phenoxy}phenyl 4-phenoxyphenyl phenyl phosphate"
        );
        assert_eq!(
            name.status,
            NameStatus::NotPreferred(NameReason::AmineOxideZwitterion)
        );
    }

    #[test]
    fn tied_ring_assemblies_choose_the_senior_component_and_round_trip() {
        let smiles = "C1(C3CC4CCC34)C(C5CC5C6CC6C7CC7C8CC8)C2CCC12";
        let name = name_smiles(smiles, &Default::default()).unwrap();
        assert_eq!(
            name.text,
            "3-([1^1,2^1:2^2,3^1:3^2,4^1-quatercyclopropan]-1^2-yl)-2,2'-bi(bicyclo[2.2.0]hexane)"
        );
        assert_eq!(name.status, nazuchem_ir::NameStatus::Preferred);
        let parsed = crate::test_support::parse_strict(&name.text).unwrap();
        assert_eq!(
            crate::test_support::canonical_smiles(smiles).unwrap(),
            crate::test_support::canonical_smiles(&parsed).unwrap()
        );
    }

    #[test]
    fn unsupported_ring_assembly_keeps_substitutive_name_and_status() {
        let name = name_smiles("C1CCC(=C2CCCC2)C1", &Default::default()).unwrap();
        assert_eq!(name.text, "cyclopentylidenecyclopentane");
        assert_eq!(
            name.status,
            nazuchem_ir::NameStatus::NotPreferred(nazuchem_ir::NameReason::RingAssembly)
        );
    }

    #[test]
    fn composite_quaterpyridine_and_assembly_substituent_locants_round_trip() {
        for expected in [
            "1^2,2^2:2^6,3^2:3^6,4^2-quaterpyridine",
            "1^1,2^1:2^2,3^1-tercyclopropane",
            "1^1,2^1:2^4,3^1:3^4,4^1:4^4,5^1-quinquephenyl",
            "1^1,2^1:2^4,3^1:3^4,4^1:4^4,5^1:5^4,6^1-sexiphenyl",
            // Symmetric units collapse to the numberings a name can cite.
            "1^1,2^1:2^4,3^1:3^4,4^1:4^4,5^1-quinquebicyclo[2.2.2]octane",
            "6H,6'H-2,2'-bipyran",
            "1H,1'H-1,1'-biindene",
            "1H,1'H-3,3'-biindole",
            "1H,1'H-[2,2'-bipyrrol]-5-ol",
            "(1H,1'H-[2,2'-bipyrrol]-5-yl)acetic acid",
            "2,3'-bifuran",
            "2,2'-bi(bicyclo[2.2.2]octane)",
            "1,1'-bi(spiro[2.2]pentane)",
            "1,1'-bi(dispiro[2.0.2.1]heptane)",
            "([2,2'-bipyridin]-5-yl)acetic acid",
            "([1^1,2^1:2^4,3^1-terphenyl]-1^4-yl)acetic acid",
            "(4'-cyano[1,1'-biphenyl]-4-yl)acetic acid",
        ] {
            let parsed = crate::test_support::parse_strict(expected).unwrap();
            let name = name_smiles(&parsed, &Default::default()).unwrap();
            assert_eq!(name.text, expected);
            assert_eq!(name.status, nazuchem_ir::NameStatus::Preferred);
        }
    }

    #[test]
    fn symmetric_polyspiro_assembly_junctions_use_every_equivalent_numbering() {
        // P-24.2.2 fixes each dispiro parent numbering; P-28.2.1 then gives
        // both equivalent terminal rings the lowest possible junction locant.
        for smiles in [
            "C1CC12CC21CC1C1CC12CC21CC1",
            "C1C2(C1)CC12CC1C1C2(C3(CC3)C2)C1",
            "C12(CC1)C1(CC1C1C3(CC43CC4)C1)C2",
            "C1(C2C3(CC43CC4)C2)C2(C1)CC12CC1",
            "C1CC21CC21CC1C1CC21C1(CC1)C2",
        ] {
            let name = name_smiles(smiles, &Default::default()).unwrap();
            assert_eq!(name.text, "1,1'-bi(dispiro[2.0.2.1]heptane)", "{smiles}");
            assert_eq!(name.status, nazuchem_ir::NameStatus::Preferred, "{smiles}");
            let parsed = crate::test_support::parse_strict(&name.text).unwrap();
            assert_eq!(
                crate::test_support::canonical_smiles(smiles).unwrap(),
                crate::test_support::canonical_smiles(&parsed).unwrap(),
                "{}",
                name.text
            );
        }
    }

    #[test]
    // Provenance: fn tests::locanted_assembly_hydrides_are_enclosed_for_two_through_six_units | adapted
    // Source: ReShiki | src/naming/openclatura/mod.rs::tests::locanted_assembly_hydrides_are_enclosed_for_two_through_six_units
    // Source: openclatura | tests/data/parser_xml_resources/multipliers.json::<fixture data>
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn locanted_assembly_hydrides_are_enclosed_for_two_through_six_units() {
        // P-28.2.1 and P-16.5 separate the component's locants from those
        // identifying the junctions, independently of the unit multiplier.
        let smiles = "C1(OCCOC1)C1COCCO1";
        let name = name_smiles(smiles, &Default::default()).unwrap();
        assert_eq!(name.text, "2,2'-bi(1,4-dioxane)");
        assert_eq!(name.status, nazuchem_ir::NameStatus::Preferred);
        let parsed = crate::test_support::parse_strict(&name.text).unwrap();
        assert_eq!(
            crate::test_support::canonical_smiles(smiles).unwrap(),
            crate::test_support::canonical_smiles(&parsed).unwrap()
        );
        for expected in [
            "2,2'-bi(1,4-dioxane)",
            "1^2,2^2:2^3,3^2-ter(1,4-dioxane)",
            "1^2,2^2:2^3,3^2:3^3,4^2-quater(1,4-dioxane)",
            "1^2,2^2:2^3,3^2:3^3,4^2:4^3,5^2-quinque(1,4-dioxane)",
            "1^2,2^2:2^3,3^2:3^3,4^2:4^3,5^2:5^3,6^2-sexi(1,4-dioxane)",
        ] {
            let parsed = crate::test_support::parse_strict(expected).unwrap();
            let name = name_smiles(&parsed, &Default::default()).unwrap();
            assert_eq!(name.text, expected);
            assert_eq!(name.status, nazuchem_ir::NameStatus::Preferred);
            let reparsed = crate::test_support::parse_strict(&name.text).unwrap();
            assert_eq!(
                crate::test_support::canonical_smiles(&parsed).unwrap(),
                crate::test_support::canonical_smiles(&reparsed).unwrap()
            );
        }
        for multiplier in ["bi", "ter", "quater", "quinque", "sexi"] {
            let enclosed = format!("2-(1,1'-{multiplier}(1,4-dioxan)-2-yl)acetic acid");
            assert_eq!(nest_enclosing_marks(&enclosed), enclosed);
        }
    }

    #[test]
    fn fused_heterocycle_assemblies_keep_parent_locants_and_indicated_hydrogen() {
        for (smiles, expected) in [
            ("c1ccc2c(c1)ccc(n2)-c1ccc2ccccc2n1", "2,2'-biquinoline"),
            ("c1ccc2c(c1)oc(c2)-c1cc2ccccc2o1", "2,2'-bi(1-benzofuran)"),
            (
                "c1ccc2c(-c3c[nH]c4ccccc34)c[nH]c2c1",
                "1H,1'H-3,3'-biindole",
            ),
        ] {
            let name = name_smiles(smiles, &Default::default()).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(name.status, nazuchem_ir::NameStatus::Preferred, "{smiles}");
            let parsed = crate::test_support::parse_strict(&name.text).unwrap();
            assert_eq!(
                crate::test_support::canonical_smiles(smiles).unwrap(),
                crate::test_support::canonical_smiles(&parsed).unwrap(),
                "{}",
                name.text
            );
        }
        // P-16.2.4.1 also permits separating the intrinsic fusion-parent
        // locant with a hyphen. Both spellings reconstruct the same hydride;
        // generation consistently encloses locanted component names.
        let alternative = "2,2'-bi-1-benzofuran";
        let parsed = crate::test_support::parse_strict(alternative).unwrap();
        assert_eq!(
            crate::test_support::canonical_smiles("c1ccc2c(c1)oc(c2)-c1cc2ccccc2o1").unwrap(),
            crate::test_support::canonical_smiles(&parsed).unwrap()
        );
    }

    #[test]
    // Provenance: fn tests::preferred_von_baeyer_controls_do_not_report_fusion_fallback | adapted
    // Source: ReShiki | src/naming/openclatura/mod.rs::tests::preferred_von_baeyer_controls_do_not_report_fusion_fallback
    // Source: openclatura | tests/unit/test_fused_bonds.py::test_fused_bonds_match_the_cycle_count
    // Source: openclatura | tests/unit/test_von_baeyer_candidate_audit.py::test_the_preferred_candidate_is_still_offered_first
    // Source: openclatura | tests/roundtrip/test_polycycles.py::test_roundtrip
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn preferred_von_baeyer_controls_do_not_report_fusion_fallback() {
        use nazuchem_ir::NameStatus;
        for (smiles, expected) in [
            ("C1CCC2CCCCC2C1", "decahydronaphthalene"),
            ("C1CC2=CC=CC=C12", "bicyclo[4.2.0]octa-1,3,5-triene"),
            ("C1CC2CCC1CC2", "bicyclo[2.2.2]octane"),
            ("C1C2CC12", "bicyclo[1.1.0]butane"),
            ("C1CC2CCC1C2", "bicyclo[2.2.1]heptane"),
            ("C1CCC2(CC1)CCCC2", "spiro[4.5]decane"),
            // A retained fused parent bypasses the fallback assessment.
            ("c1ccc2ccccc2c1", "naphthalene"),
        ] {
            let name = name_smiles(smiles, &Default::default()).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(
                name.status,
                NameStatus::Preferred,
                "{smiles}: {}",
                name.text
            );
        }
    }

    #[test]
    // Provenance: fn tests::pheno_fusion_components_choose_preferred_parents_and_round_trip | original
    // Source: ReShiki | src/naming/openclatura/mod.rs::tests::pheno_fusion_components_choose_preferred_parents_and_round_trip
    fn pheno_fusion_components_choose_preferred_parents_and_round_trip() {
        use nazuchem_ir::NameStatus;
        for (smiles, expected) in [
            (
                "c1ccc2c(c1)Nc1cc3cnc4ccccc4c3cc1O2",
                "8H-quinolino[3,4-b]phenoxazine",
            ),
            (
                "C1=CC=C2C(=C1)NC1=CC3=CN=C4C=CC=CC4=C3C=C1O2",
                "8H-quinolino[3,4-b]phenoxazine",
            ),
            (
                "c1ccc2c(c1)Nc1cc3cnc4ccccc4c3cc1S2",
                "8H-quinolino[3,4-b]phenothiazine",
            ),
            (
                "c1ccc2cc3c(cc2c1)Nc1cc2ccccc2cc1O3",
                "13H-dibenzo[b,i]phenoxazine",
            ),
            ("c1ccc2c(c1)Sc1ccc3ccccc3c1S2", "benzo[a]thianthrene"),
            ("c1ccc2c(c1)Oc1ccc3ccccc3c1S2", "benzo[a]phenoxathiine"),
            ("c1ccc2c(c1)Nc1ccc3ccccc3c1S2", "7H-benzo[c]phenothiazine"),
            (
                "c1ccc2c(c1)Nc1c(ccc3ccccc13)S2",
                "12H-benzo[a]phenothiazine",
            ),
            ("c1ccc2c(c1)Nc1ccc3ccccc3c1O2", "7H-benzo[c]phenoxazine"),
            ("c1ccc2c(c1)Oc1ccc3ccccc3c1N2", "12H-benzo[a]phenoxazine"),
            ("c1ccc2c(c1)Nc1cc3ccccc3cc1S2", "12H-benzo[b]phenothiazine"),
            ("c1ccc2cc3c(cc2c1)Sc1ccccc1S3", "benzo[b]thianthrene"),
            ("c1cc2Nc3ccccc3Oc2c2ncccc12", "7H-pyrido[3,2-c]phenoxazine"),
            (
                "c1ccc2c(c1)Oc1ccc3ncccc3c1N2",
                "12H-pyrido[3,2-a]phenoxazine",
            ),
            (
                "c1ccc2nc3cc4Oc5ccccc5Nc4cc3nc2c1",
                "14H-quinoxalino[2,3-b]phenoxazine",
            ),
            (
                "c1ccc2c(c1)Oc1cc3Sc4ccccc4Sc3cc1S2",
                "[1,4]benzodithiino[2,3-b]phenoxathiine",
            ),
            (
                "c1ccc2c(c1)Nc1cc3Sc4ccccc4Nc3cc1O2",
                "7,14-dihydro[1,4]benzothiazino[2,3-b]phenoxazine",
            ),
        ] {
            let name = name_smiles(smiles, &Default::default()).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(name.status, NameStatus::Preferred, "{smiles}");
            let parsed = crate::test_support::parse_strict(&name.text).unwrap();
            crate::test_support::verify_identity(smiles, &parsed).unwrap();
        }
    }

    #[test]
    fn pheno_fusion_derivatives_preserve_main_names_and_statuses() {
        use nazuchem_ir::{NameReason, NameStatus};
        let unregistered = NameStatus::NotPreferred(NameReason::UnregisteredRetainedComponent);
        let fallback = NameStatus::NotPreferred(NameReason::VonBaeyerFusionFallback);
        // Keep derivative behavior unchanged until its hydrogen composition and
        // parent selection can be audited together (issue #1022).
        for (smiles, expected, status) in [
            (
                "c1ccc2c(c1)N(C)c1cc3cnc4ccccc4c3cc1O2",
                "8-methyl[1,4]benzoxazino[3,2-j]phenanthridine",
                unregistered,
            ),
            (
                "c1ccc2c(c1)N(C)c1cc3Sc4ccccc4Nc3cc1O2",
                "11-methyl-4-oxa-15-thia-11,22-diazapentacyclo[12.8.0.0^{16,21}.0^{3,12}.0^{5,10}]docosa-1,3(12),5,7,9,13,16,18,20-nonaene",
                fallback,
            ),
            (
                "c1ccc2c(c1)Nc1cc3Sc4ccccc4N(C)c3cc1O2",
                "22-methyl-4-oxa-15-thia-11,22-diazapentacyclo[12.8.0.0^{16,21}.0^{3,12}.0^{5,10}]docosa-1(14),2,5,7,9,12,16,18,20-nonaene",
                fallback,
            ),
            (
                "c1ccc2c(c1)N(C)c1cc3Sc4ccccc4N(C)c3cc1O2",
                "11,22-dimethyl-4-oxa-15-thia-11,22-diazapentacyclo[12.8.0.0^{16,21}.0^{3,12}.0^{5,10}]docosa-1(14),2,5,7,9,12,16,18,20-nonaene",
                fallback,
            ),
            (
                "CN1c2ccccc2Nc2ccncc21",
                "10-methyl-5,10-dihydropyrido[3,4-b]quinoxaline",
                NameStatus::Preferred,
            ),
            (
                "c1ccc2c(c1)Oc1ccc3ccccc3c1[N-]2",
                "naphtho[2,1-b][1,4]benzoxazin-12-ide",
                unregistered,
            ),
            (
                "c1ccc2c(c1)Oc1ccc3ccccc3c1[N+]2(C)C",
                "12,12-dimethylnaphtho[2,1-b][1,4]benzoxazin-12-ium",
                unregistered,
            ),
            (
                "c1ccc2c(c1)Sc1ccc3ccccc3c1[N-]2",
                "naphtho[2,1-b][1,4]benzothiazin-12-ide",
                unregistered,
            ),
            (
                "c1ccc2c(c1)Sc1ccc3ccccc3c1[N+]2(C)C",
                "12,12-dimethylnaphtho[2,1-b][1,4]benzothiazin-12-ium",
                unregistered,
            ),
            (
                "c1ccc2c(c1)[N-]c1cc3cnc4ccccc4c3cc1O2",
                "[1,4]benzoxazino[3,2-j]phenanthridin-8-ide",
                unregistered,
            ),
            (
                "c1ccc2c(c1)[N+](C)(C)c1cc3cnc4ccccc4c3cc1O2",
                "8,8-dimethyl[1,4]benzoxazino[3,2-j]phenanthridin-8-ium",
                unregistered,
            ),
            (
                "O=C1CC=C2Oc3cc4Nc5ccccc5Sc4cc3NC2=C1",
                "4-oxa-15-thia-11,22-diazapentacyclo[12.8.0.0^{16,21}.0^{3,12}.0^{5,10}]docosa-1,3(12),5,9,13,16,18,20-octaen-8-one",
                fallback,
            ),
            (
                "O=C1CC=C2Oc3cc4N(C)c5ccccc5Sc4cc3NC2=C1",
                "22-methyl-4-oxa-15-thia-11,22-diazapentacyclo[12.8.0.0^{16,21}.0^{3,12}.0^{5,10}]docosa-1,3(12),5,9,13,16,18,20-octaen-8-one",
                fallback,
            ),
            (
                "O=C1CC=C2Oc3cc4Nc5ccccc5Sc4cc3N(C)C2=C1",
                "11-methyl-4-oxa-15-thia-11,22-diazapentacyclo[12.8.0.0^{16,21}.0^{3,12}.0^{5,10}]docosa-1,3(12),5,9,13,16,18,20-octaen-8-one",
                fallback,
            ),
            (
                "O=C1CC=C2Oc3cc4Nc5ccccc5Sc4cc3[N+](C)(C)C2=C1",
                "11,11-dimethyl-4-oxa-15-thia-11,22-diazapentacyclo[12.8.0.0^{16,21}.0^{3,12}.0^{5,10}]docosa-1,3(12),5,9,13,16,18,20-octaen-11-ium-8-one",
                fallback,
            ),
            (
                "O=C1CC=C2Oc3cc4[N+](C)(C)c5ccccc5Sc4cc3NC2=C1",
                "22,22-dimethyl-4-oxa-15-thia-11,22-diazapentacyclo[12.8.0.0^{16,21}.0^{3,12}.0^{5,10}]docosa-1(14),2,5,9,12,16,18,20-octaen-22-ium-8-one",
                fallback,
            ),
            (
                "CCN(CC)c1ccc2nc3c(cc(=O)c4ccccc34)oc2c1",
                "9-(diethylamino)naphtho[2,1-b][1,4]benzoxazin-5-one",
                unregistered,
            ),
            (
                "O=S1c2ccccc2S(=O)c2ccccc21",
                "5lambda^4,10lambda^4-thianthrene-5,10-dione",
                NameStatus::Preferred,
            ),
        ] {
            let name = name_smiles(smiles, &Default::default()).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(name.status, status, "{smiles}");
        }
    }

    #[test]
    // Provenance: fn tests::fusion_retained_component_controls_remain_preferred | adapted
    // Source: ReShiki | src/naming/openclatura/mod.rs::tests::fusion_retained_component_controls_remain_preferred
    // Source: openclatura | tests/roundtrip/test_retained_fused_production.py::test_expected_retained_rings_are_registered
    // Source: openclatura | tests/roundtrip/test_fused_parents.py::test_roundtrip
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn fusion_retained_component_controls_remain_preferred() {
        use nazuchem_ir::NameStatus;
        for (smiles, expected) in [
            (
                "c1ccc2c(c1)Cc1cc3ncccc3cc1O2",
                "11H-[1]benzopyrano[2,3-g]quinoline",
            ),
            ("S1c2ccccc2Cc2ccccc21", "9H-thioxanthene"),
            ("c1ccc2c(c1)Cc1ccccc1O2", "9H-xanthene"),
            ("c1ccc2c(c1)Nc1ccccc1O2", "10H-phenoxazine"),
            ("c1ccc2c(c1)Nc1ccccc1S2", "10H-phenothiazine"),
            (
                "C1=CC(=CC=2SC3=CC=CC=C3CC12)C(=O)O",
                "9H-thioxanthene-3-carboxylic acid",
            ),
            (
                "N1=CC=CC2=CC=C3C=C4C(=NC3=C12)C=CC=C4",
                "benzo[b][1,10]phenanthroline",
            ),
            ("c1c3ncncc3cc4nc2ccccc2cc14", "pyrimido[4,5-b]acridine"),
            ("c1ccc2cc3nccnc3cc2c1", "benzo[g]quinoxaline"),
            ("O1C=CC2=NC=3C(C=C21)=COC3", "difuro[3,2-b:3',4'-e]pyridine"),
            ("c1ccc2c(c1)oc1ccccc12", "dibenzofuran"),
        ] {
            let name = name_smiles(smiles, &Default::default()).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(name.status, NameStatus::Preferred, "{smiles}");
        }
    }

    #[test]
    // Provenance: fn tests::fusion_intrinsic_carbon_hydrogen_relocation_is_preferred_and_round_trips | adapted
    // Source: ReShiki | src/naming/openclatura/mod.rs::tests::fusion_intrinsic_carbon_hydrogen_relocation_is_preferred_and_round_trips
    // Source: openclatura | tests/unit/test_retained_bridge_h_relocation.py::test_retained_wrapper_consumes_matched_h_metadata
    // Source: openclatura | tests/roundtrip/test_retained_fused_production.py::test_expected_retained_rings_are_registered
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn fusion_intrinsic_carbon_hydrogen_relocation_is_preferred_and_round_trips() {
        use nazuchem_ir::NameStatus;
        // Issue #969: P-25.3.2.4 selects the senior retained component;
        // P-25.3.1.3 and P-25.3.3 assign its descriptor and completed locants.
        // In the two pyrido cases, parent side b is component 2 -> 3,
        // giving [2,3-b] and [3,2-b], respectively.
        for (smiles, expected) in [
            ("C1C=Cc2cc3sccc3n12", "5H-thieno[2,3-b]pyrrolizine"),
            ("C1C=Cc2cc3ccsc3n12", "7H-thieno[3,2-b]pyrrolizine"),
            ("C1=CCc2c3ccsc3cn12", "4H-thieno[3,2-a]pyrrolizine"),
            (
                "CC1C=Cc2cc3sccc3n12",
                "5-methyl-5H-thieno[2,3-b]pyrrolizine",
            ),
            ("C1C=Cc2cc3occc3n12", "5H-furo[2,3-b]pyrrolizine"),
            ("C1C=Cc2cc3ncccc3n12", "6H-pyrido[2,3-b]pyrrolizine"),
            ("C1C=Cc2cc3cccnc3n12", "8H-pyrido[3,2-b]pyrrolizine"),
            ("c1ccc2c(c1)Cc1cccn12", "9H-pyrrolo[1,2-a]indole"),
            ("C1C=CN2C=CC=C12", "1H-pyrrolizine"),
            ("C1=CC2=CC=CN2C1", "3H-pyrrolizine"),
            ("C1CC2=CC=CN2C1", "2,3-dihydro-1H-pyrrolizine"),
            ("CC1=CC2=CC=CN2C1", "2-methyl-3H-pyrrolizine"),
            ("C1C=Cc2ccc3sccc3c2N1", "1,2-dihydrothieno[2,3-h]quinoline"),
            ("c1cc2cc3occc3n2cc1", "furo[2,3-b]indolizine"),
            ("c1cn2cnc3[nH]cnc3c2n1", "3H-imidazo[2,1-i]purine"),
            (
                "S1CC=C2C1=NC=1N2C=2C(=C3C1N=C1C=CC=CC1=C3)N=CC2",
                "2H-benzo[b]pyrrolo[2,3-f]thieno[2',3':4,5]imidazo[1,2-h][1,7]naphthyridine",
            ),
            ("c1ccc2c(c1)Cc1c(ccc3ccccc13)O2", "12H-benzo[a]xanthene"),
            ("C1C=Cn2c1cc1sccc12", "7H-thieno[2,3-b]pyrrolizine"),
            // Quinolizine also has a neutral trivalent donor N5. Its
            // component C4 (input atom 13) releases H to C6 (atom 11).
            // Parent 2 -> 3 and thiophene 2 -> 3 both map to input 5 -> 4,
            // so P-25.3.1.3 gives [2,3-b]. Completed input-order locants
            // are 1,2,3,3a,10a,10,9a,9,8,7,6,5,4 (P-25.3.3).
            ("S1C=CC=2C1=CC1=CC=CCN1C2", "6H-thieno[2,3-b]quinolizine"),
        ] {
            let name = name_smiles(smiles, &Default::default()).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(name.status, NameStatus::Preferred, "{smiles}");
            let parsed = crate::test_support::parse_strict(&name.text).unwrap();
            assert_eq!(
                crate::test_support::canonical_smiles(smiles).unwrap(),
                crate::test_support::canonical_smiles(&parsed).unwrap(),
                "{}",
                name.text,
            );
        }
    }

    #[test]
    fn fusion_registered_carbazole_keeps_existing_layout_status() {
        use nazuchem_ir::{NameReason, NameStatus};
        // Carbazole is registered, but this first-order fusion still uses an
        // OPSIN-entry layout; its existing numbering status remains unverified.
        let name = name_smiles("c1ccc2c(c1)ccc1c2[nH]c2ccccc12", &Default::default()).unwrap();
        assert_eq!(name.text, "11H-benzo[a]carbazole");
        assert_eq!(
            name.status,
            NameStatus::Unverified(NameReason::OpsinFusionLayout)
        );
    }

    #[test]
    // Provenance: fn tests::known_departures_keep_their_names_and_report_the_rule_family | adapted
    // Source: ReShiki | src/naming/openclatura/mod.rs::tests::known_departures_keep_their_names_and_report_the_rule_family
    // Source: openclatura | tests/unit/test_chalcogen_functional_groups.py::test_peroxide_linkages_preserve_both_chalcogen_sites
    // Source: openclatura | tests/unit/test_rules_tables.py::test_anhydride_half_citations_are_structured_rule_data
    // Source: openclatura | tests/unit/test_audit_reconstruction.py::test_self_audit_confirms_modelled_names
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn known_departures_keep_their_names_and_report_the_rule_family() {
        use nazuchem_ir::{NameReason as R, NameStatus};
        for (smiles, expected, reason) in [
            (
                "C1=CN=C2C=CC=C3C2=C1C=C1C2=C(C3)C=CC=C2C=C1",
                "17-azapentacyclo[10.7.1.1^{3,6}.0^{10,21}.0^{16,20}]henicosa-1(19),2,4,6(21),7,9,12(20),13,15,17-decaene",
                R::VonBaeyerFusionFallback,
            ),
            (
                "OC(=O)CS(=O)(=O)CS(=O)(=O)O",
                "(sulfomethylsulfonyl)acetic acid",
                R::SulfonylPrefix,
            ),
            (
                "CNC(=N)NCC(=O)O",
                "(3-methylguanidino)acetic acid",
                R::AminoPrefix,
            ),
            (
                "N/C(=N/[H])NCC(=O)O",
                "[(Z)-guanidino]acetic acid",
                R::AminoPrefix,
            ),
            (
                "CC[S@](C)=O",
                "(S)-ethyl methyl sulfoxide",
                R::FunctionalClass,
            ),
            (
                "CP(C)(F)=O",
                "fluorodi(methyl)phosphane oxide",
                R::FunctionalClass,
            ),
            ("CN(C)Cl", "chlorodi(methyl)azane", R::HydrideParent),
            ("CC=NN", "acetaldehyde hydrazone", R::FunctionalClass),
            (
                "ClC#[N+][O-]",
                "chloromethanenitrile oxide",
                R::UnretainedParent,
            ),
            (
                "CCC(C)=NN=CC",
                "butan-2-one ethan-1-ylidenehydrazone",
                R::FunctionalClass,
            ),
            ("CO[Se]C", "dimethyl selenoperoxide", R::FunctionalClass),
            ("CSSC", "1,2-dimethyldisulfane", R::HydrideParent),
            ("CCOSC", "ethoxy(methyl)sulfane", R::HydrideParent),
            ("CCCOSS", "1-propoxydisulfane", R::HydrideParent),
            (
                "CNC(=O)C(N)=O",
                "N-methylethanediamide",
                R::UnretainedParent,
            ),
            (
                "CN(C=O)C(=O)O",
                "(N-methylformamido)formic acid",
                R::UnretainedParent,
            ),
            ("O=C(O)Cl", "hydroxyformyl chloride", R::UnretainedParent),
            ("O=C(OC)Cl", "methoxyformyl chloride", R::UnretainedParent),
            (
                "CNC(=O)Cl",
                "1-chloro-N-methylformamide",
                R::UnretainedParent,
            ),
            (
                "O=CO[PH](=O)O",
                "(hydroxyphosphinoyloxy)methanal",
                R::UnretainedParent,
            ),
            (
                "O=C(O)OP(=O)(O)c1ccccn1",
                "{[hydroxy(pyridin-2-yl)phosphoryl]oxy}formic acid",
                R::UnretainedParent,
            ),
            (
                "COC(=O)N(C=O)c1ccsc1",
                "methyl [N-(thiophen-3-yl)formamido]formate",
                R::UnretainedParent,
            ),
            (
                "CCNC(=O)CC(=O)NCC",
                "N,N'-diethylpropanediamide",
                R::DiamideLocants,
            ),
            (
                "CNC(=O)CCC(N)=O",
                "N-methylbutanediamide",
                R::DiamideLocants,
            ),
            (
                "c1cc2cccc3conc3c-2c1",
                "azuleno[4,5-c]isoxazole",
                R::FusionComponent,
            ),
            ("c1ccc(cc1)Oc1ccccc1", "phenoxybenzene", R::Multiplicative),
            (
                "c1ccc(cc1)Sc1ccccc1",
                "(phenylsulfanyl)benzene",
                R::Multiplicative,
            ),
            (
                "O=S(c1ccccc1)c1ccccc1",
                "(benzenesulfinyl)benzene",
                R::Multiplicative,
            ),
            (
                "O=S(=O)(c1ccccc1)c1ccccc1",
                "(benzenesulfonyl)benzene",
                R::Multiplicative,
            ),
            (
                "O=C(O)c1ccc(cc1)C(=O)C1CCCCC1",
                "4-(cyclohexylcarbonyl)benzoic acid",
                R::AcylPrefix,
            ),
            (
                "O=C(O)c1ccc(cc1)C(=S)C1CCCCC1",
                "4-(cyclohexylcarbonothioyl)benzoic acid",
                R::AcylPrefix,
            ),
            (
                "O=C(O)c1ccc(cc1)P(C)(C)=O",
                "4-(dimethyloxophosphanyl)benzoic acid",
                R::AcylPrefix,
            ),
            ("C[PH](C)=O", "(methyloxophosphanyl)methane", R::AcylPrefix),
            (
                "O=C(O)c1ccc([As](=O)(C)C)cc1",
                "4-(dimethyloxoarsanyl)benzoic acid",
                R::AcylPrefix,
            ),
            (
                "CC[C@@H](C)c1ccc(Sc2ccc(cc2)[C@H](C)CC)cc1",
                "1-[(2R)-butan-2-yl]-4-({4-[(2R)-butan-2-yl]phenyl}sulfanyl)benzene",
                R::Multiplicative,
            ),
            (
                "S(CC(=O)O)CC(=O)O",
                "(carboxymethylsulfanyl)acetic acid",
                R::Multiplicative,
            ),
        ] {
            let name = name_smiles(smiles, &Default::default()).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(name.status, NameStatus::NotPreferred(reason), "{smiles}");
            if expected.ends_with("acetic acid") || expected.ends_with("formic acid") {
                let parsed = crate::test_support::parse_strict(&name.text).unwrap();
                assert_eq!(
                    crate::test_support::canonical_smiles(smiles).unwrap(),
                    crate::test_support::canonical_smiles(&parsed).unwrap(),
                    "{}",
                    name.text
                );
            }
        }
    }

    #[test]
    // Provenance: fn tests::unchecked_paths_keep_their_names_and_report_the_uncertainty | adapted
    // Source: ReShiki | src/naming/openclatura/mod.rs::tests::unchecked_paths_keep_their_names_and_report_the_uncertainty
    // Source: openclatura | tests/integration/test_systematic_fusion.py::test_retained_complete_system_matrix_precedes_systematic_fusion
    // Source: openclatura | tests/unit/test_fusion_wrappers.py::test_bridged_fusion_wrapper_uses_retained_parent_locants
    // Source: openclatura | tests/unit/test_fusion_wrappers.py::test_bridge_wrapper_composes_with_shared_parent_derivative_operations
    // Source: openclatura | tests/unit/test_analysis.py::test_public_api_golden_names
    // Source: openclatura | tests/roundtrip/test_p13_fusion.py::test_roundtrip
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn unchecked_paths_keep_their_names_and_report_the_uncertainty() {
        use nazuchem_ir::{NameReason as R, NameStatus};
        for (smiles, expected, reason) in [
            ("NCC=O", "2-aminoethanal", R::SubstitutedRetainedParent),
            (
                "OC(=O)Cc1ccc(cc1)C(=O)O",
                "(4-carboxyphenyl)acetic acid",
                R::PrincipalGroupTie,
            ),
            (
                "NC(=O)CNC(=O)c1ccccc1",
                "N-(2-amino-2-oxoethyl)benzamide",
                R::PrincipalGroupTie,
            ),
            (
                "C1=Cc2cc3ccc(cc4nc(cc5ccc(cc1n2)[nH]5)C=C4)[nH]3",
                "porphyrin",
                R::UnauditedParent,
            ),
            (
                "C1=C2CCC(=N2)C=C2CCC(N2)C2CCC(=N2)C=C2CCC1=N2",
                "corrin",
                R::UnauditedParent,
            ),
            (
                "C12C(C=C(C3=CC=CC=C13)O2)=O",
                "1,4-epoxynaphthalen-2(1H)-one",
                R::AtomOrder,
            ),
            (
                "C12=CC=C(C3=CC=CC=C13)O2",
                "1,4-epoxynaphthalene",
                R::BridgedFusedParent,
            ),
            (
                "c1oc2cc3nc4ccccc4nc3cc2c1",
                "furo[2,3-b]phenazine",
                R::OpsinFusionLayout,
            ),
        ] {
            let name = name_smiles(smiles, &Default::default()).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(name.status, NameStatus::Unverified(reason), "{smiles}");
            if expected.ends_with("acetic acid") {
                let parsed = crate::test_support::parse_strict(&name.text).unwrap();
                assert_eq!(
                    crate::test_support::canonical_smiles(smiles).unwrap(),
                    crate::test_support::canonical_smiles(&parsed).unwrap(),
                    "{}",
                    name.text
                );
            }
        }
    }

    #[test]
    // Provenance: fn tests::retained_and_systematic_fused_parents_follow_parent_precedence | adapted
    // Source: ReShiki | src/naming/openclatura/mod.rs::tests::retained_and_systematic_fused_parents_follow_parent_precedence
    // Source: openclatura | tests/roundtrip/test_fused_parents.py::test_fused_parent_suite_pin_claims_are_explicit_and_evidence_backed
    // Source: openclatura | tests/roundtrip/test_fused_hydrocarbon_retained.py::test_hydrogenated_parent_uses_data_backed_preferred_name
    // Source: openclatura | tests/integration/test_systematic_fusion.py::test_retained_complete_system_matrix_precedes_systematic_fusion
    // Source: openclatura | tests/integration/test_systematic_fusion.py::test_multiple_indicated_hydrogens_are_graph_derived_and_roundtrip
    // Source: openclatura | tests/integration/test_systematic_fusion.py::test_positive_heteroatom_charge_is_audited_as_a_locanted_parent_operation
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn retained_and_systematic_fused_parents_follow_parent_precedence() {
        for (smiles, expected) in [
            ("c1ccc2ccccc2c1", "naphthalene"),
            ("c1ccc2cc3ccccc3cc2c1", "anthracene"),
            ("c1ccc2[nH]ccc2c1", "1H-indole"),
            ("c1ccc2c(c1)[nH]c1ccccc12", "9H-carbazole"),
            ("C1CCC2=CC=CC=C2C1", "1,2,3,4-tetrahydronaphthalene"),
            ("c1ccc2c(c1)CCN2", "2,3-dihydro-1H-indole"),
            ("c1oc2ncccc2c1", "furo[2,3-b]pyridine"),
            ("C1OCC2COCC12", "tetrahydro-1H,3H-furo[3,4-c]furan"),
        ] {
            assert_eq!(
                name_smiles(smiles, &Default::default())
                    .map(|name| name.text)
                    .as_deref(),
                Ok(expected),
                "{smiles}"
            );
        }
        assert_eq!(
            name_smiles("C1CCC2(C1)Oc1ccccc1O2", &Default::default())
                .map(|name| name.text)
                .as_deref(),
            Ok("spiro[[1,3]benzodioxole-2,1'-cyclopentane]"),
        );
    }

    #[test]
    fn angular_oxazole_thiazole_acetic_acid_round_trips() {
        let smiles = "O=C(O)Cc1nc2ccc3ocnc3c2s1";
        let name = name_smiles(smiles, &Default::default()).unwrap().text;
        assert_eq!(
            name,
            "([1,3]thiazolo[5,4-e][1,3]benzoxazol-7-yl)acetic acid"
        );
        let parsed = crate::test_support::parse_strict(&name).unwrap();
        assert_eq!(
            crate::test_support::canonical_smiles(smiles).unwrap(),
            crate::test_support::canonical_smiles(&parsed).unwrap()
        );
    }

    #[test]
    // Provenance: fn tests::spiro_parents_and_substituents_follow_oracle_names | adapted
    // Source: ReShiki | src/naming/openclatura/mod.rs::tests::spiro_parents_and_substituents_follow_oracle_names
    // Source: openclatura | tests/unit/test_fused_bonds.py::test_fused_bonds_match_the_cycle_count
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn spiro_parents_and_substituents_follow_oracle_names() {
        for (smiles, expected) in [
            ("C1CCC2(CC1)CCCC2", "spiro[4.5]decane"),
            ("C1CCC2(CC1)OCCO2", "1,4-dioxaspiro[4.5]decane"),
            ("O=C1CCCC12CCCCC2", "spiro[4.5]decan-1-one"),
            ("C1=CC2(C1)CC1(CCC1)C2", "dispiro[3.1.3.1]dec-1-ene"),
            ("C1=CC12NC21CC1", "7-azadispiro[2.0.2.1]hept-1-ene"),
            (
                "CN/C=C\\C1CC2(C1)CCC2",
                "(1Z)-N-methyl-2-(spiro[3.3]heptan-2-yl)ethen-1-amine",
            ),
            (
                "CC1=CC=C(C=C1)N2C(=O)C[C@@]3(C2=O)C=C(NN3)C(=O)OCC4=CC=CC=C4",
                "benzyl (5S)-7-(4-methylphenyl)-6,8-dioxo-1,2,7-triazaspiro[4.4]non-3-ene-3-carboxylate",
            ),
        ] {
            assert_eq!(
                name_smiles(smiles, &Default::default())
                    .map(|name| name.text)
                    .as_deref(),
                Ok(expected),
                "{smiles}"
            );
        }
    }

    #[test]
    fn unsupported_spiro_topologies_decline() {
        for smiles in [
            "C1CC12CC23CC34CC4",
            "S123(CC1)(CC2)CC3",
            "C1CCC2(CC1)CCCC2.C",
        ] {
            assert!(
                name_smiles(smiles, &Default::default()).is_err(),
                "{smiles}"
            );
        }
    }

    #[test]
    // Provenance: fn tests::declines_unrendered_group_and_attachment_features | adapted
    // Source: ReShiki | src/naming/openclatura/mod.rs::tests::declines_unrendered_group_and_attachment_features
    // Source: openclatura | tests/unit/test_chalcogen_functional_groups.py::test_polonium_is_parseable_but_has_an_explicit_unvalidated_boundary
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn declines_unrendered_group_and_attachment_features() {
        for smiles in [
            "CC(=[Po])[OH]",
            "C/N=N/c1cccnc1",
            "C/N=N\\c1cccnc1",
            "CC(=O)NOC(=O)Cc1nc(C(=O)O)cs1",
            "O=C(O)[C@H](O)[C@@H](O)C(=O)OO",
            "CN(C(=O)[O-])C1(B(O)O)CCC1",
            "COC(=O)NOOC1CCCCC1",
            "O[N@]1CC12CC2",
            "CN(C)[N@]1CC1(C)CO",
            "C[P@]1CCCC1C",
            "C[P@]1CCCC12CCCC2",
            "C[As@]1CCCC1C",
            "C[S@+]1CCCC1C",
        ] {
            assert!(
                name_smiles(smiles, &Default::default()).is_err(),
                "{smiles}"
            );
        }
    }

    #[test]
    fn tetrahedral_group_four_ring_hydrides_have_no_lone_pair() {
        for element in ["Si", "Ge", "Sn"] {
            let smiles = format!("C[{element}@H]1CCCC1F");
            assert!(
                name_smiles(&smiles, &Default::default()).is_ok(),
                "{smiles}"
            );
        }
        for smiles in ["C[P@]1CCCC1F", "C[As@]1CCCC1F", "C[S@+]1CCCC1F"] {
            assert_eq!(
                name_smiles(smiles, &Default::default()).unwrap_err(),
                "openclatura: N and ring lone-pair stereocentres are not validated",
                "{smiles}",
            );
        }
    }

    #[test]
    #[ignore = "development parity against the external openclatura oracle TSV"]
    fn parity_file() {
        let path = std::env::var("OPENCLATURA_PARITY").expect("OPENCLATURA_PARITY TSV path");
        let out =
            std::env::var("OPENCLATURA_PARITY_OUT").expect("OPENCLATURA_PARITY_OUT diff path");
        let input = std::fs::read_to_string(path).expect("read parity input");
        let mut output =
            std::io::BufWriter::new(std::fs::File::create(out).expect("create parity diff"));
        writeln!(output, "class\tsmiles\texpected\tactual\tstatus\treason").unwrap();
        let (mut exact, mut different, mut declined) = (0, 0, 0);
        for (index, line) in input.lines().filter(|line| !line.is_empty()).enumerate() {
            let (smiles, expected) = line.split_once('\t').expect("SMILES and expected name");
            if std::env::var_os("OPENCLATURA_PARITY_VERBOSE").is_some() {
                eprintln!("parity row {index}: {smiles}");
            }
            match name_smiles(smiles, &Default::default()) {
                Ok(actual) if actual.text == expected => exact += 1,
                Ok(actual) => {
                    different += 1;
                    let wire = serde_json::to_value(actual.status).unwrap();
                    let status = wire["status"].as_str().unwrap();
                    let reason = wire["reason"].as_str().unwrap_or("");
                    writeln!(
                        output,
                        "different\t{smiles}\t{expected}\t{}\t{status}\t{reason}",
                        actual.text
                    )
                    .unwrap();
                }
                Err(reason) => {
                    declined += 1;
                    writeln!(output, "declined\t{smiles}\t{expected}\t{reason}").unwrap();
                }
            }
            output.flush().unwrap();
        }
        output.flush().unwrap();
        println!(
            "openclatura parity: exact={exact} different={different} declined={declined} total={}",
            exact + different + declined
        );
    }
}
