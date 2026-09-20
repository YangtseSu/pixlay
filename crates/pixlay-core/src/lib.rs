//! Document model, templates, geometry, framing transforms, command history.
//!
//! Boundary: this crate must not depend on gtk or cairo, and must stay testable
//! without a display. Geometry is normalized — coordinates live in `[0, 1]`;
//! absolute pixels exist only at the render and export boundary.
//!
//! Contents arrive with S1 (contract), S2 (template geometry), S3 (framing and
//! clamp) and S6.5 (command history, project IO, hit testing).
