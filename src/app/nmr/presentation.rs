//! Text and owner IDs derived once from an accepted, immutable report.
use super::{Nucleus, Report, plot, view};
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
    pub(super) fn new(report: &Arc<Report>) -> Self {
        let rows = report
            .rows
            .iter()
            .map(|row| Site {
                label: row_label(report.nucleus, row),
                description: view::site_description(report.nucleus, row),
                shift: row_shift(row),
                details: row_details(row),
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

pub(super) fn row_label(nucleus: Nucleus, row: &Row) -> String {
    if nucleus == Nucleus::H1 {
        format!(
            "#{} · {} H{}",
            row.atom_id,
            row.hydrogen_count,
            if row.hydrogen_count > 1 { " group" } else { "" }
        )
    } else {
        format!("C #{}", row.atom_id)
    }
}

pub(super) fn row_shift(row: &Row) -> String {
    row.statistics
        .as_ref()
        .map(|statistics| format!("{:.3}", statistics.median))
        .unwrap_or_else(|| "—".into())
}

pub(super) fn row_details(row: &Row) -> String {
    format!(
        "Sphere {} · Refs {} · Obs. SD {} ppm",
        row.radius
            .map(|radius| radius.to_string())
            .unwrap_or_else(|| "—".into()),
        row.statistics
            .as_ref()
            .map(|statistics| statistics.support.to_string())
            .unwrap_or_else(|| "—".into()),
        row.statistics
            .as_ref()
            .map(|statistics| format!("{:.3}", statistics.standard_deviation))
            .unwrap_or_else(|| "—".into()),
    )
}

#[cfg(test)]
mod tests;
