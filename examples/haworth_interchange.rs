//! Check the live ChemDraw corpus through the application's import boundary.
use anyhow::Context;
use reshiki::engine::{ChemistryEngine, LocalEngine, Request};
use std::{fs, path::PathBuf};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let directory = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .context("Corpus directory required")?,
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(directory.join("manifest.json"))?)?;
    let structures = manifest["structures"]
        .as_array()
        .context("Missing structures")?;
    let mut results = Vec::new();
    let mut svg_options = resvg::usvg::Options::default();
    svg_options.fontdb_mut().load_system_fonts();
    for structure in structures {
        let id = structure["id"].as_str().context("Missing structure id")?;
        let name = structure["name"]
            .as_str()
            .context("Missing structure name")?;
        let source = if let Some(template) = reshiki::templates::LIBRARY
            .iter()
            .find(|t| t.name.replace(" · Haworth", "") == name)
        {
            template.document.clone()
        } else {
            match id {
                "carbon-5-ring" => reshiki::haworth::Ring::Five.document(42., false),
                "carbon-6-ring" => reshiki::haworth::Ring::Six.document(42., false),
                _ => anyhow::bail!("Missing source template {id}"),
            }
        };
        let output = directory.join(id).join("reshiki");
        fs::create_dir_all(&output)?;
        let svg = resvg::usvg::Tree::from_data(
            &fs::read(directory.join(id).join("svg.svg"))?,
            &svg_options,
        )?;
        let size = svg.size().to_int_size();
        anyhow::ensure!(
            size.width() <= 4096 && size.height() <= 4096,
            "SVG too large"
        );
        let mut pixels = resvg::tiny_skia::Pixmap::new(size.width(), size.height())
            .context("SVG bitmap allocation")?;
        pixels.fill(resvg::tiny_skia::Color::WHITE);
        resvg::render(
            &svg,
            resvg::tiny_skia::Transform::identity(),
            &mut pixels.as_mut(),
        );
        anyhow::ensure!(
            pixels
                .data()
                .chunks_exact(4)
                .any(|rgba| rgba != [255, 255, 255, 255]),
            "Empty SVG for {id}"
        );
        pixels.save_png(directory.join(id).join("svg-render-check.png"))?;
        let xml = reshiki::exchange::drawing::write(&source, Default::default())?;
        fs::write(output.join("reshiki.cdxml"), &xml)?;
        fs::write(
            output.join("reshiki.cdx"),
            reshiki::exchange::to_cdx(&xml).map_err(anyhow::Error::msg)?,
        )?;
        fs::write(
            output.join("reshiki.svg"),
            reshiki::export::drawing(&source, "svg").map_err(anyhow::Error::msg)?,
        )?;
        for (format, file) in [
            ("cdxml", "cdxml.cdxml"),
            ("cdx", "cdx.cdx"),
            ("mol", "mol.mol"),
            ("mol", "mol-v2000.mol"),
        ] {
            let bytes = fs::read(directory.join(id).join(file))?;
            let text = if format == "cdx" {
                reshiki::exchange::from_cdx(&bytes).map_err(anyhow::Error::msg)
            } else {
                String::from_utf8(bytes).map_err(anyhow::Error::from)
            };
            let response = match text {
                Ok(text) => {
                    LocalEngine::default()
                        .execute(Request::import(
                            if format == "cdx" { "cdxml" } else { format },
                            &text,
                        ))
                        .await
                }
                Err(error) => Err(error.to_string()),
            };
            let value = match response {
                Ok(response) => {
                    serde_json::json!({"id": id, "file": file, "analysis": response.analysis, "document": response.document})
                }
                Err(error) => serde_json::json!({"id": id, "file": file, "error": error}),
            };
            println!(
                "{id} {file}: {}",
                value.get("error").unwrap_or(&value["analysis"]["inchikey"])
            );
            results.push(value);
        }
    }
    fs::write(
        directory.join("reshiki-import.json"),
        serde_json::to_vec_pretty(&results)?,
    )?;
    let gallery = directory.join("all-haworth-chemdraw.cdxml");
    if gallery.exists() {
        let imported = LocalEngine::default()
            .execute(Request::import("cdxml", &fs::read_to_string(gallery)?))
            .await
            .map_err(anyhow::Error::msg)?;
        fs::write(
            directory.join("Haworth interchange.rsk"),
            serde_json::to_vec_pretty(&imported.document.context("Missing gallery drawing")?)?,
        )?;
    }
    Ok(())
}
