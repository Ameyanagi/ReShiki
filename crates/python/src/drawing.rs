use reshiki_io::{
    engine::{LocalEngine, Request, text_format},
    formats,
};
use reshiki_model::document::Document;
use std::sync::OnceLock;
use tokio::runtime::Runtime;

pub fn run<T>(work: impl Future<Output = Result<T, String>>) -> Result<T, String> {
    static RUNTIME: OnceLock<Result<Runtime, String>> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_name("reshiki-python")
                .build()
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(Clone::clone)?
        .block_on(work)
}

pub fn format_name(format: &str) -> String {
    match format
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase()
        .as_str()
    {
        "" => "auto",
        "smi" | "cxsmiles" => "smiles",
        "reaction_smiles" | "reaction-smiles" => "rsmi",
        "molfile" => "mol",
        "reshiki" | "moruno" => "rsk",
        "ent" => "pdb",
        "vasp" | "contcar" => "poscar",
        "extxyz" => "xyz",
        other => return other.into(),
    }
    .into()
}

fn detect(data: &[u8]) -> Result<&'static str, String> {
    if data.starts_with(b"VjCD") {
        return Ok("cdx");
    }
    let text = std::str::from_utf8(data).map_err(|error| format!("Invalid UTF-8: {error}"))?;
    let text = text.trim_start_matches('\u{feff}');
    let trimmed = text.trim_start();
    if trimmed.starts_with('{') {
        return Ok("rsk");
    }
    if trimmed.starts_with("data_")
        || text.contains("_cell_length_a")
        || text.contains("_atom_site_fract_x")
    {
        return Ok("cif");
    }
    if text.lines().any(|line| {
        ["HEADER", "ATOM  ", "HETATM", "CRYST1", "MODEL "]
            .iter()
            .any(|prefix| line.starts_with(prefix))
    }) {
        return Ok("pdb");
    }
    let lines: Vec<_> = text.lines().collect();
    if lines
        .first()
        .is_some_and(|line| line.trim().parse::<usize>().is_ok())
        && lines.len() >= 3
    {
        return Ok("xyz");
    }
    // POSCAR's scale and three lattice vectors precede its species/counts.
    if lines.len() >= 7
        && lines
            .get(1)
            .is_some_and(|line| line.trim().parse::<f64>().is_ok())
        && lines.get(2..5).is_some_and(|vectors| {
            vectors.iter().all(|line| {
                let fields: Vec<_> = line.split_whitespace().collect();
                fields.len() >= 3
                    && fields
                        .iter()
                        .take(3)
                        .all(|field| field.parse::<f64>().is_ok())
            })
        })
    {
        return Ok("poscar");
    }
    Ok(text_format(text))
}

pub async fn read(data: Vec<u8>, format: &str) -> Result<Document, String> {
    let mut format = format_name(format);
    if format == "auto" {
        format = detect(&data)?.into();
    }
    if data.len() > reshiki_io::exchange::LIMIT && format != "rsk" {
        return Err("Structure exceeds the 16 MB import limit".into());
    }
    if format == "sdf" {
        format = "mol".into();
    }
    let mut document = formats::read_with_engine(data, &format, &LocalEngine::in_process()).await?;
    // CDXML can carry a visible reaction without explicit participant roles.
    // Use the editor's inference so its existing reaction exporter can use it.
    if document.reactions.is_empty()
        && let Ok(Some(reaction)) = reshiki_model::reactions::copy_reaction(&document, &document)
    {
        document.reactions.push(reaction);
    }
    Ok(document)
}

pub fn kind(document: &Document) -> &'static str {
    if !document.reactions.is_empty() {
        "reaction"
    } else if !document.models.is_empty()
        || !document.graphics.is_empty()
        || !document.arrows.is_empty()
        || !document.annotations.is_empty()
        || !document.generic.is_empty()
        || document.atoms.is_empty()
    {
        "drawing"
    } else {
        "molecule"
    }
}

pub fn serialize(document: Document) -> Result<(Vec<u8>, String), String> {
    Ok((document.file_json()?, kind(&document).into()))
}

pub async fn clean(data: &[u8]) -> Result<Vec<u8>, String> {
    let document = Document::from_json(data)?;
    let result = LocalEngine::in_process()
        .request(Request::molecule("clean", document))
        .await?
        .document
        .ok_or("Cleanup returned no drawing")?;
    result.file_json()
}

pub async fn properties(data: &[u8]) -> Result<String, String> {
    let document = Document::from_json(data)?;
    let analysis = LocalEngine::in_process()
        .request(Request::molecule("analyze", document))
        .await?
        .analysis
        .ok_or("Analysis returned no molecular properties")?;
    serde_json::to_string(&analysis).map_err(|error| error.to_string())
}

pub async fn from_name(name: &str) -> Result<(Vec<u8>, String), String> {
    if name.trim().is_empty() {
        return Err("Enter a chemical name first".into());
    }
    let policy = reshiki_process_heap::policy::Capabilities::detect().resolve();
    let budget = nazuchem_n2s::Budget::new(
        policy.heap_bytes(),
        policy.work_units(),
        Some(policy.analysis_timeout()),
        policy.reservation_ceiling(),
        move |bytes| {
            policy
                .try_reserve(bytes)
                .map(|guard| Box::new(guard) as Box<dyn Send + Sync>)
        },
    );
    let parser = nazuchem_n2s::Parser::with_budget(budget).map_err(|error| error.to_string())?;
    let result = parser.parse(name, &nazuchem_n2s::ParseOptions::strict());
    let structure = result.structure.ok_or_else(|| {
        if result.message.is_empty() {
            format!("Could not interpret chemical name {name:?}")
        } else {
            result.message
        }
    })?;
    let smiles = structure
        .semantic_cxsmiles()
        .map_err(|error| error.to_string())?;
    serialize(read(smiles.into_bytes(), "smiles").await?)
}

/// Import an external database graph only if every stated chemical feature
/// survives native preparation and layout. Use the app's shared identity rules.
pub async fn parse_online_smiles(text: &str) -> Result<(Vec<u8>, String), String> {
    use reshiki_model::{
        chemistry::{document, smiles},
        molecule_names::{MAX_NAMING_ATOMS, MAX_SMILES_BYTES, canonical_identity, is_tetrahedral},
    };
    if text.is_empty()
        || text.len() > MAX_SMILES_BYTES
        || text.contains(['|', '>'])
        || text.chars().any(char::is_whitespace)
    {
        return Err(
            "PubChem requires ordinary molecular SMILES without reaction or CX extensions".into(),
        );
    }
    // Preparation may clean impossible tetrahedral winding or discard unsupported
    // stereo. Admit the raw external graph before either operation can do so.
    let raw = smiles::parse(text).map_err(|error| error.to_string())?;
    if raw.graph.atoms.is_empty() || raw.graph.atoms.len() > MAX_NAMING_ATOMS {
        return Err("PubChem lookup supports molecular graphs with 1–512 atoms".into());
    }
    if raw.metadata.atoms.iter().any(|atom| atom.chiral_tag > 2) {
        return Err("PubChem returned unsupported allene or coordination stereochemistry; no structure was substituted".into());
    }
    let specified = raw
        .metadata
        .atoms
        .iter()
        .filter(|atom| is_tetrahedral(atom.chiral_tag))
        .count();
    let source = smiles::read(text).map_err(|error| error.to_string())?;
    if source.dropped_cumulene_ez
        || specified
            != source
                .prepared
                .state
                .metadata
                .atoms
                .iter()
                .filter(|atom| is_tetrahedral(atom.chiral_tag))
                .count()
    {
        return Err("Native import could not retain every specified stereocenter; no PubChem structure was accepted".into());
    }
    let expected = canonical_identity(&source.prepared.state)
        .map_err(|error| format!("PubChem returned unsupported chemical semantics: {error:?}"))?;
    let imported = read(text.as_bytes().to_vec(), "smiles").await?;
    let actual = document::prepare(&imported).map_err(|error| error.to_string())?;
    let actual = canonical_identity(&actual.state).map_err(|error| {
        format!("Native import could not retain PubChem chemical semantics: {error:?}")
    })?;
    if actual != expected {
        return Err(
            "Native import changed the PubChem molecular identity; no structure was accepted"
                .into(),
        );
    }
    serialize(imported)
}
