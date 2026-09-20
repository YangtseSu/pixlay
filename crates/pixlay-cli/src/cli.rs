//! Command dispatch and result assembly.
//!
//! Split out of the binary so that the integration tests can call the same code
//! the binary runs: the contract is about stdout, stderr and exit codes, and
//! everything that decides them lives here.

use std::ffi::OsString;
use std::io::Write;

use pixlay_core::Project;
use pixlay_render::Images;

use crate::args::{self, Command, InitArgs, ProbeArgs, RenderArgs, Source, TemplatesArgs, USAGE};
use crate::encode::Format;
use crate::report::Report;
use crate::{content, encode, probe, stats};

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
            "--out {}: expected a .png, .jpg or .jpeg file",
            args.out.display()
        ))
    })?;

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

    let stopwatch = stats::Stopwatch::start();
    let images: Images = match &args.source {
        Source::Project(_) => content::images(
            &doc,
            content::Fill::NamedCells(&sources),
            args.content,
            args.dpi,
        )?,
        // The photo-free smoke path renders every slot, so the output shows the
        // template's geometry instead of a blank sheet.
        Source::Template(_) => {
            content::images(&doc, content::Fill::AllSlots, args.content, args.dpi)?
        }
    };
    let scale = match args.preview_px {
        Some(long_edge) => {
            let full = doc
                .canvas
                .pixel_size(args.dpi)
                .map_err(|error| Failure::Failed(error.to_string()))?;
            f64::from(long_edge) / f64::from(full.width.max(full.height))
        }
        None => 1.0,
    };
    let image = pixlay_render::render_rgb8(&doc, &images, args.dpi, scale, None)
        .map_err(|error| Failure::Failed(error.to_string()))?;
    let compose = stopwatch.elapsed();

    let encode_watch = stats::Stopwatch::start();
    let bytes = encode::write(&args.out, format, &image).map_err(Failure::Failed)?;
    let encode_ms = encode_watch.elapsed();

    let mut report = Report::new();
    report.text("status", "ok");
    report.text("command", "render");
    report.text("format", format.name());
    report.int("dpi", i64::from(args.dpi));
    report.int("cells", doc.cells.len() as i64);
    report.int("occupied", images.len() as i64);
    report.int("out_w", i64::from(image.width));
    report.int("out_h", i64::from(image.height));
    report.int("bytes", bytes as i64);
    report.text("content", args.content.name());
    if let Some(preview) = args.preview_px {
        report.int("preview_px", i64::from(preview));
    }
    add_stats(&mut report, args.stats, compose, Some(encode_ms));
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

    // Flat content: the probe separates a seam blend from the content, which
    // only works when each slot has a single color.
    let stopwatch = stats::Stopwatch::start();
    let images = content::images(
        &doc,
        content::Fill::NamedCells(&sources),
        content::Mode::Flat,
        args.dpi,
    )?;
    let image = pixlay_render::render_rgb8(&doc, &images, args.dpi, 1.0, None)
        .map_err(|error| Failure::Failed(error.to_string()))?;
    let compose = stopwatch.elapsed();

    let result = probe::probe(&doc, &image, args.dpi);
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
    add_stats(&mut report, args.stats, compose, None);
    emit(&report, args.json);

    // The numbers are the result, so they go to stdout; the verdict on stderr,
    // and the exit code says whether the document passed.
    match result.failure() {
        Some(reason) => Err(Failure::Failed(format!("probe failed: {reason}"))),
        None => Ok(EXIT_SUCCESS),
    }
}

fn add_stats(
    report: &mut Report,
    enable: bool,
    compose: std::time::Duration,
    encode: Option<std::time::Duration>,
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
    report.text("icc", stats::ICC);
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
