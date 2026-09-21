//! Command dispatch and result assembly.
//!
//! Split out of the binary so that the integration tests can call the same code
//! the binary runs: the contract is about stdout, stderr and exit codes, and
//! everything that decides them lives here.

use std::ffi::OsString;
use std::io::Write;

use std::collections::BTreeMap;
use std::path::PathBuf;

use pixlay_core::{PixelSize, Project, TextValues};
use pixlay_imaging::{Chroma, Export, Format, Rgb8View, SlotBitmap, icc};
use pixlay_render::Images;

use crate::args::{
    self, Command, ImageArgs, InitArgs, ProbeArgs, RenderArgs, Size, Source, TemplatesArgs,
    TextArgs, USAGE,
};
use crate::report::Report;
use crate::stats;

/// Exit codes, as the contract fixes them.
pub const EXIT_SUCCESS: u8 = 0;
pub const EXIT_USAGE: u8 = 1;
pub const EXIT_FAILURE: u8 = 2;

/// A failure with the exit code it maps to.
#[derive(Debug)]
pub enum Failure {
    /// Bad command line: exit 1.
    Usage(String),
    /// The input could not be read, decoded or rendered: exit 2.
    Failed(String),
}

/// Runs one command and prints its result.
pub fn run(argv: &[OsString]) -> Result<u8, Failure> {
    match args::parse(argv)? {
        Command::Help => {
            print!("{USAGE}");
            Ok(EXIT_SUCCESS)
        }
        Command::Version => {
            println!("pixlay-render {}", env!("CARGO_PKG_VERSION"));
            Ok(EXIT_SUCCESS)
        }
        Command::Render(args) => render(args),
        Command::Probe(args) => probe(args),
        Command::Image(args) => image(args),
        Command::Text(args) => text(args),
        Command::Templates(args) => list_templates(args),
        Command::Init(args) => init_project(args),
    }
}

/// `templates`: the library, optionally filtered to one canvas shape.
///
/// This is the query S7's picker runs and the one a caller needs before it can
/// name a template: a canvas and a template only fit each other when their aspect
/// ratios agree, and the canvas is what the user picks first. The list stays in
/// library order (by slot count), so the output is stable.
fn list_templates(args: TemplatesArgs) -> Result<u8, Failure> {
    // The filter is the library's own query, so `templates --aspect 4:3` and the
    // picker cannot disagree about what "the same aspect" means.
    let listed = match args.aspect {
        Some(aspect) => pixlay_core::templates::of_aspect(aspect),
        None => pixlay_core::templates::all(),
    };
    let mut report = Report::new();
    report.text("status", "ok");
    report.text("command", "templates");
    if let Some(aspect) = args.aspect {
        report.text("aspect", ratio_label(aspect));
    }
    for (index, template) in listed.iter().enumerate() {
        let prefix = report.row("template", index);
        report.text(&format!("{prefix}.name"), template.name.clone());
        report.int(&format!("{prefix}.slots"), template.slots.len() as i64);
        report.text(&format!("{prefix}.aspect"), ratio_label(template.aspect));
        report.int(&format!("{prefix}.version"), i64::from(template.version));
    }
    report.int("count", listed.len() as i64);
    emit(&report, args.json);
    Ok(EXIT_SUCCESS)
}

/// `init`: a template as a loadable, photo-free project.
///
/// Callers should not have to hand-write `.pixlay` JSON — the format has one
/// canonical writer (`CollageDoc::to_json`) and this is where they reach it. The
/// file is never overwritten: replacing a project the user already has is not
/// something a command called `init` should do quietly.
fn init_project(args: InitArgs) -> Result<u8, Failure> {
    let template = pixlay_core::templates::get(&args.template).ok_or_else(|| {
        Failure::Usage(format!(
            "unknown template {}; this build knows: {}",
            args.template,
            pixlay_core::templates::names().join(", ")
        ))
    })?;
    let doc = pixlay_core::templates::document(&template);
    // Validating before writing is cheap, and it keeps a bug in the library from
    // shipping as an unloadable file.
    doc.validate()
        .map_err(|error| Failure::Failed(error.to_string()))?;
    if args.out.exists() {
        return Err(Failure::Failed(format!(
            "{} exists; init never overwrites a project",
            args.out.display()
        )));
    }
    let json = doc
        .to_json()
        .map_err(|error| Failure::Failed(error.to_string()))?;
    std::fs::write(&args.out, &json)
        .map_err(|error| Failure::Failed(format!("{}: {error}", args.out.display())))?;

    let mut report = Report::new();
    report.text("status", "ok");
    report.text("command", "init");
    report.text("template", doc.template.name.clone());
    report.int("version", i64::from(doc.template.version));
    report.text("aspect", ratio_label(doc.template.aspect));
    report.text(
        "canvas",
        format!("{}x{}", doc.canvas.width_mm, doc.canvas.height_mm),
    );
    report.int("cells", doc.cells.len() as i64);
    report.int("bytes", json.len() as i64);
    emit(&report, args.json);
    Ok(EXIT_SUCCESS)
}

/// `W:H` when the ratio is one a person would name, a decimal otherwise.
///
/// The recipes declare ratios as divisions of small integers (`4/3` is
/// `1.3333333333333333`), and a caller comparing `--aspect` values by hand reads
/// `4:3` far more easily. Matching is on the numeric value, so a ratio that only
/// prints approximately stays usable.
fn ratio_label(aspect: f64) -> String {
    // Small-integer ratios cover every canvas shape the product has a name for.
    for (width, height) in [
        (1, 1),
        (4, 3),
        (3, 2),
        (16, 9),
        (2, 3),
        (3, 4),
        (9, 16),
        (2, 1),
        (1, 2),
    ] {
        if (f64::from(width) / f64::from(height) - aspect).abs() <= 1e-12 {
            return format!("{width}:{height}");
        }
    }
    format!("{aspect:.6}")
}

fn render(args: RenderArgs) -> Result<u8, Failure> {
    let format = Format::from_path(&args.out).ok_or_else(|| {
        Failure::Usage(format!(
            "--out {}: expected {}",
            args.out.display(),
            Format::EXTENSIONS
        ))
    })?;
    // A dropped flag that looks honored is worse than a refusal (S1's rule for
    // `--dpi` on `templates`): PNG and TIFF store three samples per pixel, so
    // there is nothing for `--chroma` to set.
    if args.chroma != Chroma::default() && format != Format::Jpeg {
        return Err(Failure::Usage(format!(
            "--chroma applies to JPEG only, and {} stores every sample",
            format.name()
        )));
    }

    // Load the document first: a broken project must fail before anything is
    // rendered, and its message must name the path that is wrong.
    let (doc, sources) = match &args.source {
        Source::Project(path) => {
            let project =
                Project::load(path).map_err(|error| Failure::Failed(error.to_string()))?;
            let sources = project
                .sources()
                .map_err(|error| Failure::Failed(error.to_string()))?;
            (project.doc().clone(), sources)
        }
        Source::Template(name) => {
            let template = pixlay_core::templates::get(name).ok_or_else(|| {
                Failure::Usage(format!(
                    "unknown template {name}; this build knows: {}",
                    pixlay_core::templates::names().join(", ")
                ))
            })?;
            (pixlay_core::templates::document(&template), Vec::new())
        }
    };
    doc.validate()
        .map_err(|error| Failure::Failed(error.to_string()))?;

    // The pixel grid, and the resolution the file has to carry. A `--dpi` export
    // echoes the resolution back; a `--long-edge` export derives it from the grid
    // it actually renders (`CanvasSpec::dpi_for`).
    let (canvas_px, dpi) = match args.size {
        Size::Dpi(dpi) => (
            doc.canvas
                .pixel_size(dpi)
                .map_err(|error| Failure::Failed(error.to_string()))?,
            f64::from(dpi),
        ),
        Size::LongEdge(pixels) => {
            let pixel = doc
                .canvas
                .pixel_size_for_long_edge(pixels)
                .map_err(|error| Failure::Failed(error.to_string()))?;
            (pixel, doc.canvas.dpi_for(pixel))
        }
    };
    let scale = match args.preview_px {
        Some(long_edge) => f64::from(long_edge) / f64::from(canvas_px.width.max(canvas_px.height)),
        None => 1.0,
    };
    // The bitmaps are sized in the space `draw` writes into, so a preview decodes
    // and resamples at preview size instead of paying for the export and letting
    // Cairo shrink it. `draw`'s own `scale` then brings canvas pixels to device
    // pixels, and the pattern is 1:1 in both cases.
    let bitmap_px = PixelSize {
        width: pixlay_render::output_px(canvas_px.width, scale),
        height: pixlay_render::output_px(canvas_px.height, scale),
    };

    let stopwatch = stats::Stopwatch::start();
    // The photo-free path has no cells to decode: every slot stays white.
    let images = match &args.source {
        Source::Project(_) => decode_slots(&doc, &sources, bitmap_px)?,
        Source::Template(_) => Images::new(),
    };
    let image = pixlay_render::render_rgb8_sized(&doc, &images, canvas_px, scale, None)
        .map_err(|error| Failure::Failed(error.to_string()))?;
    let compose = stopwatch.elapsed();

    // Pixels and metadata in one pass: the encoder writes the resolution and the
    // sRGB profile while it writes the image (`pixlay_imaging::encode`).
    let encode_watch = stats::Stopwatch::start();
    let bytes = pixlay_imaging::encode::write(
        &args.out,
        &Export {
            format,
            dpi,
            chroma: args.chroma,
            image: Rgb8View {
                width: image.width,
                height: image.height,
                data: &image.data,
            },
        },
    )
    .map_err(|error| Failure::Failed(error.to_string()))?;
    let encode_ms = encode_watch.elapsed();

    let mut report = Report::new();
    report.text("status", "ok");
    report.text("command", "render");
    report.text("format", format.name());
    match args.size {
        // An integer in physical mode: the resolution the user asked for, which is
        // what the file carries.
        Size::Dpi(dpi) => {
            report.int("dpi", i64::from(dpi));
        }
        // A decimal in pixel mode: the resolution the grid works out to, which is
        // the number the file carries.
        Size::LongEdge(pixels) => {
            report.int("long_edge", i64::from(pixels));
            report.float("dpi", dpi);
        }
    }
    if format == Format::Jpeg {
        report.text("chroma", args.chroma.name());
    }
    report.int("cells", doc.cells.len() as i64);
    report.int("occupied", images.len() as i64);
    report.int("text", doc.text.len() as i64);
    report.int("out_w", i64::from(image.width));
    report.int("out_h", i64::from(image.height));
    report.int("bytes", bytes as i64);
    if let Some(preview) = args.preview_px {
        report.int("preview_px", i64::from(preview));
    }
    add_stats(
        &mut report,
        args.stats,
        compose,
        Some(encode_ms),
        icc::DESCRIPTION,
    );
    emit(&report, args.json);
    Ok(EXIT_SUCCESS)
}

/// Decodes every occupied cell into the bitmap set `draw` consumes.
///
/// The buffer ladder is `pixlay_imaging`'s (docs/CONTRACT.md §4): one source at a
/// time, and the bitmaps are as large as the slots show. Nothing is cached
/// between slots — a project that points all its cells at one file decodes it
/// once per cell, which is the price of the ladder being `source + sum(bitmaps)`
/// rather than `sources + sum(bitmaps)`.
fn decode_slots(
    doc: &pixlay_core::CollageDoc,
    sources: &[Option<PathBuf>],
    canvas_px: pixlay_core::PixelSize,
) -> Result<Images, Failure> {
    let bitmaps = pixlay_imaging::slot_bitmaps(doc, canvas_px, sources)
        .map_err(|error| Failure::Failed(error.to_string()))?;
    let mut images = Images::new();
    for bitmap in bitmaps {
        images.insert(bitmap.slot, render_bitmap(&bitmap)?);
        // The decode already saw the file, so `{date}` and `{filename}` cost
        // nothing extra here (`pixlay_render::Images`).
        images.set_text_values(
            bitmap.slot,
            slot_values(
                sources.get(bitmap.slot).and_then(Option::as_ref),
                bitmap.date.clone(),
            ),
        );
    }
    Ok(images)
}

/// What one slot tells a text layer: the file name the project points at, and the
/// date the decoder found in it.
fn slot_values(source: Option<&PathBuf>, date: Option<String>) -> TextValues {
    TextValues {
        date,
        filename: source
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned()),
    }
}

/// `text`: every layer's content, with its tokens resolved.
///
/// `{date}`, `{filename}` and `{index}` come from a slot's photo, so the machine
/// surface needs a way to read what they resolve to without rendering a project
/// and reading pixels back — this is S5's `image`. Only the slots a layer actually
/// names are decoded, and each of those once.
fn text(args: TextArgs) -> Result<u8, Failure> {
    let project =
        Project::load(&args.project).map_err(|error| Failure::Failed(error.to_string()))?;
    let doc = project.doc().clone();
    let sources = project
        .sources()
        .map_err(|error| Failure::Failed(error.to_string()))?;

    let mut decoded: BTreeMap<usize, TextValues> = BTreeMap::new();
    for slot in doc.text.iter().filter_map(|layer| layer.source_slot) {
        if decoded.contains_key(&slot) {
            continue;
        }
        let source = sources.get(slot).and_then(Option::as_ref);
        let date = match source {
            Some(path) => {
                let decoded = pixlay_imaging::Source::decode(path)
                    .map_err(|error| Failure::Failed(error.to_string()))?;
                decoded
                    .exif()
                    .and_then(pixlay_imaging::exif::date_time_original)
            }
            None => None,
        };
        decoded.insert(slot, slot_values(source, date));
    }

    let mut report = Report::new();
    report.text("status", "ok");
    report.text("command", "text");
    report.int("count", doc.text.len() as i64);
    for (index, layer) in doc.text.iter().enumerate() {
        let values = layer
            .source_slot
            .and_then(|slot| decoded.get(&slot))
            .cloned()
            .unwrap_or_default();
        let prefix = report.row("text", index);
        report.text(
            &format!("{prefix}.content"),
            layer.resolve(&values, &doc.text_fallback),
        );
        report.text(&format!("{prefix}.mode"), mode_name(layer));
        report.float(&format!("{prefix}.size_rel"), layer.size_rel);
        report.float(&format!("{prefix}.rotation_deg"), layer.rotation_deg);
        if let Some(slot) = layer.source_slot {
            report.int(&format!("{prefix}.source_slot"), slot as i64);
        }
        match layer.mode {
            pixlay_core::TextMode::Free { position, anchor } => {
                report.text(
                    &format!("{prefix}.position"),
                    format!("{:.4},{:.4}", position.x, position.y),
                );
                report.text(&format!("{prefix}.anchor"), anchor_name(anchor));
            }
            pixlay_core::TextMode::Tiled { step } => {
                report.text(
                    &format!("{prefix}.step"),
                    format!("{:.4},{:.4}", step.0, step.1),
                );
                // The grid the renderer draws, so the tile cap is visible here
                // instead of only in a render that never finishes.
                if let Some((columns, rows)) = pixlay_core::tiled_grid(step) {
                    report.int(&format!("{prefix}.tiles"), (columns * rows) as i64);
                }
            }
        }
    }
    emit(&report, args.json);
    Ok(EXIT_SUCCESS)
}

fn mode_name(layer: &pixlay_core::TextLayer) -> &'static str {
    match layer.mode {
        pixlay_core::TextMode::Free { .. } => "free",
        pixlay_core::TextMode::Tiled { .. } => "tiled",
    }
}

/// The anchor as the document spells it, so the report round-trips into a project.
fn anchor_name(anchor: pixlay_core::Anchor) -> &'static str {
    use pixlay_core::Anchor;
    match anchor {
        Anchor::TopLeft => "topLeft",
        Anchor::TopCenter => "topCenter",
        Anchor::TopRight => "topRight",
        Anchor::CenterLeft => "centerLeft",
        Anchor::Center => "center",
        Anchor::CenterRight => "centerRight",
        Anchor::BottomLeft => "bottomLeft",
        Anchor::BottomCenter => "bottomCenter",
        Anchor::BottomRight => "bottomRight",
    }
}

/// Wraps one decoded slot for the renderer, keeping the region it holds.
fn render_bitmap(bitmap: &SlotBitmap) -> Result<pixlay_render::Bitmap, Failure> {
    pixlay_render::Bitmap::from_argb32_region(
        bitmap.width as i32,
        bitmap.height as i32,
        bitmap.origin,
        bitmap.display,
        bitmap.pixels.clone(),
    )
    .map_err(|error| Failure::Failed(error.to_string()))
}

/// `image`: what the decoder found in one file.
///
/// This is S4's machine-visible decode: the MIME type the loader detected, the
/// size *after* EXIF rotation, the sample depth, and the EXIF date when the file
/// carries one. Without it "HEIC decodes" and "orientation 6 is applied" could
/// only be checked by rendering a project and reading pixels back.
fn image(args: ImageArgs) -> Result<u8, Failure> {
    let source = pixlay_imaging::Source::decode(&args.photo)
        .map_err(|error| Failure::Failed(error.to_string()))?;
    let mut report = Report::new();
    report.text("status", "ok");
    report.text("command", "image");
    report.text("mime", source.mime().to_string());
    report.int("width", i64::from(source.width()));
    report.int("height", i64::from(source.height()));
    report.text(
        "depth",
        match source.depth() {
            pixlay_imaging::Depth::Eight => "8",
            pixlay_imaging::Depth::Sixteen => "16",
        },
    );
    report.text("aspect", format!("{:.6}", source.aspect()));
    match source.exif() {
        Some(exif) => {
            report.int("exif_bytes", exif.len() as i64);
            let date = pixlay_imaging::exif::date_time_original(exif);
            report.text("date", date.unwrap_or_default());
        }
        None => {
            report.int("exif_bytes", 0);
            report.text("date", "");
        }
    }
    emit(&report, args.json);
    Ok(EXIT_SUCCESS)
}

fn probe(args: ProbeArgs) -> Result<u8, Failure> {
    let project =
        Project::load(&args.project).map_err(|error| Failure::Failed(error.to_string()))?;
    let sources = project
        .sources()
        .map_err(|error| Failure::Failed(error.to_string()))?;
    let doc = project.doc().clone();
    // The probe's questions are about pixels *it* painted: an interior sample is
    // "the colour of this slot" and a background sample is "white". A text layer
    // paints over both, and a probe cannot tell a watermark from a wrong photo —
    // so it refuses the document rather than answering about it.
    if !doc.text.is_empty() {
        return Err(Failure::Failed(format!(
            "probe cannot judge a document with {} text layer(s): its samples assume \
             nothing is painted over the slots",
            doc.text.len()
        )));
    }

    let stopwatch = stats::Stopwatch::start();
    let full = doc
        .canvas
        .pixel_size(args.dpi)
        .map_err(|error| Failure::Failed(error.to_string()))?;
    // The probe renders **its own** content: flat colors, one per occupied cell
    // (`pixlay_imaging::probe_bitmaps`). Real photos cannot be probed — a white
    // photo has a white interior and a photo with a hard edge beside a seam has no
    // measurable blend — and the contract fixes flat content for the probe.
    //
    // The render is full size: the probe samples pixel coordinates, and a preview
    // would move every one of them.
    let bitmaps = pixlay_imaging::probe::probe_bitmaps(&doc, full, &sources)
        .map_err(|error| Failure::Failed(error.to_string()))?;
    let mut images = Images::new();
    for bitmap in &bitmaps {
        images.insert(bitmap.slot, render_bitmap(bitmap)?);
    }
    let image = pixlay_render::render_rgb8(&doc, &images, args.dpi, 1.0, None)
        .map_err(|error| Failure::Failed(error.to_string()))?;
    let compose = stopwatch.elapsed();

    // The probe reads the render itself: with real photos there is no placeholder
    // color to compare against (see `pixlay_imaging::probe`).
    let view = Rgb8View {
        width: image.width,
        height: image.height,
        data: &image.data,
    };
    let result = pixlay_imaging::probe::probe(&doc, &view, args.dpi);
    let mut report = Report::new();
    report.text("status", if result.ok() { "ok" } else { "failed" });
    report.text("command", "probe");
    report.int("dpi", i64::from(args.dpi));
    report.int("slots", result.slots as i64);
    report.int("occupied", result.occupied.len() as i64);
    report.int("out_w", i64::from(result.width));
    report.int("out_h", i64::from(result.height));
    report.int("bg_samples", result.background.samples as i64);
    report.int("bg_non_white", result.background.non_white as i64);
    for interior in &result.interiors {
        let prefix = report.row("slot", interior.slot);
        report.text(
            &format!("{prefix}.at"),
            format!("{:.4},{:.4}", interior.at.x, interior.at.y),
        );
        report.text(&format!("{prefix}.expected"), rgb(interior.expected));
        report.text(&format!("{prefix}.actual"), rgb(interior.actual));
        report.float(&format!("{prefix}.depth_px"), interior.depth_px);
        report.bool(&format!("{prefix}.match"), interior.matches());
    }
    for (index, seam) in result.seams.iter().enumerate() {
        let prefix = report.row("seam", index);
        report.int(&format!("{prefix}.a"), seam.a as i64);
        report.int(&format!("{prefix}.b"), seam.b as i64);
        report.float(&format!("{prefix}.length_px"), seam.length_px);
        report.int(&format!("{prefix}.rows"), seam.rows as i64);
        report.int(&format!("{prefix}.blended"), seam.blended as i64);
        report.int(&format!("{prefix}.max_run"), seam.max_run as i64);
        report.float(&format!("{prefix}.per_px"), seam.per_px());
        report.float(&format!("{prefix}.max_residual"), seam.max_residual);
        report.int(&format!("{prefix}.foreign"), seam.foreign as i64);
        report.bool(&format!("{prefix}.clean"), seam.is_clean());
    }
    report.bool("passed", result.ok());
    add_stats(&mut report, args.stats, compose, None, "none");
    emit(&report, args.json);

    // The numbers are the result, so they go to stdout; the verdict on stderr,
    // and the exit code says whether the document passed.
    match result.failure() {
        Some(reason) => Err(Failure::Failed(format!("probe failed: {reason}"))),
        None => Ok(EXIT_SUCCESS),
    }
}

/// Appends the ruler's fields. `icc` is the profile the *file* carries, so a
/// command that writes no file reports `none` rather than naming a profile
/// nothing embedded.
fn add_stats(
    report: &mut Report,
    enable: bool,
    compose: std::time::Duration,
    encode: Option<std::time::Duration>,
    icc: &str,
) {
    if !enable {
        return;
    }
    report.float("ms", compose.as_secs_f64() * 1000.0);
    if let Some(encode) = encode {
        report.float("encode_ms", encode.as_secs_f64() * 1000.0);
    }
    if let Some(peak) = stats::peak_rss_mb() {
        report.float("peak_rss_mb", peak);
    }
    report.text("icc", icc);
}

fn emit(report: &Report, json: bool) {
    let text = if json { report.json() } else { report.lines() };
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    // A closed pipe (`| head`) is not an error worth an exit code.
    let _ = handle.write_all(text.as_bytes());
    let _ = handle.flush();
}

fn rgb(pixel: [u8; 3]) -> String {
    format!("{},{},{}", pixel[0], pixel[1], pixel[2])
}
