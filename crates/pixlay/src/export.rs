//! Export: the one size parameter, in the background, with progress.
//!
//! The pipeline is the CLI's, stage for stage — decode each slot in this
//! document's own framing, composite through the single `draw`, encode pixels and
//! metadata in one pass — because it is the same product. What the GUI adds is
//! that it runs on a worker thread (a large export is 6–7 s of work, and a frozen
//! window for that long is not an option) and that it reports progress, which
//! `GtkProgressBar` shows in the window's bottom bar.
//!
//! Only two things cross the thread boundary: a [`Progress`] value and the final
//! [`Result`]. The caller turns both into a `GtkProgressBar` update and a toast.
//!
//! **Two questions are asked before an export starts** (S15c): whether the path may
//! be written at all — it may not be one of the document's own photos, the rule
//! `render` and `thumb` apply to the same path ([`destination`]) — and whether the
//! form has to confirm a file that is already there. The window asks the first one
//! so a refusal does not have to travel through a worker, and [`run`] asks it again
//! because it is the writer, so no caller can reach the file without it.

use std::path::{Path, PathBuf};
use std::time::Instant;

use gtk4::glib;

use pixlay_core::{CollageDoc, PixelSize};
use pixlay_imaging::destination::refuse_source_alias;
use pixlay_imaging::encode::{Export, Format, write};
use pixlay_imaging::{Rgb8View, Source, slot_bitmap};
use pixlay_render::{Bitmap, Images, render_rgb8};

use crate::workers::{Down, Kind, WorkerPlan};

/// Smallest long edge the export form offers, in pixels.
///
/// A floor for the form, not a limit of the format: any positive grid is valid, and a
/// minimum below the canvas's own preview grid would let an export come out smaller
/// than the picture the user approved.
pub const MIN_EXPORT_PX: u32 = 256;

/// Largest long edge the export form offers, in pixels.
///
/// `12000² = 144 MP`, inside the 200 MP pixel budget (`MAX_CANVAS_PIXELS`) for a
/// square grid, so every template aspect the form can produce is inside the
/// budget whatever the shape. The CLI's own range is wider (`--long-edge` follows
/// `MAX_LONG_EDGE_PX`) because it is a machine surface, not a form.
pub const MAX_EXPORT_PX: u32 = 12000;

/// What the export form asks for.
///
/// Three fields, because the form has three controls (S12c): the format, **one**
/// quality option — the long edge in pixels, which is all of "how big is this
/// picture" (S12d) — and where it goes.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// The long edge the export is rendered at, in pixels.
    pub long_edge: u32,
    pub format: Format,
    pub path: PathBuf,
}

/// Where an export has got to. The fractions are the bar's, and they are a
/// decision: decoding is the long stage, so it owns the first half.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Progress {
    Decoding { done: usize, total: usize },
    Rendering,
    Encoding,
}

impl Progress {
    pub fn fraction(self) -> f64 {
        match self {
            // Half the bar for the decodes, assuming each photo costs about the
            // same; the last one leaves a sliver so "Rendering" is visible.
            Self::Decoding { done, total } => {
                let total = total.max(1) as f64;
                (done as f64 / total) * 0.45
            }
            Self::Rendering => 0.6,
            Self::Encoding => 0.8,
        }
    }
}

/// What the caller gets back: enough to say what happened without looking at the
/// file again.
#[derive(Clone, Debug)]
pub struct Report {
    pub path: PathBuf,
    pub bytes: u64,
    pub width: i32,
    pub height: i32,
    pub long_edge: u32,
    pub ms: u128,
}

/// Whether `path` may be written, and whether a file is already there.
///
/// The two questions the export form asks before it does anything: `Err` is the
/// reason to show and the export must not start, `Ok(true)` means the file exists and
/// replacing it is the user's to confirm (ruling 2026-09-24: an existing file is
/// confirmed before it is replaced).
///
/// The alias rule is `pixlay_imaging::destination`'s — the same one `render` and
/// `thumb` apply to the same path — so the GUI's refusal and the CLI's are one rule
/// with one message.
pub fn destination(path: &Path, sources: &[Option<PathBuf>]) -> Result<bool, String> {
    refuse_source_alias(path, sources).map_err(|alias| alias.to_string())?;
    Ok(path.exists())
}

/// The folder an export with no remembered one goes to: the pictures directory,
/// `XDG_PICTURES_DIR` or `~/Pictures`.
///
/// `GTK`'s `GtkFileDialog` and this function read the same `XDG_PICTURES_DIR` through
/// GLib, so the folder the save dialog opens on and the one a path is suggested in
/// cannot disagree. `None` is an account with no pictures directory at all, which the
/// caller answers with the bare file name.
///
/// It lived in the picker until S22, which deleted that stage; the export is what
/// still asks the question (ruling 2026-09-24, PIX-010).
pub fn default_folder() -> Option<PathBuf> {
    if let Some(dir) = glib::user_special_dir(glib::UserDirectory::Pictures)
        && dir.is_dir()
    {
        return Some(dir);
    }
    let fallback = glib::home_dir().join("Pictures");
    fallback.is_dir().then_some(fallback)
}

/// Renders and writes one export. Synchronous: the background and the test paths
/// differ only in which thread calls it.
pub fn run(
    doc: &CollageDoc,
    sources: &[Option<PathBuf>],
    settings: &Settings,
    progress: &(dyn Fn(Progress) + Sync),
) -> Result<Report, String> {
    // The writer's own guard, not only the form's: an export may never be the way one
    // of the document's photos is lost, and this is the function that reaches the file
    // (S15c, PIX-001).
    refuse_source_alias(&settings.path, sources).map_err(|alias| alias.to_string())?;
    // The extension is the whole interface between a file and its pixels, and the form
    // resolves the name through this same rule before it calls here (S15h, PIX-010):
    // refusing again is what makes it true for a direct caller too. `expected` names
    // the format the form asked for, in the wording the CLI's `--out` uses.
    let expected = match settings.format {
        Format::Png => ".png",
        Format::Jpeg => ".jpg or .jpeg",
    };
    if Format::from_path(&settings.path) != Some(settings.format) {
        return Err(format!("{}: expected {expected}", settings.path.display()));
    }
    let started = Instant::now();
    let canvas_px = grid(doc, settings)?;

    // The bitmaps are sized in the export's own pixel grid: `draw` blits them at
    // the size they already are, so a preview-sized bitmap would leave the canvas
    // magnifying (S4's rule; the CLI does the same).
    let occupied = sources.iter().filter(|source| source.is_some()).count();
    let mut images = Images::new();
    let mut done = 0;
    for (slot, source) in sources.iter().enumerate() {
        let Some(path) = source else {
            continue;
        };
        let decoded =
            Source::decode(path).map_err(|error| format!("{}: {error}", path.display()))?;
        let bitmap = slot_bitmap(doc, &decoded, slot, canvas_px)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let render_bitmap = Bitmap::from_argb32_region(
            bitmap.width as i32,
            bitmap.height as i32,
            bitmap.origin,
            bitmap.display,
            bitmap.pixels,
        )
        .map_err(|error| error.to_string())?;
        images.insert(slot, render_bitmap);
        done += 1;
        progress(Progress::Decoding {
            done,
            total: occupied,
        });
    }

    progress(Progress::Rendering);
    let image =
        render_rgb8(doc, &images, canvas_px, 1.0, None).map_err(|error| error.to_string())?;

    progress(Progress::Encoding);
    let bytes = write(
        &settings.path,
        &Export {
            format: settings.format,
            image: Rgb8View {
                width: image.width,
                height: image.height,
                data: &image.data,
            },
        },
    )
    .map_err(|error| error.to_string())?;

    Ok(Report {
        path: settings.path.clone(),
        bytes,
        width: image.width,
        height: image.height,
        long_edge: settings.long_edge,
        ms: started.elapsed().as_millis(),
    })
}

/// The pixel grid the export renders.
///
/// The one size parameter (S12d): the form asks the one question a person asks
/// about a picture — how large the file is — and the grid is the template's own
/// aspect at that long edge.
pub fn grid(doc: &CollageDoc, settings: &Settings) -> Result<PixelSize, String> {
    PixelSize::for_long_edge(doc.template.aspect, settings.long_edge)
        .map_err(|error| error.to_string())
}

/// Runs [`run`] on a worker thread, calling `report` on the main context.
///
/// `report` receives progress and, at the end, the result; it is `Send` because
/// it is invoked from the worker's thread and must not touch a widget itself —
/// the window's own closure wraps each call in `MainContext::invoke` with a
/// `SendWeakRef`, which is the only way a GTK object may be reached from here.
///
/// `Err` is a thread that could not be started (S15h, PIX-014): the window then
/// clears the progress state it had just set and says so, instead of showing a
/// progress bar that nothing will ever move.
pub fn spawn(
    doc: CollageDoc,
    sources: Vec<Option<PathBuf>>,
    settings: Settings,
    report: impl Fn(Event) + Send + Sync + 'static,
    plan: WorkerPlan,
) -> Result<(), Down> {
    let report = std::sync::Arc::new(report);
    plan.start(Kind::Export, move || {
        let progress_report = std::sync::Arc::clone(&report);
        let progress = move |progress: Progress| progress_report(Event::Progress(progress));
        let outcome = run(&doc, &sources, &settings, &progress);
        report(Event::Finished(outcome));
    })
}

/// What an export reports back.
pub enum Event {
    Progress(Progress),
    Finished(Result<Report, String>),
}
