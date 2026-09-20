//! GTK4 and libadwaita shell and interaction.
//!
//! Boundary: the only crate allowed to depend on gtk. It must not manipulate
//! pixels — every pixel operation goes through `pixlay-imaging` or
//! `pixlay-render`. GTK types are not `Send`/`Sync`, so background decoding and
//! scaling must hand their results back to the main thread.
//!
//! The GUI and the `pixlay` binary target arrive with S7; until then this crate
//! only pins the application identity that packaging metadata depends on.

/// Application id. The desktop file, the icon name and the AppStream metadata
/// must all match it exactly.
pub const APP_ID: &str = "org.yangtse.Pixlay";
