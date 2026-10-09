//! Opt-in online nomenclature parsing and source-backed name lookup.
//!
//! OPSIN runs on EMBL-EBI's server. PubChem names are database results, not a
//! local general-purpose naming algorithm. No remote depiction or executable
//! is used: every returned SMILES passes the native chemical graph parser.
use crate::{chemistry, document::Document, editing};
use reqwest::{Client, Url};
use serde::Deserialize;
use std::{collections::HashSet, time::Duration};

const PUBCHEM: &str = "https://pubchem.ncbi.nlm.nih.gov/rest/pug/";
const OPSIN: &str = "https://www.ebi.ac.uk/opsin/ws/";
const MAX_RESPONSE: usize = 1_048_576;
const MAX_CANDIDATES: usize = 16;
static REQUEST_TIME: tokio::sync::Mutex<Option<tokio::time::Instant>> =
    tokio::sync::Mutex::const_new(None);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NameSource {
    #[default]
    Opsin,
    PubChem,
}
impl std::fmt::Display for NameSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Opsin => "Systematic name · OPSIN online",
            Self::PubChem => "Common name · PubChem online",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provenance {
    Opsin,
    PubChem(u64),
}
impl Provenance {
    pub fn label(&self) -> String {
        match self {
            Self::Opsin => "OPSIN · EMBL-EBI online parser".into(),
            Self::PubChem(cid) => format!("PubChem CID {cid} · source lookup"),
        }
    }
    pub fn url(&self) -> String {
        match self {
            Self::Opsin => "https://www.ebi.ac.uk/opsin/".into(),
            Self::PubChem(cid) => format!("https://pubchem.ncbi.nlm.nih.gov/compound/{cid}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Record {
    pub title: String,
    /// The source's systematic name. OPSIN parses the supplied name; it does
    /// not produce a new systematic name from a graph.
    pub systematic_name: Option<String>,
    pub smiles: String,
    pub canonical_smiles: String,
    pub synonyms: Vec<String>,
    pub warnings: Vec<String>,
    pub provenance: Provenance,
}

#[derive(Debug, Clone)]
pub struct Identity {
    pub smiles: String,
    pub warnings: Vec<String>,
}

fn canonical_state(state: &chemistry::stereo::perception::State) -> Result<String, String> {
    if state.graph.atoms.is_empty() || state.graph.atoms.len() > 512 {
        return Err("Naming supports molecular graphs with 1–512 atoms".into());
    }
    if state
        .graph
        .atoms
        .iter()
        .any(|a| a.atomic_number == 0 || a.radical_electrons != 0)
        || state
            .graph
            .bonds
            .iter()
            .any(|b| !(1..=4).contains(&b.order))
        || !state.metadata.groups.is_empty()
    {
        return Err(
            "Naming does not support query atoms, radicals, exotic bonds or relative stereo groups"
                .into(),
        );
    }
    if state.properties.atoms.iter().any(|a| a.unknown)
        || state
            .metadata
            .bonds
            .iter()
            .any(|b| b.unknown_stereo || b.stereo == 1)
    {
        return Err("Resolve unknown stereochemistry before naming; wavy stereo cannot be sent as unspecified stereo".into());
    }
    let mut state = state.clone();
    for atom in &mut state.metadata.atoms {
        atom.map_number = 0;
        atom.map_present = false;
    }
    chemistry::smiles::write::write(
        &state,
        chemistry::smiles::write::Options {
            ignore_maps: true,
            ..Default::default()
        },
    )
    .map(|s| s.text)
    .map_err(|e| e.to_string())
}

/// Canonical isomeric graph identity, including isotope, charge and specified
/// stereo. Map numbers and atom order are not molecular identity.
pub fn canonical_smiles(text: &str) -> Result<String, String> {
    if text.len() > 32_768 || text.contains('|') || text.contains('>') {
        return Err(
            "Naming requires ordinary molecular SMILES without reaction or CX extensions".into(),
        );
    }
    let imported = chemistry::smiles::read(text).map_err(|e| e.to_string())?;
    canonical_state(&imported.prepared.state)
}

/// Require a complete connected molecule. Abbreviation selection expands to
/// its complete underlying graph; display labels never replace chemical atoms.
pub fn selected_identity(document: &Document, selected: &[u64]) -> Result<Identity, String> {
    let ids: HashSet<_> = editing::analysis_atoms(document, selected)
        .into_iter()
        .collect();
    if ids.is_empty() {
        return Err("Select one complete molecule before looking up its name".into());
    }
    if document
        .bonds
        .iter()
        .any(|b| ids.contains(&b.a) != ids.contains(&b.b))
    {
        return Err("The selection cuts a bond. Select the complete molecule before naming".into());
    }
    let part = editing::selection(document, &ids.iter().copied().collect::<Vec<_>>());
    document_identity(&part)
}

pub fn document_identity(document: &Document) -> Result<Identity, String> {
    if document.atoms.is_empty() || document.atoms.len() > 512 {
        return Err("Naming supports molecular graphs with 1–512 atoms".into());
    }
    if document
        .bonds
        .iter()
        .any(|b| !b.projection && b.display == "wavy")
    {
        return Err("Resolve wavy/unknown stereochemistry before naming".into());
    }
    let molecule = chemistry::document::prepare(document).map_err(|e| e.to_string())?;
    let smiles = canonical_state(&molecule.state)?;
    if smiles.contains('.') {
        return Err("Select a single connected molecule; mixtures and disconnected salts are not supported for structure-to-name lookup".into());
    }
    Ok(Identity {
        smiles,
        warnings: vec!["Names describe specified stereochemistry only. Unspecified centers remain unspecified; no absolute configuration is inferred from a name lookup.".into()],
    })
}

pub fn verify_identity(expected: &str, actual: &str) -> Result<(), String> {
    if canonical_smiles(expected)? != canonical_smiles(actual)? {
        return Err("The source structure differs in connectivity, charge, isotope, tautomer or stereochemistry. No name was assigned".into());
    }
    Ok(())
}

#[derive(Clone)]
pub struct Service {
    client: Client,
}
impl Service {
    pub fn new() -> Result<Self, String> {
        Client::builder()
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!(
                "ReShiki/",
                env!("CARGO_PKG_VERSION"),
                " chemical-naming"
            ))
            .build()
            .map(|client| Self { client })
            .map_err(|e| e.to_string())
    }

    async fn json(&self, url: Url, body: Option<String>) -> Result<serde_json::Value, String> {
        // Serialize starts across tabs and services to <=2 requests/second.
        let mut last = REQUEST_TIME.lock().await;
        if let Some(time) = *last {
            tokio::time::sleep_until(time + Duration::from_millis(500)).await;
        }
        *last = Some(tokio::time::Instant::now());
        drop(last);
        let request = if let Some(body) = body {
            self.client
                .post(url)
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(body)
        } else {
            self.client.get(url)
        };
        let mut response = request.send().await.map_err(|_| "The online naming service could not be reached. Check your connection and try again".to_owned())?;
        let status = response.status();
        if status.as_u16() == 404 {
            return Err("No supported result was found. Try a more specific systematic name or the other name source".into());
        }
        if !status.is_success() {
            return Err(format!(
                "The online naming service returned HTTP {status}. Try again later"
            ));
        }
        if response
            .content_length()
            .is_some_and(|n| n > MAX_RESPONSE as u64)
        {
            return Err("The naming response exceeds the supported size".into());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "The naming response could not be read")?
        {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err("The naming response exceeds the supported size".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes)
            .map_err(|_| "The naming service returned an invalid response".into())
    }

    pub async fn resolve_name(
        &self,
        name: &str,
        source: NameSource,
    ) -> Result<Vec<Record>, String> {
        let name = name.trim();
        if name.is_empty() || name.len() > 2_048 || name.chars().any(char::is_control) {
            return Err(
                "Enter a chemical name of at most 2048 bytes without control characters".into(),
            );
        }
        match source {
            NameSource::Opsin => {
                let mut url = Url::parse(OPSIN).map_err(|e| e.to_string())?;
                url.path_segments_mut()
                    .map_err(|_| "Invalid OPSIN service URL")?
                    .pop_if_empty()
                    .push(&format!("{name}.json"));
                Ok(vec![parse_opsin(self.json(url, None).await?, name)?])
            }
            NameSource::PubChem => {
                let mut url = Url::parse(PUBCHEM).map_err(|e| e.to_string())?;
                url.path_segments_mut()
                    .map_err(|_| "Invalid PubChem service URL")?
                    .pop_if_empty()
                    .extend(["compound", "name", name, "cids", "JSON"]);
                url.query_pairs_mut().append_pair("name_type", "complete");
                let ids = parse_cids(self.json(url, None).await?)?;
                self.properties(&ids).await
            }
        }
    }

    async fn properties(&self, ids: &[u64]) -> Result<Vec<Record>, String> {
        let cids = ids.iter().map(u64::to_string).collect::<Vec<_>>().join(",");
        let url = Url::parse(&format!(
            "{PUBCHEM}compound/cid/{cids}/property/IUPACName,SMILES,Title/JSON"
        ))
        .map_err(|e| e.to_string())?;
        parse_properties(self.json(url, None).await?, ids)
    }

    pub async fn lookup_structure(&self, identity: Identity) -> Result<Record, String> {
        let mut url = Url::parse(&format!("{PUBCHEM}compound/fastidentity/smiles/cids/JSON"))
            .map_err(|e| e.to_string())?;
        url.query_pairs_mut()
            .append_pair("identity_type", "same_stereo_isotope")
            .append_pair("MaxRecords", "17")
            .append_pair("MaxSeconds", "10");
        let mut encoded = Url::parse("https://localhost/").map_err(|e| e.to_string())?;
        encoded
            .query_pairs_mut()
            .append_pair("smiles", &identity.smiles);
        let body = encoded
            .query()
            .ok_or("Could not encode molecular input")?
            .to_owned();
        let ids = parse_cids(self.json(url, Some(body)).await?)?;
        let mut matches = self
            .properties(&ids)
            .await?
            .into_iter()
            .filter(|r| r.canonical_smiles == identity.smiles);
        let mut record = matches.next().ok_or(
            "PubChem standardized this structure to a different identity. No name was assigned",
        )?;
        if matches.next().is_some() {
            return Err(
                "More than one source record matches this structure. No unique name was assigned"
                    .into(),
            );
        }
        record.warnings.extend(identity.warnings);
        if let Provenance::PubChem(cid) = record.provenance {
            let url = Url::parse(&format!("{PUBCHEM}compound/cid/{cid}/synonyms/JSON"))
                .map_err(|e| e.to_string())?;
            match self
                .json(url, None)
                .await
                .and_then(|value| parse_synonyms(value, cid))
            {
                Ok(synonyms) => record.synonyms = synonyms,
                Err(error) => record
                    .warnings
                    .push(format!("Synonyms unavailable: {error}")),
            }
        }
        Ok(record)
    }
}

fn parse_cids(value: serde_json::Value) -> Result<Vec<u64>, String> {
    let values = value
        .get("IdentifierList")
        .and_then(|v| v.get("CID"))
        .and_then(serde_json::Value::as_array)
        .ok_or("The service returned no compound identifiers")?;
    if values.is_empty() {
        return Err("No matching compounds were found".into());
    }
    if values.len() > MAX_CANDIDATES {
        return Err("This query has too many interpretations. Use a more specific name or molecular identity".into());
    }
    let mut ids = Vec::new();
    for value in values {
        let cid = value
            .as_u64()
            .filter(|id| *id > 0)
            .ok_or("Invalid source compound identifier")?;
        if ids.contains(&cid) {
            return Err("Duplicate source compound identifier".into());
        }
        ids.push(cid);
    }
    Ok(ids)
}

#[derive(Deserialize)]
struct Property {
    #[serde(rename = "CID")]
    cid: u64,
    #[serde(rename = "SMILES", alias = "IsomericSMILES")]
    smiles: String,
    #[serde(rename = "IUPACName")]
    name: String,
    #[serde(rename = "Title")]
    title: String,
}
fn parse_properties(value: serde_json::Value, ids: &[u64]) -> Result<Vec<Record>, String> {
    let values = value
        .get("PropertyTable")
        .and_then(|v| v.get("Properties"))
        .cloned()
        .ok_or("Missing source molecular properties")?;
    let properties: Vec<Property> =
        serde_json::from_value(values).map_err(|_| "Invalid source molecular properties")?;
    if properties.len() != ids.len() {
        return Err(
            "The service omitted one or more interpretations; no result was accepted".into(),
        );
    }
    let mut seen = HashSet::new();
    properties
        .into_iter()
        .map(|p| {
            if !ids.contains(&p.cid)
                || !seen.insert(p.cid)
                || p.name.trim().is_empty()
                || p.name.len() > 4096
                || p.title.len() > 4096
            {
                return Err("The service returned inconsistent molecular properties".into());
            }
            Ok(Record {
                canonical_smiles: canonical_smiles(&p.smiles)?,
                smiles: p.smiles,
                title: p.title,
                systematic_name: Some(p.name),
                synonyms: vec![],
                warnings: vec![],
                provenance: Provenance::PubChem(p.cid),
            })
        })
        .collect()
}

fn parse_opsin(value: serde_json::Value, name: &str) -> Result<Record, String> {
    let status = value
        .get("status")
        .and_then(serde_json::Value::as_str)
        .ok_or("Missing OPSIN parser status")?;
    if !matches!(status, "SUCCESS" | "WARNING") {
        let message = value
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("Unsupported name");
        return Err(format!(
            "OPSIN could not interpret this name: {}",
            message.chars().take(1024).collect::<String>()
        ));
    }
    let smiles = value
        .get("smiles")
        .and_then(serde_json::Value::as_str)
        .ok_or("OPSIN returned no molecular structure")?
        .to_owned();
    let mut warnings = Vec::new();
    if status == "WARNING"
        || value
            .get("warnings")
            .is_some_and(|v| v.as_array().is_some_and(|w| !w.is_empty()))
    {
        warnings.push("OPSIN reported ambiguity or incomplete stereochemical interpretation. Review the structure and warning before insertion.".into());
        if let Some(message) = value
            .get("message")
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
        {
            warnings.push(message.chars().take(1024).collect());
        }
        if let Some(values) = value.get("warnings").and_then(serde_json::Value::as_array) {
            warnings.extend(values.iter().take(16).map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| v.to_string())
                    .chars()
                    .take(1024)
                    .collect()
            }));
        }
    }
    Ok(Record {
        title: name.into(),
        systematic_name: None,
        canonical_smiles: canonical_smiles(&smiles)?,
        smiles,
        synonyms: vec![],
        warnings,
        provenance: Provenance::Opsin,
    })
}

fn parse_synonyms(value: serde_json::Value, cid: u64) -> Result<Vec<String>, String> {
    let records = value
        .get("InformationList")
        .and_then(|v| v.get("Information"))
        .and_then(serde_json::Value::as_array)
        .ok_or("Missing source synonyms")?;
    let [record] = records.as_slice() else {
        return Err("Inconsistent synonym source records".into());
    };
    if record.get("CID").and_then(serde_json::Value::as_u64) != Some(cid) {
        return Err("Synonym source identity changed".into());
    }
    let values = record
        .get("Synonym")
        .and_then(serde_json::Value::as_array)
        .ok_or("Missing source synonyms")?;
    Ok(values
        .iter()
        .filter_map(serde_json::Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= 256)
        .take(12)
        .map(str::to_owned)
        .collect())
}

#[cfg(test)]
mod tests;
