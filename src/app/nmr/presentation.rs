//! Text and owner IDs derived once from an accepted, immutable report.
use super::{Nucleus, Report, plot, view};
use reshiki::document::Document;
use reshiki::nmr::Row;
use std::sync::Arc;

pub(super) struct Presentation {
    report: Arc<Report>,
    pub(super) rows: Vec<Site>,
    pub(super) value: String,
    pub(super) supported_count: usize,
    pub(super) sorted_ids: Vec<u64>,
    pub(super) plot_labels: plot::Labels,
}

pub(super) struct Site {
    pub(super) label: String,
    pub(super) description: String,
    pub(super) shift: String,
    pub(super) details: String,
}

impl Presentation {
    pub(super) fn new(report: &Arc<Report>, document: &Document) -> Self {
        let rows = report
            .rows
            .iter()
            .map(|row| {
                let label = row_label(report.nucleus, row, element(document, row));
                Site {
                    description: view::site_description(report.nucleus, row, &label),
                    label,
                    shift: row_shift(row),
                    details: row_details(row),
                }
            })
            .collect();
        let mut sorted_ids: Vec<_> = report
            .rows
            .iter()
            .filter(|row| row.shift().is_some())
            .map(|row| row.atom_id)
            .collect();
        sorted_ids.sort_unstable();
        sorted_ids.dedup();
        Self {
            report: Arc::clone(report),
            rows,
            value: format!("{}. {}", report.data_version, report.conditions),
            supported_count: report.supported_count(),
            sorted_ids,
            plot_labels: plot::Labels::new(report),
        }
    }

    pub(super) fn matches(&self, report: &Arc<Report>) -> bool {
        Arc::ptr_eq(&self.report, report)
    }
}

/// The element of the site's atom in `document`, which the report's chemistry
/// identity keeps unchanged while the report is current.
pub(super) fn element<'a>(document: &'a Document, row: &Row) -> Option<&'a str> {
    document.atom(row.atom_id).map(|atom| atom.element.as_str())
}

/// The site's name in the results table and accessible names: the atom's
/// element and number (`C #1`), plus its attached-H count for ¹H (`C #1 · 3H`).
/// Plot markers and drawing labels show the same `#1`.
pub(super) fn row_label(nucleus: Nucleus, row: &Row, element: Option<&str>) -> String {
    let atom = format!("{} #{}", element.unwrap_or("Atom"), row.atom_id);
    if nucleus == Nucleus::H1 {
        format!("{atom} · {}H", row.hydrogen_count)
    } else {
        atom
    }
}

pub(super) fn row_shift(row: &Row) -> String {
    row.statistics
        .as_ref()
        .map(|statistics| format!("{:.3}", statistics.median))
        .unwrap_or_else(|| "—".into())
}

pub(super) fn row_details(row: &Row) -> String {
    let shell = row
        .radius
        .map_or_else(|| "—".into(), |radius| radius.to_string());
    match &row.statistics {
        Some(statistics) => format!(
            "Shell {shell} · {} references · SD {:.3} ppm",
            statistics.support, statistics.standard_deviation
        ),
        None => format!("Shell {shell} · no references"),
    }
}

#[cfg(test)]
mod tests;
