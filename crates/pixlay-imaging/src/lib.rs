//! Decoding, resampling, color adjustment, EXIF and color space handling.
//!
//! Boundary: no gtk. The crate exposes synchronous, pure functions — threading
//! and channel plumbing belong to the caller (the CLI may parallelize, the GUI
//! must hand results back to the main thread). Recolor and 16-bit intermediate
//! buffers live here, not in the renderer.
//!
//! Contents arrive with S4.
