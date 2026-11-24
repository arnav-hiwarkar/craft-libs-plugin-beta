mod app;
#[cfg(not(target_family = "wasm"))]
mod cache;
pub mod data;
mod host;
mod images;
mod journal;
pub mod kernels;
mod package;
mod panel;
mod plugin;
mod registry;
#[cfg(not(target_family = "wasm"))]
mod runtime;
#[cfg(target_family = "wasm")]
#[path = "unavailable.rs"]
mod runtime;
#[cfg(all(feature = "testing", not(target_family = "wasm")))]
pub mod testing;

pub use app::{EffectRegistry, ParamValues};
pub use craft_plugin::{
    AudioRequest, Blend, Call, Camera, CommandSpec, ContributedPreset, Contributions, ControlsReply, Cull, Draw, EffectSpec, Entry, Focus, FrameDesc,
    FrameRequest, Gradient, LayerPlacement, Layout, Light, LightKind, Manifest, MaskPath, PanelReply, PanelSpec, Param, ParamKind, ParamsAt,
    Permission, PickFile, Pixel, Place, Plan, PlanData, PlanImage, PlanPass, Rect, RenderCx, RunMode, Scene, TaskSpec, TaskState, TaskStatus,
    UiEvent, UiEventKind, UiInput, Value, Widget, applied_padding, default_curves_text,
};
pub use host::{Access, Host, MAX_FRAME_SCALE, Origin, Pixels, Samples, check_frame};
pub use images::{MAX_IMAGE_SIDE, PluginImage};
pub use journal::{LogLine, LogPage};
pub use package::{
    EXTENSION as PACKAGE_EXTENSION, MAX_ENTRIES as PACKAGE_MAX_ENTRIES, MAX_UNPACKED_BYTES as PACKAGE_MAX_BYTES, PackageError, unpack,
};
pub use panel::{MAX_BACKLOG, Retained};
#[doc(hidden)]
pub use plugin::pooled_generator_buffers;
pub use plugin::{Limits, Plugin, PluginError, validate};
pub use registry::{Approved, Bundle, BundleError, Consent, Refusal, Registry};

pub mod ui {
    pub use craft_plugin::{
        AppEvent, ControlsReply, Drag, Dropped, Key, Modifiers, Paint, PanelReply, Pointer, PointerPhase, Root, Shape, TextStyle, Topic, UiEvent,
        UiEventKind, UiInput, Widget,
    };
}

pub mod viewer {
    pub use craft_plugin::{Bind, Cursor, Handle, Mark, Overlay, Space, ToolEvent, ToolEventKind, ToolSpec};
}
