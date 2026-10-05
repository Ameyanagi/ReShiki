//! Format-specific copy. Conversion is side-effect free; the application checks
//! the originating document before publishing the prepared representations.
use super::{CDX_TYPES, LIMIT, Representation};
use crate::{document::Document, engine::LocalEngine, export};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::borrow::Cow;
mod chemdoodle;

const TEXT: &str = "public.utf8-plain-text";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyFormat {
    Png,
    Svg,
    Pdf,
    Mol,
    Smiles,
    Inchi,
    Cdxml,
    Cdx,
    Rxn,
    ReactionSmiles,
    ChemDoodleReaction,
}
impl CopyFormat {
    pub const ALL: [Self; 11] = [
        Self::Png,
        Self::Svg,
        Self::Pdf,
        Self::Mol,
        Self::Smiles,
        Self::Inchi,
        Self::Cdxml,
        Self::Cdx,
        Self::Rxn,
        Self::ReactionSmiles,
        Self::ChemDoodleReaction,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Svg if !super::available() => "SVG text",
            Self::Svg => "SVG",
            Self::Pdf => "PDF",
            Self::Mol => "MOL",
            Self::Smiles => "SMILES",
            Self::Inchi => "InChI",
            Self::Cdxml => "CDXML",
            Self::Cdx => "CDX",
            Self::Rxn => "RXN · V3000",
            Self::ReactionSmiles => "Reaction SMILES",
            Self::ChemDoodleReaction => "ChemDoodle JSON · reaction",
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Svg => "svg",
            Self::Pdf => "pdf",
            Self::Mol => "mol",
            Self::Smiles => "smiles",
            Self::Inchi => "inchi",
            Self::Cdxml => "cdxml",
            Self::Cdx => "cdx",
            Self::Rxn => "rxn",
            Self::ReactionSmiles => "rsmi",
            Self::ChemDoodleReaction => "chemdoodle-reaction",
        }
    }

    pub fn is_reaction(self) -> bool {
        matches!(
            self,
            Self::Rxn | Self::ReactionSmiles | Self::ChemDoodleReaction
        )
    }

    pub fn is_molecular(self) -> bool {
        matches!(self, Self::Mol | Self::Smiles | Self::Inchi)
    }

    pub fn is_chemical(self) -> bool {
        self.is_molecular() || self.is_reaction()
    }

    /// Availability of a clipboard transport, separate from conversion support.
    pub fn available(self) -> bool {
        super::available() || !matches!(self, Self::Png | Self::Pdf | Self::Cdx)
    }

    /// Cheap menu checks only. The existing exporters still validate chemistry
    /// and reject features the chosen format cannot preserve.
    pub fn unavailable_reason(self, doc: &Document) -> Option<&'static str> {
        if doc.all_ids().is_empty() {
            return Some("There is nothing to copy.");
        }
        if !self.available() {
            return Some("PNG, PDF and CDX need native clipboard support; use Export.");
        }
        if self.is_molecular() {
            if doc.atoms.is_empty() {
                return Some("MOL, SMILES and InChI need molecular atoms.");
            }
            if !doc.reactions.is_empty() {
                return Some("For molecular formats, select a molecule from the reaction.");
            }
        }
        if self.is_reaction() {
            return reaction_reason(doc).or_else(|| {
                (self == Self::ChemDoodleReaction)
                    .then(|| chemdoodle::reason(doc))
                    .flatten()
            });
        }
        None
    }
}

fn reaction_reason(doc: &Document) -> Option<&'static str> {
    let [reaction] = doc.reactions.as_slice() else {
        return Some("Reaction formats need one complete defined reaction.");
    };
    if reaction.reactants.is_empty() || reaction.products.is_empty() {
        return Some("Assign reactants and products in Reactions before copying.");
    }
    let participants: std::collections::HashSet<_> = reaction
        .reactants
        .iter()
        .chain(&reaction.products)
        .chain(&reaction.agents)
        .flat_map(|participant| participant.atoms.iter().copied())
        .collect();
    if doc
        .atoms
        .iter()
        .any(|atom| !participants.contains(&atom.id))
    {
        return Some("Select only the defined reaction, including all its participants.");
    }
    None
}

/// The ordinary Copy selection rules also retain required attachment and
/// abbreviation members. Unselected molecules are never added implicitly.
pub fn selection_or_drawing(doc: &Document, selected: &[u64]) -> Document {
    if selected.is_empty() {
        doc.clone()
    } else {
        crate::editing::selection(doc, selected)
    }
}

/// Chemical formats may infer one clear reaction without changing the drawing.
/// Inspect the source as well, since selection can prune incomplete explicit roles.
pub fn chemical_snapshot<'a>(
    source: &Document,
    snapshot: &'a Document,
) -> Result<Cow<'a, Document>, &'static str> {
    let Some(reaction) = crate::reactions::copy_reaction(source, snapshot)? else {
        return Ok(Cow::Borrowed(snapshot));
    };
    if snapshot.reactions.as_slice() == std::slice::from_ref(&reaction) {
        return Ok(Cow::Borrowed(snapshot));
    }
    let mut chemical = snapshot.clone();
    chemical.reactions = vec![reaction];
    Ok(Cow::Owned(chemical))
}

#[derive(Debug, Clone)]
pub struct PreparedCopy {
    pub format: CopyFormat,
    pub notices: Vec<String>,
    representations: Vec<Representation>,
    pub(super) text: Option<String>,
}
impl PreparedCopy {
    /// Also usable for an explicit text-only handoff on platforms without the
    /// native clipboard helper. No credentials or external service are involved.
    pub fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }
}

/// Prepare the complete chosen payload before touching the existing clipboard.
/// Warnings are retained; a failed conversion never falls back to another format.
pub async fn prepare_as(
    engine: LocalEngine,
    original: Document,
    format: CopyFormat,
) -> Result<PreparedCopy, String> {
    original.validate()?;
    // Conversion remains usable offline even without a native clipboard (e.g.
    // for an explicit manual handoff). Availability is enforced by the UI/write.
    if original.all_ids().is_empty() {
        return Err("There is nothing to copy".into());
    }
    if format.is_reaction() {
        if let Some(reason) = reaction_reason(&original) {
            return Err(reason.into());
        }
    } else if format.is_molecular() {
        if original.atoms.is_empty() {
            return Err("Select or draw a molecule before copying".into());
        }
        if !original.reactions.is_empty() {
            return Err(
                "Select a molecule, or use RXN / Reaction SMILES for a defined reaction".into(),
            );
        }
    }

    let mut notices = Vec::new();
    let (bytes, text) = if format == CopyFormat::ChemDoodleReaction {
        let output = tokio::task::spawn_blocking(move || chemdoodle::write(&original))
            .await
            .map_err(|error| format!("Could not prepare ChemDoodle JSON: {error}"))??;
        notices.push("Reaction structure and roles copied as ChemDoodle JSON. Captions, drawing styles and other artwork are not included; keep the native drawing.".into());
        (output.as_bytes().to_vec(), Some(output))
    } else if matches!(format, CopyFormat::Png | CopyFormat::Svg | CopyFormat::Pdf) {
        let (doc, notice) = export::figure_document(&engine, original).await?;
        notices.extend(notice);
        let figure = tokio::task::spawn_blocking(move || {
            #[cfg(windows)]
            if format == CopyFormat::Svg {
                return export::clipboard_svg(&doc).map(|bytes| export::Figure {
                    bytes,
                    detail: None,
                });
            }
            export::clipboard_figure(&doc, format.code())
        })
        .await
        .map_err(|error| format!("Could not render clipboard figure: {error}"))??;
        notices.extend(figure.detail);
        let bytes = figure.bytes;
        let text = (format == CopyFormat::Svg)
            .then(|| String::from_utf8(bytes.clone()).map_err(|error| error.to_string()))
            .transpose()?;
        (bytes, text)
    } else {
        if format.is_molecular()
            && (!original.annotations.is_empty()
                || !original.arrows.is_empty()
                || !original.graphics.is_empty())
        {
            notices.push("Chemical structure copied; captions, arrows and other drawing objects are not included in this format".into());
        }
        // Match ordinary editable Copy: external drawings carry visible ink,
        // not theme-dependent palette references or a page background.
        let doc = if matches!(format, CopyFormat::Cdxml | CopyFormat::Cdx) {
            crate::canvas_theme::for_paste(
                crate::depth_appearance::materialize(&original).into_owned(),
                crate::canvas_theme::CanvasTheme::Light,
            )
        } else {
            original
        };
        let mut request = crate::engine::Request::molecule("export", doc);
        request.format = Some(format.code().into());
        let response = engine.request(request).await?;
        notices.extend(response.warnings);
        let output = response.output.ok_or("No data was exported")?;
        if format == CopyFormat::Cdx {
            (
                STANDARD.decode(&output).map_err(|_| "Invalid CDX output")?,
                None,
            )
        } else {
            (output.as_bytes().to_vec(), Some(output))
        }
    };
    from_output(format, bytes, text, notices)
}

fn from_output(
    format: CopyFormat,
    bytes: Vec<u8>,
    text: Option<String>,
    notices: Vec<String>,
) -> Result<PreparedCopy, String> {
    if bytes.is_empty() {
        return Err("No data was exported".into());
    }
    let kinds: &[&str] = match format {
        CopyFormat::Png => &["public.png"],
        CopyFormat::Svg => &["public.svg-image"],
        CopyFormat::Pdf => &["com.adobe.pdf"],
        CopyFormat::Mol => &["com.mdli.molfile"],
        CopyFormat::Smiles => &["org.opensmiles.smiles"],
        CopyFormat::Cdxml => &["chemical/x-cdxml"],
        CopyFormat::Cdx => &CDX_TYPES,
        CopyFormat::Inchi
        | CopyFormat::Rxn
        | CopyFormat::ReactionSmiles
        | CopyFormat::ChemDoodleReaction => &[],
    };
    let count = kinds.len() + usize::from(text.is_some());
    if bytes.len() > LIMIT / count.max(1) {
        return Err("Combined clipboard representations exceed 64 MB".into());
    }
    let mut representations: Vec<_> = if kinds.is_empty() {
        Vec::new()
    } else {
        Representation::aliases(kinds, STANDARD.encode(&bytes).into()).collect()
    };
    if let Some(text) = &text {
        representations.push(Representation::new(TEXT, text.as_bytes()));
    }
    Ok(PreparedCopy {
        format,
        notices,
        representations,
        text,
    })
}

/// Publish a prepared payload using the same native clipboard transport as
/// ordinary Copy. All conversion and representation checks have already passed.
pub async fn write_prepared(copy: PreparedCopy) -> Result<Vec<String>, String> {
    super::invoke("write", &copy.representations).await?;
    Ok(copy.notices)
}

#[cfg(test)]
mod tests;
