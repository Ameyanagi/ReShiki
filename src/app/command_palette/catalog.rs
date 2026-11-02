//! The palette's commands: user intents, each run through the message, hotkey
//! or placement path that its button, menu or key already uses. Keys come
//! from the same tables that menus, tooltips and Help read.
use super::super::{
    App, InspectorTab, Message, abbreviations, assistant, cleanup, generic, import, inspector,
    keyboard_drawing, models3d, naming, nmr, numeric_transforms,
    numeric_transforms::Field,
    optimization, pages, palettes, printing, reaction_mapping, reactions,
    shortcuts::{self, TabKey, keys, tab_keys},
    tabs, view_settings,
};
use crate::canvas::Tool;
use iced::keyboard::Modifiers;
use reshiki::{
    arrows::Preset as ArrowPreset,
    bonds::{BondPreset, DoublePosition},
    chains::ChainMode,
    editing::{Arrange, Transform},
    graphics::GraphicKind,
    models3d::Display,
    rings::Preset as RingPreset,
    typography::{Script, StyleChange, TextAlign},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Category {
    Insert,
    Bond,
    Atom,
    Edit,
    Analyze,
    File,
    View,
    Tool,
}

impl Category {
    pub(crate) const ALL: [Self; 8] = [
        Self::Insert,
        Self::Bond,
        Self::Atom,
        Self::Edit,
        Self::Analyze,
        Self::File,
        Self::View,
        Self::Tool,
    ];
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Insert => "Insert",
            Self::Bond => "Bond",
            Self::Atom => "Atom",
            Self::Edit => "Edit",
            Self::Analyze => "Analyze",
            Self::File => "File",
            Self::View => "View",
            Self::Tool => "Tool",
        }
    }
}

/// What must be targeted before a command can run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Need {
    Nothing,
    Blocked(&'static str),
    Selection,
    /// A selection the Scale field can resize: none of it shown only in 3D.
    Resizable,
    Flippable,
    Molecule,
    Bond,
    IonicBond,
    ChargedRing,
    Atom,
    OneAtom,
    NmrReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ring {
    Size(u8, bool),
    Preset(RingPreset),
}

impl Ring {
    /// The ring hotkeys at an atom and at a bond, from `hotkeys::ring_edit`.
    pub(crate) fn keys(self) -> (Option<&'static str>, Option<&'static str>) {
        match self {
            Self::Size(3, false) => (Some("v"), Some("v")),
            Self::Size(4, false) => (Some("u"), Some("4")),
            Self::Size(5, false) => (Some("7"), Some("5")),
            Self::Size(6, false) => (Some("6"), Some("6")),
            Self::Size(7, false) => (None, Some("7")),
            Self::Size(8, false) => (None, Some("8")),
            Self::Preset(RingPreset::Benzene) => (Some("a"), Some("a")),
            Self::Preset(RingPreset::ChairUp) => (None, Some("9")),
            Self::Preset(RingPreset::ChairDown) => (None, Some("0")),
            Self::Preset(RingPreset::Cyclopentadiene) => (None, Some("z")),
            _ => (None, None),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Run {
    /// Messages sent in order, as their buttons send them.
    Send(Vec<Message>),
    /// Show fresh formula and mass for the selection or the whole drawing.
    MolecularProperties,
    /// Restyle the targeted bonds, or choose that bond tool.
    Bond(BondPreset),
    /// A selection message, after selecting the pointed bond.
    OnBonds(Message),
    /// A selection message, after selecting the pointed atom.
    OnAtoms(Message),
    /// A contextual atom key, as if pressed over the target atom.
    AtomKey(&'static str),
    /// Relabel the target atom, or choose the atom tool for this element.
    Element(&'static str),
    /// Label the target atom as this group, or add the group at the center.
    Group(String),
    Ring(Ring),
    /// A library template by index and its id, checked when run.
    Template(usize, String),
    SetCharge(i32),
    Name(String),
    Smiles(String),
    SaveAs(String),
    OpenNamed(String),
    SaveAndClose,
    DiscardAndClose,
    CloseWindow,
}

#[derive(Debug, Clone)]
pub(crate) struct Command {
    pub name: String,
    pub keys: String,
    pub category: Category,
    pub synonyms: String,
    pub detail: String,
    /// Vim-style ex commands; an exact alias ranks first.
    pub aliases: &'static [&'static str],
    pub need: Need,
    pub run: Run,
}

impl Command {
    pub(crate) fn new(
        name: impl Into<String>,
        category: Category,
        detail: impl Into<String>,
        synonyms: &str,
        run: Run,
    ) -> Self {
        let keys = match &run {
            Run::Send(messages) => match messages.as_slice() {
                [message] => shortcuts::label(message).unwrap_or_default(),
                _ => String::new(),
            },
            Run::OnBonds(message) | Run::OnAtoms(message) => {
                shortcuts::label(message).unwrap_or_default()
            }
            _ => String::new(),
        };
        Self {
            name: name.into(),
            keys,
            category,
            synonyms: synonyms.into(),
            detail: detail.into(),
            aliases: &[],
            need: Need::Nothing,
            run,
        }
    }
    fn keys(mut self, keys: impl Into<String>) -> Self {
        self.keys = keys.into();
        self
    }
    fn need(mut self, need: Need) -> Self {
        self.need = need;
        self
    }
    fn alias(mut self, aliases: &'static [&'static str]) -> Self {
        self.aliases = aliases;
        self
    }
    /// The key hint: vim aliases first, then the chord.
    pub(crate) fn hint(&self) -> String {
        let aliases = self
            .aliases
            .iter()
            .map(|a| format!(":{a}"))
            .collect::<Vec<_>>()
            .join(" ");
        match (aliases.is_empty(), self.keys.is_empty()) {
            (true, _) => self.keys.clone(),
            (false, true) => aliases,
            (false, false) => format!("{aliases} · {}", self.keys),
        }
    }
}

impl super::super::help::search::Searchable for Command {
    fn keys(&self) -> &str {
        &self.keys
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn synonyms(&self) -> String {
        self.synonyms.clone()
    }
    fn description(&self) -> &str {
        &self.detail
    }
    fn scope(&self) -> &str {
        self.category.label()
    }
}

fn send(
    name: impl Into<String>,
    category: Category,
    detail: impl Into<String>,
    synonyms: &str,
    message: Message,
) -> Command {
    Command::new(name, category, detail, synonyms, Run::Send(vec![message]))
}

/// Single-character keys, for reverse lookups in the hotkey tables.
const KEYS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789+-";

fn key_list(matches: impl Fn(&str) -> bool) -> String {
    KEYS.chars()
        .map(String::from)
        .filter(|k| matches(k))
        .collect::<Vec<_>>()
        .join(" / ")
}

/// Atom hotkeys that are not an element, a group or a ring.
const ATOM_KEYS: &[(&str, &str, &str, &str)] = &[
    (
        "+",
        "Increase charge",
        "Add one positive formal charge",
        "cation plus positive",
    ),
    (
        "-",
        "Decrease charge",
        "Add one negative formal charge",
        "anion minus negative",
    ),
    (
        "1",
        "Extend chain",
        "Grow a carbon along the chain",
        "grow add carbon sprout",
    ),
    ("0", "Add branch", "Add a carbon branch", "grow substituent"),
    (
        "2",
        "Add carbonyl",
        "Make a terminal acetyl group or an internal ketone",
        "ketone c=o oxo",
    ),
    (
        "4",
        "Grow through a wedge",
        "Add a carbon through a wedged bond",
        "stereo 3d",
    ),
    (
        "5",
        "Grow through a hash",
        "Add a carbon through a hashed bond",
        "stereo 3d",
    ),
    (
        "8",
        "Add =CH₂",
        "Add an exocyclic methylene",
        "methylene alkene",
    ),
    (
        "9",
        "Dimethyl",
        "Add two methyl groups",
        "gem-dimethyl isopropyl",
    ),
    (
        "K",
        "Add tert-butyl",
        "Grow a tert-butyl group",
        "tbu t-butyl",
    ),
    ("k", "Add sulfonyl", "Add a sulfonyl group", "so2 sulfone"),
    (
        "z",
        "Add alkyne",
        "Add a carbon through a triple bond",
        "triple ethynyl",
    ),
    ("d", "Deuterium", "Change the atom to D (²H)", "isotope"),
    (
        "r",
        "Variable R",
        "Label the atom R",
        "substituent generic markush",
    ),
    (
        "x",
        "Variable X",
        "Label the atom X",
        "halide generic markush",
    ),
    (
        "j",
        "Cp ligand",
        "Add a tilted Cp ligand",
        "cyclopentadienyl hapto organometallic",
    ),
    (
        "J",
        "Arene ligand",
        "Add a tilted arene ligand",
        "hapto organometallic",
    ),
    (
        "Enter",
        "Edit atom label",
        "Edit the atom's label",
        "rename type label",
    ),
];

/// English element name for `symbol`, read from OPSIN's element token list
/// (which names every element except hydrogen).
fn element_name(symbol: &str) -> Option<String> {
    if symbol == "H" {
        return Some("Hydrogen".into());
    }
    let tag = format!("value=\"[{symbol}]\"");
    include_str!("../../../crates/opsin/resources/xml/elementaryAtoms.xml")
        .lines()
        .find(|line| line.contains(&tag))
        .and_then(|line| line.split('>').nth(1)?.split(['|', '<']).next())
        .map(|name| {
            let mut chars = name.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect()
            })
        })
}

/// Every command; built when the palette opens, so toggles describe their
/// current state and the template library is current.
pub(crate) fn build(app: &App) -> Vec<Command> {
    let mut list = inserts(app);
    list.extend(bonds());
    list.extend(atoms());
    list.extend(edits());
    list.extend(analysis());
    list.extend(files());
    list.extend(view(app));
    list.extend(tools());
    if cfg!(target_arch = "wasm32") {
        list.retain(|command| match &command.run {
            Run::Send(messages) => !messages.iter().any(browser_unavailable),
            _ => true,
        });
    }
    list
}

/// Commands the browser build cannot run: the assistant needs a local Codex
/// process, and printing a native dialog.
fn browser_unavailable(message: &Message) -> bool {
    matches!(
        message,
        Message::Assistant(_) | Message::Printing(_) | Message::Inspector(InspectorTab::Assistant)
    )
}

fn inserts(app: &App) -> Vec<Command> {
    use Category::Insert;
    let detail = "Attach to the selected or pointed atom or bond, or add at the center of the view";
    let mut list = vec![];
    for (size, name) in [
        (3, "Cyclopropane"),
        (4, "Cyclobutane"),
        (5, "Cyclopentane"),
        (6, "Cyclohexane"),
        (7, "Cycloheptane"),
        (8, "Cyclooctane"),
    ] {
        list.push(Command::new(
            name,
            Insert,
            detail,
            &format!("{size}-membered ring cycloalkane saturated ring{size}"),
            Run::Ring(Ring::Size(size, false)),
        ));
    }
    list.push(Command::new(
        "Benzene",
        Insert,
        detail,
        "phenyl aromatic ring c6h6 arene",
        Run::Ring(Ring::Preset(RingPreset::Benzene)),
    ));
    list.push(Command::new(
        "Benzene (aromatic circle)",
        Insert,
        detail,
        "delocalized robinson aromatic ring",
        Run::Ring(Ring::Size(6, true)),
    ));
    for (preset, synonyms) in [
        (RingPreset::ChairUp, "cyclohexane chair conformation"),
        (RingPreset::ChairDown, "cyclohexane chair conformation"),
        (RingPreset::Cyclopentadiene, "cp diene cyclopentadienyl"),
        (RingPreset::HaworthFive, "furanose sugar haworth"),
        (RingPreset::HaworthSix, "pyranose sugar haworth glucose"),
    ] {
        list.push(Command::new(
            preset.to_string(),
            Insert,
            detail,
            synonyms,
            Run::Ring(Ring::Preset(preset)),
        ));
    }
    for (index, template) in app.templates.library.iter().enumerate() {
        list.push(Command::new(
            template.name.clone(),
            Insert,
            format!("Template · {}", template.group),
            &format!("template {}", template.keywords.join(" ")),
            Run::Template(index, template.id.clone()),
        ));
    }
    for example in import::EXAMPLES {
        list.push(send(
            example.0,
            Insert,
            "Insert this example molecule",
            "example molecule",
            Message::Example(example.1),
        ));
    }
    for group in reshiki::abbreviations::groups() {
        let keys = key_list(|k| {
            reshiki::hotkeys::atom_label(k).map(|(l, _)| l) == Some(group.label.as_str())
        });
        list.push(
            Command::new(
                group.label.clone(),
                Insert,
                "Label the selected or pointed atom as this group, or add it at the center",
                &format!(
                    "group abbreviation substituent {} {} {}",
                    group.category,
                    group.names.join(" "),
                    group.aliases.join(" ")
                ),
                Run::Group(group.label),
            )
            .keys(keys),
        );
    }
    let terminal = app.command_target().atom().is_some_and(|id| {
        app.tab
            .doc
            .bonds
            .iter()
            .filter(|bond| bond.a == id || bond.b == id)
            .count()
            == 1
    });
    for symbol in reshiki::editing::ELEMENTS {
        let name = element_name(symbol).unwrap_or_else(|| symbol.to_string());
        let mut synonyms = format!("element atom {symbol}");
        if *symbol == "O" && terminal {
            synonyms.push_str(" hydroxy hydroxyl");
        }
        list.push(
            Command::new(
                if name == *symbol {
                    format!("Element {symbol}")
                } else {
                    format!("{name} ({symbol})")
                },
                Insert,
                "Change the selected or pointed atom, or choose the atom tool",
                &synonyms,
                Run::Element(symbol),
            )
            .keys(key_list(|k| {
                reshiki::hotkeys::atom_label(k) == Some((symbol, reshiki::atom_text::Mode::Auto))
            })),
        );
    }
    list
}

fn bonds() -> Vec<Command> {
    use Category::Bond;
    let mut list: Vec<_> =
        BondPreset::ALL
            .into_iter()
            .map(|preset| {
                let name = match preset {
                    BondPreset::Dotted => "Hydrogen bond".into(),
                    _ => format!("{} bond", preset.name()),
                };
                let synonyms = match preset {
                    BondPreset::Single => "order",
                    BondPreset::Double => "order alkene",
                    BondPreset::Triple => "order alkyne",
                    BondPreset::Quadruple => "order",
                    BondPreset::Wedge => "stereo up bold wedge chirality",
                    BondPreset::HashedWedge => "stereo down hash dash wedge chirality",
                    BondPreset::HollowWedge => "stereo open wedge",
                    BondPreset::Wavy => "squiggly undefined stereo",
                    BondPreset::Bold => "thick",
                    BondPreset::Dashed => "dotted coordination dash",
                    BondPreset::Dotted => "dotted h-bond",
                    BondPreset::Hashed => "hash",
                    BondPreset::DashedDouble => "aromatic delocalized resonance partial",
                    BondPreset::DoubleDashed => "delocalized partial",
                    BondPreset::BoldDouble => "thick",
                    BondPreset::CrossedDouble => "either e z stereo",
                    BondPreset::Dative => "coordinate arrow",
                };
                Command::new(
                name,
                Bond,
                format!(
                    "Make the selected or pointed bonds {}; with no bond, choose that bond tool",
                    preset.name().to_lowercase()
                ),
                synonyms,
                Run::Bond(preset),
            )
            .keys(key_list(|k| reshiki::hotkeys::bond_preset(k) == Some(preset)))
            })
            .collect();
    for (position, key, name) in [
        (DoublePosition::Left, "l", "Double bond line left"),
        (DoublePosition::Center, "c", "Double bond centered"),
        (DoublePosition::Right, "r", "Double bond line right"),
    ] {
        list.push(
            Command::new(
                name,
                Bond,
                "Place the double bond's second line",
                "placement offset position",
                Run::OnBonds(Message::BondPosition(position)),
            )
            .keys(key)
            .need(Need::Bond),
        );
    }
    list.push(
        Command::new(
            "Reverse bond direction",
            Bond,
            "Swap the ends of wedges and other directional bonds",
            "flip wedge swap",
            Run::OnBonds(Message::ReverseBonds),
        )
        .need(Need::Bond),
    );
    list.push(
        send(
            "Make ionic",
            Bond,
            "Separate selected metal–heteroatom single bonds into charged ions",
            "disconnect salt sodium potassium calcium ionic bond",
            Message::MakeIonic,
        )
        .need(Need::IonicBond),
    );
    list.push(
        send(
            "Centre charge in ring",
            Bond,
            "Toggle the net charge at the centre of a selected aromatic ion circle",
            "center charge cyclopentadienide tropylium delocalized",
            Message::CenterRingCharge,
        )
        .need(Need::ChargedRing),
    );
    list.push(
        Command::new(
            "Bring bond forward",
            Bond,
            "Draw the bond on top at a crossing",
            "crossing depth front",
            Run::OnBonds(Message::BondDepth(true)),
        )
        .need(Need::Bond),
    );
    list.push(
        Command::new(
            "Send bond behind",
            Bond,
            "Draw the bond underneath at a crossing",
            "crossing depth back",
            Run::OnBonds(Message::BondDepth(false)),
        )
        .need(Need::Bond),
    );
    list
}

fn atoms() -> Vec<Command> {
    use Category::Atom;
    let mut list: Vec<_> = ATOM_KEYS
        .iter()
        .map(|(key, name, detail, synonyms)| {
            Command::new(*name, Atom, *detail, synonyms, Run::AtomKey(key))
                .keys(match *key {
                    "Enter" => keys(Modifiers::empty(), "Enter"),
                    key => key.into(),
                })
                .need(Need::OneAtom)
        })
        .collect();
    list.push(
        Command::new(
            "Remove charge",
            Atom,
            "Make the atom neutral",
            "neutral zero charge",
            Run::SetCharge(0),
        )
        .need(Need::OneAtom),
    );
    for (value, name) in [(1, "Radical"), (0, "Remove radical")] {
        list.push(
            Command::new(
                name,
                Atom,
                "Set the radical electrons of the selected or pointed atoms",
                "unpaired electron dot",
                Run::OnAtoms(Message::AtomRadical(value)),
            )
            .need(Need::Atom),
        );
    }
    list
}

fn edits() -> Vec<Command> {
    use Category::Edit;
    let selection = |name: &str, detail: &str, synonyms: &str, message| {
        send(name, Edit, detail, synonyms, message).need(Need::Selection)
    };
    let mut list = vec![
        send(
            "Undo",
            Edit,
            "Step back through your edits",
            "revert history",
            Message::Undo,
        )
        .alias(&["u", "undo"]),
        send(
            "Redo",
            Edit,
            "Step forward through your edits",
            "history",
            Message::Redo,
        )
        .alias(&["red", "redo"]),
        selection(
            "Delete",
            "Delete the selection",
            "remove erase",
            Message::Delete,
        ),
        selection(
            "Duplicate",
            "Copy the selection beside itself",
            "clone copy",
            Message::Duplicate,
        ),
        send(
            "Select all",
            Edit,
            "Select everything",
            "everything",
            Message::SelectAll,
        ),
        send(
            "Invert selection",
            Edit,
            "Select everything that is not selected",
            "reverse inverse",
            Message::InvertSelection,
        ),
        selection(
            "Group",
            "Group the selection so it moves together",
            "combine",
            Message::Group,
        ),
        selection("Ungroup", "Split a group", "split", Message::Ungroup),
        selection(
            "Flip horizontally",
            "Mirror the selection left to right",
            "mirror reflect",
            Message::Transform(Transform::FlipHorizontal),
        )
        .need(Need::Flippable),
        selection(
            "Flip vertically",
            "Mirror the selection top to bottom",
            "mirror reflect",
            Message::Transform(Transform::FlipVertical),
        )
        .need(Need::Flippable),
        selection(
            "Clean up structure",
            "Preview a cleaned-up layout",
            "tidy beautify layout",
            Message::Cleanup(cleanup::Action::Begin),
        )
        .keys(keys(Modifiers::COMMAND | Modifiers::SHIFT, "K")),
        selection(
            "3D optimization",
            "Generate an optimized 3D conformer",
            "conformer mmff uff geometry optimize",
            Message::Optimization(optimization::Action::Begin),
        ),
        selection(
            "Show as 3D model",
            "Show the selected molecule as a ball & stick model",
            "ball stick conformer structure view",
            Message::Models3d(models3d::Action::Show(Display::ThreeD)),
        ),
        selection(
            "Show as 2D and 3D",
            "Show the 3D model beside the 2D drawing",
            "both ball stick conformer structure view",
            Message::Models3d(models3d::Action::Show(Display::Both)),
        ),
        selection(
            "Show as 2D drawing",
            "Show the selected molecule as its 2D drawing",
            "structure view flat",
            Message::Models3d(models3d::Action::Show(Display::TwoD)),
        ),
        selection(
            "Show as 2D from the 3D view",
            "Flatten the 3D model as it is turned now into the 2D drawing",
            "flatten projection layout structure view",
            Message::Models3d(models3d::Action::Flatten),
        ),
        selection(
            "Join fragments",
            super::super::joining::JOIN_COMMAND_HINT,
            "merge fuse connect",
            Message::Shortcut(shortcuts::Action::Join),
        ),
        selection(
            "Reaction arrow and copy",
            "Add a reaction arrow and a copy of the selected molecule",
            "scheme product next step",
            Message::Shortcut(shortcuts::Action::ReactionCopy),
        )
        .keys(keys(Modifiers::COMMAND | Modifiers::SHIFT, "Right")),
        selection(
            "Aromatic circle / alternating bonds",
            "Switch a selected aromatic ring's display",
            "kekule delocalized benzene",
            Message::AromaticDisplay,
        ),
        selection(
            "Toggle saturated / aromatic ring",
            "Convert a selected complete ring",
            "aromaticity benzene",
            Message::ToggleSelectedRing,
        ),
        selection(
            "Clear ring fill",
            "Remove the selected rings' fill",
            "color",
            Message::ClearRingFill,
        ),
        selection(
            "Clear highlights",
            "Remove highlight colors from the selection",
            "color",
            Message::ClearHighlights,
        )
        .alias(&["noh"]),
        selection(
            "Frame selection",
            "Draw a rectangle around the selection",
            "box border rectangle",
            Message::AddFrame(GraphicKind::Rectangle),
        ),
        selection(
            "Smooth",
            "Align gentle joints and remove redundant nodes from paths, curves and curved arrows",
            "pen bezier cleanup bow",
            Message::Graphics(super::super::graphics::Action::Cleanup(
                super::super::graphics::Cleanup::Smooth,
            )),
        ),
        selection(
            "Symmetrise",
            "Average a path, curve or curved arrow across its best mirror axis",
            "symmetric symmetry pen bezier bow",
            Message::Graphics(super::super::graphics::Action::Cleanup(
                super::super::graphics::Cleanup::Symmetrise { upright: false },
            )),
        ),
        selection(
            "Symmetrise upright",
            "Average a path or curve across its best vertical or horizontal mirror axis",
            "symmetric symmetry pen bezier bow",
            Message::Graphics(super::super::graphics::Action::Cleanup(
                super::super::graphics::Cleanup::Symmetrise { upright: true },
            )),
        ),
        selection(
            "Expand abbreviations",
            "Show the atoms of the selected groups",
            "ungroup label",
            Message::Abbreviations(abbreviations::Action::Expand),
        ),
        send(
            "Expand all abbreviations",
            Edit,
            "Show the atoms of every group",
            "label",
            Message::Abbreviations(abbreviations::Action::ExpandAll),
        ),
        selection(
            "Expand generic structure",
            "Expand the selected generic group",
            "markush r group",
            Message::Generic(generic::Action::Expand),
        ),
    ];
    for (name, degrees) in [
        ("Rotate 15° clockwise", 15.),
        ("Rotate 15° counterclockwise", -15.),
        ("Rotate 1° clockwise", 1.),
        ("Rotate 1° counterclockwise", -1.),
    ] {
        list.push(selection(
            name,
            "Rotate the selection",
            if degrees < 0. {
                "turn spin ccw anticlockwise"
            } else {
                "turn spin cw"
            },
            Message::Transform(Transform::Rotate(degrees)),
        ));
    }
    for (name, transform) in [
        ("Tilt up", Transform::TiltX(-12.)),
        ("Tilt down", Transform::TiltX(12.)),
        ("Tilt left", Transform::TiltY(12.)),
        ("Tilt right", Transform::TiltY(-12.)),
    ] {
        list.push(selection(
            name,
            "Tilt the selection in 3D by 12°",
            "3d perspective",
            Message::Transform(transform),
        ));
    }
    for (name, arrange) in [
        ("Align left", Arrange::AlignLeft),
        ("Align centers vertically", Arrange::AlignVertical),
        ("Align right", Arrange::AlignRight),
        ("Align top", Arrange::AlignTop),
        ("Align middles horizontally", Arrange::AlignHorizontal),
        ("Align bottom", Arrange::AlignBottom),
        ("Distribute horizontally", Arrange::DistributeHorizontal),
        ("Distribute vertically", Arrange::DistributeVertical),
    ] {
        list.push(selection(
            name,
            "Arrange the selected objects",
            "arrange line up spacing",
            Message::Arrange(arrange),
        ));
    }
    for (name, change) in [
        ("Bold text", StyleChange::Bold(true)),
        ("Italic text", StyleChange::Italic(true)),
        ("Underline text", StyleChange::Underline(true)),
        ("Subscript text", StyleChange::Script(Script::Subscript)),
        ("Superscript text", StyleChange::Script(Script::Superscript)),
        ("Normal script text", StyleChange::Script(Script::Normal)),
    ] {
        list.push(selection(
            name,
            "Style the selected text",
            "font typography",
            Message::TextStyle(change),
        ));
    }
    for (name, align) in [
        ("Align text left", TextAlign::Left),
        ("Center text", TextAlign::Center),
        ("Align text right", TextAlign::Right),
    ] {
        list.push(selection(
            name,
            "Align the selected text",
            "paragraph justify",
            Message::TextAlign(align),
        ));
    }
    list
}

fn analysis() -> Vec<Command> {
    use Category::Analyze;
    let molecule = |name: &str, detail: &str, synonyms: &str, messages| {
        Command::new(name, Analyze, detail, synonyms, Run::Send(messages)).need(Need::Molecule)
    };
    let mut list = vec![
        // The Properties panel's "Predict NMR…" button sends Open, which predicts.
        molecule(
            "Predict NMR",
            "Predict ¹H or ¹³C NMR shifts for the selected molecule",
            "nmr predict spectrum shifts 1h 13c proton carbon",
            vec![Message::Nmr(nmr::Action::Open)],
        ),
        Command::new(
            "Molecular formula and properties",
            Analyze,
            "Calculate the formula, mass and other properties",
            "formula molecular weight mass mw analyze logp tpsa",
            Run::MolecularProperties,
        )
        .need(Need::Molecule),
        molecule(
            "Name molecule",
            "Generate the chemical name of the selected molecule",
            "iupac name naming",
            vec![
                Message::Inspector(InspectorTab::Names),
                Message::Naming(naming::Action::Generate),
            ],
        ),
        Command::new(
            "Copy NMR report",
            Analyze,
            "Copy the predicted shifts",
            "nmr clipboard",
            Run::Send(vec![Message::Nmr(nmr::Action::Copy)]),
        )
        .need(Need::NmrReport),
        Command::new(
            "Export NMR report",
            Analyze,
            "Save the predicted shifts",
            "nmr file",
            Run::Send(vec![Message::Nmr(nmr::Action::Export)]),
        )
        .need(Need::NmrReport),
        send(
            "Reaction roles",
            Analyze,
            "Assign reactants, products and agents",
            "reaction scheme roles",
            Message::Reaction(reactions::Action::Open),
        ),
        send(
            "Atom mapping",
            Analyze,
            "Open reaction atom mapping",
            "reaction map",
            Message::Mapping(reaction_mapping::Action::Open),
        ),
        Command::new(
            "Map atoms automatically",
            Analyze,
            "Propose a reaction atom mapping",
            "reaction auto map",
            Run::Send(vec![
                Message::Mapping(reaction_mapping::Action::Open),
                Message::Mapping(reaction_mapping::Action::Auto),
            ]),
        ),
        send(
            "AI assistant",
            Analyze,
            "Open the drawing assistant",
            "ai codex chat",
            Message::Assistant(assistant::Action::Open),
        ),
    ];
    list.push(send(
        "Find abbreviations",
        Analyze,
        "Find groups that can be abbreviated",
        "contract label",
        Message::Abbreviations(abbreviations::Action::Find),
    ));
    list
}

fn files() -> Vec<Command> {
    use Category::File;
    let (command, alt, shift) = (Modifiers::COMMAND, Modifiers::ALT, Modifiers::SHIFT);
    let mut list = vec![
        send(
            "New drawing",
            File,
            "Create a drawing in a new tab",
            "file document tab",
            Message::New,
        )
        .alias(&["tabnew", "new", "enew"]),
        send(
            "Open…",
            File,
            "Open a drawing in a new tab",
            "file load",
            Message::Open,
        )
        .alias(&["e", "edit"]),
        send(
            "Save",
            File,
            "Save the drawing",
            "file write",
            Message::Save,
        )
        .alias(&["w", "write"]),
        send(
            "Save as…",
            File,
            "Save a copy under a new name",
            "file write",
            Message::SaveAs,
        )
        .alias(&["sav", "saveas"]),
        Command::new(
            "Save and close tab",
            File,
            "Save, then close the tab",
            "write quit",
            Run::SaveAndClose,
        )
        .alias(&["wq", "x"]),
        send(
            "Close tab",
            File,
            "Close the tab, asking to save changes",
            "quit",
            Message::Tabs(tabs::Action::Close(None)),
        )
        .keys(tab_keys(TabKey::Close))
        .alias(&["q", "quit", "close"]),
        Command::new(
            "Close tab without saving",
            File,
            "Close the tab and discard its changes",
            "quit discard",
            Run::DiscardAndClose,
        )
        .alias(&["q!"]),
        Command::new(
            "Close window",
            File,
            "Close every tab, asking to save changes",
            "quit all exit",
            Run::CloseWindow,
        )
        .alias(&["qa", "qall"]),
        send(
            "Next tab",
            File,
            "Switch to the next drawing",
            "cycle switch",
            Message::Tabs(tabs::Action::Cycle(true)),
        )
        .keys(tab_keys(TabKey::Next))
        .alias(&["tabn", "tabnext"]),
        send(
            "Previous tab",
            File,
            "Switch to the previous drawing",
            "cycle switch",
            Message::Tabs(tabs::Action::Cycle(false)),
        )
        .keys(tab_keys(TabKey::Previous))
        .alias(&["tabp", "tabprevious"]),
        send(
            "Import structure or name",
            File,
            "Insert SMILES, MOL, InChI or a chemical name",
            "smiles inchi mol paste text",
            Message::Inspector(InspectorTab::Import),
        )
        .keys(keys(command, "I")),
        send(
            "Import file…",
            File,
            "Insert structures from files",
            "mol sdf cdxml cdx",
            Message::Imports(import::Action::Choose),
        ),
        send(
            "Export panel",
            File,
            "Open Export",
            "save as format",
            Message::Inspector(InspectorTab::Export),
        )
        .keys(keys(command | shift, "E")),
        send(
            "Print…",
            File,
            "Print the drawing",
            "paper",
            Message::Printing(printing::Action::Start(reshiki::printing::Scope::Document)),
        ),
        send(
            "Page setup",
            File,
            "Set the page size and orientation",
            "paper layout a4 letter",
            Message::Pages(pages::Action::Open),
        ),
        send(
            "Copy",
            File,
            "Copy the selection",
            "clipboard",
            Message::Copy(false),
        ),
        send(
            "Cut",
            File,
            "Copy and remove the selection",
            "clipboard",
            Message::Copy(true),
        )
        .need(Need::Selection),
        send(
            "Paste",
            File,
            "Paste from the clipboard",
            "clipboard insert",
            Message::Paste,
        ),
        send(
            "Paste picture",
            File,
            "Paste the clipboard's picture",
            "image clipboard",
            Message::PastePicture,
        ),
        send(
            "Copy as image",
            File,
            "Copy as a picture",
            "png picture clipboard",
            Message::CopyImage,
        ),
        send(
            "Copy CDXML",
            File,
            "Copy as ChemDraw CDXML",
            "chemdraw clipboard",
            Message::CopyAs(reshiki::clipboard::CopyFormat::Cdxml),
        )
        .keys(keys(command, "D")),
        send(
            "Copy SMILES",
            File,
            "Copy as SMILES",
            "chemical text clipboard",
            Message::CopyChemicalText,
        )
        .keys(keys(command | alt, "C")),
        send(
            "Copy MOL",
            File,
            "Copy as MOL text",
            "molfile clipboard",
            Message::CopyAs(reshiki::clipboard::CopyFormat::Mol),
        )
        .keys(keys(command | alt, "O")),
    ];
    for format in reshiki::clipboard::CopyFormat::ALL {
        list.push(send(
            format!("Copy as {}", format.label()),
            File,
            "Copy in this format",
            "clipboard",
            Message::CopyAs(format),
        ));
    }
    for format in inspector::FigureFormat::ALL {
        list.push(send(
            format!("Export {format}"),
            File,
            "Save the drawing as a figure",
            "file figure image",
            Message::Export(format.code()),
        ));
    }
    for format in inspector::ChemicalFormat::ALL {
        list.push(send(
            format!("Export {format}"),
            File,
            "Save the drawing's structures",
            "file chemical",
            Message::Export(format.code()),
        ));
    }
    list
}

fn view(app: &App) -> Vec<Command> {
    use super::super::atom_labels::{Action as Labels, Scope};
    use Category::View;
    let mut list = vec![
        send(
            "Fit drawing",
            View,
            "Zoom so the whole drawing fits",
            "zoom show all",
            Message::Fit,
        ),
        send(
            "Zoom in",
            View,
            "Magnify the view",
            "magnify bigger",
            Message::Zoom(1.25),
        ),
        send(
            "Zoom out",
            View,
            "Shrink the view",
            "smaller",
            Message::Zoom(0.8),
        ),
        send(
            "Actual size",
            View,
            "Zoom to 100%",
            "zoom 100",
            Message::Zoom(1. / app.tab.camera.zoom),
        ),
        send(
            "Toggle grid",
            View,
            "Show or hide the grid",
            "view",
            Message::View(view_settings::Action::Grid),
        ),
        send(
            "Toggle smart guides",
            View,
            "Snap to other objects' edges and centers",
            "snap align",
            Message::View(view_settings::Action::SmartGuides(
                !app.appearance.smart_guides,
            )),
        ),
        send(
            "Toggle rulers",
            View,
            "Show or hide the rulers",
            "guides measure",
            Message::Shortcut(shortcuts::Action::Rulers),
        )
        .keys(keys(Modifiers::COMMAND, ";")),
        send(
            "Toggle crosshair",
            View,
            "Show or hide the crosshair",
            "guides cursor",
            Message::Shortcut(shortcuts::Action::Crosshair),
        )
        .keys(keys(Modifiers::COMMAND | Modifiers::ALT, "X")),
        send(
            "Toggle fixed bond length",
            View,
            "Constrain new bonds to the drawing length",
            "constraint snap geometry",
            Message::Shortcut(shortcuts::Action::FixedLength),
        )
        .keys(keys(Modifiers::COMMAND, "L")),
        send(
            "Toggle fixed angles",
            View,
            "Constrain new bonds to fixed angles",
            "constraint snap geometry",
            Message::Shortcut(shortcuts::Action::FixedAngles),
        )
        .keys(keys(Modifiers::COMMAND, "E")),
        send(
            "View options",
            View,
            "Show or hide the view options bar",
            "settings",
            Message::View(view_settings::Action::Toggle),
        ),
        send(
            "Toggle inspector",
            View,
            "Show or hide the side panel",
            "panel sidebar",
            Message::ToggleInspector,
        ),
        send(
            "Keyboard drawing",
            View,
            "Turn keyboard drawing on or off",
            "hotspot accessibility",
            Message::KeyboardDrawing(keyboard_drawing::Action::Toggle),
        ),
        Command::new(
            if app.tab.doc.atom_labels.stereo {
                "Hide stereodescriptors"
            } else {
                "Show stereodescriptors"
            },
            View,
            "Show or hide R/S, r/s, E/Z, M/P and seqCis/seqTrans labels throughout the drawing",
            "CIP pseudoasymmetric r/s stereodescriptor R/S E/Z M/P Ra/Sa seqCis seqTrans chirality descriptor",
            Run::Send(vec![
                Message::Labels(Labels::Scope(Scope::Drawing)),
                Message::Labels(Labels::Stereo(!app.tab.doc.atom_labels.stereo)),
                Message::Labels(Labels::Scope(app.tab.labels.scope)),
            ]),
        ),
        send(
            "Help & shortcuts",
            View,
            "Search every shortcut",
            "help keyboard reference",
            Message::ToggleHelp,
        )
        .keys(super::super::help::registry::finder_keys()),
        send(
            "Shortcut examples",
            View,
            "Open labeled shortcut examples in a new tab",
            "help tutorial",
            Message::OpenShortcutExamples,
        ),
        send(
            "Reset bond drawing",
            View,
            "Restore the default bond length and angles",
            "default length",
            Message::ResetBondDrawing,
        ),
    ];
    for (tab, name) in [
        (InspectorTab::Properties, "Properties panel"),
        (InspectorTab::Predict, "Predict panel"),
        (InspectorTab::Names, "Chemical names panel"),
        (InspectorTab::Import, "Import panel"),
        (InspectorTab::Export, "Export panel"),
        (InspectorTab::Templates, "Templates panel"),
        (InspectorTab::Abbreviations, "Abbreviations panel"),
        (InspectorTab::Labels, "Atom labels panel"),
        (InspectorTab::Reactions, "Reactions panel"),
        (InspectorTab::Pages, "Pages panel"),
        (InspectorTab::DrawingStyle, "Drawing style panel"),
        (InspectorTab::ThemeGenerator, "Theme generator panel"),
        (InspectorTab::Assistant, "Assistant panel"),
    ] {
        if tab == InspectorTab::Export {
            continue; // "Export panel" above carries its shortcut.
        }
        list.push(send(
            name,
            View,
            "Open this inspector panel",
            "inspector sidebar tab",
            Message::Inspector(tab),
        ));
    }
    for mode in crate::appearance::Mode::ALL {
        list.push(send(
            format!("Appearance: {mode}"),
            View,
            "Light or dark interface",
            "theme dark mode light mode",
            Message::Appearance(mode),
        ));
    }
    for theme in reshiki::canvas_theme::CanvasTheme::ALL {
        list.push(send(
            format!("Canvas: {theme:?}"),
            View,
            "Paper color of the drawing",
            "theme background paper",
            Message::CanvasTheme(theme),
        ));
    }
    for theme in reshiki::canvas_theme::ColorTheme::ALL {
        list.push(send(
            format!("Color theme: {theme}"),
            View,
            "Atom colors",
            "palette element colors",
            Message::ColorTheme(theme),
        ));
    }
    for preset in reshiki::document_styles::Preset::ALL {
        list.push(send(
            format!("Drawing style: {preset}"),
            View,
            "Apply a journal drawing style",
            "journal acs style publication",
            Message::QuickDrawingStyle(super::super::document_styles::Choice::Journal(preset)),
        ));
    }
    list
}

fn tools() -> Vec<Command> {
    use Category::Tool as T;
    let tool = |name: &str, synonyms: &str, tool| {
        send(
            name,
            T,
            "Choose this drawing tool",
            synonyms,
            Message::Tool(tool),
        )
    };
    let mut list = vec![
        tool("Select tool", "pointer arrow pick", Tool::Select).keys("v"),
        tool("Lasso tool", "freehand select", Tool::Lasso).keys("l"),
        tool("Tilt tool", "3d rotate perspective", Tool::Tilt),
        tool(
            "Straight chain tool",
            "alkyl zigzag carbon chain",
            Tool::Chain(ChainMode::Straight),
        )
        .keys(keys(Modifiers::SHIFT, "X")),
        tool(
            "Snaking chain tool",
            "alkyl carbon chain",
            Tool::Chain(ChainMode::Snaking),
        ),
        tool("Atom tool", "element", Tool::Atom),
        tool("Ring tool", "cycle polygon", Tool::Ring).keys("r"),
        tool(
            "Benzene tool",
            "ring preset phenyl aromatic arene",
            Tool::RingPreset(RingPreset::Benzene),
        )
        .keys("j"),
        tool(
            "Cyclopentadiene tool",
            "ring preset cp diene cyclopentadienyl",
            Tool::RingPreset(RingPreset::Cyclopentadiene),
        )
        .keys(keys(Modifiers::SHIFT, "J")),
        tool("Text tool", "caption annotation label write", Tool::Text).keys("t"),
        tool("Eraser", "erase delete rubber", Tool::Erase),
        tool("Edit points", "path nodes bezier", Tool::EditPoints),
    ];
    for (preset, synonyms) in [
        (
            RingPreset::ChairUp,
            "ring preset cyclohexane chair up conformation",
        ),
        (
            RingPreset::ChairDown,
            "ring preset cyclohexane chair down conformation",
        ),
        (
            RingPreset::HaworthFive,
            "ring preset furanose sugar haworth five",
        ),
        (
            RingPreset::HaworthSix,
            "ring preset pyranose sugar haworth six glucose",
        ),
    ] {
        list.push(tool(
            &format!("{preset} tool"),
            synonyms,
            Tool::RingPreset(preset),
        ));
    }
    for preset in ArrowPreset::ALL {
        list.push(send(
            format!("{preset} arrow"),
            T,
            "Choose this arrow tool",
            "arrow reaction scheme",
            Message::Palette(palettes::Action::Arrow(*preset)),
        ));
    }
    for family in [
        palettes::Family::Rectangles,
        palettes::Family::Ellipses,
        palettes::Family::Brackets,
    ] {
        for (label, option) in palettes::graphic_options(family) {
            list.push(send(
                label,
                T,
                "Choose this shape tool",
                "graphic shape draw",
                Message::Palette(palettes::Action::Graphic(option)),
            ));
        }
    }
    for kind in reshiki::scientific::SymbolKind::ALL {
        let synonyms = match kind {
            reshiki::scientific::SymbolKind::PartialPositive
            | reshiki::scientific::SymbolKind::PartialNegative => {
                "symbol graphic partial charge delta δ"
            }
            _ => "symbol graphic",
        };
        list.push(tool(
            &GraphicKind::Symbol(*kind).to_string(),
            synonyms,
            Tool::Graphic(GraphicKind::Symbol(*kind)),
        ));
    }
    for kind in reshiki::scientific::OrbitalKind::ALL {
        list.push(tool(
            &GraphicKind::Orbital(*kind).to_string(),
            "orbital graphic",
            Tool::Graphic(GraphicKind::Orbital(*kind)),
        ));
    }
    list
}

/// What a command word with a value does.
#[derive(Clone, Copy)]
enum Word {
    Rotate(f32),
    Tilt(Field, f32),
    Scale,
    Zoom,
    Charge,
    FontSize,
    BondLength,
    Ring,
    Chain,
    Element,
}

/// Command words, in the order that settles a shared prefix ("ch" is charge
/// when its value fits). Tilt signs follow the fixed Tilt up/down/left/right
/// rows.
const WORDS: &[(&[&str], Word)] = &[
    (&["rotate"], Word::Rotate(1.)),
    (&["rotate", "cw"], Word::Rotate(1.)),
    (&["rotate", "ccw"], Word::Rotate(-1.)),
    (&["rotate", "clockwise"], Word::Rotate(1.)),
    (&["rotate", "counterclockwise"], Word::Rotate(-1.)),
    (&["tilt", "up"], Word::Tilt(Field::TiltX, -1.)),
    (&["tilt", "down"], Word::Tilt(Field::TiltX, 1.)),
    (&["tilt", "left"], Word::Tilt(Field::TiltY, 1.)),
    (&["tilt", "right"], Word::Tilt(Field::TiltY, -1.)),
    (&["tilt", "x"], Word::Tilt(Field::TiltX, 1.)),
    (&["tilt", "y"], Word::Tilt(Field::TiltY, 1.)),
    (&["scale"], Word::Scale),
    (&["zoom"], Word::Zoom),
    (&["charge"], Word::Charge),
    (&["chain"], Word::Chain),
    (&["font", "size"], Word::FontSize),
    (&["font"], Word::FontSize),
    (&["bond", "length"], Word::BondLength),
    (&["ring"], Word::Ring),
    (&["element"], Word::Element),
];

/// Whether the typed words start the command's words, the first with at
/// least two letters: "rot", "bond len", "tilt x".
fn starts(typed: &[&str], words: &[&str]) -> bool {
    typed.len() == words.len()
        && typed.first().is_some_and(|w| w.len() >= 2)
        && typed.iter().zip(words).all(|(t, w)| w.starts_with(t))
}

/// Commands that take a value typed after their word or its prefix:
/// "rot 37", "rotate ccw 30", "tilt up 20", "scale 150", "zo 150",
/// "ch -1", "el Pd", "font 12", "bond len 14", "ri 7", "chain 5".
pub(crate) fn parameterised(app: &App, query: &str) -> Option<Command> {
    if let Some((word, name)) = query.trim().split_once(char::is_whitespace)
        && matches!(word, "w" | "sav" | "saveas" | "e" | "edit")
    {
        let name = name.trim();
        if !name.is_empty() {
            let name = super::super::files::native_drawing_name(name);
            let (title, detail, run) = match word {
                "w" | "sav" | "saveas" => (
                    format!("Save as {name}…"),
                    format!("Choose where to save {name}"),
                    Run::SaveAs(name),
                ),
                _ => (
                    format!("Open {name}"),
                    format!("Open {name} next to the current drawing"),
                    Run::OpenNamed(name),
                ),
            };
            return Some(Command::new(title, Category::File, detail, "", run));
        }
    }
    let query = query.trim().to_lowercase();
    let (typed, value) = query.rsplit_once(char::is_whitespace)?;
    let typed: Vec<&str> = typed.split_whitespace().collect();
    let number = value.parse::<f32>().ok().filter(|n| n.is_finite());
    WORDS
        .iter()
        .filter(|(words, _)| starts(&typed, words))
        .find_map(|(_, word)| command_with(app, *word, number, value))
}

fn command_with(app: &App, word: Word, number: Option<f32>, value: &str) -> Option<Command> {
    let command = match (word, number) {
        // Through the Rotation field, so full turns keep Undo and Redo.
        (Word::Rotate(sign), Some(n)) => {
            let degrees = sign * n;
            let way = if degrees < 0. {
                "counterclockwise"
            } else {
                "clockwise"
            };
            Command::new(
                format!("Rotate selection {}° {way}", degrees.abs()),
                Category::Edit,
                "Rotate the selection clockwise; negative turns counterclockwise",
                "",
                Run::Send(vec![
                    Message::NumericTransform(numeric_transforms::Action::Input(
                        Field::Rotation,
                        (degrees % 360.).to_string(),
                    )),
                    Message::NumericTransform(numeric_transforms::Action::Apply(Field::Rotation)),
                ]),
            )
            .need(Need::Selection)
        }
        // As the Tilt X and Y fields and the fixed tilt rows: molecules shown
        // as 3D models turn their model.
        (Word::Tilt(field, sign), Some(n)) => {
            let degrees = sign * n;
            let (way, transform) = match field {
                Field::TiltX if degrees < 0. => ("up", Transform::TiltX(degrees)),
                Field::TiltX => ("down", Transform::TiltX(degrees)),
                _ if degrees < 0. => ("right", Transform::TiltY(degrees)),
                _ => ("left", Transform::TiltY(degrees)),
            };
            send(
                format!("Tilt selection {way} {}°", degrees.abs()),
                Category::Edit,
                "Tilt the selection in 3D",
                "",
                Message::Transform(transform),
            )
            .need(Need::Selection)
        }
        // Through the Scale field, so its limits and 3D-model rule apply.
        (Word::Scale, Some(n)) if n > 0. => Command::new(
            format!("Scale selection to {n}%"),
            Category::Edit,
            "Scale the selection uniformly, relative to its size",
            "",
            Run::Send(vec![
                Message::NumericTransform(numeric_transforms::Action::Input(
                    Field::Scale,
                    n.to_string(),
                )),
                Message::NumericTransform(numeric_transforms::Action::Apply(Field::Scale)),
            ]),
        )
        .need(Need::Resizable),
        (Word::Zoom, Some(n)) if n > 0. => send(
            format!("Zoom to {n}%"),
            Category::View,
            "Set the zoom",
            "",
            Message::Zoom(n / 100. / app.tab.camera.zoom),
        ),
        (Word::Charge, Some(n)) if n.fract() == 0. && n.abs() <= 8. => Command::new(
            format!("Set charge to {n:+}"),
            Category::Atom,
            "Set the atom's formal charge",
            "",
            Run::SetCharge(n as i32),
        )
        .need(Need::OneAtom),
        (Word::FontSize, Some(n)) => Command::new(
            format!("Font size {n} pt"),
            Category::Edit,
            "Set the selected text's size",
            "",
            Run::Send(vec![
                Message::FontSize(value.into()),
                Message::ApplyFontSize,
            ]),
        )
        .need(if (4. ..=144.).contains(&n) {
            Need::Nothing
        } else {
            Need::Blocked("Enter a font size from 4 to 144 pt")
        }),
        (Word::BondLength, Some(n)) if n > 0. => send(
            format!("New bond length {n} pt"),
            Category::View,
            "For bonds you draw next; use scale to resize the drawing",
            "",
            Message::DrawingLength(value.into()),
        ),
        (Word::Ring, Some(n)) => Command::new(
            format!("Insert {n}-membered ring"),
            Category::Insert,
            "Attach to the selected or pointed atom or bond, or add at the center",
            "",
            Run::Ring(Ring::Size(n as u8, false)),
        )
        .need(if (3. ..=8.).contains(&n) && n.fract() == 0. {
            Need::Nothing
        } else {
            Need::Blocked("Rings have 3 to 8 atoms")
        }),
        (Word::Chain, Some(n)) => {
            static LIMIT: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
                format!("Chains have 1 to {} atoms", reshiki::chains::MAX_ATOMS)
            });
            Command::new(
                format!("Chain tool with {n} atoms"),
                Category::Tool,
                "Draw chains of this length",
                "",
                Run::Send(vec![
                    Message::ChainAtoms(n.to_string()),
                    Message::Tool(Tool::Chain(ChainMode::Straight)),
                ]),
            )
            .need(
                if n >= 1. && n.fract() == 0. && n <= reshiki::chains::MAX_ATOMS as f32 {
                    Need::Nothing
                } else {
                    Need::Blocked(&LIMIT)
                },
            )
        }
        (Word::Element, None) => {
            let symbol = reshiki::editing::ELEMENTS
                .iter()
                .find(|s| s.eq_ignore_ascii_case(value))?;
            Command::new(
                format!("Element {symbol}"),
                Category::Insert,
                "Change the selected or pointed atom, or choose the atom tool",
                "",
                Run::Element(symbol),
            )
        }
        _ => return None,
    };
    Some(command)
}
