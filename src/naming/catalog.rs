//! A finite offline alias index; never a replacement for nomenclature rules.
//! The selected OPSIN names/data retain their MIT terms. See
//! licenses/name-aliases/NOTICE for source pins and the ChEBI attribution.
use super::canonical_smiles;
use serde::Deserialize;
use std::{collections::HashMap, sync::OnceLock};

pub const VERSION: &str = "ReShiki retained aliases 1";
const MAX_ENTRIES: usize = 40;
const MAX_ALIASES: usize = 5;
const DATA: &str = include_str!("catalog/data.json");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasSource {
    pub name: &'static str,
    pub version: &'static str,
    pub record: String,
    pub url: String,
    pub license: &'static str,
}
impl AliasSource {
    pub fn label(&self) -> String {
        format!(
            "{} {} · {} · {}",
            self.name, self.version, self.record, self.license
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alias {
    pub name: String,
    pub source: AliasSource,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    version: u32,
    entries: Vec<Entry>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    id: String,
    smiles: String,
    aliases: Vec<Name>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Name {
    name: String,
    source: Source,
    evidence: Vec<Evidence>,
}
#[derive(Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Source {
    Opsin,
    Chebi422,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Evidence {
    file: String,
    token: String,
}

type Index = HashMap<String, Vec<Alias>>;
static INDEX: OnceLock<Result<Index, String>> = OnceLock::new();

fn index() -> Result<&'static Index, String> {
    INDEX.get_or_init(load).as_ref().map_err(Clone::clone)
}

fn load() -> Result<Index, String> {
    // These are compile-time bytes. Keep initialization finite and fail closed
    // on conflicting aliases, duplicate identities or invalid graph data.
    if DATA.len() > 65_536 {
        return Err("The bundled alias catalog exceeds its byte limit".into());
    }
    let catalog: Catalog = serde_json::from_str(DATA)
        .map_err(|_| "The bundled offline alias catalog is invalid".to_owned())?;
    if catalog.version != 1 || catalog.entries.is_empty() || catalog.entries.len() > MAX_ENTRIES {
        return Err("Unsupported offline alias catalog version or size".into());
    }
    let mut index = Index::new();
    let mut names = HashMap::new();
    for entry in catalog.entries {
        if entry.id.is_empty() || entry.aliases.is_empty() || entry.aliases.len() > MAX_ALIASES {
            return Err("The offline alias record has an invalid identifier or size".into());
        }
        let identity = canonical_smiles(&entry.smiles)?;
        if identity.contains('.') {
            return Err("Offline aliases require a connected molecular identity".into());
        }
        let mut aliases = Vec::new();
        for alias in entry.aliases {
            if alias.name.is_empty()
                || alias.name.len() > 128
                || alias.name.chars().any(char::is_control)
            {
                return Err("The offline alias text is invalid".into());
            }
            if names
                .insert(alias.name.to_ascii_lowercase(), identity.clone())
                .is_some()
            {
                return Err(
                    "The offline alias catalog contains a duplicate or ambiguous name".into(),
                );
            }
            let source = match alias.source {
                Source::Opsin => {
                    let file = alias
                        .evidence
                        .first()
                        .ok_or("Missing OPSIN alias evidence")?;
                    if alias.evidence.iter().any(|e| {
                        e.token.is_empty() || e.file.contains('/') || !e.file.ends_with(".xml")
                    }) {
                        return Err("Invalid OPSIN alias resource reference".into());
                    }
                    AliasSource {
                        name: "OPSIN",
                        version: opsin::UPSTREAM_VERSION,
                        record: entry.id.clone(),
                        url: format!(
                            "https://github.com/dan2097/opsin/blob/{}/opsin-core/src/main/resources/uk/ac/cam/ch/wwmm/opsin/{}",
                            opsin::UPSTREAM_COMMIT,
                            file.file
                        ),
                        license: "MIT",
                    }
                }
                Source::Chebi422 => {
                    if alias.name != "(+)-lactic acid"
                        || identity != canonical_smiles("C[C@H](O)C(=O)O")?
                        || !alias.evidence.is_empty()
                    {
                        return Err(
                            "The ChEBI retained alias must identify the exact S-lactic graph"
                                .into(),
                        );
                    }
                    AliasSource {
                        name: "ChEBI:422",
                        version: "2024-04-18",
                        record: entry.id.clone(),
                        url: "https://www.ebi.ac.uk/chebi/CHEBI:422".into(),
                        license: "CC BY 4.0",
                    }
                }
            };
            aliases.push(Alias {
                name: alias.name,
                source,
            });
        }
        if index.insert(identity, aliases).is_some() {
            return Err("The offline alias catalog repeats a chemical identity".into());
        }
    }
    Ok(index)
}

/// Exact isomeric identity only: no stereo, isotope, charge or tautomer
/// relaxation, no fuzzy spelling, and no online fallback. Maps and atom order
/// follow the same identity policy as local naming. Empty means no entry in
/// this finite catalog, not that the molecule has no names anywhere.
pub fn aliases_for_smiles(smiles: &str) -> Result<Vec<Alias>, String> {
    let identity = canonical_smiles(smiles)?;
    if identity.contains('.') {
        return Err("Offline aliases require one connected molecule".into());
    }
    Ok(index()?.get(&identity).cloned().unwrap_or_default())
}

#[cfg(test)]
mod tests;
