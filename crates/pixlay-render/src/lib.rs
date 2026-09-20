//! The single rendering path: `draw(doc, target)` on Cairo and pangocairo.
//!
//! Boundary: no gtk. Preview and export must both go through this crate; a
//! second rendering implementation is forbidden. The canvas only blits and
//! clips — decoding, resampling, rotation interpolation and color adjustment
//! happen in `pixlay-imaging`, so Cairo never sees a bitmap that is not already
//! the right size.
//!
//! Contents arrive with S1.
