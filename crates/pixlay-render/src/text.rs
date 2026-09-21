//! Canvas-level text layers: one Pango layout per layer, drawn by [`draw`].
//!
//! Text is a *content* layer: the frozen evaluation order puts it after the slots,
//! so a caption's position is in canvas space and rotating or reframing a photo
//! under it cannot move it (`AGENTS.md`, "Hard constraints"). The tiled
//! watermark is the same layer with a different mode — one mechanism, not two.
//!
//! # What decides the pixels
//!
//! * **Font size** is `size_rel * canvas height` in *canvas* pixels, so the layout
//!   is computed in the same space at every scale. A preview at `scale = 0.25`
//!   lays the text out identically to the export and only the glyph raster is
//!   drawn smaller, which is what keeps "preview and export agree" true with text.
//! * **Hinting and subpixel antialiasing are off** ([`draw_layers`] sets them on
//!   the context): hinted metrics snap glyph positions to device pixels, which
//!   makes the same document lay out differently at two scales, and LCD filtering
//!   would put colored fringes on an orange-peel-smooth export.
//! * **The family is the system's `sans-serif`.** v1 has no font field
//!   (`docs/CONTRACT.md` §6), so the
//!   renderer asks fontconfig for the generic family and never names a font;
//!   tests pin the environment with `FONTCONFIG_FILE` instead.
//! * **Line breaking is Pango's**, which is the product's answer for CJK: it
//!   implements the Unicode line-breaking rules, so no line starts with `。`, `，`,
//!   `）”` and no line ends with `（` (kinsoku, `docs/STEPS.md` S5).
//! * **Punctuation squeezing is ours**, through the font's `halt` feature — see
//!   [`compressed_ranges`].

use cairo::Context;
use pangocairo::pango;
use pixlay_core::{Anchor, CollageDoc, PixelSize, TextLayer, TextMode, tiled_grid};

use crate::bitmap::Images;
use crate::error::RenderError;

/// The family every text layer is drawn in.
///
/// Generic on purpose: fontconfig resolves it to whatever the user's system
/// considers its sans (on Arch, Noto Sans / Noto Sans CJK), and v1 has no font
/// field for a document to name its own. Nothing here hard-codes a font name, so
/// nothing here can fail because a particular font is missing.
const FAMILY: &str = "sans-serif";

/// The OpenType feature that draws a CJK mark at half width.
///
/// `halt` — "alternate half widths" — is a *positioning* feature (GPOS), which is
/// what makes it the right tool: the advance shrinks while the glyph comes from
/// the font, so how a compressed mark looks stays the font designer's decision.
const COMPRESSION_FEATURE: &str = "halt";

/// CJK punctuation whose blank a neighbouring mark absorbs (JLREQ 3.1.2,
/// 連続する約物; CLREQ's 标点挤压): commas, periods, brackets and quotes, opening
/// and closing alike.
const COMPRESSIBLE: &str = "、。，．：；？！）］｝〕〉》」』】〙〛’”（［｛〔〈《「『【〘〚“‘・ー…‥";

/// Draws every text layer of `doc`, in array order, over what is already there.
///
/// `ctx` must be in canvas pixels, exactly as [`draw`](crate::draw) leaves it: the
/// layers are positioned in canvas space and are not affected by bands or scale.
pub(crate) fn draw_layers(
    ctx: &Context,
    doc: &CollageDoc,
    images: &Images,
    canvas: PixelSize,
) -> Result<(), RenderError> {
    if doc.text.is_empty() {
        return Ok(());
    }
    // Set once, before any layout exists: the options are read when a font is
    // created, so setting them later would apply to nothing.
    let mut options = cairo::FontOptions::new()?;
    options.set_hint_style(cairo::HintStyle::None);
    options.set_hint_metrics(cairo::HintMetrics::Off);
    options.set_antialias(cairo::Antialias::Gray);
    ctx.set_font_options(&options);

    let (canvas_width, canvas_height) = (f64::from(canvas.width), f64::from(canvas.height));
    for layer in &doc.text {
        let values = layer
            .source_slot
            .and_then(|slot| images.text_values(slot))
            .cloned()
            .unwrap_or_default();
        let content = layer.resolve(&values, &doc.text_fallback);
        if content.is_empty() {
            continue;
        }
        let font_px = layer.size_rel * canvas_height;
        ctx.set_source_rgba(
            f64::from(layer.color.r) / 255.0,
            f64::from(layer.color.g) / 255.0,
            f64::from(layer.color.b) / 255.0,
            f64::from(layer.color.a) / 255.0,
        );
        match layer.mode {
            TextMode::Free { position, anchor } => {
                // The box is the canvas width: long text wraps at the canvas edge
                // rather than running off it, and the box that results is what
                // `anchor` places. The canvas is the only width a layer has — v1
                // gives a text layer no size of its own (`docs/CONTRACT.md` §6).
                let layout = layout(ctx, &content, font_px, Some(canvas_width));
                let (offset_x, offset_y) = anchor_offset(&layout, anchor);
                ctx.save()?;
                ctx.translate(position.x * canvas_width, position.y * canvas_height);
                ctx.rotate(layer.rotation_deg.to_radians());
                // `move_to`, not `rel_move_to`: the canvas has no current point
                // until something moves to one, and the offset is already relative
                // to the anchor the context is translated to.
                ctx.move_to(offset_x, offset_y);
                pangocairo::functions::show_layout(ctx, &layout);
                ctx.restore()?;
            }
            TextMode::Tiled { step } => {
                // A watermark is one mark per tile, so its tiles are not wrapped:
                // at the canvas width a long string would break into lines that
                // overlap the tile below it.
                let layout = layout(ctx, &content, font_px, None);
                let (offset_x, offset_y) = box_origin(&layout);
                let (columns, rows) = tiled_grid(step).ok_or(RenderError::TooManyTiles {
                    x: step.0,
                    y: step.1,
                    max: TextLayer::MAX_TILES,
                })?;
                for column in 0..columns {
                    for row in 0..rows {
                        // The grid starts at the canvas origin: the first tile's
                        // anchor *is* (0, 0), and each tile rotates about its own
                        // anchor (contract §1, "Tiled phase").
                        ctx.save()?;
                        ctx.translate(
                            column as f64 * step.0 * canvas_width,
                            row as f64 * step.1 * canvas_height,
                        );
                        ctx.rotate(layer.rotation_deg.to_radians());
                        // The same pen offset as the free case: this anchor is the
                        // tile's grid point, and `move_to` is already relative to it.
                        ctx.move_to(offset_x, offset_y);
                        pangocairo::functions::show_layout(ctx, &layout);
                        ctx.restore()?;
                    }
                }
            }
        }
    }
    Ok(())
}

/// Lays `content` out at `font_px`, optionally wrapped at `wrap_px`.
///
/// Public because the tests measure *this* layout — line ends after Pango's line
/// breaking, and the advance of a compressed mark — rather than a copy of it.
pub fn layout(ctx: &Context, content: &str, font_px: f64, wrap_px: Option<f64>) -> pango::Layout {
    let layout = pangocairo::functions::create_layout(ctx);
    let mut description = pango::FontDescription::new();
    description.set_family(FAMILY);
    // `set_absolute_size` is in pango units (1/1024 px), not in pixels: the spike
    // measured 0.07 px text from forgetting the multiplication.
    description.set_absolute_size(font_px * f64::from(pango::SCALE));
    layout.set_font_description(Some(&description));
    layout.set_text(content);
    layout.set_width(match wrap_px {
        Some(width) => (width * f64::from(pango::SCALE)).round() as i32,
        None => -1,
    });
    let ranges = compressed_ranges(content);
    if !ranges.is_empty() {
        let attributes = pango::AttrList::new();
        for (start, end) in ranges {
            let mut feature = pango::AttrFontFeatures::new(COMPRESSION_FEATURE);
            feature.set_start_index(start);
            feature.set_end_index(end);
            attributes.insert(feature);
        }
        layout.set_attributes(Some(&attributes));
    }
    layout
}

/// The byte ranges to draw at half width: every CJK mark that another CJK mark
/// follows immediately.
///
/// Pango does not compress punctuation by itself — measured 2026-09-21, `。，` is
/// two full ems, exactly like two isolated marks — and the squeeze is a *typographic*
/// rule about a run, not a property of a character: a mark that stands alone keeps
/// its full width, which is what keeps the blank half of `。` doing its job as the
/// end of a sentence. So the run's marks are asked for `halt` and the last one is
/// left alone: `。”` costs 1.5 em instead of 2, `。。` the same, and a lone `。`
/// still costs 1.
///
/// The ranges are byte ranges into `content`, which is what Pango's attributes are
/// indexed by. A `\n` ends a run: the marks either side of it are on different
/// lines, and compressing across a line break would pull a line's start out of
/// alignment with the rest.
fn compressed_ranges(content: &str) -> Vec<(u32, u32)> {
    let mut ranges = Vec::new();
    let mut previous: Option<(u32, u32)> = None;
    for (offset, character) in content.char_indices() {
        let start = offset as u32;
        let end = start + character.len_utf8() as u32;
        if !COMPRESSIBLE.contains(character) {
            previous = None;
            continue;
        }
        if let Some(open) = previous {
            ranges.push(open);
        }
        previous = Some((start, end));
    }
    ranges
}

/// Where to put the pen so that the layout's box sits under `anchor`.
///
/// The caller has already moved to the anchor point, so this is the offset back to
/// the box's top-left corner: the anchor's fraction of the box, plus the box's own
/// reported origin. `show_layout` draws the layout at the current point, but the
/// box it reports starts at `logical.x, logical.y` — a wrapped paragraph's first
/// line can be indented, so the two are not the same point.
fn anchor_offset(layout: &pango::Layout, anchor: Anchor) -> (f64, f64) {
    let (_, logical) = layout.extents();
    let (width, height) = (
        f64::from(logical.width()) / f64::from(pango::SCALE),
        f64::from(logical.height()) / f64::from(pango::SCALE),
    );
    let (offset_x, offset_y) = (
        f64::from(logical.x()) / f64::from(pango::SCALE),
        f64::from(logical.y()) / f64::from(pango::SCALE),
    );
    let (fraction_x, fraction_y) = anchor_fractions(anchor);
    (
        offset_x - fraction_x * width,
        offset_y - fraction_y * height,
    )
}

/// Which point of its own box an anchor names, as `(x, y)` fractions.
fn anchor_fractions(anchor: Anchor) -> (f64, f64) {
    let x = match anchor {
        Anchor::TopLeft | Anchor::CenterLeft | Anchor::BottomLeft => 0.0,
        Anchor::TopCenter | Anchor::Center | Anchor::BottomCenter => 0.5,
        Anchor::TopRight | Anchor::CenterRight | Anchor::BottomRight => 1.0,
    };
    let y = match anchor {
        Anchor::TopLeft | Anchor::TopCenter | Anchor::TopRight => 0.0,
        Anchor::CenterLeft | Anchor::Center | Anchor::CenterRight => 0.5,
        Anchor::BottomLeft | Anchor::BottomCenter | Anchor::BottomRight => 1.0,
    };
    (x, y)
}

/// The pen offset that puts the layout's box's top-left corner on the origin — the
/// tiled mode's anchor (contract §1: the first tile's anchor is the canvas origin).
fn box_origin(layout: &pango::Layout) -> (f64, f64) {
    anchor_offset(layout, Anchor::TopLeft)
}
