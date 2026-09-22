//! Export: the two sizing modes, in the background, with progress.
//!
//! The pipeline is the CLI's, stage for stage — decode each slot in this
//! document's own framing, composite through the single `draw`, encode pixels and
//! metadata in one pass — because it is the same product. What the GUI adds is
//! that it runs on a worker thread (an A0 sheet is 6–7 s of work, and a frozen
//! window for that long is not an option) and that it reports progress, which
//! `GtkProgressBar` shows in the window's bottom bar.
//!
//! Only two things cross the thread boundary: a [`Progress`] value and the final
//! [`Result`]. The caller turns both into a `GtkProgressBar` update and a toast.

use std::path::PathBuf;
use std::time::Instant;

use pixlay_core::{CollageDoc, PixelSize};
use pixlay_imaging::encode::{Chroma, Export, Format, write};
use pixlay_imaging::{Rgb8View, Source, slot_bitmap};
use pixlay_render::{Bitmap, Images, render_rgb8_sized};

/// What the export form asks for.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub size: Size,
    pub format: Format,
    pub chroma: Chroma,
    pub path: PathBuf,
}

/// The two mutually exclusive sizing requests (`docs/CONTRACT.md` §5).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Size {
    /// A resolution: the grid is `round(mm / 25.4 * dpi)` and the file carries
    /// exactly this DPI.
    Dpi(u32),
    /// A pixel count on the long edge; the file carries the resolution the grid
    /// works out to.
    LongEdge(u32),
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
    pub dpi: f64,
    pub ms: u128,
}

/// Renders and writes one export. Synchronous: the background and the test paths
/// differ only in which thread calls it.
pub fn run(
    doc: &CollageDoc,
    sources: &[Option<PathBuf>],
    settings: &Settings,
    progress: &(dyn Fn(Progress) + Sync),
) -> Result<Report, String> {
    let started = Instant::now();
    let (canvas_px, dpi) = grid(doc, settings)?;

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
        render_rgb8_sized(doc, &images, canvas_px, 1.0, None).map_err(|error| error.to_string())?;

    progress(Progress::Encoding);
    let bytes = write(
        &settings.path,
        &Export {
            format: settings.format,
            dpi,
            chroma: settings.chroma,
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
        dpi,
        ms: started.elapsed().as_millis(),
    })
}

/// The pixel grid and the resolution the file has to carry.
pub fn grid(doc: &CollageDoc, settings: &Settings) -> Result<(PixelSize, f64), String> {
    match settings.size {
        Size::Dpi(dpi) => {
            let pixel = doc
                .canvas
                .pixel_size(dpi)
                .map_err(|error| error.to_string())?;
            Ok((pixel, f64::from(dpi)))
        }
        Size::LongEdge(pixels) => {
            let pixel = doc
                .canvas
                .pixel_size_for_long_edge(pixels)
                .map_err(|error| error.to_string())?;
            let dpi = doc.canvas.dpi_for(pixel);
            Ok((pixel, dpi))
        }
    }
}

/// Runs [`run`] on a worker thread, calling `report` on the main context.
///
/// `report` receives progress and, at the end, the result; it is `Send` because
/// it is invoked from the worker's thread and must not touch a widget itself —
/// the window's own closure wraps each call in `MainContext::invoke` with a
/// `SendWeakRef`, which is the only way a GTK object may be reached from here.
pub fn spawn(
    doc: CollageDoc,
    sources: Vec<Option<PathBuf>>,
    settings: Settings,
    report: impl Fn(Event) + Send + Sync + 'static,
) {
    let report = std::sync::Arc::new(report);
    std::thread::Builder::new()
        .name("pixlay-export".to_string())
        .spawn(move || {
            let progress_report = std::sync::Arc::clone(&report);
            let progress = move |progress: Progress| progress_report(Event::Progress(progress));
            let outcome = run(&doc, &sources, &settings, &progress);
            report(Event::Finished(outcome));
        })
        .expect("the export thread can be started");
}

/// What an export reports back.
pub enum Event {
    Progress(Progress),
    Finished(Result<Report, String>),
}
