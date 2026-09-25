//! Command dispatch and result assembly.
//!
//! Split out of the binary so that the integration tests can call the same code
//! the binary runs: the contract is about stdout, stderr and exit codes, and
//! everything that decides them lives here.

use std::ffi::OsString;
use std::io::Write;
use std::os::unix::ffi::OsStrExt;

use std::path::{Path, PathBuf};

use pixlay_core::{Command as Edit, CropTransform, History, PixelSize, Project, canvas_grid};
use pixlay_imaging::destination::refuse_source_alias;
use pixlay_imaging::preview::{GESTURE_STEP_DEG, preview_source_long_edge};
use pixlay_imaging::{Export, Format, Preview, Rgb8View, SlotBitmap, gesture_grid, icc};
use pixlay_render::Images;

use crate::args::{
    self, Command, DEFAULT_LONG_EDGE_PX, EditArgs, GestureArgs, HitArgs, ImageArgs, InitArgs,
    ProbeArgs, RenderArgs, SaveArgs, ScanArgs, Source, SwitchArgs, TemplatesArgs, ThumbArgs, USAGE,
};
use crate::report::Report;
use crate::stats;

/// How long one step of a live gesture has to fit in, in milliseconds.
///
/// `1000 / 60`: a step is one frame of a gesture the user is dragging, and a step
/// that misses a frame at 60 Hz is one they can see. It is the threshold ruling 1
/// (2026-09-22) hands the decision to — below it the preview keeps the single
/// cairo renderer, above it the preview gains a second, GPU one with the
/// divergence risk that ruling accepted — so the constant names a frame rate
/// rather than a target somebody picked.
pub const GESTURE_STEP_BUDGET_MS: f64 = 1000.0 / 60.0;

/// Budget for one layout switch, in milliseconds: the click to the frame that shows
/// the new preview (S18).
///
/// **It is S18's own baseline, measured before any optimisation** — the human's
/// finding 1 of 2026-09-25 ("switching a layout in the editor takes too long before
/// the new preview is on screen") turned into a number, so that a decision can be
/// made about it. The measured source: `--release` on this machine, 2026-09-25, the
/// eight-photo verification project clicked in a **fresh session** at the editor's
/// default window (canvas widget 1100x575), once per candidate the band offers —
/// the window's canvas half (three runs each) and the CLI's `switch_ms`, which is
/// the same pipeline without the blit and the frame:
///
/// | click | grid | window | CLI |
/// |---|---|---|---|
/// | `mosaic-8-s14` → `strip-8-8x1` | 735x551 → 980x551 | 144.9 / 146.8 / 146.8 ms | 123.0 / 123.2 / 125.2 ms |
/// | `mosaic-8-s14` → `grid-8-4x2` | 735x551 → 827x551 | 199.7 / 201.7 / 202.2 ms | 174.2 / 178.0 / 181.2 ms |
///
/// Both clicks move the preview-grade edge, so both decode all seven files (the
/// copies are keyed by the edge, S15f/PIX-004). The budget is the worse row's worst
/// run rounded up to the next 10 ms, so it is a regression line rather than a
/// tolerance: what it says is "a layout switch costs about this much". The CLI's
/// numbers are this table's floor — the difference is one display frame plus the
/// window's own blit, measured 2026-09-25 at a constant **+21 ms** on both rows —
/// and the full table is in `docs/CONTRACT.md` §8, "S18".
///
/// Two numbers belong beside it and neither is in it: the **band's** rebuild after
/// the click (165.7–168.0 ms in the window, more than the canvas half — S21
/// replaces it with a sketch) is `band_ms`, and 100 ms is the usual instant-response
/// threshold, which this baseline does not meet — whether that matters is the
/// human's ruling, which this number exists to be given against.
///
/// It governs the **preview** and not the whole command: the band's rebuild runs
/// after the canvas's job on the same worker and is reported rather than added,
/// because the finding is about the picture on the canvas. `switch_ms` is the number
/// compared against this.
pub const SWITCH_BUDGET_MS: f64 = 210.0;

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
        Command::Scan(args) => scan(args),
        Command::Thumb(args) => thumb(args),
        Command::Templates(args) => list_templates(args),
        Command::Init(args) => init_project(args),
        Command::Edit(args) => edit_project(args),
        Command::Hit(args) => hit(args),
        Command::Save(args) => save_project(args),
        Command::Gesture(args) => gesture(args),
        Command::Switch(args) => switch(args),
    }
}

/// `hit`: which slot a normalized canvas point falls in (S6.5).
///
/// The GUI's most basic question, and the one it asks on every press and drag. It
/// is a geometry question and nothing else: no photo is decoded, no pixel is
/// touched, and a project whose photos have moved still answers. The point is in
/// normalized canvas coordinates — the same space `probe` prints its slot sample
/// points in — so a caller can feed a `probe` row straight back in.
///
/// "No slot" is an *answer*, not a failure: `grid-4-2x2g` has a gutter and every
/// template has an outside, exactly like `templates --aspect 7:5` reporting
/// `count = 0`. So the exit code stays 0 and `slot = none` says why.
fn hit(args: HitArgs) -> Result<u8, Failure> {
    let template = match &args.source {
        // A project carries its own geometry, which is the point of embedding it:
        // the hit test follows the document, not whatever this build's library
        // holds under the same name today.
        Source::Project(path) => Project::load(path)
            .map_err(|error| Failure::Failed(error.to_string()))?
            .doc()
            .template
            .clone(),
        Source::Template(name) => pixlay_core::templates::get(name).ok_or_else(|| {
            Failure::Usage(format!(
                "unknown template {name}; this build knows: {}",
                pixlay_core::templates::names().join(", ")
            ))
        })?,
    };

    let mut report = Report::new();
    report.text("status", "ok");
    report.text("command", "hit");
    report.text("template", template.name.clone());
    report.int("version", i64::from(template.version));
    report.int("slots", template.slots.len() as i64);
    report.text("at", format!("{:.4},{:.4}", args.at.x, args.at.y));
    match template.slot_at(args.at) {
        Some(slot) => {
            report.bool("hit", true);
            report.int("slot", slot as i64);
        }
        None => {
            report.bool("hit", false);
            report.text("slot", "none");
        }
    }
    emit(&report, args.json);
    Ok(EXIT_SUCCESS)
}

/// `save`: read a project and write it out, atomically (S6.5).
///
/// This is the machine surface of "the user changed the document": the GUI saves
/// through the same `Project::save_as`, and the CLI's copy is how that path is
/// exercised without a window. The document is validated on the way in, so a file
/// this build cannot open is never rewritten, and the write goes through a
/// temporary file and a rename, so a failure leaves the previous file intact.
fn save_project(args: SaveArgs) -> Result<u8, Failure> {
    let project =
        Project::load(&args.project).map_err(|error| Failure::Failed(error.to_string()))?;
    project
        .save_as(&args.out)
        .map_err(|error| Failure::Failed(error.to_string()))?;
    let bytes = std::fs::metadata(&args.out)
        .map_err(|error| Failure::Failed(format!("{}: {error}", args.out.display())))?
        .len();

    let doc = project.doc();
    let mut report = Report::new();
    report.text("status", "ok");
    report.text("command", "save");
    report.text("template", doc.template.name.clone());
    report.int("version", i64::from(doc.template.version));
    report.text("aspect", ratio_label(doc.template.aspect));
    report.int("cells", doc.cells.len() as i64);
    report.int("bytes", bytes as i64);
    emit(&report, args.json);
    Ok(EXIT_SUCCESS)
}

/// `edit`: change one cell's framing and/or the document's frame, and write the
/// document back (S11).
///
/// This is the machine surface of what S15's compose stage does by hand — the
/// rotation, zoom and pan of a cell, and the frame's three fields — because
/// `AGENTS.md` allows nothing that only the GUI can do. Two properties make it
/// usable as a tool rather than as a second editor:
///
/// * **the stored crop is the fit** of what was asked for. A crop is a request and
///   what gets drawn is what covers it, so storing the request would leave a
///   document whose numbers are not the picture; storing the fit makes the written
///   file say what it draws. A cell with no photo has nothing to fit against (the
///   clamp is defined against a photo's aspect) and keeps the numbers as given.
/// * **it is idempotent.** Fitting a fit returns it bit for bit, so `edit` applied
///   twice to the same project writes the same bytes — which is the property S3
///   established for the clamp, re-asserted through the new entry point.
///
/// The write goes through [`Project::save_as`], the same call `save` makes, so a
/// copy that lands in another directory has its relative photo paths rebased.
fn edit_project(args: EditArgs) -> Result<u8, Failure> {
    let project =
        Project::load(&args.project).map_err(|error| Failure::Failed(error.to_string()))?;
    // Every cell has to resolve before anything is edited: like `save`, `edit`
    // refuses a project that points at a deleted photo instead of rewriting it
    // around the hole.
    project
        .sources()
        .map_err(|error| Failure::Failed(error.to_string()))?;
    // Photos named by this run are checked and rebased the way `init --photo` does
    // it: a photo that is not there is refused rather than written into the
    // project, and what is stored is relative to the project file the document is
    // expressed against (`Project::save_as` then rebases the copy).
    let added = stored_photos(&args.add_photos, &args.project)?;
    let replaced = match &args.photo {
        Some(photo) => stored_photos(std::slice::from_ref(photo), &args.project)?.pop(),
        None => None,
    };

    // Every structural change goes through the same `Command`s the GUI sends, so
    // "the same document" is a property of one implementation rather than of two
    // editors kept in step by hand (S14's criterion: the new flags round-trip onto
    // the document the window's own operations produce).
    let mut history =
        History::new(project.doc().clone()).map_err(|error| Failure::Failed(error.to_string()))?;

    if let Some(name) = &args.template {
        let template = pixlay_core::templates::get(name).ok_or_else(|| {
            Failure::Usage(format!(
                "unknown template {name}; this build knows: {}",
                pixlay_core::templates::names().join(", ")
            ))
        })?;
        apply(&mut history, Edit::SetTemplate { template })?;
    }
    // The layout's own count control, before any photo lands: `--add-cell` takes
    // the layout with one slot more and leaves the cell empty, which is what the
    // window's `+` now does (S14b).
    if args.add_cell {
        apply(&mut history, Edit::AddCell)?;
    }
    if args.remove_cell {
        apply(&mut history, Edit::RemoveLastCell)?;
    }
    // The swap is applied *before* the framing flags, so `--swap 0,3 --slot 0
    // --rotate 10` frames the cell that ends up at index 0 rather than the one that
    // was there.
    if let Some((left, right)) = args.swap {
        apply(&mut history, Edit::SwapCells { left, right })?;
    }
    if !added.is_empty() {
        apply(&mut history, Edit::AddPhotos { photos: added })?;
    }

    // The frame is a document field, and since S15 a command of its own: the
    // `Frame…` dialog commits the same `SetFrame`, so the CLI's three flags and the
    // window's three rows are one writer rather than two, and an impossible frame is
    // refused where it is asked for (naming the slot the gap emptied) instead of
    // later, when the document is validated for writing.
    //
    // It is applied **before the framing flags below** (S15f, PIX-009): a crop is
    // stored as its fit, and the fit reads the frame's `covering` — the cell's
    // outline clipped to its inset rectangle — so `--gap 0.04 --zoom 1.4` has to be
    // fitted against the frame the file will carry. Fitting first and applying the
    // frame afterwards wrote a crop the renderer then refits: the file did not hold
    // the fit it claimed, and the same edit run twice moved the bytes.
    if args.frame.any() {
        let mut frame = history.doc().frame;
        args.frame.apply(&mut frame);
        apply(&mut history, Edit::SetFrame { frame })?;
    }

    if let Some(slot) = args.slot {
        // Checked against the document the structural flags produced: a layout
        // switch can drop the cell the framing was meant for.
        let slots = history.doc().cells.len();
        if slot >= slots {
            return Err(Failure::Usage(format!(
                "--slot {slot} does not exist; the template has {slots} slots"
            )));
        }
        if args.clear {
            // "Empty the cell" is no photo *and* no framing, which is the default
            // cell — one command since S15 (`Command::ClearCell`), so the window's own
            // clear button and `Delete` mean the same thing as this flag and are one
            // undo step each.
            apply(&mut history, Edit::ClearCell { slot })?;
        } else {
            if let Some(photo) = replaced {
                apply(
                    &mut history,
                    Edit::SetSource {
                        slot,
                        source: Some(photo),
                    },
                )?;
            }
            if args.rotate.is_some() || args.zoom.is_some() || args.offset.is_some() {
                let request = history.doc().cells[slot].crop.normalized();
                let request = CropTransform {
                    zoom: args.zoom.unwrap_or(request.zoom),
                    offset: args.offset.unwrap_or(request.offset),
                    rotation_deg: args.rotate.unwrap_or(request.rotation_deg),
                }
                .normalized();
                let doc = history.doc().clone();
                let photo = doc.cells[slot].source.as_deref().map(|source| {
                    if source.is_absolute() {
                        source.to_path_buf()
                    } else {
                        project.dir().join(source)
                    }
                });
                // The fit is taken in the document's own space — the template's
                // aspect, not a preview grid's — because this is the number that
                // gets written. It is the *edited* request that is fitted, not the
                // crop the document already had: `fit_crop` is the same reference
                // `draw` will use, **including the frame applied above** (S15f,
                // PIX-009).
                let fitted = match photo {
                    Some(photo) => {
                        let source = pixlay_imaging::Source::decode(&photo)
                            .map_err(|error| Failure::Failed(error.to_string()))?;
                        doc.fit_crop(slot, request, doc.template.aspect, source.aspect())
                            .map_err(|error| Failure::Failed(error.to_string()))?
                            .transform
                    }
                    // Nothing to cover: the request is stored as it stands, and the
                    // fit is applied when the cell gets a photo (`draw` recomputes
                    // it).
                    None => request,
                };
                apply(&mut history, Edit::SetCrop { slot, crop: fitted })?;
            }
        }
    }

    // Validating before writing is what keeps a bug in the library from shipping as
    // an unloadable file.
    let edited = Project::new(history.doc().clone(), &args.project)
        .map_err(|error| Failure::Failed(error.to_string()))?;
    edited
        .save_as(&args.out)
        .map_err(|error| Failure::Failed(error.to_string()))?;
    let bytes = std::fs::metadata(&args.out)
        .map_err(|error| Failure::Failed(format!("{}: {error}", args.out.display())))?
        .len();

    let doc = edited.doc();
    let mut report = Report::new();
    report.text("status", "ok");
    report.text("command", "edit");
    report.text("template", doc.template.name.clone());
    report.int("version", i64::from(doc.template.version));
    report.int("cells", doc.cells.len() as i64);
    report.int(
        "photos",
        doc.cells
            .iter()
            .filter(|cell| cell.source.is_some())
            .count() as i64,
    );
    report.float("gap", doc.frame.gap_rel);
    report.float("radius", doc.frame.radius_rel);
    let border = doc.frame.color;
    report.text("border", rgb([border.r, border.g, border.b]));
    if let Some(slot) = args.slot {
        let cell = &doc.cells[slot];
        report.int("slot", slot as i64);
        report.bool("occupied", cell.source.is_some());
        report.float("zoom", cell.crop.zoom);
        report.text(
            "offset",
            format!("{:.4},{:.4}", cell.crop.offset.0, cell.crop.offset.1),
        );
        report.float("rotation_deg", cell.crop.rotation_deg);
    }
    report.int("bytes", bytes as i64);
    emit(&report, args.json);
    Ok(EXIT_SUCCESS)
}

/// One command of S14's structural vocabulary, applied to a project's history.
///
/// The same call the window makes, so "the same document" is a property of the
/// implementation rather than of two editors kept in step by hand. The history's
/// own answer — whether the command was a *step* — is not the CLI's question: it
/// keeps no undo stack, and a command that asks for the state the document is
/// already in is the same success as one that changes it, with the report
/// describing the document that came out either way (S15d).
fn apply(history: &mut History, command: Edit) -> Result<(), Failure> {
    history
        .apply(command)
        .map(|_step| ())
        .map_err(|error| Failure::Failed(error.to_string()))
}

/// `templates`: the library, optionally filtered to one layout shape or one slot
/// count.
///
/// This is the query S7's picker runs and the one a caller needs before it can
/// name a template. `--slots` is the layout gallery's own query (S14): the
/// candidates for a collage of n photos are exactly the templates with n slots, and
/// this is that list from the outside. The list stays in library order (by slot
/// count), so the output is stable.
fn list_templates(args: TemplatesArgs) -> Result<u8, Failure> {
    // The filters are the library's own queries, so `templates --aspect 4:3`,
    // `templates --slots 5` and the picker cannot disagree about what "the same
    // aspect" or "the layouts with that count" mean.
    let mut listed = match args.aspect {
        Some(aspect) => pixlay_core::templates::of_aspect(aspect),
        None => pixlay_core::templates::all(),
    };
    if let Some(slots) = args.slots {
        listed.retain(|template| template.slots.len() == slots);
    }
    let mut report = Report::new();
    report.text("status", "ok");
    report.text("command", "templates");
    if let Some(aspect) = args.aspect {
        report.text("aspect", ratio_label(aspect));
    }
    if let Some(slots) = args.slots {
        report.int("slots", slots as i64);
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

/// `init`: a template as a loadable project, with the photos the caller names.
///
/// Callers should not have to hand-write `.pixlay` JSON — the format has one
/// canonical writer (`CollageDoc::to_json`) and this is where they reach it. The
/// file is never overwritten: replacing a project the user already has is not
/// something a command called `init` should do quietly.
///
/// With `--photo`, **argument order is cell order** and the mapping goes through
/// the selection policy (`pixlay_core::Selection`), which is the same function the
/// picker and the layout stage use — so "the third photo the user picked is the
/// third cell" is one rule with one implementation, and the 1..=9 clamp and the
/// slot-count check are applied here exactly as they are in the GUI.
fn init_project(args: InitArgs) -> Result<u8, Failure> {
    let template = pixlay_core::templates::get(&args.template).ok_or_else(|| {
        Failure::Usage(format!(
            "unknown template {}; this build knows: {}",
            args.template,
            pixlay_core::templates::names().join(", ")
        ))
    })?;
    let doc = if args.photos.is_empty() {
        // The photo-free project S2 shipped: an empty cell renders white.
        pixlay_core::templates::document(&template)
    } else {
        let photos = stored_photos(&args.photos, &args.out)?;
        let selection = pixlay_core::Selection::new(photos)
            .map_err(|error| Failure::Usage(error.to_string()))?;
        selection
            .document(&template)
            .map_err(|error| Failure::Usage(error.to_string()))?
    };
    // Validating before writing is cheap, and it keeps a bug in the library from
    // shipping as an unloadable file.
    doc.validate()
        .map_err(|error| Failure::Failed(error.to_string()))?;
    let json = doc
        .to_json()
        .map_err(|error| Failure::Failed(error.to_string()))?;
    // Created, not looked up and then written: `create_new` is the refusal itself, so
    // two `init`s that race leave exactly one winner rather than both seeing an absent
    // path and one truncating the other's project — and a symbolic link at the path,
    // dangling or not, is a file that is already there rather than a name to write
    // through (S15c, PIX-015).
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args.out)
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::AlreadyExists => Failure::Failed(format!(
                "{} exists; init never overwrites a project",
                args.out.display()
            )),
            _ => Failure::Failed(format!("{}: {error}", args.out.display())),
        })?;
    if let Err(error) = file.write_all(json.as_bytes()) {
        // The file is this call's own — `create_new` is what created it — so a failed
        // write takes it away again rather than leaving a truncated project that every
        // later `init` refuses to replace.
        drop(file);
        let _ = std::fs::remove_file(&args.out);
        return Err(Failure::Failed(format!("{}: {error}", args.out.display())));
    }

    let mut report = Report::new();
    report.text("status", "ok");
    report.text("command", "init");
    report.text("template", doc.template.name.clone());
    report.int("version", i64::from(doc.template.version));
    report.text("aspect", ratio_label(doc.template.aspect));
    report.int("cells", doc.cells.len() as i64);
    // `photos` is what was asked for, `cells` is what the template has: they are
    // equal for a project with photos, and 0 against N for the photo-free one.
    report.int(
        "photos",
        doc.cells
            .iter()
            .filter(|cell| cell.source.is_some())
            .count() as i64,
    );
    report.int("bytes", json.len() as i64);
    emit(&report, args.json);
    Ok(EXIT_SUCCESS)
}

/// The `source` paths `init` stores for the photos it was given.
///
/// Two jobs, in the order that keeps the messages useful: a photo that is not
/// there is refused (exit 2, the path named — the same rule a loaded project
/// follows, where a missing photo must fail loudly rather than export a white
/// hole), and then each path is expressed the way a *written* `source` should be.
fn stored_photos(photos: &[PathBuf], project: &Path) -> Result<Vec<PathBuf>, Failure> {
    for photo in photos {
        if !photo.is_file() {
            return Err(Failure::Failed(format!(
                "{}: no such photo",
                photo.display()
            )));
        }
    }
    Ok(photos
        .iter()
        .map(|photo| stored_source(photo, project))
        .collect())
}

/// One photo's path as a project should store it: relative to the project file
/// when the two share a root, absolute otherwise.
///
/// The rule `Project::save_as` applies to a copy, applied where the project is
/// first created: a project whose photos sit beside it can be moved or zipped, and
/// one whose photos are on another filesystem keeps an absolute path, which the
/// format accepts as it stands. Lexical (`pixlay_core::relative_to`), so nothing
/// here needs the filesystem to answer, and a photo behind an unmounted drive is
/// still expressible.
fn stored_source(photo: &Path, project: &Path) -> PathBuf {
    let Ok(photo) = std::path::absolute(photo) else {
        return photo.to_path_buf();
    };
    let Ok(project) = std::path::absolute(project) else {
        return photo;
    };
    let dir = project
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    pixlay_core::relative_to(dir, &photo).unwrap_or(photo)
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
    // Load the document first: a broken project must fail before anything is
    // rendered, and its message must name the path that is wrong.
    let (mut doc, sources) = match &args.source {
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
    // The output may not be one of the document's own photos: source images are
    // read-only, and a render that wrote its pixels over one would destroy the
    // user's photo after reading it (S15c, PIX-001). Asked before a single decode,
    // so a refused render costs nothing and writes nothing.
    refuse_source_alias(&args.out, &sources).map_err(|alias| Failure::Usage(alias.to_string()))?;
    // The frame flags are an override for this render, applied before the document
    // is validated: a gap that leaves a cell with nothing visible is a failure of
    // *this* run, and the error names the cell. Nothing is written back — `edit` is
    // the command that stores a frame.
    args.frame.apply(&mut doc.frame);
    doc.validate()
        .map_err(|error| Failure::Failed(error.to_string()))?;

    // The pixel grid, from the one size parameter there is: the long edge, which
    // is exact, with the other edge following the template's aspect rounded half
    // away from zero.
    let long_edge = args.long_edge.unwrap_or(DEFAULT_LONG_EDGE_PX);
    let canvas_px = PixelSize::for_long_edge(doc.template.aspect, long_edge)
        .map_err(|error| Failure::Failed(error.to_string()))?;
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
    // `--preview-px` is a long edge, and the grid it derives is the one that gets
    // allocated: a square template at the flag's own maximum is 400 MP, so the
    // canvas pixel budget is checked on the *scaled* grid too (S15e, PIX-003).
    // Asked here — before the first decode and before `draw` — so a refused render
    // costs nothing and writes nothing.
    bitmap_px
        .validate()
        .map_err(|error| Failure::Failed(error.to_string()))?;

    let stopwatch = stats::Stopwatch::start();
    // The photo-free path has no cells to decode: every slot stays white.
    let images = match &args.source {
        Source::Project(_) => decode_slots(&doc, &sources, bitmap_px)?,
        Source::Template(_) => Images::new(),
    };
    let image = pixlay_render::render_rgb8(&doc, &images, canvas_px, scale, None)
        .map_err(|error| Failure::Failed(error.to_string()))?;
    let compose = stopwatch.elapsed();

    // Pixels and metadata in one pass: the encoder writes the sRGB profile while
    // it writes the image (`pixlay_imaging::encode`).
    let encode_watch = stats::Stopwatch::start();
    let bytes = pixlay_imaging::encode::write(
        &args.out,
        &Export {
            format,
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
    // The long edge the output was *actually* rendered at — `max(out_w, out_h)` of
    // the image just written, which is the number a consumer comparing two renders
    // needs. A preview render is a different size from the export base its grid is
    // scaled from, so the request is reported separately as `preview_px` below
    // (S15h, PIX-019).
    report.int("long_edge", i64::from(image.width.max(image.height)));
    report.int("cells", doc.cells.len() as i64);
    report.int("occupied", images.len() as i64);
    // The frame the render used, always: with `--gap`/`--radius`/`--border-color`
    // it is this run's override, otherwise the document's own, and a caller that
    // cannot see which of the two it got cannot measure a frame.
    report.float("gap", doc.frame.gap_rel);
    report.float("radius", doc.frame.radius_rel);
    let border = doc.frame.color;
    report.text("border", rgb([border.r, border.g, border.b]));
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
    }
    Ok(images)
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

/// `scan`: the photos in a directory, as the picker's grid sees them (S9).
///
/// Stage 1 of the flow — "open a folder and browse" — has to be measurable
/// without a window (`AGENTS.md`: nothing may be possible only in the GUI), and
/// what a grid needs from a folder is a list: the path, what the decoder says the
/// file is, the size **after** EXIF rotation (a tile and a preview lay out
/// against the size a person sees, not the size the file stores), the date a
/// caption can use, and `mtime`, which is the key S12's decode cache invalidates
/// on.
///
/// Two properties the picker depends on:
///
/// * **a refusal is a row, not a skip.** A corrupt file, an image past the decode
///   cap or a dangling symlink is `status = failed` with the decoder's own reason,
///   so the grid can show a broken tile and the user can act on it. Omitting the
///   file silently would make "the folder has nothing" and "the folder has
///   something this build cannot read" the same answer.
/// * **the order is lexical and stable.** Two runs over an unchanged directory are
///   byte-identical. The listing's paths and `mtime` values *are* its input, so
///   they are reported rather than stripped the way every other command strips
///   them: a caller that cannot see a file's mtime has to stat the filesystem
///   again to trust the answer.
fn scan(args: ScanArgs) -> Result<u8, Failure> {
    let stopwatch = stats::Stopwatch::start();
    if !args.dir.is_dir() {
        return Err(Failure::Failed(format!(
            "{}: not a directory",
            args.dir.display()
        )));
    }
    let paths = pixlay_imaging::list_folder(&args.dir, args.recursive)
        .map_err(|error| Failure::Failed(error.to_string()))?;

    let mut report = Report::new();
    report.text("status", "ok");
    report.text("command", "scan");
    // Both the folder and every file are byte fields: they come off the
    // filesystem, so they are bytes and not text, and the report has to carry the
    // name it was given (S15h, PIX-018).
    report.bytes("dir", args.dir.as_os_str().as_bytes());
    report.bool("recursive", args.recursive);
    let mut failed = 0;
    for (index, path) in paths.iter().enumerate() {
        let prefix = report.row("file", index);
        report.bytes(&format!("{prefix}.path"), path.as_os_str().as_bytes());
        match facts(path) {
            Ok(facts) => {
                report.text(&format!("{prefix}.status"), "ok");
                report.text(&format!("{prefix}.mime"), facts.mime);
                report.int(&format!("{prefix}.width"), i64::from(facts.width));
                report.int(&format!("{prefix}.height"), i64::from(facts.height));
                report.text(&format!("{prefix}.date"), facts.date);
                report.int(&format!("{prefix}.mtime"), facts.mtime);
            }
            Err(reason) => {
                failed += 1;
                report.text(&format!("{prefix}.status"), "failed");
                report.text(&format!("{prefix}.reason"), reason);
            }
        }
    }
    report.int("count", paths.len() as i64);
    report.int("failed", failed);
    add_stats(&mut report, args.stats, stopwatch.elapsed(), None, "none");
    emit(&report, args.json);
    Ok(EXIT_SUCCESS)
}

/// What one file is, as far as the grid is concerned.
struct Facts {
    mime: String,
    width: u32,
    height: u32,
    date: String,
    mtime: i64,
}

/// Decodes one file far enough to describe it.
///
/// The whole frame, not the loader's early dimensions: `ImageDetails` is a hint
/// ("often correct … for an early rendering estimate", glycin's own words) and it
/// is not the size after rotation, which is the size the grid is laid out
/// against. `image` makes the same call, so the two commands cannot disagree
/// about a file's size.
fn facts(path: &Path) -> Result<Facts, String> {
    let source = pixlay_imaging::Source::decode(path).map_err(|error| error.to_string())?;
    Ok(Facts {
        mime: source.mime().to_string(),
        width: source.width(),
        height: source.height(),
        date: source
            .exif()
            .and_then(pixlay_imaging::exif::date_time_original)
            .unwrap_or_default(),
        mtime: mtime_seconds(path),
    })
}

/// A file's modification time, in whole seconds since the Unix epoch.
///
/// Zero when the platform cannot say: a cache key that matches nothing is safer
/// than one that matches a file it never looked at, and the row is still a row.
fn mtime_seconds(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|since| since.as_secs() as i64)
        .unwrap_or(0)
}

/// `thumb`: one photo's preview pixels as a file (S9).
///
/// The picker's expensive half is the decode plus the resample to the size the pane
/// shows, and this is that half with a number attached: S13 holds the widget's
/// texture to these pixels, and `--stats` reports what one preview costs, which is the
/// budget S12's cache and coarse-grid decisions are made against. **`--region` is the
/// window's 1:1 view** (S15j): the pane's own picture is a rectangle of the photo
/// resampled so that its long edge is the rectangle's (`thumbnail_region`, the same
/// call), and with `--px` equal to that edge this command writes exactly those pixels.
fn thumb(args: ThumbArgs) -> Result<u8, Failure> {
    let format = Format::from_path(&args.out).ok_or_else(|| {
        Failure::Usage(format!(
            "--out {}: expected {}",
            args.out.display(),
            Format::EXTENSIONS
        ))
    })?;
    // The output may not be the photo itself: `thumb --photo a.jpg --out a.jpg` would
    // replace the photo with its own preview (S15c, PIX-001). One source, because
    // this command has one.
    refuse_source_alias(&args.out, &[Some(args.photo.clone())])
        .map_err(|alias| Failure::Usage(alias.to_string()))?;
    let stopwatch = stats::Stopwatch::start();
    let source = pixlay_imaging::Source::decode(&args.photo)
        .map_err(|error| Failure::Failed(error.to_string()))?;
    let preview = match args.region {
        // A region the photo does not contain is a failure rather than a usage error:
        // the rectangle is well-formed, and the file's own size is what refuses it
        // (`docs/CONTRACT.md` §5).
        Some(region) => pixlay_imaging::thumbnail_region(&source, region, args.px),
        None => pixlay_imaging::thumbnail(&source, args.px),
    }
    .map_err(|error| Failure::Failed(error.to_string()))?;
    let compose = stopwatch.elapsed();

    let encode_watch = stats::Stopwatch::start();
    let bytes = pixlay_imaging::encode::write(
        &args.out,
        &Export {
            format,
            image: Rgb8View {
                width: preview.width,
                height: preview.height,
                data: &preview.pixels,
            },
        },
    )
    .map_err(|error| Failure::Failed(error.to_string()))?;
    let encode_ms = encode_watch.elapsed();

    let mut report = Report::new();
    report.text("status", "ok");
    report.text("command", "thumb");
    report.text("format", format.name());
    report.text("mime", source.mime().to_string());
    report.int("src_w", i64::from(source.width()));
    report.int("src_h", i64::from(source.height()));
    // The rectangle that was resampled, printed back the way it was asked for: the whole
    // photo when `--region` was not given, so "which pixels are these" has one answer
    // whichever spelling the caller used.
    let region = args
        .region
        .unwrap_or_else(|| pixlay_imaging::Rect::whole(source.width(), source.height()));
    report.text(
        "region",
        format!(
            "{},{},{},{}",
            region.x, region.y, region.width, region.height
        ),
    );
    report.int("px", i64::from(args.px));
    report.int("out_w", i64::from(preview.width));
    report.int("out_h", i64::from(preview.height));
    report.int("bytes", bytes as i64);
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

/// `gesture`: one live framing step, measured (S12).
///
/// The stutter this step exists for is architectural: a wheel notch or a drag
/// produces one step every few milliseconds, and every one of them used to decode
/// the photo it framed and resample it from scratch. The probe drives the same
/// [`Preview`] the window's decoding thread drives, on the project it is given, and
/// splits the cost the way the problem splits:
///
/// * `open` — the document as a window opens on it, at the resting grid: every
///   occupied cell decoded and built once. That is the cost the product always
///   paid, and it is not what a gesture pays.
/// * `cold` — the first step of a live gesture, at the grid a gesture draws at
///   ([`gesture_grid`]). The sources are warm by then and the grid is not, so this
///   is the one-off a release and a fresh gesture start pay.
/// * `warm` — every step after it: one cell rebuilt, a frame either way.
/// * `refine` — the release: the resting grid again, rebuilt from the warmed
///   sources, which is what makes the released frame a real render instead of an
///   upscaled one.
///
/// The step is a *straightening* one, [`GESTURE_STEP_DEG`] further per step,
/// because it is the most per-step work the editor can be asked for: a rotation
/// grows the region the cell shows, and since S11 the clamp pays for the angle with
/// zoom instead of reducing it. A number that fits the frame budget here fits it
/// for a pan or a zoom too.
///
/// `src_w`/`src_h` are the size of the source the warm step resampled — the
/// preview-grade reduction (S12b), not the file. Before the reduction the step read
/// the decoded photo, so against a 6000-px file the field went from 6000 to the
/// copy's own long edge; "the big decode left the step" is therefore a number the
/// report carries rather than an assumption, and `open`'s and `refine`'s copies
/// (the resting grid's, one [`preview_source_long_edge`] wide) and `cold`'s (the
/// gesture grid's, half of it) are that function of the grids the report already
/// prints.
///
/// What it does **not** measure: the cairo blit of the finished bitmaps and the
/// widget's own paint. Those are the window's, and a windowless command cannot
/// reach them; what it measures is the half that used to re-decode.
fn gesture(args: GestureArgs) -> Result<u8, Failure> {
    let project =
        Project::load(&args.project).map_err(|error| Failure::Failed(error.to_string()))?;
    let sources = project
        .sources()
        .map_err(|error| Failure::Failed(error.to_string()))?;
    let doc = project.doc().clone();

    let occupied: Vec<usize> = sources
        .iter()
        .enumerate()
        .filter_map(|(slot, source)| source.is_some().then_some(slot))
        .collect();
    if occupied.is_empty() {
        return Err(Failure::Failed(format!(
            "{}: the document has no photo to gesture on",
            args.project.display()
        )));
    }
    let slot = match args.slot {
        Some(slot) if occupied.contains(&slot) => slot,
        Some(slot) => {
            return Err(Failure::Usage(format!(
                "--slot {slot} has no photo (occupied cells: {})",
                occupied
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        None => occupied[0],
    };

    // The grid the window rests at, and the grid it draws at while a gesture is
    // live — the editor's own two (`pixlay_imaging::gesture_grid`). The resting
    // grid is derived by the same function the window's and `render`'s are, so the
    // rounding and the canvas pixel budget are one rule rather than three
    // (S15e, PIX-003): a 20000-px grid on a 4:3 canvas is 300 MP and is refused
    // before a single decode.
    let resting = PixelSize::for_long_edge(doc.template.aspect, args.grid)
        .map_err(|error| Failure::Failed(error.to_string()))?;
    let moving = gesture_grid(resting);

    let mut preview = Preview::new();
    let watch = stats::Stopwatch::start();
    let build = |preview: &mut Preview, doc: &pixlay_core::CollageDoc, grid: PixelSize| {
        let step = stats::Stopwatch::start();
        let built = preview.build(doc, &sources, grid);
        (built, step.elapsed().as_secs_f64() * 1000.0)
    };
    // One cell framed one step further, the way a straightening gesture frames it.
    // The *request* is what moves; the fit the pipeline draws with is recomputed
    // from it on every build, exactly as the window's is.
    let frame = |step: u32| {
        let mut framed = doc.clone();
        framed.cells[slot].crop = pixlay_core::CropTransform {
            rotation_deg: doc.cells[slot].crop.rotation_deg + GESTURE_STEP_DEG * f64::from(step),
            ..doc.cells[slot].crop
        }
        .normalized();
        framed
    };

    let (open, open_ms) = build(&mut preview, &doc, resting);
    let mut failed = open.failed.clone();
    let (cold, cold_ms) = build(&mut preview, &frame(1), moving);
    failed.extend(cold.failed.iter().cloned());
    let mut warm_ms = Vec::with_capacity(args.steps as usize - 1);
    let mut warm_decodes = 0;
    let mut warm_src = PixelSize {
        width: 0,
        height: 0,
    };
    for step in 2..=args.steps {
        let (built, ms) = build(&mut preview, &frame(step), moving);
        warm_ms.push(ms);
        warm_decodes += built.decodes;
        // The step the verdict is about, and the copy it read: every warm step
        // frames the same cell at the same grid, so the last one is the answer.
        warm_src = built.source_px;
        failed.extend(built.failed.iter().cloned());
    }
    let (refined, refine_ms) = build(&mut preview, &frame(args.steps), resting);
    failed.extend(refined.failed.iter().cloned());
    let total = watch.elapsed();

    if !failed.is_empty() {
        // A step cannot be timed on a cell that does not render, and a sequence
        // with a hole in it describes nothing.
        return Err(Failure::Failed(format!(
            "{}: {}",
            args.project.display(),
            failed
                .iter()
                .map(|(slot, reason)| format!("slot {slot}: {reason}"))
                .collect::<Vec<_>>()
                .join("; ")
        )));
    }

    // The *median* step, not the mean: the mean of 24 steps on a busy machine is a
    // number about the machine, and the question is what a step costs. The worst
    // one is printed as `warm_max_ms` rather than hidden.
    let mut sorted = warm_ms.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).expect("no NaN in a duration"));
    let warm_median = sorted[sorted.len() / 2];
    let warm_max = sorted[sorted.len() - 1];
    // The verdict is about the steady state: an occasional scheduler hiccup is
    // printed rather than judged, and a gesture's first step is a one-off
    // (`cold_ms`).
    let verdict = if warm_median <= GESTURE_STEP_BUDGET_MS {
        "pipeline_holds"
    } else {
        "gpu_preview"
    };

    let mut report = Report::new();
    report.text("command", "gesture");
    report.text("template", doc.template.name.clone());
    report.int("version", i64::from(doc.template.version));
    report.int("slots", doc.cells.len() as i64);
    report.int("occupied", occupied.len() as i64);
    report.int("slot", slot as i64);
    report.int("steps", i64::from(args.steps));
    report.float("step_deg", GESTURE_STEP_DEG);
    report.int("grid_w", i64::from(resting.width));
    report.int("grid_h", i64::from(resting.height));
    report.int("gesture_w", i64::from(moving.width));
    report.int("gesture_h", i64::from(moving.height));
    report.int("open_decodes", open.decodes as i64);
    report.int("cold_decodes", cold.decodes as i64);
    report.int("warm_decodes", warm_decodes as i64);
    report.int("refine_decodes", refined.decodes as i64);
    report.int("src_w", i64::from(warm_src.width));
    report.int("src_h", i64::from(warm_src.height));
    report.float("budget_ms", GESTURE_STEP_BUDGET_MS);
    report.text("verdict", verdict);
    if args.stats {
        report.float("open_ms", open_ms);
        report.float("cold_ms", cold_ms);
        report.float("warm_ms", warm_median);
        report.float("warm_max_ms", warm_max);
        report.float("refine_ms", refine_ms);
    }
    add_stats(&mut report, args.stats, total, None, "none");
    emit(&report, args.json);
    Ok(EXIT_SUCCESS)
}

/// `switch`: one layout change, measured (S18).
///
/// The human's finding 1 of 2026-09-25 — "switching a layout in the editor takes too
/// long before the new preview is on screen" — as a number, before any code changes.
/// It drives the same pipeline the window's click drives, in the same order and with
/// the same calls:
///
/// * `template_ms` — `Command::SetTemplate` through a `History` (the window's own
///   `apply`), and the new resting grid derived from the new aspect and the same
///   canvas box ([`canvas_grid`](pixlay_core::canvas_grid)). The window then refreshes
///   a dozen widgets; a windowless command has none of them, so this is the document's
///   half of the click and the smallest of the three.
/// * `sources_ms` — the preview-grade copies the new grid asks for
///   ([`Preview::warm_sources`]): a decode per file whose copy is not in hand, plus
///   the reduction. This is the phase the finding is probably about: the copies are
///   keyed by the edge they were reduced to (S15f, PIX-004), so a switch that keeps
///   the edge — two layouts of the same aspect in the same box — finds them in hand
///   and decodes nothing, while one that moves the edge (any of the default window's
///   candidate layouts, whose grids are 735x551 and 980x551) pays a decode per photo.
/// * `composite_ms` — the cell bitmaps at the new grid, resampled from those copies
///   and quantized: the `Preview::build` the window's worker runs.
/// * `band_ms`, with `--band` — the layout band's rebuild: every candidate of the
///   document's new cell count, rendered at
///   [`candidate_grid`](pixlay_core::templates::candidate_grid) from the canvas's own
///   edge. The window runs that loop on the worker after the canvas's job
///   (`decode.rs`); here it is the same calls in the same order, and the render is
///   thrown away — what is measured is its cost, and "with and without the band" is
///   two runs of this command.
///
/// `switch_ms` is the first three, which is what the user waits for: the picture on
/// the canvas. It is the number `SWITCH_BUDGET_MS` judges, and the band's share is
/// reported beside it rather than added to it. What no windowless command can reach
/// is the rest of the window's own frame — the blit into `Images`, `queue_draw`, and
/// the paint — so `crates/pixlay/tests/switch.rs` measures the same change from
/// inside the window and S18's Result compares the two.
///
/// A project whose photos are all there but one of which cannot be decoded is
/// refused (exit 2), the rule `gesture` follows: a switch timed on a cell that does
/// not render is a number about a different document.
fn switch(args: SwitchArgs) -> Result<u8, Failure> {
    let project =
        Project::load(&args.project).map_err(|error| Failure::Failed(error.to_string()))?;
    let from_template = project.doc().template.name.clone();
    let before = project.doc().clone();
    let sources = project
        .sources()
        .map_err(|error| Failure::Failed(error.to_string()))?;
    let template = pixlay_core::templates::get(&args.template).ok_or_else(|| {
        Failure::Usage(format!(
            "unknown template {}; this build knows: {}",
            args.template,
            pixlay_core::templates::names().join(", ")
        ))
    })?;
    // The grid the editor is resting at before the click: the widget's own box, less
    // the canvas margin, fitted to the document's aspect — `canvas_grid`, the same
    // function the window's `resting_grid` is. This is the grid a *click* happens at,
    // and the reason the ruler takes a widget rather than one long edge: a layout
    // change moves the grid's *shape*, and with it the preview-grade edge.
    let before_grid = canvas_grid(before.template.aspect, args.canvas.0, args.canvas.1);

    // One `Preview` is one editing session (S12b), and the click happens inside one:
    // the editor is already showing this document on this grid, so its caches hold
    // that layout's copies. A fresh cache here would measure an open, not a click.
    let mut preview = Preview::new();
    let watch = stats::Stopwatch::start();
    let open_step = stats::Stopwatch::start();
    let opened = preview.build(&before, &sources, before_grid);
    let open_ms = open_step.elapsed().as_secs_f64() * 1000.0;

    // ---- the click ---------------------------------------------------------
    let mut history = History::new(before).map_err(|error| Failure::Failed(error.to_string()))?;
    let template_step = stats::Stopwatch::start();
    apply(&mut history, Edit::SetTemplate { template })?;
    let doc = history.doc().clone();
    let grid = canvas_grid(doc.template.aspect, args.canvas.0, args.canvas.1);
    let template_ms = template_step.elapsed().as_secs_f64() * 1000.0;
    // A layout with fewer cells drops the last ones — that is how `SetTemplate`
    // resizes a document — and the resolved sources follow the same way.
    let mut sources = sources;
    sources.resize(doc.cells.len(), None);
    let occupied = sources.iter().filter(|source| source.is_some()).count();

    // ---- the worker's two halves ------------------------------------------
    // The window's worker runs one `build`; its work is the copies (a decode per file
    // whose copy is not in hand) plus the bitmaps, and the ruler times them apart
    // because the first is what a switch between two aspects pays for.
    let sources_step = stats::Stopwatch::start();
    let decodes = preview.warm_sources(&sources, grid);
    let sources_ms = sources_step.elapsed().as_secs_f64() * 1000.0;
    let composite_step = stats::Stopwatch::start();
    let built = preview.build(&doc, &sources, grid);
    let composite_ms = composite_step.elapsed().as_secs_f64() * 1000.0;

    // ---- the band, when it is asked for ------------------------------------
    let mut band_ms = 0.0;
    let mut band_candidates = 0;
    if args.band {
        let band_step = stats::Stopwatch::start();
        // The canvas's own edge (S14): the band's copies *are* the canvas's, so its
        // builds decode nothing the switch has not decoded already.
        let edge = preview_source_long_edge(grid);
        let listed = pixlay_core::templates::with_slots(doc.cells.len());
        for listed_template in &listed {
            let mut candidate = doc.clone();
            candidate.template = listed_template.clone();
            candidate
                .cells
                .resize(listed_template.slots.len(), Default::default());
            let slice = &sources[..candidate.cells.len().min(sources.len())];
            let candidate_grid = pixlay_core::templates::candidate_grid(listed_template.aspect);
            let candidate_built =
                preview.build_at_source_edge(&candidate, slice, candidate_grid, edge);
            let mut images = Images::new();
            for bitmap in &candidate_built.bitmaps {
                images.insert(bitmap.slot, render_bitmap(bitmap)?);
            }
            pixlay_render::render_rgb8(&candidate, &images, candidate_grid, 1.0, None)
                .map_err(|error| Failure::Failed(error.to_string()))?;
        }
        band_candidates = listed.len();
        band_ms = band_step.elapsed().as_secs_f64() * 1000.0;
    }
    let total = watch.elapsed();

    // A cell that does not render is not a cell a switch can be timed on — `gesture`'s
    // rule. The band's own candidates are not asked: a candidate carrying an
    // unreadable photo is a candidate with a white cell, which is what the window's
    // band shows too.
    let mut failed = opened.failed.clone();
    failed.extend(built.failed.iter().cloned());
    if !failed.is_empty() {
        return Err(Failure::Failed(format!(
            "{}: {}",
            args.project.display(),
            failed
                .iter()
                .map(|(slot, reason)| format!("slot {slot}: {reason}"))
                .collect::<Vec<_>>()
                .join("; ")
        )));
    }

    // The number the finding is about: the click to the pixels, without the band,
    // which runs after the canvas's own job.
    let switch_ms = template_ms + sources_ms + composite_ms;
    let verdict = if switch_ms <= SWITCH_BUDGET_MS {
        "within_budget"
    } else {
        "over_budget"
    };

    let mut report = Report::new();
    report.text("command", "switch");
    report.text("from_template", from_template);
    report.text("template", doc.template.name.clone());
    report.int("version", i64::from(doc.template.version));
    report.int("slots", doc.cells.len() as i64);
    report.int("occupied", occupied as i64);
    report.int("canvas_w", i64::from(args.canvas.0));
    report.int("canvas_h", i64::from(args.canvas.1));
    report.int("from_grid_w", i64::from(before_grid.width));
    report.int("from_grid_h", i64::from(before_grid.height));
    report.int("grid_w", i64::from(grid.width));
    report.int("grid_h", i64::from(grid.height));
    report.int("open_decodes", opened.decodes as i64);
    report.int("decodes", decodes as i64);
    report.int("src_w", i64::from(built.source_px.width));
    report.int("src_h", i64::from(built.source_px.height));
    if args.band {
        report.int("band_candidates", band_candidates as i64);
    }
    report.float("budget_ms", SWITCH_BUDGET_MS);
    if args.stats {
        // A reading of a measurement, so it is printed with the measurements: the
        // documented exception to "the same input is byte-identical" is `--stats`,
        // and a verdict that could flip between two runs of the same command line
        // must not sit in the shape that promises it cannot.
        report.text("verdict", verdict);
        report.float("open_ms", open_ms);
        report.float("template_ms", template_ms);
        report.float("sources_ms", sources_ms);
        report.float("composite_ms", composite_ms);
        report.float("switch_ms", switch_ms);
        if args.band {
            report.float("band_ms", band_ms);
        }
    }
    add_stats(&mut report, args.stats, total, None, "none");
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

    let stopwatch = stats::Stopwatch::start();
    let long_edge = args.long_edge.unwrap_or(DEFAULT_LONG_EDGE_PX);
    // The one size parameter, the same number `render` would use: the probe
    // samples pixel coordinates, and a preview grid would move every one of them.
    let full = PixelSize::for_long_edge(doc.template.aspect, long_edge)
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
    let image = pixlay_render::render_rgb8(&doc, &images, full, 1.0, None)
        .map_err(|error| Failure::Failed(error.to_string()))?;
    let compose = stopwatch.elapsed();

    // The probe reads the render itself: with real photos there is no placeholder
    // color to compare against (see `pixlay_imaging::probe`).
    let view = Rgb8View {
        width: image.width,
        height: image.height,
        data: &image.data,
    };
    let result = pixlay_imaging::probe::probe(&doc, &view);
    let mut report = Report::new();
    report.text("status", if result.ok() { "ok" } else { "failed" });
    report.text("command", "probe");
    report.int("long_edge", i64::from(long_edge));
    report.int("slots", result.slots as i64);
    report.int("occupied", result.occupied.len() as i64);
    report.int("out_w", i64::from(result.width));
    report.int("out_h", i64::from(result.height));
    report.int("bg_samples", result.background.samples as i64);
    report.int("bg_off_backdrop", result.background.off_backdrop as i64);
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
