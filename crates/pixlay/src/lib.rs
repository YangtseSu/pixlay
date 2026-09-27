//! GTK4 and libadwaita shell and interaction.
//!
//! Boundary (`AGENTS.md`, "Module boundaries"): the only crate allowed to depend
//! on GTK, and the only one allowed to translate a string. It must not manipulate
//! pixels — every pixel operation goes through `pixlay-imaging` or
//! `pixlay-render` — and no GTK object ever crosses a thread boundary.
//!
//! **The editor is the window** (S22, ruling 31): it opens on the collage, photos
//! enter from outside it (`Add photos…`, an empty cell's `+`, `Replace`, a drop from
//! the file manager, `Open…`, or `pixlay a.jpg b.jpg …` on the command line), and the
//! document is edited through `pixlay-core`'s command vocabulary (`state.rs` holds the
//! history), drawn by the single `pixlay_render::draw` (`canvas.rs`), and exported
//! through the same pipeline the CLI uses (`export.rs`). What is specific to a window
//! — the gestures, the background threads and their progress — is in `canvas.rs`,
//! `decode.rs` and `window.rs`, and how those threads start (and what a window does
//! when one cannot) is `workers.rs`.
//!
//! `a11y.rs` is the one place an accessible name is set.
//!
//! Everything machine-checkable about this layer is in `tests/`: the accelerator
//! table, the accessible names, the adaptive minimum, the pixels the window draws
//! against the ones `pixlay-render render` writes, the main path, and the
//! `po/POTFILES` set.

pub mod a11y;
pub mod app;
pub mod canvas;
pub mod decode;
pub mod dialogs;
pub mod export;
pub mod i18n;
pub mod layout;
pub mod picture;
pub mod settings;
pub mod state;
pub mod window;
pub mod workers;

pub use app::run;
pub use window::EditorWindow;
