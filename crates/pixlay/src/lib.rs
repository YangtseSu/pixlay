//! GTK4 and libadwaita shell and interaction.
//!
//! Boundary (`AGENTS.md`, "Module boundaries"): the only crate allowed to depend
//! on GTK, and the only one allowed to translate a string. It must not manipulate
//! pixels — every pixel operation goes through `pixlay-imaging` or
//! `pixlay-render` — and no GTK object ever crosses a thread boundary.
//!
//! The window is one document at a time, edited through `pixlay-core`'s command
//! vocabulary (`state.rs` holds the history), drawn by the single
//! `pixlay_render::draw` (`canvas.rs`), and exported through the same pipeline the
//! CLI uses (`export.rs`). What is specific to a window — the utility pane, the
//! gestures, the background threads and their progress — is in `sidebar.rs`,
//! `canvas.rs` and `window.rs`.
//!
//! `a11y.rs` is the one place an accessible name is set.
//!
//! Everything machine-checkable about this layer is in `tests/`: the accelerator
//! table, the accessible names, the adaptive minimum, the pixels the window draws
//! against the ones `pixlay-render render` writes, and the `po/POTFILES` set.

pub mod a11y;
pub mod app;
pub mod canvas;
pub mod decode;
pub mod export;
pub mod i18n;
pub mod sidebar;
pub mod state;
pub mod window;

pub use app::run;
pub use window::EditorWindow;

/// Application id. The desktop file, the icon name and the AppStream metadata
/// must all match it exactly.
pub const APP_ID: &str = "org.yangtse.Pixlay";
