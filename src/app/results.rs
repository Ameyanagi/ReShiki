//! One exhaustive table of async completions, their owners and modal admissions.

use super::Message;

/// Who receives a completion: the tab that started the work, or the app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Owner {
    Document,
    App,
}

/// Every async completion kind. A new result-bearing message must map to one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Completion {
    /// EngineDone.
    Engine,
    /// Optimization(WorkerDone).
    Optimization,
    /// Models3d(Generated).
    Models3d,
    /// Nmr(Finished | Exported).
    Nmr,
    /// Exported.
    Export,
    /// FigureExported.
    FigureExport,
    /// Printing(Prepared | Finished).
    Printing,
    /// DrawingStyle(Loaded | Saved).
    DrawingStyle,
    /// Assistant(Poll | Done).
    AssistantDocument,
    /// LabelsReady.
    Labels,
    /// Mapping(Ready).
    Mapping,
    /// InspectorAction(PropertiesCalculated).
    Properties,
    /// Imports(Loaded).
    Imports,
    /// Naming(Finished).
    Naming,
    /// ClipboardRead, ClipboardWritten, CopyAsPrepared and CopyAsWritten.
    Clipboard,
    /// Pasted.
    Pasted,
    /// Pictures(Loaded).
    Pictures,
    /// Assistant(Connected | PreferencesSaved | ImageRead | TextPasted | SetupHelpOpened).
    #[cfg_attr(
        target_arch = "wasm32",
        expect(dead_code, reason = "The browser has no assistant connection")
    )]
    AssistantApp,
    /// Templates(Finished | Imported | WarningAcknowledged).
    Templates,
    /// Generic(Expanded).
    Generic,
    /// Autosaved.
    Autosave,
    /// Saved.
    Save,
    /// FilePrepared.
    FilePrepared,
    /// ThemeFile(Loaded | Saved).
    ThemeFile,
    /// Opened, the legacy file-open receipt.
    Opened,
    /// OfficePrepared.
    OfficePrepared,
    /// Updates(Checked | Saved | Prepared | RecoveryCleared | Restarted | Opened).
    Updates,
    /// Accessibility(Geometry | Snapshot | Installed | Published).
    Accessibility,
    /// Recovery(Finished), delivered before tab routing and modal gates.
    Recovery,
    /// Desktop(Ready | Finished | Office).
    #[cfg(any(windows, target_os = "linux"))]
    Desktop,
    /// MacFiles(Open), the native file-open receipt.
    #[cfg(target_os = "macos")]
    MacFiles,
}

impl Completion {
    fn owner(self) -> Owner {
        match self {
            Self::Engine
            | Self::Optimization
            | Self::Models3d
            | Self::Nmr
            | Self::Export
            | Self::FigureExport
            | Self::Printing
            | Self::DrawingStyle
            | Self::AssistantDocument
            | Self::Labels
            | Self::Mapping
            | Self::Properties
            | Self::Imports
            | Self::Naming
            | Self::Clipboard
            | Self::Pasted
            | Self::Pictures => Owner::Document,
            Self::AssistantApp
            | Self::Templates
            | Self::Generic
            | Self::Autosave
            | Self::Save
            | Self::FilePrepared
            | Self::ThemeFile
            | Self::Opened
            | Self::OfficePrepared
            | Self::Updates
            | Self::Accessibility
            | Self::Recovery => Owner::App,
            #[cfg(any(windows, target_os = "linux"))]
            Self::Desktop => Owner::App,
            #[cfg(target_os = "macos")]
            Self::MacFiles => Owner::App,
        }
    }
}

/// A modal gate's column in the admission table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Modal {
    /// atom_text::background, also used by color_popover::keeps_open.
    AtomText,
    /// gates::preview_passthrough, shared by the cleanup and 3D previews.
    Preview,
    /// cleanup_preview_gate: the shared preview results plus its own.
    Cleanup,
    /// optimization_gate, the 3D preview.
    Optimization,
    /// updates::background, while the update dialog owns input.
    UpdatesDialog,
}

/// Which modal gates let a completion through. Each row decides every modal.
struct Admission {
    atom_text: bool,
    preview: bool,
    cleanup: bool,
    optimization: bool,
    updates_dialog: bool,
}

const NOWHERE: Admission = Admission {
    atom_text: false,
    preview: false,
    cleanup: false,
    optimization: false,
    updates_dialog: false,
};

fn admission(completion: Completion) -> Admission {
    match completion {
        // Models3d drops its result itself while a modal is open, so its
        // pending job is cleared in every modal.
        Completion::Engine
        | Completion::Models3d
        | Completion::Nmr
        | Completion::Export
        | Completion::FigureExport
        | Completion::Naming
        | Completion::Clipboard
        | Completion::Opened => Admission {
            atom_text: true,
            preview: true,
            cleanup: true,
            optimization: true,
            updates_dialog: true,
        },
        Completion::Printing | Completion::Imports | Completion::Pictures => Admission {
            atom_text: true,
            preview: false,
            cleanup: true,
            optimization: true,
            updates_dialog: true,
        },
        Completion::AssistantDocument | Completion::AssistantApp | Completion::Properties => {
            Admission {
                atom_text: true,
                preview: false,
                cleanup: false,
                optimization: true,
                updates_dialog: true,
            }
        }
        Completion::Optimization
        | Completion::DrawingStyle
        | Completion::Labels
        | Completion::Mapping
        | Completion::Pasted
        | Completion::ThemeFile => Admission {
            atom_text: false,
            preview: false,
            cleanup: false,
            optimization: true,
            updates_dialog: true,
        },
        Completion::Save => Admission {
            atom_text: true,
            preview: true,
            cleanup: true,
            optimization: true,
            updates_dialog: false,
        },
        Completion::Updates => Admission {
            atom_text: true,
            preview: false,
            cleanup: false,
            optimization: true,
            updates_dialog: false,
        },
        Completion::Autosave | Completion::FilePrepared => Admission {
            atom_text: false,
            preview: false,
            cleanup: false,
            optimization: true,
            updates_dialog: false,
        },
        // early_results_gate handles Templates, and discards Generic while any of these modals
        // is open (generic_expansion_blocked), so no modal gate sees them.
        Completion::Templates | Completion::Generic => NOWHERE,
        // Handled in App::update or update_front before any gate.
        Completion::OfficePrepared | Completion::Accessibility | Completion::Recovery => NOWHERE,
        #[cfg(any(windows, target_os = "linux"))]
        Completion::Desktop => NOWHERE,
        #[cfg(target_os = "macos")]
        Completion::MacFiles => NOWHERE,
    }
}

/// Whether a message is an async completion, and who owns it. Owner::Document is
/// tabs::document_result: deferred while exiting, delivered to its originating tab.
pub(super) fn async_result(message: &Message) -> Option<Owner> {
    completion(message).map(Completion::owner)
}

/// Whether `modal` lets this completion through. Non-completions are never admitted here;
/// each gate lists its own lifecycle messages.
pub(super) fn admitted(modal: Modal, message: &Message) -> bool {
    let Some(completion) = completion(message) else {
        return false;
    };
    let admission = admission(completion);
    match modal {
        Modal::AtomText => admission.atom_text,
        Modal::Preview => admission.preview,
        Modal::Cleanup => admission.cleanup,
        Modal::Optimization => admission.optimization,
        Modal::UpdatesDialog => admission.updates_dialog,
    }
}

/// No wildcard arms: a new Message variant or sub-action fails to compile until classified.
fn completion(message: &Message) -> Option<Completion> {
    match message {
        Message::Recovery(action) => match action {
            super::recovery_manager::Action::Finished(..) => Some(Completion::Recovery),
            #[cfg(target_arch = "wasm32")]
            super::recovery_manager::Action::OwnersReady(..) => Some(Completion::Recovery),
            super::recovery_manager::Action::Show
            | super::recovery_manager::Action::Toggle(..)
            | super::recovery_manager::Action::RestoreSelected
            | super::recovery_manager::Action::RestoreAll
            | super::recovery_manager::Action::Discard
            | super::recovery_manager::Action::ConfirmDiscard
            | super::recovery_manager::Action::Close => None,
        },
        Message::Accessibility(action) => {
            use super::accessibility::Action;
            match action {
                Action::Geometry(..)
                | Action::Snapshot(..)
                | Action::Installed(..)
                | Action::Published(..) => Some(Completion::Accessibility),
                // Dispatch wraps a routed message; its inner message is classified there.
                Action::Window(..)
                | Action::Refresh
                | Action::Native(..)
                | Action::Dispatch(..) => None,
            }
        }
        Message::InspectorAction(action) => {
            use super::inspector::Action;
            match action {
                Action::PropertiesCalculated(..) => Some(Completion::Properties),
                Action::Section(..)
                | Action::ModelControls(_)
                | Action::Figure(..)
                | Action::FigureMenu(..)
                | Action::Chemical(..)
                | Action::ChemicalMenu(..)
                | Action::RefreshProperties
                | Action::RefreshPropertiesFor(..)
                | Action::Centroid
                | Action::Attachment(..)
                | Action::DepthBonds
                | Action::RingArc => None,
            }
        }
        Message::Nmr(action) => {
            use super::nmr::Action;
            match action {
                Action::Finished(..) | Action::Exported(..) => Some(Completion::Nmr),
                Action::Open
                | Action::Close
                | Action::Dock
                | Action::Undock
                | Action::Predict(..)
                | Action::ToggleCollapsed
                | Action::ToggleDetails
                | Action::TogglePlot
                | Action::ToggleLabels
                | Action::LabelMode(..)
                | Action::LabelMenu(..)
                | Action::Move(..)
                | Action::Resize(..)
                | Action::ResetPosition
                | Action::RenderedSize(..)
                | Action::SelectRow(..)
                | Action::SelectMarker(..)
                | Action::Copy
                | Action::Export => None,
            }
        }
        Message::Updates(action) => {
            use super::updates::Action;
            match action {
                Action::Checked(..)
                | Action::Saved(..)
                | Action::Prepared(..)
                | Action::RecoveryCleared
                | Action::Restarted(..)
                | Action::Opened(..) => Some(Completion::Updates),
                Action::Show(..)
                | Action::Check(..)
                | Action::Channel(..)
                | Action::Portable
                | Action::Automatic(..)
                | Action::Download
                | Action::Install
                | Action::Poll => None,
            }
        }
        Message::Mapping(action) => {
            use super::reaction_mapping::Action;
            match action {
                Action::Ready(..) => Some(Completion::Mapping),
                Action::Open
                | Action::Text(..)
                | Action::Apply
                | Action::Clear
                | Action::Pair
                | Action::Show(..)
                | Action::Position
                | Action::ResetPositions
                | Action::Auto
                | Action::ApplyProposal
                | Action::Dismiss
                | Action::ReviewPair(..)
                | Action::Align => None,
            }
        }
        Message::Generic(action) => {
            use super::generic::Action;
            match action {
                Action::Expanded { .. } => Some(Completion::Generic),
                Action::Expand
                | Action::ExpandComponent(..)
                | Action::RepeatLabel(..)
                | Action::Pattern(..)
                | Action::DefineRepeat(..)
                | Action::ClearRepeat(..) => None,
            }
        }
        Message::DrawingStyle(action) => {
            use super::document_styles::Action;
            match action {
                Action::Loaded(..) | Action::Saved(..) => Some(Completion::DrawingStyle),
                Action::Open
                | Action::Cancel
                | Action::Apply
                | Action::Preset(..)
                | Action::Custom
                | Action::Source(..)
                | Action::Name(..)
                | Action::Font(..)
                | Action::Input(..)
                | Action::CommitField(..)
                | Action::DisplayUnit(..)
                | Action::Advanced(..)
                | Action::Matching(..)
                | Action::Scale(..)
                | Action::Load
                | Action::Save(..)
                | Action::ExportMenu(..) => None,
            }
        }
        Message::Models3d(action) => match action {
            super::models3d::Action::Generated(..) => Some(Completion::Models3d),
            super::models3d::Action::Show(..)
            | super::models3d::Action::Update
            | super::models3d::Action::Regenerate
            | super::models3d::Action::Set(..)
            | super::models3d::Action::SlideEnd
            | super::models3d::Action::Unchanged
            | super::models3d::Action::Flatten
            | super::models3d::Action::Layout(..) => None,
        },
        Message::Optimization(action) => {
            use super::optimization::Action;
            match action {
                Action::WorkerDone(..) => Some(Completion::Optimization),
                Action::Begin
                | Action::Field(..)
                | Action::Start
                | Action::Stop
                | Action::Apply
                | Action::Cancel
                | Action::PinSelected
                | Action::UnpinSelected
                | Action::ClearPins
                | Action::ShowOriginal(..)
                | Action::Rotate(..)
                | Action::Roll(..)
                | Action::DepthEnhancement(..)
                | Action::ClearDepthAppearance => None,
            }
        }
        Message::Printing(action) => {
            use super::printing::Action;
            match action {
                Action::Prepared(..) | Action::Finished(..) => Some(Completion::Printing),
                Action::Start(..) => None,
            }
        }
        Message::Pictures(action) => {
            use super::pictures::Action;
            match action {
                Action::Loaded(..) => Some(Completion::Pictures),
                Action::Replace
                | Action::Width(..)
                | Action::Height(..)
                | Action::Lock(..)
                | Action::Resize(..)
                | Action::RestoreAspect => None,
            }
        }
        #[cfg(target_arch = "wasm32")]
        Message::Assistant(action) => matches!(action, super::assistant::Action::Poll)
            .then_some(Completion::AssistantDocument),
        #[cfg(not(target_arch = "wasm32"))]
        Message::Assistant(action) => {
            use super::assistant::Action;
            match action {
                Action::Connected(..)
                | Action::SetupHelpOpened(..)
                | Action::ImageRead { .. }
                | Action::TextPasted { .. }
                | Action::PreferencesSaved(..) => Some(Completion::AssistantApp),
                Action::Poll | Action::Done { .. } => Some(Completion::AssistantDocument),
                Action::Open
                | Action::Connect
                | Action::SetupHelp(..)
                | Action::ImageExample
                | Action::Input(..)
                | Action::Paste { .. }
                | Action::OpenImage
                | Action::ClearImage
                | Action::ViewImage(..)
                | Action::Example(..)
                | Action::Model(..)
                | Action::Menu(..)
                | Action::Search(..)
                | Action::ChatScrolled { .. }
                | Action::JumpToResult
                | Action::Effort(..)
                | Action::Tier(..)
                | Action::Replace(..)
                | Action::AutoApply(..)
                | Action::Send
                | Action::Retry
                | Action::Improve
                | Action::PreviewTarget(..)
                | Action::PreviewEdit(..)
                | Action::Stop
                | Action::Reset
                | Action::Apply
                | Action::Reject => None,
            }
        }
        Message::LabelsReady(..) => Some(Completion::Labels),
        Message::Templates(action) => {
            use super::template_library::Action;
            match action {
                Action::Imported(..) | Action::Finished(..) | Action::WarningAcknowledged(..) => {
                    Some(Completion::Templates)
                }
                Action::Browse
                | Action::Forward
                | Action::Search(..)
                | Action::SearchAll
                | Action::Filter(..)
                | Action::Collection(..)
                | Action::Name(..)
                | Action::Category(..)
                | Action::BeginSave
                | Action::EditDetails
                | Action::SaveDetails
                | Action::CancelDetails
                | Action::Replace
                | Action::Remove
                | Action::Restore
                | Action::Favorite(..)
                | Action::Anchor(..)
                | Action::Connection(..)
                | Action::RememberAnchor
                | Action::Repeat(..)
                | Action::Import
                | Action::Export
                | Action::Reload => None,
            }
        }
        Message::Imports(action) => {
            use super::import::Action;
            match action {
                // A chosen file belongs to the drawing it was chosen for.
                Action::Loaded(..) | Action::Read(..) => Some(Completion::Imports),
                Action::InputKind(..)
                | Action::Clear
                | Action::Edit(..)
                | Action::ReplaceText(..)
                | Action::Menu(..)
                | Action::ExamplesMenu(..)
                | Action::Choose
                | Action::Files(..)
                | Action::Hovered(..)
                | Action::Dropped(..)
                | Action::Left => None,
            }
        }
        Message::Naming(action) => {
            use super::naming::Action;
            match action {
                #[cfg(target_arch = "wasm32")]
                Action::Refresh => None,
                Action::Finished(..) => Some(Completion::Naming),
                Action::Name(..)
                | Action::ClearName
                | Action::Cancel
                | Action::Resolve
                | Action::Generate
                | Action::ShowMoleculeName(..)
                | Action::HideMoleculeName(..)
                | Action::Smiles(..)
                | Action::UpdatePreview
                | Action::RestorePreview
                | Action::PreviewEdit(..)
                | Action::Acknowledge(..)
                | Action::Insert
                | Action::InsertName
                | Action::AddNameBelow(..)
                | Action::PreviewOpen(..)
                | Action::DetailsOpen(..)
                | Action::CopyName
                | Action::Caption
                | Action::Alias(..)
                | Action::OpenPanel(..)
                | Action::AskOnline
                | Action::ConfirmOnline(..)
                | Action::DismissOnline
                | Action::OpenOnlineSource => None,
            }
        }
        Message::ThemeFile(action) => {
            use super::theme_files::Action;
            match action {
                Action::Loaded(..) | Action::Saved(..) => Some(Completion::ThemeFile),
                Action::Choose(..) | Action::Import => None,
            }
        }
        Message::CopyAsPrepared(..)
        | Message::CopyAsWritten(..)
        | Message::ClipboardWritten { .. }
        | Message::ClipboardRead { .. } => Some(Completion::Clipboard),
        Message::Pasted(..) => Some(Completion::Pasted),
        Message::Autosaved(..) => Some(Completion::Autosave),
        Message::EngineDone { .. } | Message::EngineFinished { .. } => Some(Completion::Engine),
        Message::Opened(..) => Some(Completion::Opened),
        Message::FilePrepared(..) => Some(Completion::FilePrepared),
        Message::OfficePrepared(..) => Some(Completion::OfficePrepared),
        #[cfg(any(windows, target_os = "linux"))]
        Message::Desktop(action) => {
            use super::desktop::Action;
            match action {
                Action::Ready | Action::Finished | Action::Office(..) => Some(Completion::Desktop),
                Action::Request(..) | Action::Poll => None,
            }
        }
        #[cfg(target_arch = "wasm32")]
        Message::Host(_) | Message::SaveDownload(_) => None,
        #[cfg(target_os = "macos")]
        Message::MacFiles(action) => {
            use super::macos_files::Action;
            match action {
                Action::Open(..) => Some(Completion::MacFiles),
            }
        }
        Message::Saved(..) => Some(Completion::Save),
        Message::Exported(..) => Some(Completion::Export),
        Message::FigureExported(..) => Some(Completion::FigureExport),
        Message::ExportReady(export) | Message::SaveExport(_, export) => Some(match export.kind {
            super::figure_export::Kind::Figure => Completion::FigureExport,
            super::figure_export::Kind::Structure => Completion::Export,
            super::figure_export::Kind::Nmr => Completion::Nmr,
        }),
        // Tab wraps a routed message; its inner message is classified where it is delivered.
        Message::Tab(..) => None,
        #[cfg(target_os = "linux")]
        Message::LinuxClipboardWindow(_) => None,
        // Controls, view/window lifecycle and sub-enums without completion variants.
        Message::ContextMenu(..)
        | Message::ModifiersChanged(..)
        | Message::CopyFrame(..)
        | Message::StyleMenu(..)
        | Message::ObjectToolbar(..)
        | Message::NumericTransform(..)
        | Message::Reaction(..)
        | Message::DepthAppearance(..)
        | Message::InlineText(..)
        | Message::AtomText(..)
        | Message::Join(..)
        | Message::KeyboardDrawing(..)
        | Message::Pages(..)
        | Message::Escape
        | Message::CancelEngine(..)
        | Message::Palette(..)
        | Message::ContextKey(..)
        | Message::Shortcut(..)
        | Message::AromaticDisplay
        | Message::Abbreviations(..)
        | Message::Labels(..)
        | Message::TemplateNavigate(..)
        | Message::InspectorScroll(..)
        | Message::ResetBondDrawing
        | Message::FixedLength(..)
        | Message::FixedAngles(..)
        | Message::DrawingLength(..)
        | Message::ChainAtoms(..)
        | Message::ChainAngle(..)
        | Message::ApplyBondPreset(..)
        | Message::BondPosition(..)
        | Message::BondColor(..)
        | Message::ApplyBondColor
        | Message::Arc(..)
        | Message::Graphics(..)
        | Message::RemoveMark(..)
        | Message::RotateMark(..)
        | Message::AtomRadical(..)
        | Message::ToggleInspector
        | Message::Inspector(..)
        | Message::InsertInput
        | Message::ToggleHelp
        | Message::Help(..)
        | Message::Commands(..)
        | Message::OpenShortcutExamples
        | Message::Viewport(..)
        | Message::Canvas(..)
        | Message::Tool(..)
        | Message::Element(..)
        | Message::CaptionAction(..)
        | Message::TextStyle(..)
        | Message::FontSize(..)
        | Message::ApplyFontSize
        | Message::ColorScope(..)
        | Message::ClearRingFill
        | Message::ClearHighlights
        | Message::TextColor(..)
        | Message::ApplyTextColor
        | Message::TextAlign(..)
        | Message::LabelAlign(..)
        | Message::TextSpacing(..)
        | Message::TextWidth(..)
        | Message::ApplyTextWidth
        | Message::Import
        | Message::Example(..)
        | Message::Cleanup(..)
        | Message::Analyze
        | Message::Undo
        | Message::Redo
        | Message::Delete
        | Message::SelectAll
        | Message::InvertSelection
        | Message::Group
        | Message::Ungroup
        | Message::IntegralGroup(..)
        | Message::AddFrame(..)
        | Message::View(..)
        | Message::Appearance(..)
        | Message::CanvasTheme(..)
        | Message::ColorTheme(..)
        | Message::QuickDrawingStyle(..)
        | Message::ThemeGenerator(..)
        | Message::Fit
        | Message::Zoom(..)
        | Message::New
        | Message::Open
        | Message::Save
        | Message::Export(..)
        | Message::CopySmiles
        | Message::CopyChemicalText
        | Message::Copy(..)
        | Message::CopyImage
        | Message::CopyAs(..)
        | Message::Paste
        | Message::PastePicture
        | Message::Duplicate
        | Message::Transform(..)
        | Message::Arrange(..)
        | Message::ReverseBonds
        | Message::BondDepth(..)
        | Message::RingSize(..)
        | Message::AromaticRing(..)
        | Message::ToggleAromaticRing
        | Message::ToggleSelectedRing
        | Message::ArrowStyle(..)
        | Message::ArrowAction(..)
        | Message::ArrowLabelGap(..)
        | Message::CustomElement(..)
        | Message::ApplyElement
        | Message::InsertTemplate(..)
        | Message::SaveAs
        | Message::Tick
        | Message::Restore
        | Message::DismissRecovery
        | Message::Charge(..)
        | Message::Isotope(..)
        | Message::ApplyIsotope
        | Message::Tabs(..)
        | Message::Close(..)
        | Message::Discard
        | Message::Cancel
        | Message::Prompt(_) => None,
        Message::DownloadName(_) => None,
    }
}

#[cfg(test)]
mod tests;
