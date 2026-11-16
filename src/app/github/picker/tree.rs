//! The picker's Tree view: the whole repository as folders that open in
//! place, one row each, and a preview of the selected file beside them.
//! Searching keeps the matches in the tree with their folders opened.
use super::*;
use crate::app::icons::{Glyph, Icon};
use iced::widget::column;

/// A row's height, to tell which rows are on screen.
const ROW: f32 = 28.;
const TREE: &str = "github-tree";
const TRUNCATED: &str = "Only part of this repository is listed";

pub(in crate::app::github) struct Row<'a> {
    pub(in crate::app::github) entry: &'a Entry,
    pub(in crate::app::github) depth: usize,
    pub(in crate::app::github) open: bool,
    /// The folders and files listed in a folder.
    items: usize,
}

/// The rows on screen, by index.
pub(super) fn on_screen(view: Option<(f32, f32)>, len: usize) -> std::ops::Range<usize> {
    let (top, height) = view.unwrap_or((0., 420.));
    let first = (top / ROW) as usize;
    let last = ((top + height) / ROW).ceil() as usize + 1;
    first.min(len)..last.min(len)
}

impl Picker {
    /// Folders first, then files, each by name, the open folders' below
    /// them. Searching, the folders and files whose names match, with the
    /// folders above them opened.
    pub(in crate::app::github) fn rows(&self) -> Vec<Row<'_>> {
        let Some((entries, _)) = &self.whole else {
            return Vec::new();
        };
        let mut children: HashMap<&str, Vec<&Entry>> = HashMap::new();
        for entry in entries.iter().filter(|e| e.folder || gh::listed(&e.name)) {
            children.entry(parent(&entry.name)).or_default().push(entry);
        }
        for listed in children.values_mut() {
            listed.sort_by_cached_key(|entry| (!entry.folder, last(&entry.name).to_lowercase()));
        }
        let query = self.query();
        let kept: Option<HashSet<&str>> = (!query.is_empty()).then(|| {
            let mut kept = HashSet::new();
            for entry in children.values().flatten() {
                if last(&entry.name).to_lowercase().contains(&query) {
                    let mut path = entry.name.as_str();
                    while !path.is_empty() && kept.insert(path) {
                        path = parent(path);
                    }
                }
            }
            kept
        });
        let mut rows = Vec::new();
        let mut stack: Vec<(&Entry, usize)> = (children.get("").into_iter().flatten().rev())
            .map(|entry| (*entry, 0))
            .collect();
        while let Some((entry, depth)) = stack.pop() {
            if kept
                .as_ref()
                .is_some_and(|kept| !kept.contains(entry.name.as_str()))
            {
                continue;
            }
            let below = children.get(entry.name.as_str()).filter(|_| entry.folder);
            let open = entry.folder && (kept.is_some() || self.opened.contains(&entry.name));
            rows.push(Row {
                entry,
                depth,
                open,
                items: below.map_or(0, Vec::len),
            });
            if open {
                stack.extend(below.into_iter().flatten().rev().map(|e| (*e, depth + 1)));
            }
        }
        rows
    }

    /// Scrolls the tree so the selected row shows.
    pub(super) fn reveal_row(&mut self) -> Task<Message> {
        let rows = self.rows();
        let Some(at) = (rows.iter()).position(|row| Some(&row.entry.name) == self.row.as_ref())
        else {
            return Task::none();
        };
        let (top, height) = self.tree_view.unwrap_or((0., 420.));
        let y = at as f32 * ROW;
        let to = if y < top {
            y
        } else if y + ROW > top + height {
            y + ROW - height
        } else {
            return Task::none();
        };
        self.tree_view = Some((to, height));
        iced::widget::operation::scroll_to(
            TREE,
            iced::widget::operation::AbsoluteOffset { x: 0., y: to },
        )
    }
}

impl App {
    /// The rows on screen, `+ New drawing` and `+ New folder` last, and the
    /// selected file's preview pane.
    pub(super) fn picker_tree<'a>(&'a self, picker: &'a Picker) -> Element<'a, Message> {
        let Some((_, truncated)) = &picker.whole else {
            return container(text("Listing…").size(12).style(muted_text))
                .center(Length::Fill)
                .into();
        };
        let signed_in = self.github.client().is_some();
        let rows = picker.rows();
        let range = on_screen(picker.tree_view, rows.len());
        // The rows off screen are only room to scroll.
        let mut list = column![Space::new().height(range.start as f32 * ROW)];
        for (at, row) in rows.iter().enumerate().take(range.end).skip(range.start) {
            list = list.push(tree_row(picker, at, row, signed_in));
        }
        list = list.push(Space::new().height((rows.len() - range.end) as f32 * ROW));
        let query = picker.query();
        if rows.is_empty() {
            let empty = match query.is_empty() {
                true => "Empty",
                false => "No match in this repository.",
            };
            list = list.push(container(text(empty).size(13).style(muted_text)).padding(10));
        }
        if picker.can_create() && query.is_empty() {
            list = list.push(new_item_row(picker, false));
            list = list.push(new_item_row(picker, true));
        }
        if *truncated {
            list = list.push(note(TRUNCATED));
        }
        if picker.signed_out {
            list = list.push(note(SIGNED_OUT_HINT));
        }
        let list = scrollable(list)
            .id(TREE)
            .height(Length::Fill)
            .on_scroll(|view| {
                let top = view.absolute_offset().y;
                msg(Action::TreeScrolled(top, view.bounds().height))
            });
        row![
            container(list)
                .padding(4)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(thumb_frame),
            tree_pane(picker, signed_in),
        ]
        .spacing(12)
        .height(Length::Fill)
        .into()
    }
}

fn note<'a>(line: &'static str) -> Element<'a, Message> {
    container(text(line).size(11).style(muted_text))
        .padding([6, 8])
        .into()
}

/// Chevron, icon, name, the format of a file other than a drawing, and who
/// changed it when, muted at the end. Signed out, names and formats only.
fn tree_row<'a>(
    picker: &'a Picker,
    at: usize,
    row: &Row<'a>,
    signed_in: bool,
) -> Element<'a, Message> {
    let entry = row.entry;
    let name = last(&entry.name);
    let chevron = match (entry.folder, row.open) {
        (false, _) => "",
        (true, true) => "▾",
        (true, false) => "▸",
    };
    let icon = if entry.folder {
        Icon::Folder
    } else {
        Icon::File
    };
    let mut content = row![
        Space::new().width(row.depth as f32 * 18.),
        text(chevron).size(11).width(12).style(muted_text),
        iced::widget::canvas(Glyph(icon, true)).width(16).height(16),
        text(name.to_owned())
            .size(13)
            .wrapping(text::Wrapping::None),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    let mut spoken = match entry.folder {
        true => format!("{name}, folder"),
        false => name.to_owned(),
    };
    if let Some(label) = gh::format_label(name) {
        content = content.push(format_label(label));
    }
    content = content.push(Space::new().width(Length::Fill));
    let meta = match entry.folder {
        _ if !signed_in => None,
        true => Some(match row.items {
            0 => "Empty".to_owned(),
            1 => "1 item".to_owned(),
            n => format!("{n} items"),
        }),
        false => (picker.changed.get(&entry.name))
            .map(|(who, at)| format!("{who} · {}", super::super::versions::tile_time(*at))),
    };
    if let Some(meta) = meta {
        spoken = format!("{spoken}, {meta}");
        content = content.push(text(meta).size(12).style(muted_text));
    }
    let selected = picker.row.as_ref() == Some(&entry.name);
    let mut button = reshiki::accessibility::button(format!("github-row-{at}"), spoken, content)
        .width(Length::Fill)
        .height(ROW)
        .padding([0, 8])
        .style(crate::app::workspace::control(selected))
        .on_press(msg(Action::Row(entry.name.clone())));
    if entry.folder {
        return button.expanded(row.open).into();
    }
    button = button.checked(selected);
    file_menu(picker, entry.name.clone(), button.into())
}

/// The selected file: its preview, name, path, who changed it when and its
/// format, and whether it opens read only.
fn tree_pane<'a>(picker: &'a Picker, signed_in: bool) -> Element<'a, Message> {
    let content: Element<'a, Message> = match picker.selected_file() {
        None => container(
            text("Select a drawing to preview it.")
                .size(12)
                .style(muted_text),
        )
        .center_x(Length::Fill)
        .padding([24, 0])
        .into(),
        Some((path, sha)) => {
            let name = last(&path).to_owned();
            let changed = (picker.changed.get(&path))
                .filter(|_| signed_in)
                .map(|(who, at)| format!("{who} · {}", super::super::versions::tile_time(*at)));
            let facts: Vec<String> = (changed.into_iter())
                .chain(gh::format_label(&name).map(str::to_owned))
                .collect();
            let read_only =
                !name.ends_with(".rsk") || picker.repo.as_ref().is_some_and(|r| !r.push);
            let mut pane = column![].spacing(4);
            if signed_in {
                pane = pane.push(
                    container(drawing(picker, &sha))
                        .width(Length::Fill)
                        .height(136)
                        .padding(6)
                        .style(thumb_frame),
                );
            }
            pane = pane
                .push(
                    text(name)
                        .size(13)
                        .font(iced::Font {
                            weight: iced::font::Weight::Semibold,
                            ..iced::Font::with_name(reshiki::style::ui_font_family())
                        })
                        .wrapping(text::Wrapping::WordOrGlyph),
                )
                .push(
                    text(path)
                        .size(11)
                        .style(muted_text)
                        .wrapping(text::Wrapping::WordOrGlyph),
                );
            if !facts.is_empty() {
                pane = pane.push(text(facts.join(" · ")).size(11).style(muted_text));
            }
            if read_only {
                pane = pane.push(
                    row![
                        iced::widget::canvas(Glyph(Icon::Eye, true))
                            .width(14)
                            .height(14),
                        text("Read only: Open makes a local copy")
                            .size(11)
                            .style(muted_text),
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center),
                );
            }
            pane.into()
        }
    };
    container(content)
        .width(200)
        .height(Length::Fill)
        .padding(10)
        .style(thumb_frame)
        .into()
}

/// `+ New drawing` or `+ New folder` as the last rows; clicked, a name field
/// in place, `.rsk` fixed after a drawing's name. Enter creates it.
fn new_item_row(picker: &Picker, folder: bool) -> Element<'_, Message> {
    let label = if folder { "New folder" } else { "New drawing" };
    if let Some((naming, name)) = &picker.new_item
        && *naming == folder
    {
        let field =
            reshiki::accessibility::text_input(ITEM_NAME, format!("{label} name"), "Name", name)
                .style(crate::appearance::input_style)
                .on_input(|name| msg(Action::NewItemName(name)))
                .on_submit(msg(Action::CreateItem))
                .size(12)
                .padding(4)
                .width(200);
        let suffix = (!folder).then(|| text(".rsk").size(12).style(muted_text));
        return row![Space::new().width(26), field]
            .push(suffix)
            .spacing(4)
            .height(ROW)
            .align_y(Alignment::Center)
            .into();
    }
    reshiki::accessibility::button(
        if folder {
            "github-new-folder"
        } else {
            "github-new-drawing"
        },
        label,
        row![Space::new().width(12), text(format!("+ {label}")).size(13)]
            .spacing(6)
            .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .height(ROW)
    .padding([0, 8])
    .style(crate::app::workspace::control(false))
    .on_press(msg(Action::NewItem(folder)))
    .into()
}
