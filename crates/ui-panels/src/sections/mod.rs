//! Sections and title rows: `TitleRow`, `FooterBar`, `Section`, `GroupBox` and `Panel`; `DragSource`, a child a drag starts
//! from; `KeyArea`, a region a click focuses that takes its keys.

mod drag_source;
pub(crate) mod footer;
mod group_box;
mod key_area;
#[cfg(feature = "testing")]
mod sample;
mod section;
mod stack;
mod title_row;
mod words;

// Panel.
mod panel;

pub use drag_source::{DragSource, DragSourceOutput, DragSourceStyle};
pub use footer::{DialogRole, DropAction, DropActionOutput, DropActionStyle, FooterAction, FooterBar, FooterButton, FooterDrop, FooterOutput};
pub use group_box::{GroupBox, GroupBoxStyle};
pub use key_area::{KeyArea, KeyAreaOutput, KeyAreaStyle};
pub use section::{Section, SectionOutput, SectionStyle};
pub use stack::{SectionStack, SectionStackOutput, SectionStackStyle, StackRows, StackSlot};
#[doc(hidden)]
pub use title_row::MenuRows;
pub use title_row::{HeaderAction, HeaderCommand, HeaderStrength, TitleOutput, TitleRow, TitleRowStyle};
pub use words::SectionWords;

pub use panel::{Panel, PanelOutput, PanelStyle};

// Reorderable sections.
mod layout;
mod reorderable;

pub use layout::{SectionEvent, SectionKey, SectionLayout, SectionState};
pub use reorderable::{ReorderableSections, ReorderableSectionsOutput, SectionButton, SectionCommands, SectionSpec};
pub use words::ReorderableWords;
