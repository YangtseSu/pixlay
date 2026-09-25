//! The sketch renderer: one template's geometry as ink on paper.
//!
//! A template carries geometry and no style, so a drawing of its cells' outlines
//! is a complete account of it — which is what the layout band shows instead of a
//! render of the user's own photos, and what the CLI's `render --sketch` writes
//! (ruling 32 of the plan of 2026-09-25; the band's record is `docs/CONTRACT.md`
//! §8, "S21"). The outlines are the *same* polygons [`draw`](crate::draw) places
//! photos with — [`outline_path`], the one place normalized coordinates become
//! canvas pixels — so a sketch and a render cannot disagree about where a cell is.
//!
//! Two things this deliberately does not draw: the document's frame (`gapRel` /
//! `radiusRel` / the backdrop colour are the *document's*, not the template's, and
//! a sketch is the template's geometry) and anything about a photo.

use cairo::{Context, Format, ImageSurface, LineJoin, Operator};
use pixlay_core::{PixelSize, Polygon, Rgba8, Template};

use crate::Rgb8Image;
use crate::draw::{outline_path, rgb8};
use crate::error::RenderError;

/// The sketch's parameters: the sheet's ground, the ink its cell outlines are
/// stroked in, and the stroke's width in canvas pixels.
///
/// The two colours' alpha is ignored: a sketch is an opaque drawing — its paper
/// fills the surface — so it can be written as a JPEG as well as a PNG.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sketch {
    /// The sheet's ground: the colour of everything a cell's outline is not on.
    pub paper: Rgba8,
    /// The colour the cell outlines are stroked in.
    pub ink: Rgba8,
    /// Stroke width in canvas pixels, finite and positive.
    pub stroke_px: f64,
}

impl Default for Sketch {
    /// White paper, black ink, a one-pixel line: the drawing a caller gets when
    /// it names no colours and no width — the CLI's own defaults.
    fn default() -> Self {
        Self {
            paper: Rgba8::WHITE,
            ink: Rgba8::BLACK,
            stroke_px: 1.0,
        }
    }
}

/// Draws `template` as a sketch at the pixel grid `canvas_px`.
///
/// The grid is the caller's to size, as it is for [`render_rgb8`](crate::render_rgb8):
/// `PixelSize::for_long_edge(aspect, n)` gives the declared aspect's shape, and a
/// grid of another shape stretches the geometry — normalized coordinates carry no
/// aspect of their own.
pub fn sketch_rgb8(
    template: &Template,
    canvas_px: PixelSize,
    sketch: &Sketch,
) -> Result<Rgb8Image, RenderError> {
    if !sketch.stroke_px.is_finite() || sketch.stroke_px <= 0.0 {
        return Err(RenderError::InvalidStroke(sketch.stroke_px));
    }
    // Rgb24: the paper fills the surface, so there is no alpha anywhere and the
    // conversion below has nothing to composite.
    let surface = ImageSurface::create(Format::Rgb24, canvas_px.width, canvas_px.height)?;
    let ctx = Context::new(&surface)?;

    ctx.set_operator(Operator::Source);
    set_source(&ctx, sketch.paper);
    ctx.paint()?;

    // `Source` for the ground, `Over` for the ink: an outline stroked over a
    // paper-coloured line must stay ink, not vanish into it.
    ctx.set_operator(Operator::Over);
    set_source(&ctx, sketch.ink);
    ctx.set_line_width(sketch.stroke_px);
    // A sketch's corners are geometry, not typography: the default miter join
    // keeps a right angle square.
    ctx.set_line_join(LineJoin::Miter);

    // **The sheet's own edge**, stroked inset by half a stroke so the ink lies
    // inside the paper: a normalized canvas has exactly one rectangle, and the
    // cells that reach it would otherwise lose the outer half of their line to
    // the surface and leave the sheet's right and bottom edges blank. A template
    // whose cells keep a margin inside the sheet shows both rectangles, which is
    // its geometry.
    let (width, height) = (f64::from(canvas_px.width), f64::from(canvas_px.height));
    let inset = sketch.stroke_px / 2.0;
    ctx.rectangle(
        inset,
        inset,
        (width - sketch.stroke_px).max(0.0),
        (height - sketch.stroke_px).max(0.0),
    );
    ctx.stroke()?;

    // **The half-pixel shift** for the cells. A normalized coordinate lands
    // wherever it lands, and a one-pixel line centred on a pixel *boundary*
    // covers two pixels at half each — a grey, two-pixel line. Half a pixel of
    // translation puts an even split on a pixel *centre*, so a line is the width
    // it says (measured 2026-09-26: a two-cell strip's seam at x = 64.0 was
    // 127,127,127 on columns 63 and 64; at 64.5 it is 0,0,0 on one).
    ctx.save()?;
    ctx.translate(0.5, 0.5);
    for (index, slot) in template.slots.iter().enumerate() {
        if slot.outline.points.len() < Polygon::MIN_VERTICES {
            return Err(RenderError::DegenerateSlot { slot: index });
        }
        outline_path(&ctx, &slot.outline, canvas_px);
        ctx.stroke()?;
    }
    ctx.restore()?;
    surface.flush();
    rgb8(&surface)
}

/// Sets one of the sketch's two colours, ignoring its alpha (see [`Sketch`]).
fn set_source(ctx: &Context, color: Rgba8) {
    ctx.set_source_rgb(
        f64::from(color.r) / 255.0,
        f64::from(color.g) / 255.0,
        f64::from(color.b) / 255.0,
    );
}
