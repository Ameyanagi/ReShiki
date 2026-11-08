// SPDX-License-Identifier: MIT OR Apache-2.0
// Origin: original
// Source: ReShiki | src/naming.rs | 8e09fa7f89e47d8465772eabf2ef7f2ea491af3a
// Copyright (c) 2026 Ameyanagi and ReShiki contributors
// Changes: Extract shared naming vocabulary without changing its representation;
// expose NameStatus::severity to preserve assessment ordering across crates.

//! Shared naming vocabulary extracted from original ReShiki code.
//!
//! Phase A preserves the existing naming wire representation. The structure IR
//! is deferred to Phase B.
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Notation used for isotopically substituted structures.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IsotopeStyle {
    #[default]
    Iupac,
    Boughton,
}

/// Preference assessment made by the rules that produced a generated name.
/// This is independent of the later OPSIN structure reconstruction check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", content = "reason", rename_all = "snake_case")]
pub enum NameStatus {
    Preferred,
    NotPreferred(NameReason),
    Unverified(NameReason),
}
impl NameStatus {
    /// Downgrade strength: a known departure outranks an unchecked preference,
    /// and any recorded reason outranks the uncertified default.
    pub fn severity(self) -> u8 {
        match self {
            Self::Preferred => 0,
            Self::Unverified(NameReason::UnauditedPath) => 1,
            Self::Unverified(_) => 2,
            Self::NotPreferred(_) => 3,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Preferred => "Preferred IUPAC name",
            Self::NotPreferred(_) => "Systematic name, not the preferred IUPAC name",
            Self::Unverified(_) => "Systematic name, preference not verified",
        }
    }

    pub fn detail(self) -> &'static str {
        match self {
            Self::Preferred => {
                "This name's path is in an audited family, and no rule on it is known to depart from the preferred IUPAC name (IUPAC 2013)."
            }
            Self::NotPreferred(reason) | Self::Unverified(reason) => reason.detail(),
        }
    }
}

/// The naming rule family responsible for a preference assessment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NameReason {
    SulfonylPrefix,
    AminoPrefix,
    CationPrefix,
    FunctionalClass,
    HydrideParent,
    UnretainedParent,
    DiamideLocants,
    FusionComponent,
    UnregisteredRetainedComponent,
    HigherOrderFusion,
    BenzoFusion,
    Multiplicative,
    AcylPrefix,
    RingAssembly,
    VonBaeyerFusionFallback,
    LinearPhane,
    OpsinFusionLayout,
    FusionMultiparent,
    HydroPrefixLocants,
    BridgedFusedParent,
    UnauditedParent,
    UnauditedPath,
    SubstitutedRetainedParent,
    AtomOrder,
    AmineChalcogenide,
    AmineOxideZwitterion,
    PrincipalGroupTie,
    LambdaParentHydrogen,
    SpiroHydrogen,
    IsotopePlacement,
    BoughtonIsotope,
    RadicalIon,
    AnionParent,
    CationParent,
    IonicSuffixOrder,
    SaltIonOrder,
    Phane,
    RelativeRingStereo,
    EndoExoStereo,
    UnknownConfiguration,
    AxialAlternative,
}
impl NameReason {
    pub fn detail(self) -> &'static str {
        match self {
            Self::AxialAlternative => {
                "Uses Ra/Sa axial descriptors; M and P are preferred to Ra and Sa (P-93.5.7.1)."
            }
            Self::SulfonylPrefix => {
                "Uses a concatenated sulfonyl or sulfinyl prefix such as N-methyltosylamino or (methylsulfinyl)amino; preferred IUPAC names use acid-derived forms such as N,4-dimethylbenzene-1-sulfonamido or methanesulfinamido (P-65.3.2.2.2, P-66.1.1.4.3)."
            }
            Self::AminoPrefix => {
                "Uses an acylamino prefix instead of the preferred acid-derived amido form (P-66.1.1.4.3), or a guanidino prefix substituted on its outer nitrogens or with an E/Z descriptor, such as 3-methylguanidino; preferred IUPAC names cite carbamimidoyl inside an amino prefix, as in (N-methylcarbamimidoyl)amino (P-66.4.1.2.1.3)."
            }
            Self::CationPrefix => {
                "Uses the accepted ammonio/iminio alternative for a substituted cationic nitrogen prefix because the preferred N,N-dimethylmethanaminiumyl-type form is not generated yet (P-73.6 and the 2020 corrections to P-74.1.3)."
            }
            Self::FunctionalClass => {
                "Uses a functional-class form such as a hydrazone, a chiral sulfoxide, a mixed chalcogen peroxide or a phosphane oxide with halide or alkoxy ligands; preferred IUPAC names use substitutive or acid-derived forms such as ethylidenehydrazine, (S)-(methanesulfinyl)ethane, [(phenylselanyl)tellanyl]benzene or dimethylphosphinic fluoride (P-68.3.1.2.2, P-93.3.4.1, P-63.3.2, P-68.3.2.3.1)."
            }
            Self::HydrideParent => {
                "Uses a substituted parent hydride such as chloro(methyl)azane, 1,2-dimethyldisulfane or ethoxy(methyl)sulfane; preferred IUPAC names use methylhypochlorous amide, (methyldisulfanyl)methane or [(methylsulfanyl)oxy]ethane (P-68.5.3, P-63.3.1, P-63.3.2)."
            }
            Self::UnretainedParent => {
                "Uses a substituted methanal or methanenitrile oxide, an N-substituted ethanediamide, or a formic acid, formyl halide or formamide substituted on carbon; preferred IUPAC names use N-substituted oxamide (P-66.1.1.1.2.1) or carbonic and carbamic acid derivatives such as carbonochloridic acid (P-65.1.8.1)."
            }
            Self::DiamideLocants => {
                "Uses N or N' locants on a di- or polyamide; the preferred IUPAC name cites the parent locant on each nitrogen, as in N1-methylbutanediamide (P-66.1.1.3.1.1)."
            }
            Self::FusionComponent => {
                "Uses isoxazole or isothiazole as a fusion parent component, as in cyclopenta[c]isoxazole or benzo[d]isothiazole; preferred IUPAC names use [1,2]oxazole and [1,2]thiazole, or a benzo name such as 1,2-benzothiazole (P-25.3.2.1.2, P-25.2.2.4)."
            }
            Self::UnregisteredRetainedComponent => {
                "Uses a junior fusion parent although a senior retained component is present, as in 9-(diethylamino)naphtho[2,1-b][1,4]benzoxazin-5-one rather than 9-(diethylamino)-5H-benzo[a]phenoxazin-5-one (P-25.3.2.4; P-25.2.1, Table 2.8; P-25.2.2.2, P-25.2.2.3)."
            }
            Self::HigherOrderFusion => {
                "Cites an isolated benzo ring as a separate higher-order component, as in [1,3]thiazolo[4',5':1,2]benzo[3,4-d][1,3]oxazole; preferred IUPAC names treat the benzo ring and an ortho-fused heteromonocycle as one benzoheterocycle component, as in [1,3]thiazolo[5,4-e][1,3]benzoxazole (P-25.3.5, P-25.2.2.4; FR-3.5)."
            }
            Self::BenzoFusion => {
                "Uses a fusion descriptor for a benzene ring ortho-fused to a heteromonocycle, as in benzo[b]oxepine; preferred IUPAC names cite the heteroatom locants of the whole bicycle before its benzo name, as in 1-benzoxepine, or the retained name, as in quinoline (P-25.2.2.4, P-25.2.1)."
            }
            Self::Multiplicative => {
                "Names identical senior parent units joined by a linking group substitutively; the preferred IUPAC name is multiplicative. The units include identical substituents and suffix groups at identical locants, while the central linking group need not be symmetric, as in 1,1'-oxydibenzene or 2,2'-sulfanediyldiacetic acid (P-15.3, P-51.1.5, P-51.3.1)."
            }
            Self::AcylPrefix => {
                "Uses an acyl prefix built from a substituent, such as (4-methylphenyl)carbonyl, cyclohexylcarbonothioyl, (dimethyloxophosphanyl) or (dimethyloxoarsanyl), or an oxophosphanyl prefix on a parent, as in (methyloxophosphanyl)methane; preferred IUPAC names use the acid-derived forms 4-methylbenzoyl, cyclohexanecarbothioyl, dimethylphosphinoyl, dimethylarsinoyl and dimethyl-lambda5-phosphanone (P-65.1.7.4.2, P-67.1.4.1.1.3, P-68.3.2.3.1)."
            }
            Self::RingAssembly => {
                "Uses a substitutive or ring-assembly name where the preferred name is not yet supported. This includes double-bond junctions and assemblies requiring skeletal-replacement or hydro prefixes (P-28, P-31.2.3.3.5, P-52.2.7)."
            }
            Self::LinearPhane => {
                "The structure is a linear phane: at least four ring systems, two of them terminal, along a path of at least seven skeletal nodes. Its preferred IUPAC name is a phane name, such as 2,4,6-trioxa-1,7(1),3,5(1,4)-tetrabenzenaheptaphane, even where substitutive or multiplicative names exist (P-52.2.5.3). ReShiki does not yet generate linear phane names."
            }
            Self::VonBaeyerFusionFallback => {
                "Uses a von Baeyer name, with skeletal replacement where needed, for a bridge-free ortho- or ortho- and peri-fused ring system with at least two rings of five or more members; preferred IUPAC names use fusion nomenclature, as in decahydronaphthalene rather than bicyclo[4.4.0]decane (P-52.2.4.4.2, P-31.1.4.2, P-31.2.3.3.2)."
            }
            Self::OpsinFusionLayout => {
                "Uses a fusion layout or citation order selected for OPSIN compatibility; its preferred IUPAC numbering or alphabetical citation order (FR-3.6 / P-25.3.4.2.3) has not been verified."
            }
            Self::FusionMultiparent => {
                "Cites the base fusion component and at least two identical attached components; whether a multiparent name such as furo[3,2-b:4,5-b']difuran is required (P-25.3.7.1) has not been verified."
            }
            Self::HydroPrefixLocants => {
                "Uses locanted hydro prefixes for a completely hydrogenated polycyclic parent; the omission of these locants required by P-31.2.3.3.2 has not been verified."
            }
            Self::BridgedFusedParent => {
                "The preferred parent choice for this bridged-fused system has not been verified."
            }
            Self::UnauditedParent => {
                "Uses a parent hydride family whose preferred-name choice has not been audited: porphyrin, corrin and gonane have no PIN designation (P-10/P-12); the von Baeyer versus fusion choice for polycycles of three or more rings has not been checked (P-52.2.4.3, P-31.1.4.3.4); skeletal-replacement chains and monocycles have unchecked conditions (P-15.4.3, P-22.2.3), as do skeletal-replacement fusion hydrides and chains requiring replacement with four heterounits (P-51.4.1, P-15.4.3.2); spiro compounds of identical components need spirobi names (P-24.3)."
            }
            Self::SpiroHydrogen => {
                "The completed spiro parent’s indicated, added or hydro hydrogen state and suffix composition have not been verified (P-24.5, P-31.2, P-58.2)."
            }
            Self::UnauditedPath => {
                "The naming path that built this name has not been audited against the IUPAC 2013 preferred-name rules (P-10/P-12)."
            }
            Self::SubstitutedRetainedParent => {
                "Whether a substituted acetaldehyde keeps its retained name in the preferred IUPAC name has not been verified."
            }
            Self::AtomOrder => {
                "The name can depend on input atom order; its preferred numbering has not been verified."
            }
            Self::AmineChalcogenide => {
                "The parent or oxide locant of this amine or imine oxide, sulfide, selenide or telluride has not been verified against the preferred IUPAC form (P-62.5)."
            }
            Self::AmineOxideZwitterion => {
                "Uses a zwitterionic amine or imine oxide name; preferred IUPAC names use functional-class nomenclature for one oxide or the lambda-convention for multiple oxides (P-62.5, P-74.2.1.2, P-74.2.2.1.9)."
            }
            Self::RelativeRingStereo => {
                "Uses cis/trans for an achiral cyclic compound; preferred IUPAC names use CIP r/s descriptors, while cis/trans is used in general nomenclature (P-93.5.1.1.2)."
            }
            Self::EndoExoStereo => {
                "Uses endo, exo, syn or anti for relative configuration in a bicyclic von Baeyer system; preferred IUPAC names use CIP descriptors, because endo/exo does not distinguish an enantiomer from its mirror image or the racemate (P-93.5.2.2.1)."
            }
            Self::UnknownConfiguration => {
                "Omits a descriptor for unknown configuration; preferred IUPAC names indicate unknown configuration with Ξ or ξ (P-91.2.1.2.1)."
            }
            Self::PrincipalGroupTie => {
                "Equally senior principal groups, such as a chain and a ring carboxylic acid, were chosen by atom order; the preferred parent (P-44.1) has not been verified."
            }
            Self::LambdaParentHydrogen => {
                "The indicated or added hydrogen of this λ heteroatom ring was not derived from the complete expanded-valence parent (P-25.7.1.1, λ-convention Lm-3.1–3.2), so its hydrogen descriptors may not be the preferred ones."
            }
            Self::IsotopePlacement => {
                "The isotope descriptor reconstructs the exact structure, but its preferred placement and numbering under P-82.2 and P-82.6 have not been verified."
            }
            Self::BoughtonIsotope => {
                "Uses Boughton/CAS isotope notation; IUPAC prefers parenthesised nuclide descriptors (P-80, P-82.2)."
            }
            Self::RadicalIon => {
                "Uses the descriptive ‘radical cation’ or ‘radical anion’ form (RC-85.1); preferred systematic radical-ion names use the cumulative ionic and radical suffixes of P-75 (including the cation suffixes in P-73)."
            }
            Self::IonicSuffixOrder => {
                "The suffix order of a skeletal -ide with a characteristic cationic suffix such as -aminium is not settled: P-70.3.3.2.2 places cumulative suffixes before functional suffixes, whereas P-74.1.2 places cationic suffixes before anionic suffixes."
            }
            Self::SaltIonOrder => {
                "The order of the ions in this salt has not been verified: P-14.5 orders them by their Roman letters, then locants, leaving out stereo and isotope descriptors, and this name has descriptors or spellings that the ordering does not yet cover."
            }
            Self::AnionParent => {
                "The anionic parent, suffix or retained-name choice for this anion has not been verified (P-72.7, P-72.2.2.2.1, P-72.2.2.2.2); for example, methoxide is preferred to methanolate."
            }
            Self::CationParent => {
                "Cites the cationic centre only in a substituent prefix, as in (1-methylpyridin-1-ium-4-yl)methanol. A cation is senior to neutral classes, so the preferred IUPAC name takes a cationic parent, as in 4-(hydroxymethyl)-1-methylpyridin-1-ium (P-41, P-73.1); ReShiki does not choose that parent yet."
            }
            Self::Phane => {
                "The structure is a cyclophane: at least six nodes, two or more rings and a mancude ring. Cyclic phane names rank above fused, bridged fused and von Baeyer names, so the preferred IUPAC name is a phane name (P-52.2.5.1, P-52.2.5.2). ReShiki has not verified the numbering and citation order of its phane name."
            }
        }
    }
}

/// An independently reconstructed alternative generated name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlternativeName {
    pub name: String,
    pub status: NameStatus,
}
