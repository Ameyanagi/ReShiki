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
mod tests {
    use super::*;
    use crate::{
        document::Point,
        reactions::{Participant, Reaction},
    };

    fn molecule() -> Document {
        let mut doc = Document::default();
        let carbon = doc.add_atom("C", Point::default());
        let oxygen = doc.add_atom("O", Point::new(42., 0.));
        doc.add_bond(carbon, oxygen, 1, "plain");
        doc
    }

    #[tokio::test]
    async fn dark_depth_copy_as_cdxml_and_cdx_preserves_exact_source_ink() {
        use crate::{canvas_theme::CanvasTheme, palette::Color};
        for frozen in [false, true] {
            let mut original = Document {
                canvas_theme: CanvasTheme::Dark,
                ..Document::default()
            };
            for (i, element) in ["O", "C", "O"].into_iter().enumerate() {
                let id = original.add_atom(element, Point::new(i as f32 * 42., 0.));
                original.atom_mut(id).unwrap().depth = i as f32 * 20. - 20.;
                if i > 0 {
                    original.add_bond(id - 1, id, 1, "plain");
                }
            }
            let ids = original.all_ids();
            crate::depth_appearance::enable(&mut original, &ids, 0.6).unwrap();
            if frozen {
                crate::depth_appearance::freeze(&mut original, &ids);
            }
            let before = original.clone();
            for format in [CopyFormat::Cdxml, CopyFormat::Cdx] {
                let copy = prepare_as(Default::default(), original.clone(), format)
                    .await
                    .unwrap();
                let xml = if format == CopyFormat::Cdxml {
                    copy.text().unwrap().to_owned()
                } else {
                    let representation = copy
                        .representations
                        .iter()
                        .find(|r| r.kind == CDX_TYPES[0])
                        .unwrap();
                    crate::exchange::from_cdx(&representation.bytes().unwrap()).unwrap()
                };
                let restored = crate::chemistry::cdxml::import_cdxml(&xml)
                    .unwrap()
                    .document;
                let rear = restored
                    .atoms
                    .iter()
                    .min_by(|a, b| a.position.x.total_cmp(&b.position.x))
                    .unwrap();
                assert_eq!(
                    rear.text_style.as_ref().unwrap().color.rgb(),
                    [102; 3],
                    "{format:?}/{frozen}"
                );
                let bond = restored
                    .bonds
                    .iter()
                    .find(|b| b.a == rear.id || b.b == rear.id)
                    .unwrap();
                assert_eq!(bond.color.rgb(), [140; 3], "{format:?}/{frozen}");
                assert!(restored.depth_appearance.is_empty());
                assert_eq!(original.depth_appearance, before.depth_appearance);
                assert_eq!(original.bonds[0].color, Color::Ink);
                assert_eq!(original, before);
            }
        }
    }

    #[test]
    fn selection_does_not_expand_to_other_molecules_or_replace_the_source() {
        let mut doc = molecule();
        let selected = doc.all_ids();
        doc.add_atom("N", Point::new(200., 0.));
        let before = doc.clone();
        let part = selection_or_drawing(&doc, &selected);
        assert_eq!(part.atoms.len(), 2);
        assert_eq!(part.bonds.len(), 1);
        assert_eq!(selection_or_drawing(&doc, &[]), before);
        assert_eq!(doc, before);
    }

    #[test]
    fn reaction_formats_require_complete_explicit_roles_without_unselected_atoms() {
        let mut doc = molecule();
        assert!(reaction_reason(&doc).is_some());
        let product = doc.add_atom("O", Point::new(240., 0.));
        let arrow = doc.next_id();
        doc.arrows.push(crate::document::Arrow::new(
            arrow,
            Point::new(90., 0.),
            Point::new(170., 0.),
            Default::default(),
            Default::default(),
        ));
        let mut reaction = Reaction::new(arrow);
        reaction.reactants.push(Participant {
            atoms: vec![1, 2],
            coefficient: 1,
        });
        doc.reactions.push(reaction);
        assert!(reaction_reason(&doc).is_some());
        doc.reactions[0].products.push(Participant {
            atoms: vec![product],
            coefficient: 1,
        });
        assert!(reaction_reason(&doc).is_none());
        let complete = doc.reactions[0].ids();
        assert!(reaction_reason(&selection_or_drawing(&doc, &[1, 2])).is_some());
        doc.add_atom("N", Point::new(420., 0.));
        assert!(reaction_reason(&doc).is_some());
        assert!(reaction_reason(&selection_or_drawing(&doc, &complete)).is_none());
        assert!(CopyFormat::Mol.unavailable_reason(&doc).is_some());
    }

    #[test]
    fn chosen_formats_have_native_types_and_exact_text_without_hidden_native_data() {
        for (format, kind) in [
            (CopyFormat::Png, "public.png"),
            (CopyFormat::Svg, "public.svg-image"),
            (CopyFormat::Pdf, "com.adobe.pdf"),
            (CopyFormat::Mol, "com.mdli.molfile"),
            (CopyFormat::Smiles, "org.opensmiles.smiles"),
            (CopyFormat::Cdxml, "chemical/x-cdxml"),
            (CopyFormat::Cdx, CDX_TYPES[0]),
        ] {
            let binary = matches!(format, CopyFormat::Png | CopyFormat::Pdf | CopyFormat::Cdx);
            let data = "exact payload\n";
            let copy = from_output(
                format,
                data.as_bytes().to_vec(),
                (!binary).then(|| data.into()),
                vec!["keep warning".into()],
            )
            .unwrap();
            assert!(copy.representations.iter().any(|r| r.kind == kind));
            assert!(
                copy.representations
                    .iter()
                    .all(|r| r.kind != super::super::NATIVE)
            );
            if format == CopyFormat::Cdx {
                assert_eq!(copy.representations.len(), CDX_TYPES.len());
                assert!(copy.representations.iter().all(|item| {
                    std::sync::Arc::ptr_eq(&copy.representations[0].data, &item.data)
                }));
            }
            assert_eq!(copy.text(), (!binary).then_some(data));
            assert_eq!(copy.notices, ["keep warning"]);
            for representation in copy.representations {
                assert_eq!(representation.bytes().unwrap(), data.as_bytes());
            }
        }
        for format in [
            CopyFormat::Inchi,
            CopyFormat::Rxn,
            CopyFormat::ReactionSmiles,
            CopyFormat::ChemDoodleReaction,
        ] {
            let copy = from_output(format, b"text".to_vec(), Some("text".into()), vec![]).unwrap();
            assert_eq!(copy.representations.len(), 1);
            assert_eq!(copy.representations[0].kind, TEXT);
        }
    }

    #[tokio::test]
    async fn molecular_formats_export_selected_graph_and_invalid_conversion_has_no_payload() {
        let engine = LocalEngine::default();
        let doc = molecule();
        let before = doc.clone();
        let smiles = prepare_as(engine.clone(), doc.clone(), CopyFormat::Smiles)
            .await
            .unwrap();
        assert_eq!(smiles.text(), Some("CO"));
        let mol = prepare_as(engine.clone(), doc.clone(), CopyFormat::Mol)
            .await
            .unwrap();
        let imported = crate::chemistry::molfile::read(mol.text().unwrap()).unwrap();
        assert_eq!(imported.molecule.ids.len(), 2);
        assert_eq!(doc, before);
        assert!(
            prepare_as(engine.clone(), doc, CopyFormat::Rxn)
                .await
                .is_err()
        );
        assert!(
            prepare_as(engine, Document::default(), CopyFormat::Mol)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn figure_payloads_are_the_chosen_format_without_a_hidden_editable_drawing() {
        let mut drawing = Document::default();
        drawing.arrows.push(crate::document::Arrow::new(
            1,
            Point::default(),
            Point::new(90., 0.),
            Default::default(),
            Default::default(),
        ));
        for format in [CopyFormat::Png, CopyFormat::Svg, CopyFormat::Pdf] {
            let copy = prepare_as(Default::default(), drawing.clone(), format)
                .await
                .unwrap();
            assert_eq!(copy.format, format);
            let bytes = copy.representations[0].bytes().unwrap();
            match format {
                CopyFormat::Png => {
                    let image = image::load_from_memory(&bytes).unwrap().into_rgba8();
                    assert_eq!(image.get_pixel(0, 0)[3], 0);
                    assert!(copy.notices.iter().any(|notice| notice.contains("dpi")));
                    assert!(copy.text().is_none());
                }
                CopyFormat::Svg => {
                    let svg = std::str::from_utf8(&bytes).unwrap();
                    let tree = roxmltree::Document::parse(svg).unwrap();
                    assert!(tree.root_element().has_tag_name("svg"));
                    assert_eq!(copy.text(), Some(svg));
                }
                CopyFormat::Pdf => {
                    assert!(bytes.starts_with(b"%PDF-"));
                    assert!(copy.text().is_none());
                }
                _ => unreachable!(),
            }
            assert!(
                copy.representations
                    .iter()
                    .all(|rep| rep.kind != super::super::NATIVE)
            );
            assert_eq!(
                copy.representations.len(),
                if format == CopyFormat::Svg { 2 } else { 1 }
            );
        }
    }

    #[tokio::test]
    async fn reaction_payload_retains_participants_and_export_warnings() {
        let mut doc = molecule();
        let product = doc.add_atom("O", Point::new(240., 0.));
        let arrow = doc.next_id();
        doc.arrows.push(crate::document::Arrow::new(
            arrow,
            Point::new(90., 0.),
            Point::new(170., 0.),
            Default::default(),
            Default::default(),
        ));
        let mut reaction = Reaction::new(arrow);
        reaction.reactants.push(Participant {
            atoms: vec![1, 2],
            coefficient: 1,
        });
        reaction.products.push(Participant {
            atoms: vec![product],
            coefficient: 1,
        });
        doc.reactions.push(reaction);
        let rxn = prepare_as(Default::default(), doc.clone(), CopyFormat::Rxn)
            .await
            .unwrap();
        let roundtrip = crate::chemistry::reaction::read_rxn(rxn.text().unwrap()).unwrap();
        assert_eq!(roundtrip.reactants.len(), 1);
        assert_eq!(roundtrip.products.len(), 1);
        assert!(
            rxn.notices
                .iter()
                .any(|notice| notice == crate::chemistry::reaction::EXPORT_WARNING)
        );
        let smiles = prepare_as(Default::default(), doc, CopyFormat::ReactionSmiles)
            .await
            .unwrap();
        assert_eq!(smiles.text(), Some("CO>>O"));
        assert_eq!(rxn.notices, smiles.notices);
    }
}
