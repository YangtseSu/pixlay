//! Command dispatch and result assembly.
//!
//! Split out of the binary so that the integration tests can call the same code
//! the binary runs: the contract is about stdout, stderr and exit codes, and
//! everything that decides them lives here.

use std::ffi::OsString;
use std::io::Write;

use pixlay_core::Project;
use pixlay_render::Images;

use crate::args::{self, Command, ProbeArgs, RenderArgs, Source, USAGE};
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
    }
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
