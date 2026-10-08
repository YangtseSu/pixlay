// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

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
//! **Since S25 the export is one dialog: the platform's own** (ruling 36). Pressing
//! Export opens `GtkFileDialog::save` seeded by [`seed`] — the folder the last export
//! used, the pictures directory when there is none, and the name the document
//! suggests — and the path it answers becomes a [`Request`]: the long edge comes from
//! the app's own settings (`crate::settings`, ruling 39) and **the name's extension
//! decides the format** ([`format_for`], S25c — the CLI's own `--out` rule, so a `.png`
//! name is a PNG whether or not the settings' format row says JPEG). A name with an
//! extension this build does not write is refused with the message the CLI gives;
//! replacing a file that is already there is the platform's own confirmation, which is
//! why this module does not ask about it.
//!
//! **The source-image rule is still asked twice** (S15c): the window asks it before it
//! starts anything, so a refusal does not have to travel through a worker, and [`run`]
//! asks it again because it is the writer — no caller can reach the file without it.

use std::path::{Path, PathBuf};
use std::time::Instant;

use gtk4::glib;

use pixlay_core::{CollageDoc, PixelSize};
use pixlay_imaging::destination::refuse_source_alias;
use pixlay_imaging::encode::{Export, Format, write};
use pixlay_imaging::{Rgb8View, Source, slot_bitmap};
use pixlay_render::{Bitmap, Images, render_rgb8};

use crate::workers::{Down, Kind, WorkerPlan};

/// Smallest long edge the Settings surface offers, in pixels.
///
/// A floor for the surface, not a limit of the format: any positive grid is valid, and a
/// minimum below the canvas's own preview grid would let an export come out smaller
/// than the picture the user approved.
pub const MIN_EXPORT_PX: u32 = 256;

/// Largest long edge the export form offers, in pixels.
///
/// `12000² = 144 MP`, inside the 200 MP pixel budget (`MAX_CANVAS_PIXELS`) for a
/// square grid, so every template aspect the surface can produce is inside the
/// budget whatever the shape. The CLI's own range is wider (`--long-edge` follows
/// `MAX_LONG_EDGE_PX`) because it is a machine surface, not a form.
pub const MAX_EXPORT_PX: u32 = 12000;

/// What one export asks for: the long edge the settings hold, and the file the
/// platform's save dialog answered (S25, ruling 36).
///
/// It was `Settings` until S25, when the word went to the app's own remembered settings
/// (`crate::settings::Settings`): what this type carries is a *request* — where to write,
/// and how big. **The format is not one of its fields** (S25c): it is the path's own
/// extension ([`format_for`]), so a request cannot name a format its file would lie
/// about, and the GUI and the CLI decide a format the same way.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    /// The long edge the export is rendered at, in pixels.
    pub long_edge: u32,
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

/// Whether `path` may be written: `Err` is the reason to show and the export must not
/// start.
///
/// The window asks this before it starts a worker, so a refusal is a toast rather than
/// a failed export; [`run`] asks it again because it is the writer.
///
/// The alias rule is `pixlay_imaging::destination`'s — the same one `render` and
/// `thumb` apply to the same path — so the GUI's refusal and the CLI's are one rule
/// with one message.
///
/// **A file that is already there is not this function's question any more** (S25): it
/// was, while the app's own form asked before replacing it, and since ruling 36 that
/// confirmation is the platform's save dialog's own. What is left here is the one
/// question no dialog can ask — whether the path is one of the document's own photos.
pub fn destination(path: &Path, sources: &[Option<PathBuf>]) -> Result<(), String> {
    refuse_source_alias(path, sources).map_err(|alias| alias.to_string())
}

/// The extension a format's files carry when the name does not say otherwise.
pub fn extension(format: Format) -> &'static str {
    match format {
        Format::Avif => "avif",
        Format::Jpeg => "jpg",
        Format::Png => "png",
    }
}

/// The format a path's own extension means (S25c), or the refusal an extension this
/// build does not write earns.
///
/// **The extension is the whole interface between a file and its pixels** (S15h,
/// PIX-010), and it is the only thing that decides a format here: the CLI's `--out` has
/// always worked this way, and since S25c the GUI's export does too, so a `.png` name is
/// a PNG whatever the settings' format row says. Both JPEG spellings are the JPEG format
/// and the comparison is case-insensitive (`Format::from_path`), so `photo.JPEG` is a
/// JPEG; a name with no extension, or with one this build does not write, is refused
/// with the message the CLI gives for the same mistake.
///
/// The window asks this before it starts a worker, so a refusal is a plain toast, and
/// [`run`] asks it again because it is the function that reaches the file — which is also
/// what makes the format a property of the path rather than a caller's field.
pub fn format_for(path: &Path) -> Result<Format, String> {
    Format::from_path(path)
        .ok_or_else(|| format!("{}: expected {}", path.display(), Format::EXTENSIONS))
}

/// What the export's own save dialog opens on (S25, ruling 36).
#[derive(Clone, Debug, PartialEq)]
pub struct Seed {
    /// The folder it opens in: the settings' last export directory when that is still
    /// a directory, the pictures directory otherwise, and `None` when this account has
    /// neither — which leaves GTK's own default.
    pub folder: Option<PathBuf>,
    /// The name it suggests, extension included.
    pub name: String,
}

/// The seed for one export: the name the document suggests, and the folder to open in.
///
/// The folder is the last export's (ruling 36: the settings remember it) and falls back
/// to [`default_folder`] — the pictures directory — for an account that has never
/// exported, and to nothing at all when there is no such directory either. A remembered
/// directory that has been deleted since falls back the same way rather than opening the
/// dialog somewhere that is not there.
pub fn seed(name: String, last_export_dir: Option<&Path>) -> Seed {
    let folder = last_export_dir
        .filter(|dir| dir.is_dir())
        .map(Path::to_path_buf)
        .or_else(default_folder);
    Seed { folder, name }
}

/// The folder an export with no remembered one goes to: the pictures directory,
/// `XDG_PICTURES_DIR` or `~/Pictures`.
///
/// `GTK`'s `GtkFileDialog` and this function read the same `XDG_PICTURES_DIR` through
/// GLib, so the folder the save dialog opens on and the one a path is suggested in
/// cannot disagree. `None` is an account with no pictures directory at all, which the
/// caller answers by leaving the dialog's own default folder alone.
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
    settings: &Request,
    progress: &(dyn Fn(Progress) + Sync),
) -> Result<Report, String> {
    // The writer's own guard, not only the window's: an export may never be the way one
    // of the document's photos is lost, and this is the function that reaches the file
    // (S15c, PIX-001).
    destination(&settings.path, sources)?;
    // The format is the name's own (S25c), asked here as well as by the window: refusing
    // an extension this build does not write is what keeps a direct caller from writing
    // bytes under a name that lies about them.
    let format = format_for(&settings.path)?;
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
            format,
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
/// The one size parameter (S12d): a person asks one question about a picture — how large
/// the file is — and the grid is the template's own aspect at that long edge.
pub fn grid(doc: &CollageDoc, settings: &Request) -> Result<PixelSize, String> {
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
    settings: Request,
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
