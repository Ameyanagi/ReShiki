//! Capture CIF ORTEP figures through the same SVG/PNG/PDF export as the editor.
use anyhow::Context;
use reshiki_model::models3d::{self, Display, Look, Probability, Style};
use std::{fs, path::PathBuf};

fn main() -> anyhow::Result<()> {
    let output = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .context("Usage: ortep_qa OUTPUT_DIRECTORY")?,
    );
    fs::create_dir_all(&output)?;
    let (mut doc, _) = reshiki_io::crystal::read(
        "cif",
        include_str!("../../../tests/fixtures/crystal/cod-7033930.cif"),
    )?;
    models3d::set_display(&mut doc, 0, Display::ThreeD);
    for (name, style, molecule_only, look) in [
        ("ball-stick", Style::BallStick, false, Look::Flat),
        ("ortep-50", Style::Ellipsoids, true, Look::Flat),
        ("ortep-50-glossy", Style::Ellipsoids, true, Look::Glossy),
    ] {
        let model = doc.models.first_mut().context("CIF model")?;
        model.style = style;
        model.probability = Probability::Fifty;
        model.molecule_only = molecule_only;
        model.look = look;
        models3d::require_version(&mut doc);
        doc.validate().map_err(anyhow::Error::msg)?;
        fs::write(
            output.join(format!("{name}.rsk")),
            doc.file_json().map_err(anyhow::Error::msg)?,
        )?;
        for format in ["svg", "png", "pdf"] {
            let figure = reshiki_io::export::figure(&doc, format).map_err(anyhow::Error::msg)?;
            fs::write(output.join(format!("{name}.{format}")), figure.bytes)?;
        }
    }
    Ok(())
}
