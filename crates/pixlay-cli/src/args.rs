// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! Argument parsing for the machine surface.
//!
//! No argument-parsing crate: the surface is six subcommands and a score of
//! flags, and the contract (exit codes, stdout purity, locale independence)
//! needs exact control over every message.
//!
//! Exit codes are part of the contract:
//!
//! * `0` success
//! * `1` usage error — unknown flags, out-of-range values, unknown template
//! * `2` the document could not be read, decoded, written or rendered

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use pixlay_core::{Frame, MAX_LONG_EDGE_PX, Point, Rgba8};
use pixlay_imaging::Rect;

use crate::cli::Failure;

/// Largest preview edge in pixels; a preview larger than this cannot be reviewed
/// by eye anyway.
pub const MAX_PREVIEW_PX: i32 = 20000;

/// Largest long edge `thumb` produces, in pixels.
///
/// A preview is bounded by the window, not by this: the largest picture
/// the shell draws is a full-window photo (a 4K window is 3840 px, and a HiDPI
/// one 7680, S13). 8192 therefore leaves room over the biggest preview the
/// product has and still keeps one preview's buffer trivially small; past it the
/// caller wants a render, which is `render --preview-px`, not a thumbnail
/// (`docs/CONTRACT.md` §5).
pub const MAX_THUMB_PX: u32 = 8192;

/// Steps one `gesture` sequence has by default: enough that the median is a
/// median, few enough that the command stays a second on a small photo (measured
/// 2026-09-22: 59 warm steps of the verify project at a 1600-px grid are 0.4 s;
/// the same on a 24 MP photo are 11 s, which is a measurement somebody asked for).
pub const DEFAULT_GESTURE_STEPS: u32 = 60;

/// Steps a `gesture` sequence may have at all. The first step is the cold one and
/// the rest are warm, so two is the shortest sequence that has both.
pub const MIN_GESTURE_STEPS: u32 = 2;

/// Most steps one `gesture` sequence may have: a minute of a 60 Hz gesture.
pub const MAX_GESTURE_STEPS: u32 = 3600;

/// The canvas widget's size at the editor's default window (1100x760), in logical
/// pixels: what `switch` measures at when `--canvas` is not given.
///
/// Measured 2026-09-25, `--release`, in the GUI test that drives the same change
/// (`crates/pixlay/tests/switch.rs`): the editor's canvas widget was **1100x575**, so
/// a 4:3 document rested at a 735x551 grid and a 16:9 one at 980x551. **Both numbers
/// are the session's, and both have moved.** The height is the band's (ruling 40
/// dropped the candidate's caption; measured 2026-09-26 the canvas is **1100x594**, a
/// 4:3 grid of 760x570), and the width is what the window's content gets: a display
/// GTK cannot treat as a session frames the window *inside* its own surface, so the
/// same window reads **1090x584** on an Xvfb with no window manager (measured
/// 2026-09-26; the numbers are in that test). The default is here rather than taken
/// from the shell because the CLI must not depend on gtk4 (`AGENTS.md`, module
/// boundaries), and the test prints the box it measured on every run, so a window
/// layout that moves the widget is a fact the next measurement shows rather than a
/// constant that silently drifts.
pub const DEFAULT_CANVAS_BOX: (i32, i32) = (1100, 575);

/// The one size parameter: what `render` uses when `--long-edge` is not given,
/// and what `probe` measures at, and what the window's export form starts at
/// (the GUI's own copy is in `crates/pixlay/src/window.rs`).
///
/// 4000 px: a square grid of it is 16 MP, an eighth of the 200 MP pixel budget
/// (measured 2026-09-20, `docs/CONTRACT.md` §8), so the default never touches the
/// limit whatever the template's shape.
pub const DEFAULT_LONG_EDGE_PX: u32 = 4000;

/// File extension a project written by `init` must have. A `.pixlay` is a
/// document, and a mistyped extension is more likely a typo than an intention.
pub const PROJECT_EXTENSION: &str = "pixlay";

pub const USAGE: &str = "\
pixlay-render - headless renderer and probe for Pixlay projects

USAGE:
    pixlay-render render --project <file.pixlay> --out <file> [OPTIONS]
    pixlay-render render --template <name> --long-edge <n> --out <file> [OPTIONS]
    pixlay-render render --template <name> --sketch --out <file> [SKETCH OPTIONS]
    pixlay-render probe  --project <file.pixlay> [OPTIONS]
    pixlay-render image  --photo <file> [--json]
    pixlay-render scan   --dir <path> [--recursive] [--json]
    pixlay-render thumb  --photo <file> --px <n> --out <file> [--region <x>,<y>,<w>,<h>] [--json]
    pixlay-render templates [--aspect <ratio>] [--slots <n>] [--json]
    pixlay-render init --template <name> --out <file.pixlay> [--photo <p>...] [--json]
    pixlay-render gesture --project <file.pixlay> --grid <px> [--slot <i>] [--steps <n>] [--json]
    pixlay-render switch --project <file.pixlay> --template <name> [--canvas <w>x<h>] [--band] [--json]
    pixlay-render edit   --project <file.pixlay> --out <file.pixlay> [EDIT OPTIONS] [--json]
    pixlay-render hit    --project <file.pixlay> --at <x>,<y> [--json]
    pixlay-render hit    --template <name> --at <x>,<y> [--json]
    pixlay-render save   --project <file.pixlay> --out <file.pixlay> [--json]
    pixlay-render --help | --version

RENDER OPTIONS:
    --project <file>    Project to render. Paths inside it are relative to it.
    --template <name>   Render a template with no photos (see `templates`); with
                        --sketch, draw the template's geometry instead.
    --out <file>        Output file. Format comes from the extension:
                        .png, .jpg, .jpeg. Required. An existing file is
                        replaced; a path that names one of the project's own
                        photos is refused (exit 1), because a source image is
                        never written to.
    --long-edge <n>     Export a long edge of exactly n pixels, 1..=30000.
                        Default 4000. The other edge follows the template's
                        aspect ratio, rounded half away from zero, so a square
                        stays exactly square. Exclusive with --preview-px, which
                        sizes the preview instead of the export.
    --preview-px <n>    Render the long edge at n pixels instead of full size,
                        1..=20000. The same draw, only the scale changes (and
                        the bitmaps are sized for it, so a preview does not pay
                        for the export). Exclusive with --long-edge, which sizes
                        the export itself.

FRAME OPTIONS (render, edit):
    --gap <rel>         The distance between two photos, as a fraction of the
                        canvas height, 0..=1. Half of it comes off every side of
                        each cell and the whole gap off the sheet's own edge, so
                        two neighbouring cells are this far apart and so are the
                        outermost photos from the border. `render` applies this
                        to the render only; `edit` stores it in the document.
                        Default is the document's own.
    --radius <rel>      Corner radius of a cell, as a fraction of the canvas
                        height, 0..=1, clamped to half the smaller side of the
                        cell. 0 is a square corner.
    --border-color <r,g,b>
                        The colour of the canvas itself, 0..=255 per channel, as
                        the backdrop of the gaps, of a rounded corner and of an
                        empty cell. The alpha of a stored colour is always 255:
                        the backdrop is painted, not blended, so an export is
                        never transparent and a preview is the same picture.

SKETCH OPTIONS (render --sketch):
    Draws a template's *geometry*: its cells in paper, every cell's outline and
    every part of the sheet no cell covers in ink. This is what the window's
    layout band shows for each candidate, so the same grid and the same three
    parameters reproduce a candidate's pixels exactly. The sheet's aspect is the
    template's declared one, so --long-edge sizes the image as it does for a
    render; a sketch has no frame, no photos and no --preview-px.
    --paper <r,g,b>     The colour a cell is filled with, 0..=255 per channel.
                        Default 255,255,255: a document-like sheet, where the
                        window passes its theme's own colours.
    --ink <r,g,b>       The colour of everything a cell is not — the outlines and
                        the sheet's ground between the cells — 0..=255 per
                        channel. Default 0,0,0.
    --stroke <px>       Stroke width, positive and finite. Default 1. The sheet's
                        own edge is stroked inside the image, so the border is a
                        full line and not a clipped one; a shared edge is one line
                        of this width, because both neighbours stroke the same
                        path.

PROBE OPTIONS:
    --project <file>    Project to probe. Required.
    --long-edge <n>     Grid to probe at, 1..=30000. Default 4000.
                        Prints, for that grid: which photo each occupied cell
                        shows, whether anything the slots do not cover is the
                        document's backdrop, how much two neighbours blend along
                        their shared edge, and whether the frame's gap measures
                        what the document claims — at the seams and at the
                        sheet's own border. Exit 2 when the verdict is not passed.

IMAGE OPTIONS:
    --photo <file>      Decode one photo and report what the decoder found: the
                        detected MIME type, the size after EXIF rotation, the
                        sample depth (8 or 16 bits) and the EXIF date when the
                        file carries one.

SCAN OPTIONS:
    --dir <path>        Directory whose photos to list. Required. One row per
                        photo: path, MIME type, width and height (after EXIF
                        rotation), the EXIF date when there is one, and mtime in
                        seconds, plus `count` and `failed`. The order is lexical
                        by path. Extensions: .jpg .jpeg .png .heic .heif .avif
                        .jxl .webp .tif .tiff. A value that carries a control
                        byte or is not UTF-8 is escaped (`\\\\`, `\\n`, `\\r`,
                        `\\t`, `\\xNN`), so a filename's newline cannot add a
                        field line.
    --recursive         Descend into subdirectories. Off by default: one folder
                        is what a caller asks for, and a whole home directory is
                        not a listing anybody reads.

THUMB OPTIONS:
    --photo <file>      Photo to preview. Required.
    --px <n>            Long edge of the preview, 1..=8192. Required. The other
                        edge keeps the source's ratio — the photo's, or the
                        --region rectangle's when one is given — at least 1
                        pixel.
    --region <x>,<y>,<w>,<h>
                        Resample one rectangle of the photo instead of all of
                        it: the rectangle's origin in the photo's own pixels,
                        then its width and height, at least 1 each. --px is
                        still the output's long edge, so --px equal to the
                        larger of w and h is that rectangle at its own size
                        (what the window's 1:1 preview asks for), and a
                        smaller one is a fit of the rectangle. A rectangle
                        the photo does not contain is refused (exit 2).
    --out <file>        Preview file, .png / .jpg / .jpeg.
                        Required (a screen-sized image). An existing file is
                        replaced; --photo itself is refused (exit 1).

GESTURE OPTIONS:
    --project <file>    Project to measure a live gesture on. Required, and every
                        occupied cell must decode: the number is about one step,
                        not about an unreadable file.
    --grid <px>         Long edge of the resting canvas grid, 1..=20000. Required:
                        the grid the window draws at is what the step costs, and
                        the editor computes it from its own size
                        (`crates/pixlay`'s canvas widget). The gesture grid is
                        half of it, as the editor's is.
    --slot <i>          The cell the gesture frames. Default: the first occupied
                        one. A cell with no photo cannot be framed.
    --steps <n>         Steps in one gesture, 2..=3600. Default 60. The first step
                        is the cold one (the gesture grid built from scratch), the
                        rest are warm, and `warm_ms` is their median.
    The gesture is the straightening one: each step turns the cell by
    `step_deg` further, which is the most expensive per-step work the editor does
    (a rotation grows the region the cell shows, and the clamp pays for the angle
    with zoom). The report carries the counts either way; `--stats` adds the
    measured times, and `verdict` says whether the warm step fit in one 60 Hz
    frame. The exit code is 0 whatever the verdict: the measurement is the result,
    and an exit code that moved with the host's speed would make the same input's
    result depend on the machine.

SWITCH OPTIONS:
    --project <file>    Project whose layout to switch. Required, and every
                        occupied cell must decode: a switch whose photos could not
                        be drawn is not the number this measures.
    --template <name>   The layout to switch to, by name — the vocabulary `edit
                        --template` and the document's own `template.name` use.
                        Required. Any template this build ships: the window's own
                        click takes a candidate of the document's cell count, which
                        `templates --slots <n>` lists.
    --canvas <w>x<h>    The canvas widget the window shows the document in, in
                        logical pixels, 1..=20000 each. Optional: the default is
                        1100x575, the box this ruler's own numbers were taken at
                        (the window's canvas moves with the band and with the
                        session — the GUI test that drives the same switch prints
                        the box it measured on every run, and that is the number to
                        pass). The grid is *derived* from it for each document
                        (`pixlay_core::canvas_grid`), because a layout change moves
                        the grid: at the default box a canvas rests at 735x551 for
                        a 4:3 sheet and 980x551 for a 16:9 one, so the switch's
                        preview-grade copies are for another edge.
    --band              Measure the layout band's rebuild with the switch: every
                        candidate of the new cell count, drawn as a *sketch* of
                        its geometry at the grid the window draws a candidate at
                        (S21; the window's own band does the same with its
                        theme's colours). Its share is `band_ms`, so `with and
                        without the band` is two runs of this command.
    `switch` measures the layout change the window's click produces, on a project
    already open on another layout: `template_ms` (the SetTemplate step and the new
    grid), `sources_ms` (the preview-grade copies — one decode per file whose copy
    is not in hand, because the new grid asks for another edge) and `composite_ms`
    (the cell bitmaps, resampled and quantized),
    with `switch_ms` their sum. The window additionally blits them and paints a
    frame, which a windowless command cannot reach; `verdict` reads `switch_ms`
    against `budget_ms` and the exit code is 0 whatever it says, for `gesture`'s
    reason: the measurement is the result (S18).

TEMPLATES OPTIONS:
    --aspect <ratio>    List only the templates authored for this layout shape,
                        as W:H (4:3) or a decimal (1.333333). Omit to list all.
    --slots <n>         List only the templates with exactly n slots, 1..=9. This
                        is the layout gallery's own query (S14): the candidates
                        for a collage of n photos. The two filters combine.

HIT OPTIONS:
    --project <file>    Project whose layout to test. Required unless --template
                        is given.
    --template <name>   Test a template with no project. The geometry is the
                        library's; a project's own embedded geometry is not.
    --at <x>,<y>        The point to test, in normalized canvas coordinates
                        (0,0 top-left to 1,1 bottom-right), like the `at` field
                        `probe` prints. Required, and both components must be
                        inside 0..=1.

SAVE OPTIONS:
    --project <file>    Project to read. Required. It is validated on the way in,
                        so a document this build cannot open is not rewritten.
    --out <file>        Project to write, .pixlay. Required. An existing file is
                        **replaced** — that is what saving is — and the write is
                        atomic (a temporary file in the same directory, renamed
                        over the target), keeping the file's own permissions.
                        Relative photo paths are rebased when the copy lands in
                        another directory, so it still finds its photos.

INIT OPTIONS:
    --template <name>   Template of the project to create. Required.
    --out <file>        Project to write, .pixlay. Required, and never
                        overwritten: `init` refuses to replace an existing file.
    --photo <file>      A photo of the project, repeated once per photo:
                        **argument order is cell order**. 1..=9 photos
                        inclusive, and the template's slot count must equal the
                        number of photos (both bounds are named on refusal).
                        A list past nine is refused (exit 1) rather than
                        trimmed: a machine caller may not have input dropped
                        silently. Omit for the photo-free project. Paths are
                        stored relative to the project file when the two share a
                        root, absolute otherwise, and a photo that is not there
                        is refused rather than written into the project.

EDIT OPTIONS:
    --project <file>    Project to read. Required.
    --out <file>        Project to write, .pixlay. Required. An existing file is
                        replaced, atomically, exactly as `save` does; relative
                        photo paths are rebased when the copy lands elsewhere.
    --slot <i>          The cell the framing flags below apply to. Without it
                        they are refused: every other flag is about the whole
                        document.
    --photo <file>      The photo the `--slot` cell shows instead of the one it
                        has. Needs --slot, and a file that is not there is
                        refused rather than written into the project.
    --add-cell          Take the layout with one slot more. A cell the layout kept
                        is placed again first (see --remove-cell), so this is also
                        how a kept photo comes back; otherwise the new cell is
                        empty — the window's `+` (`Command::AddCell`). The cell
                        count is the layout's, so this is how a collage grows to
                        hold one more photo.
    --remove-cell       Take the layout with one slot fewer — the window's `−`. The
                        cell leaves the sheet **whole and kept**: a photo leaves
                        the collage only when it is deleted (`--slot <i> --clear`,
                        a replace, a cut), and a later growth places the kept cell
                        again, so `--remove-cell` then `--add-cell` is the
                        document it was (S28, ruling 43). Refused at one cell, the
                        floor (a single photo is a legal collage, and the sheet is
                        its layout). The mirror image of --add-cell, and refused
                        together with it: run `edit` twice for both.
    --add-photo <file>  Append a photo: it goes to the first empty cell, and if
                        there is none the layout grows by one slot — an arrival
                        always lands in a cell of its own, so a kept cell keeps
                        waiting (S28). Repeated once per photo, in argument order.
                        Refused past nine photos in the document, placed and kept
                        together (exit 2: the document cannot take it, and nothing
                        is written), where `init --photo` refuses such a list as a
                        usage error (exit 1): the CLI never trims a list, and the
                        two exit codes are its contract.
    --swap <i>,<j>      Exchange two cells whole — photo and framing both, since
                        the framing is what makes a photo look right in *that*
                        cell. Refused for the same cell twice (exit 1) and for a
                        cell the layout does not have (exit 1).
    --template <name>   Switch the document to another layout (see
                        `templates`), keeping the surviving cells' photos and
                        framing. The count is not required to match: a layout with
                        fewer slots keeps the tail it cannot place, and one with
                        more places the kept cells again and then appends empty
                        ones (S28, ruling 43).
    --rotate <deg>      Set the cell's rotation to any finite angle, clockwise
                        on screen. It is stored wrapped into -180..=180 and is
                        never reduced by the clamp; the zoom is raised to
                        whatever covering that exact angle needs.
    --zoom <z>          Set the cell's zoom, 0 < z <= 1000, as displayed photo
                        width over the cell's width.
    --offset <x>,<y>    Set the photo's centre, from -1 to 1 cell widths and
                        heights away from the cell's centre.
    --clear             Empty the cell: no photo, and its framing back to its
                        default. Exclusive with the framing flags and with
                        --photo.
    The edit is applied in the order --template, --add-cell/--remove-cell, the
    --swap, --add-photo, --slot/--photo and then the framing, so the framing is
    fitted against the document the earlier flags produced. The stored `crop` is
    the *fit* of what was asked for (a crop is a request; what is drawn is what
    covers), so `edit` applied twice to the same project writes the same bytes. A
    cell with no photo has nothing to fit against and keeps the numbers as given;
    the fit returns when the cell gets a photo. The report's `kept` counts the
    cells a layout change took off the sheet: they are kept, not lost, and a
    later growth places them again.

COMMON OPTIONS:
    --json              Print one JSON object instead of key = value lines.
    --stats             Add measured fields: ms, peak_rss_mb, icc. `render`
                        adds encode_ms as well; `gesture` adds its four phase
                        times and the worst warm step; `switch` adds its three
                        phases, the band's share and the verdict; `probe` does
                        not encode.
    -h, --help          Print this help.
    -V, --version       Print the version.

OUTPUT:
    stdout carries the result and nothing else. Diagnostics go to stderr. The
    output is byte-identical for identical input; with --stats the measured
    fields (ms, peak_rss_mb) vary, as measurements do.

EXIT CODES:
    0  success
    1  usage error (unknown flag, out-of-range value, unknown template, an
       --out that names one of the project's own photos)
    2  project, decode, render or write failure, or a probe verdict of `failed`
       (the failing path or the failing check is on stderr)
";

/// What the command line asked for.
pub enum Command {
    Render(RenderArgs),
    Probe(ProbeArgs),
    Image(ImageArgs),
    Scan(ScanArgs),
    Thumb(ThumbArgs),
    Templates(TemplatesArgs),
    Init(InitArgs),
    Edit(EditArgs),
    Hit(HitArgs),
    Save(SaveArgs),
    Gesture(GestureArgs),
    Switch(SwitchArgs),
    Help,
    Version,
}

/// Which document a command starts from.
pub enum Source {
    /// A `.pixlay` project, photos included.
    Project(PathBuf),
    /// A template with no photos at all: the smoke path of `AGENTS.md`.
    Template(String),
}

pub struct RenderArgs {
    pub source: Source,
    pub out: PathBuf,
    /// The one size parameter; `None` renders at [`DEFAULT_LONG_EDGE_PX`].
    pub long_edge: Option<u32>,
    pub preview_px: Option<i32>,
    /// Frame overrides for this render only: the document is not changed, and
    /// nothing is written back to it (`edit` is the command that stores a frame).
    pub frame: FrameArgs,
    /// `--sketch`: draw the template's geometry instead of a document. `Some`
    /// carries the three parameters, already at their defaults where the command
    /// line named none.
    pub sketch: Option<SketchArgs>,
    pub stats: bool,
    pub json: bool,
}

/// The sketch's three parameters as the command line carries them: the sheet's
/// ground, the ink of its cell outlines, and the stroke's width in pixels.
pub struct SketchArgs {
    pub paper: Rgba8,
    pub ink: Rgba8,
    pub stroke_px: f64,
}

impl SketchArgs {
    /// The renderer's own view of the three.
    pub fn style(&self) -> pixlay_render::Sketch {
        pixlay_render::Sketch {
            paper: self.paper,
            ink: self.ink,
            stroke_px: self.stroke_px,
        }
    }
}

/// The frame flags, as a command line carries them: each is `Some` only when the
/// caller asked for it, so "not given" and "given the value the document already
/// has" stay different things.
#[derive(Default)]
pub struct FrameArgs {
    pub gap: Option<f64>,
    pub radius: Option<f64>,
    pub border: Option<Rgba8>,
}

impl FrameArgs {
    /// Whether any of the three was given.
    pub fn any(&self) -> bool {
        self.gap.is_some() || self.radius.is_some() || self.border.is_some()
    }

    /// Applies the given ones to `frame`, leaving the others as the document had
    /// them.
    pub fn apply(&self, frame: &mut Frame) {
        if let Some(gap) = self.gap {
            frame.gap_rel = gap;
        }
        if let Some(radius) = self.radius {
            frame.radius_rel = radius;
        }
        if let Some(border) = self.border {
            frame.color = border;
        }
    }
}

/// `edit`: one cell's framing and/or photo, the document's frame, its layout, and
/// the photos it holds.
pub struct EditArgs {
    pub project: PathBuf,
    pub out: PathBuf,
    /// The cell the framing flags and `--photo` apply to. `None` edits the frame,
    /// the layout or the photo count alone, which is what makes `--rotate` without
    /// `--slot` a usage error rather than a silent no-op.
    pub slot: Option<usize>,
    /// The photo the `--slot` cell shows instead. Only with `slot`.
    pub photo: Option<PathBuf>,
    /// Photos to append (`--add-photo`), in argument order.
    pub add_photos: Vec<PathBuf>,
    /// Take the layout with one slot more, empty (`--add-cell`).
    pub add_cell: bool,
    /// Take the layout with one slot less (`--remove-cell`).
    pub remove_cell: bool,
    /// Exchange two cells whole (`--swap <i>,<j>`).
    pub swap: Option<(usize, usize)>,
    /// Switch the layout, keeping the surviving cells (`--template`).
    pub template: Option<String>,
    pub rotate: Option<f64>,
    pub zoom: Option<f64>,
    pub offset: Option<(f64, f64)>,
    /// Empty the cell: no photo, default framing.
    pub clear: bool,
    pub frame: FrameArgs,
    pub json: bool,
}

pub struct ImageArgs {
    pub photo: PathBuf,
    pub json: bool,
}

/// `scan`: a directory of photos, listed. Stage 1's machine surface (S9).
pub struct ScanArgs {
    pub dir: PathBuf,
    /// Descend into subdirectories.
    pub recursive: bool,
    pub stats: bool,
    pub json: bool,
}

/// `thumb`: one photo (or one rectangle of it), resampled to a preview. A preview's
/// costly half (S9), and since S15j the same call a 1:1 preview makes.
pub struct ThumbArgs {
    pub photo: PathBuf,
    /// Long edge of the preview, 1..=`MAX_THUMB_PX`.
    pub px: u32,
    /// `--region x,y,w,h`: the rectangle of the photo to resample. `None` is the whole
    /// photo, which is what every `thumb` before S15j asked for.
    pub region: Option<Rect>,
    pub out: PathBuf,
    pub stats: bool,
    pub json: bool,
}

/// `gesture`: one live framing step, measured (S12).
pub struct GestureArgs {
    pub project: PathBuf,
    /// Long edge of the resting canvas grid, 1..=`MAX_PREVIEW_PX`.
    pub grid: u32,
    /// The cell the gesture frames. `None` takes the first occupied one.
    pub slot: Option<usize>,
    /// Steps in the sequence, 2..=`MAX_GESTURE_STEPS`.
    pub steps: u32,
    pub stats: bool,
    pub json: bool,
}

/// `switch`: one layout change, measured (S18).
pub struct SwitchArgs {
    pub project: PathBuf,
    /// The layout to switch to, by name — the vocabulary `edit --template` and the
    /// document's own `template.name` use.
    pub template: String,
    /// The canvas widget's size, in logical pixels: the box both grids are derived
    /// from (`pixlay_core::canvas_grid`), which is what the window does. Default:
    /// [`DEFAULT_CANVAS_BOX`].
    pub canvas: (i32, i32),
    /// Whether the band's rebuild is measured with it: the candidates a layout
    /// change puts on the strip, rendered the way the window renders them
    /// (`pixlay_core::templates::candidate_grid`).
    pub band: bool,
    pub stats: bool,
    pub json: bool,
}

pub struct ProbeArgs {
    pub project: PathBuf,
    /// The one size parameter; `None` probes at [`DEFAULT_LONG_EDGE_PX`].
    pub long_edge: Option<u32>,
    pub stats: bool,
    pub json: bool,
}

pub struct TemplatesArgs {
    /// Canvas aspect ratio to filter by, already parsed. `None` lists the library.
    pub aspect: Option<f64>,
    /// Slot count to filter by. `None` lists every count — this is the layout
    /// gallery's query, `Selection::layouts` spelled as a flag (S14).
    pub slots: Option<usize>,
    pub json: bool,
}

pub struct InitArgs {
    pub template: String,
    pub out: PathBuf,
    /// The photos of the project, **argument order = cell order**. Empty writes
    /// the photo-free project S2 shipped.
    pub photos: Vec<PathBuf>,
    pub json: bool,
}

/// `hit`: the point → slot question, answered with no rendering at all.
pub struct HitArgs {
    pub source: Source,
    /// Normalized canvas coordinates, both components inside `0..=1`.
    pub at: Point,
    pub json: bool,
}

/// `save`: read a project and write it out, atomically.
pub struct SaveArgs {
    pub project: PathBuf,
    pub out: PathBuf,
    pub json: bool,
}

#[derive(Default)]
struct Flags {
    project: Option<PathBuf>,
    template: Option<String>,
    out: Option<PathBuf>,
    long_edge: Option<u32>,
    preview_px: Option<i32>,
    /// `--photo`, repeatable: `image` and `thumb` take exactly one, `init` takes
    /// one per cell in argument order, and `edit` takes the one `--slot` shows.
    photos: Vec<PathBuf>,
    /// `--add-photo`, repeatable: the photos `edit` appends.
    add_photos: Vec<PathBuf>,
    /// `--add-cell`: take the layout with one slot more.
    add_cell: bool,
    /// `--remove-cell`: take the layout with one slot less.
    remove_cell: bool,
    /// `--swap <i>,<j>`: exchange two cells whole.
    swap: Option<(usize, usize)>,
    /// `--slots <n>`: the layout gallery's count filter for `templates`.
    slots: Option<usize>,
    aspect: Option<f64>,
    at: Option<Point>,
    dir: Option<PathBuf>,
    recursive: bool,
    px: Option<u32>,
    region: Option<Rect>,
    grid: Option<u32>,
    steps: Option<u32>,
    /// `--band`: measure the layout band's own rebuild as part of a switch.
    band: bool,
    /// `--sketch`: draw a template's geometry instead of a document's photos.
    sketch: bool,
    /// `--paper` / `--ink` / `--stroke`: the sketch's own parameters.
    paper: Option<Rgba8>,
    ink: Option<Rgba8>,
    stroke: Option<f64>,
    /// `--canvas <w>x<h>`: the canvas widget a switch's two grids are derived from.
    canvas: Option<(i32, i32)>,
    gap: Option<f64>,
    radius: Option<f64>,
    border: Option<Rgba8>,
    slot: Option<usize>,
    rotate: Option<f64>,
    zoom: Option<f64>,
    offset: Option<(f64, f64)>,
    clear: bool,
    stats: bool,
    json: bool,
}

impl Flags {
    /// Whether any of the three frame flags was given.
    fn frame_any(&self) -> bool {
        self.gap.is_some() || self.radius.is_some() || self.border.is_some()
    }
}

/// The first flag a subcommand does not accept, with the reason to quote back.
///
/// A flag missing from a subcommand's list is refused there rather than ignored:
/// a silently dropped `--project` on `templates` looks like it was honored.
fn first_rejected(name: &str, flags: &Flags) -> Option<(&'static str, &'static str)> {
    let valid: &[&str] = match name {
        "render" => &[
            "project",
            "template",
            "out",
            "long-edge",
            "preview-px",
            "gap",
            "radius",
            "border-color",
            "sketch",
            "paper",
            "ink",
            "stroke",
            "stats",
        ],
        "probe" => &["project", "long-edge", "stats"],
        "image" => &["photo"],
        "scan" => &["dir", "recursive", "stats"],
        "thumb" => &["photo", "px", "out", "region", "stats"],
        "templates" => &["aspect", "slots"],
        "init" => &["template", "out", "photo"],
        "edit" => &[
            "project",
            "out",
            "slot",
            "photo",
            "add-photo",
            "add-cell",
            "remove-cell",
            "swap",
            "template",
            "rotate",
            "zoom",
            "offset",
            "clear",
            "gap",
            "radius",
            "border-color",
        ],
        "hit" => &["project", "template", "at"],
        "save" => &["project", "out"],
        "gesture" => &["project", "grid", "slot", "steps", "stats"],
        "switch" => &["project", "template", "canvas", "band", "stats"],
        _ => &[],
    };
    let present: [(&'static str, bool); 30] = [
        ("project", flags.project.is_some()),
        ("template", flags.template.is_some()),
        ("out", flags.out.is_some()),
        ("long-edge", flags.long_edge.is_some()),
        ("preview-px", flags.preview_px.is_some()),
        ("photo", !flags.photos.is_empty()),
        ("add-photo", !flags.add_photos.is_empty()),
        ("add-cell", flags.add_cell),
        ("remove-cell", flags.remove_cell),
        ("swap", flags.swap.is_some()),
        ("slots", flags.slots.is_some()),
        ("aspect", flags.aspect.is_some()),
        ("at", flags.at.is_some()),
        ("dir", flags.dir.is_some()),
        ("recursive", flags.recursive),
        ("px", flags.px.is_some()),
        ("region", flags.region.is_some()),
        ("grid", flags.grid.is_some()),
        ("steps", flags.steps.is_some()),
        ("band", flags.band),
        ("canvas", flags.canvas.is_some()),
        ("gap", flags.gap.is_some()),
        ("radius", flags.radius.is_some()),
        ("border-color", flags.border.is_some()),
        ("slot", flags.slot.is_some()),
        ("rotate", flags.rotate.is_some()),
        ("zoom", flags.zoom.is_some()),
        ("offset", flags.offset.is_some()),
        ("clear", flags.clear),
        ("stats", flags.stats),
    ];
    present
        .into_iter()
        .find(|(flag, present)| *present && !valid.contains(flag))
        .map(|(flag, _)| (flag, reason(name, flag)))
}

/// Why `flag` does not belong to `subcommand`, for the usage message.
///
/// The phrase completes "`--flag` is not valid for `<subcommand>`: …", so it
/// starts at the verb. Named per pair rather than per flag, because `--out` is
/// refused by `probe` and by `templates` for different reasons and a message that
/// names the wrong one is worse than none.
fn reason(name: &str, flag: &str) -> &'static str {
    match (name, flag) {
        ("render", "aspect") => "render takes no --aspect; list the templates first",
        ("render", "photo") => "render takes photos from a project, not from --photo",
        ("probe", "photo") => "probe takes photos from a project",
        ("image", "project") => "image decodes one file; use --photo",
        ("image", "out") => "image writes no file",
        ("image", "template") => "image decodes one file; use --photo",
        ("probe", "template") => "probe reads a project",
        ("probe", "out") => "probe writes no file",
        ("probe", "preview-px") => "probe always renders at full size",
        ("image", "long-edge") => "image decodes at the file's own size",
        ("scan", "project") => "scan lists a directory, not a project",
        ("templates", "long-edge") => "templates only lists the library",
        ("init", "long-edge") => "init only writes the project file",
        ("probe", "aspect") => "probe filters no template list",
        ("render" | "probe" | "image", "at") => {
            "only `hit` tests one point; the other commands work on a whole document"
        }
        // The flags that belong to exactly one subcommand, whatever the caller
        // typed them on: `--dir`, `--recursive` and `--px` are `scan`'s and
        // `thumb`'s, and saying which command owns one is more use than "not
        // accepted here".
        (_, "dir") => "only `scan` lists a directory",
        (_, "recursive") => "only `scan` descends into subdirectories",
        (_, "px") => "only `thumb` sizes a preview",
        (_, "region") => "only `thumb` resamples one rectangle of a photo",
        (_, "grid") => "only `gesture` measures at a canvas grid",
        (_, "steps") => "only `gesture` runs a sequence of steps",
        (_, "band") => "only `switch` measures the layout band's rebuild",
        (_, "sketch") => "only `render` draws a template's geometry as a sketch",
        (_, "paper" | "ink" | "stroke") => {
            "only `render --sketch` has a paper, an ink and a stroke"
        }
        (_, "canvas") => "only `switch` measures at a canvas widget's size",
        (
            "probe" | "image" | "scan" | "thumb" | "templates" | "init" | "hit" | "save",
            "gap" | "radius" | "border-color",
        ) => "only `render` and `edit` take the frame",
        (_, "slots") => "only `templates` filters the library by slot count",
        (_, "add-photo") => "only `edit` appends a photo",
        (_, "add-cell") => "only `edit` grows the layout",
        (_, "remove-cell") => "only `edit` shrinks the layout",
        (_, "swap") => "only `edit` exchanges two cells",
        (_, "slot" | "rotate" | "zoom" | "offset" | "clear") => {
            "only `edit` changes one cell's framing"
        }
        ("hit", _) => "hit reads a layout and answers about one point in it",
        ("save", _) => "save reads a project and writes a project",
        ("templates", _) => "templates only lists the library",
        ("scan", _) => "scan reads a directory; run it with --dir",
        ("thumb", _) => "thumb takes one photo, its preview size and an output file",
        ("init", "project") => "init takes a template, not a project",
        ("init", _) => "init only writes the project file",
        _ => "not accepted here",
    }
}

pub fn parse(argv: &[OsString]) -> Result<Command, Failure> {
    let Some(head) = argv.first() else {
        return Err(Failure::Usage("missing subcommand".to_string()));
    };
    let head = head
        .to_str()
        .ok_or_else(|| Failure::Usage("subcommand must be valid UTF-8".to_string()))?;
    let subcommand = match head {
        "render" | "probe" | "image" | "templates" | "init" | "edit" | "hit" | "save" | "scan"
        | "thumb" | "gesture" | "switch" => head,
        "--help" | "-h" | "help" => return Ok(Command::Help),
        "--version" | "-V" | "version" => return Ok(Command::Version),
        other if other.starts_with('-') => {
            return Err(Failure::Usage(format!("unknown option {other}")));
        }
        other => return Err(Failure::Usage(format!("unknown subcommand {other}"))),
    };

    let mut flags = Flags::default();
    let mut rest = argv[1..].iter();
    while let Some(arg) = rest.next() {
        let text = arg.to_str().ok_or_else(|| {
            Failure::Usage(
                "options must be valid UTF-8; only paths may be arbitrary bytes".to_string(),
            )
        })?;
        let Some(body) = text.strip_prefix("--") else {
            if text == "-h" {
                return Ok(Command::Help);
            }
            if text == "-V" {
                return Ok(Command::Version);
            }
            return Err(Failure::Usage(format!("unexpected argument {text}")));
        };
        let (name, inline) = match body.split_once('=') {
            Some((name, value)) => (name, Some(OsString::from(value))),
            None => (body, None),
        };
        // Values may be arbitrary bytes (paths), so they are taken raw.
        let mut value = |name: &str| -> Result<OsString, Failure> {
            match inline.clone() {
                Some(value) => Ok(value),
                None => rest
                    .next()
                    .cloned()
                    .ok_or_else(|| Failure::Usage(format!("option --{name} needs a value"))),
            }
        };
        match name {
            "json" => flags.json = true,
            "stats" => flags.stats = true,
            "help" => return Ok(Command::Help),
            "project" => set_once(
                &mut flags.project,
                PathBuf::from(value("project")?),
                "project",
            )?,
            "out" => set_once(&mut flags.out, PathBuf::from(value("out")?), "out")?,
            "template" => {
                let raw = value("template")?;
                let name = raw
                    .to_str()
                    .ok_or_else(|| Failure::Usage("--template must be valid UTF-8".to_string()))?;
                set_once(&mut flags.template, name.to_string(), "template")?;
            }
            "preview-px" => {
                let raw = number(&value("preview-px")?, "preview-px")?;
                let pixels = i32::try_from(raw).map_err(|_| {
                    Failure::Usage(format!(
                        "--preview-px must be a positive integer, got {raw}"
                    ))
                })?;
                if !(1..=MAX_PREVIEW_PX).contains(&pixels) {
                    return Err(Failure::Usage(format!(
                        "--preview-px {pixels} is outside 1..={MAX_PREVIEW_PX}"
                    )));
                }
                set_once(&mut flags.preview_px, pixels, "preview-px")?;
            }
            "long-edge" => {
                let raw = number(&value("long-edge")?, "long-edge")?;
                let pixels = u32::try_from(raw).map_err(|_| {
                    Failure::Usage(format!("--long-edge must be a positive integer, got {raw}"))
                })?;
                if !(1..=MAX_LONG_EDGE_PX).contains(&pixels) {
                    return Err(Failure::Usage(format!(
                        "--long-edge {pixels} is outside 1..={MAX_LONG_EDGE_PX}"
                    )));
                }
                set_once(&mut flags.long_edge, pixels, "long-edge")?;
            }
            "photo" => flags.photos.push(PathBuf::from(value("photo")?)),
            "add-photo" => flags.add_photos.push(PathBuf::from(value("add-photo")?)),
            "add-cell" => flags.add_cell = true,
            "remove-cell" => flags.remove_cell = true,
            "swap" => {
                let swap = parse_swap(&value("swap")?)?;
                set_once(&mut flags.swap, swap, "swap")?;
            }
            "slots" => {
                let raw = number(&value("slots")?, "slots")?;
                let slots = usize::try_from(raw).map_err(|_| {
                    Failure::Usage(format!("--slots must be a positive integer, got {raw}"))
                })?;
                if !(pixlay_core::MIN_SLOTS..=pixlay_core::MAX_SLOTS).contains(&slots) {
                    return Err(Failure::Usage(format!(
                        "--slots {slots} is outside {}..={}",
                        pixlay_core::MIN_SLOTS,
                        pixlay_core::MAX_SLOTS
                    )));
                }
                set_once(&mut flags.slots, slots, "slots")?;
            }
            "dir" => set_once(&mut flags.dir, PathBuf::from(value("dir")?), "dir")?,
            "recursive" => flags.recursive = true,
            "px" => {
                let raw = number(&value("px")?, "px")?;
                let pixels = u32::try_from(raw).map_err(|_| {
                    Failure::Usage(format!("--px must be a positive integer, got {raw}"))
                })?;
                if !(1..=MAX_THUMB_PX).contains(&pixels) {
                    return Err(Failure::Usage(format!(
                        "--px {pixels} is outside 1..={MAX_THUMB_PX}"
                    )));
                }
                set_once(&mut flags.px, pixels, "px")?;
            }
            "region" => {
                let raw = value("region")?;
                let region = parse_region(&raw)?;
                set_once(&mut flags.region, region, "region")?;
            }
            "grid" => {
                let raw = number(&value("grid")?, "grid")?;
                let pixels = u32::try_from(raw).map_err(|_| {
                    Failure::Usage(format!("--grid must be a positive integer, got {raw}"))
                })?;
                if !(1..=MAX_PREVIEW_PX as u32).contains(&pixels) {
                    return Err(Failure::Usage(format!(
                        "--grid {pixels} is outside 1..={MAX_PREVIEW_PX}"
                    )));
                }
                set_once(&mut flags.grid, pixels, "grid")?;
            }
            "steps" => {
                let raw = number(&value("steps")?, "steps")?;
                let steps = u32::try_from(raw).map_err(|_| {
                    Failure::Usage(format!("--steps must be a positive integer, got {raw}"))
                })?;
                if !(MIN_GESTURE_STEPS..=MAX_GESTURE_STEPS).contains(&steps) {
                    return Err(Failure::Usage(format!(
                        "--steps {steps} is outside {MIN_GESTURE_STEPS}..={MAX_GESTURE_STEPS}"
                    )));
                }
                set_once(&mut flags.steps, steps, "steps")?;
            }
            "band" => flags.band = true,
            "sketch" => flags.sketch = true,
            "paper" => {
                let color = parse_color(&value("paper")?)?;
                set_once(&mut flags.paper, color, "paper")?;
            }
            "ink" => {
                let color = parse_color(&value("ink")?)?;
                set_once(&mut flags.ink, color, "ink")?;
            }
            "stroke" => {
                let width = float(&value("stroke")?, "stroke")?;
                if width <= 0.0 {
                    return Err(Failure::Usage(format!(
                        "--stroke {width} is not a line: a stroke width is positive"
                    )));
                }
                set_once(&mut flags.stroke, width, "stroke")?;
            }
            "canvas" => {
                let box_size = parse_size(&value("canvas")?, "canvas")?;
                set_once(&mut flags.canvas, box_size, "canvas")?;
            }
            "aspect" => {
                let raw = value("aspect")?;
                let aspect = parse_aspect(&raw)?;
                set_once(&mut flags.aspect, aspect, "aspect")?;
            }
            "gap" => {
                let gap = float(&value("gap")?, "gap")?;
                if !(0.0..=pixlay_core::MAX_FRAME_REL).contains(&gap) {
                    return Err(Failure::Usage(format!(
                        "--gap {gap} is outside 0..={}",
                        pixlay_core::MAX_FRAME_REL
                    )));
                }
                set_once(&mut flags.gap, gap, "gap")?;
            }
            "radius" => {
                let radius = float(&value("radius")?, "radius")?;
                if !(0.0..=pixlay_core::MAX_FRAME_REL).contains(&radius) {
                    return Err(Failure::Usage(format!(
                        "--radius {radius} is outside 0..={}",
                        pixlay_core::MAX_FRAME_REL
                    )));
                }
                set_once(&mut flags.radius, radius, "radius")?;
            }
            "border-color" => {
                let color = parse_color(&value("border-color")?)?;
                set_once(&mut flags.border, color, "border-color")?;
            }
            "slot" => {
                let raw = number(&value("slot")?, "slot")?;
                let slot = usize::try_from(raw).map_err(|_| {
                    Failure::Usage(format!("--slot must be a cell index, got {raw}"))
                })?;
                set_once(&mut flags.slot, slot, "slot")?;
            }
            "rotate" => {
                let rotate = float(&value("rotate")?, "rotate")?;
                set_once(&mut flags.rotate, rotate, "rotate")?;
            }
            "zoom" => {
                let zoom = float(&value("zoom")?, "zoom")?;
                if !(f64::MIN_POSITIVE..=pixlay_core::MAX_ZOOM).contains(&zoom) {
                    return Err(Failure::Usage(format!(
                        "--zoom {zoom} is outside 0..={}",
                        pixlay_core::MAX_ZOOM
                    )));
                }
                set_once(&mut flags.zoom, zoom, "zoom")?;
            }
            "offset" => {
                let raw = value("offset")?;
                let offset = parse_offset(&raw)?;
                set_once(&mut flags.offset, offset, "offset")?;
            }
            "clear" => flags.clear = true,
            "at" => {
                let raw = value("at")?;
                let point = parse_point(&raw)?;
                set_once(&mut flags.at, point, "at")?;
            }
            other => return Err(Failure::Usage(format!("unknown option --{other}"))),
        }
    }

    if let Some((flag, why)) = first_rejected(subcommand, &flags) {
        return Err(Failure::Usage(format!(
            "--{flag} is not valid for {subcommand}: {why}"
        )));
    }

    match subcommand {
        "probe" => {
            let project = flags
                .project
                .ok_or_else(|| Failure::Usage("probe needs --project".to_string()))?;
            Ok(Command::Probe(ProbeArgs {
                project,
                long_edge: flags.long_edge,
                stats: flags.stats,
                json: flags.json,
            }))
        }
        "image" => {
            let photo = one_photo("image", &flags)?;
            Ok(Command::Image(ImageArgs {
                photo,
                json: flags.json,
            }))
        }
        "scan" => {
            let dir = flags
                .dir
                .ok_or_else(|| Failure::Usage("scan needs --dir <path>".to_string()))?;
            Ok(Command::Scan(ScanArgs {
                dir,
                recursive: flags.recursive,
                stats: flags.stats,
                json: flags.json,
            }))
        }
        "thumb" => {
            let photo = one_photo("thumb", &flags)?;
            let px = flags
                .px
                .ok_or_else(|| Failure::Usage("thumb needs --px <n>".to_string()))?;
            let out = flags
                .out
                .ok_or_else(|| Failure::Usage("thumb needs --out <file>".to_string()))?;
            Ok(Command::Thumb(ThumbArgs {
                photo,
                px,
                region: flags.region,
                out,
                stats: flags.stats,
                json: flags.json,
            }))
        }
        "gesture" => {
            let project = flags.project.ok_or_else(|| {
                Failure::Usage("gesture needs --project <file.pixlay>".to_string())
            })?;
            let grid = flags
                .grid
                .ok_or_else(|| Failure::Usage("gesture needs --grid <px>".to_string()))?;
            Ok(Command::Gesture(GestureArgs {
                project,
                grid,
                slot: flags.slot,
                steps: flags.steps.unwrap_or(DEFAULT_GESTURE_STEPS),
                stats: flags.stats,
                json: flags.json,
            }))
        }
        "switch" => {
            let project = flags.project.ok_or_else(|| {
                Failure::Usage("switch needs --project <file.pixlay>".to_string())
            })?;
            let template = flags
                .template
                .ok_or_else(|| Failure::Usage("switch needs --template <name>".to_string()))?;
            Ok(Command::Switch(SwitchArgs {
                project,
                template,
                canvas: flags.canvas.unwrap_or(DEFAULT_CANVAS_BOX),
                band: flags.band,
                stats: flags.stats,
                json: flags.json,
            }))
        }
        "templates" => Ok(Command::Templates(TemplatesArgs {
            aspect: flags.aspect,
            slots: flags.slots,
            json: flags.json,
        })),
        "hit" => {
            let at = flags
                .at
                .ok_or_else(|| Failure::Usage("hit needs --at <x>,<y>".to_string()))?;
            let source = source_of("hit", &flags)?;
            Ok(Command::Hit(HitArgs {
                source,
                at,
                json: flags.json,
            }))
        }
        "save" => {
            let project = flags
                .project
                .ok_or_else(|| Failure::Usage("save needs --project <file>".to_string()))?;
            let out = flags
                .out
                .ok_or_else(|| Failure::Usage("save needs --out <file.pixlay>".to_string()))?;
            require_project_extension(&out)?;
            Ok(Command::Save(SaveArgs {
                project,
                out,
                json: flags.json,
            }))
        }
        "init" => {
            let template = flags
                .template
                .ok_or_else(|| Failure::Usage("init needs --template <name>".to_string()))?;
            let out = flags
                .out
                .ok_or_else(|| Failure::Usage("init needs --out <file.pixlay>".to_string()))?;
            require_project_extension(&out)?;
            Ok(Command::Init(InitArgs {
                template,
                out,
                photos: std::mem::take(&mut flags.photos),
                json: flags.json,
            }))
        }
        "edit" => {
            // A framing flag without a cell to apply it to has nothing to edit, and
            // taking it as "the frame, then" would be a silent drop (the rule
            // a dropped `--project` on `templates` follows). The checks come before the moves so
            // the message is about the command line, not about a consumed flag.
            let framing = flags.rotate.is_some()
                || flags.zoom.is_some()
                || flags.offset.is_some()
                || flags.clear;
            if framing && flags.slot.is_none() {
                return Err(Failure::Usage(
                    "--rotate/--zoom/--offset/--clear need --slot <i>: they are about one cell"
                        .to_string(),
                ));
            }
            if !flags.photos.is_empty() && flags.slot.is_none() {
                return Err(Failure::Usage(
                    "--photo needs --slot <i>: it is the photo that cell shows (--add-photo appends)"
                        .to_string(),
                ));
            }
            if flags.photos.len() > 1 {
                return Err(Failure::Usage(format!(
                    "--slot edits one cell; --photo was given {} times",
                    flags.photos.len()
                )));
            }
            if flags.clear && !flags.photos.is_empty() {
                return Err(Failure::Usage(
                    "--clear empties the cell; it and --photo are mutually exclusive".to_string(),
                ));
            }
            if flags.clear
                && (flags.rotate.is_some() || flags.zoom.is_some() || flags.offset.is_some())
            {
                return Err(Failure::Usage(
                    "--clear empties the cell; it and the framing flags are mutually exclusive"
                        .to_string(),
                ));
            }
            // The two batch flags are opposites, and applying both would make the
            // result depend on which one ran first. One command, one intent.
            if flags.remove_cell && flags.add_cell {
                return Err(Failure::Usage(
                    "--remove-cell and --add-cell cannot be one edit: add first, then drop"
                        .to_string(),
                ));
            }
            let changes = framing
                || !flags.photos.is_empty()
                || !flags.add_photos.is_empty()
                || flags.add_cell
                || flags.remove_cell
                || flags.swap.is_some()
                || flags.template.is_some();
            if !changes && !flags.frame_any() {
                return Err(Failure::Usage(
                    "edit needs something to change: --slot with a framing flag, --photo, \
                     --add-photo, --add-cell, --remove-cell, --swap, --template, or \
                     --gap/--radius/--border-color"
                        .to_string(),
                ));
            }
            let project = flags
                .project
                .take()
                .ok_or_else(|| Failure::Usage("edit needs --project <file.pixlay>".to_string()))?;
            let out = flags
                .out
                .take()
                .ok_or_else(|| Failure::Usage("edit needs --out <file.pixlay>".to_string()))?;
            require_project_extension(&out)?;
            Ok(Command::Edit(EditArgs {
                project,
                out,
                slot: flags.slot,
                photo: flags.photos.first().cloned(),
                add_photos: std::mem::take(&mut flags.add_photos),
                add_cell: flags.add_cell,
                remove_cell: flags.remove_cell,
                swap: flags.swap,
                template: flags.template.take(),
                rotate: flags.rotate,
                zoom: flags.zoom,
                offset: flags.offset,
                clear: flags.clear,
                frame: FrameArgs {
                    gap: flags.gap,
                    radius: flags.radius,
                    border: flags.border,
                },
                json: flags.json,
            }))
        }
        _ => {
            let source = source_of("render", &flags)?;
            if flags.long_edge.is_some() && flags.preview_px.is_some() {
                return Err(Failure::Usage(
                    "--preview-px renders a preview of the export; --long-edge sizes the export"
                        .to_string(),
                ));
            }
            // A sketch is a *template's* geometry: the document's own fields (its
            // frame, its photos, a preview of a finished collage) have nothing to
            // apply to, and taking one as if it did would be a silent drop.
            let colors = flags.paper.is_some() || flags.ink.is_some() || flags.stroke.is_some();
            if !flags.sketch && colors {
                return Err(Failure::Usage(
                    "--paper, --ink and --stroke are the sketch's parameters: give --sketch"
                        .to_string(),
                ));
            }
            let sketch = if flags.sketch {
                if !matches!(source, Source::Template(_)) {
                    return Err(Failure::Usage(
                        "--sketch draws a template's geometry: give --template <name>".to_string(),
                    ));
                }
                if flags.preview_px.is_some() {
                    return Err(Failure::Usage(
                        "--sketch draws at --long-edge; there is no document to preview"
                            .to_string(),
                    ));
                }
                if flags.frame_any() {
                    return Err(Failure::Usage(
                        "--sketch draws a template; the frame (--gap/--radius/--border-color) is a document's"
                            .to_string(),
                    ));
                }
                let defaults = pixlay_render::Sketch::default();
                Some(SketchArgs {
                    paper: flags.paper.unwrap_or(defaults.paper),
                    ink: flags.ink.unwrap_or(defaults.ink),
                    stroke_px: flags.stroke.unwrap_or(defaults.stroke_px),
                })
            } else {
                None
            };
            let out = flags
                .out
                .ok_or_else(|| Failure::Usage("render needs --out <file>".to_string()))?;
            Ok(Command::Render(RenderArgs {
                source,
                out,
                long_edge: flags.long_edge,
                preview_px: flags.preview_px,
                frame: FrameArgs {
                    gap: flags.gap,
                    radius: flags.radius,
                    border: flags.border,
                },
                sketch,
                stats: flags.stats,
                json: flags.json,
            }))
        }
    }
}

/// Which document a command starts from: `--project` and `--template` are the two
/// ways to name one (a template with no photos), and both `render` and `hit` take
/// either.
fn source_of(name: &str, flags: &Flags) -> Result<Source, Failure> {
    match (&flags.project, &flags.template) {
        (Some(_), Some(_)) => Err(Failure::Usage(
            "--project and --template are mutually exclusive".to_string(),
        )),
        (Some(path), None) => Ok(Source::Project(path.clone())),
        (None, Some(template)) => Ok(Source::Template(template.clone())),
        (None, None) => Err(Failure::Usage(format!(
            "{name} needs --project <file> or --template <name>"
        ))),
    }
}

/// A `.pixlay` is a document, and a mistyped extension is more likely a typo than
/// an intention — the same rule `init` has always had.
fn require_project_extension(out: &Path) -> Result<(), Failure> {
    if out.extension().and_then(|value| value.to_str()) != Some(PROJECT_EXTENSION) {
        return Err(Failure::Usage(format!(
            "--out {}: expected a .{PROJECT_EXTENSION} file",
            out.display()
        )));
    }
    Ok(())
}

/// Parses a point in normalized canvas coordinates: `x,y`, both inside `0..=1`.
///
/// The range is part of the surface rather than a courtesy: the canvas *is*
/// `[0,1]`, so a point outside it is not a hit test with an unusual answer, it is a
/// caller that mis-scaled something. Refusing it here is what keeps `hit`'s
/// answers meaningful.
fn parse_point(value: &OsString) -> Result<Point, Failure> {
    let text = value
        .to_str()
        .ok_or_else(|| Failure::Usage("--at must be valid UTF-8".to_string()))?;
    let parsed = text
        .split_once(',')
        .ok_or_else(|| Failure::Usage(format!("--at must be x,y, got {text}")))?;
    let component = |what: &str, raw: &str| -> Result<f64, Failure> {
        let value = raw
            .trim()
            .parse::<f64>()
            .map_err(|_| Failure::Usage(format!("--at {what} must be a number, got {raw}")))?;
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(Failure::Usage(format!(
                "--at {what} {value} is outside 0..=1: canvas coordinates are normalized"
            )));
        }
        Ok(value)
    };
    let x = component("x", parsed.0)?;
    let y = component("y", parsed.1)?;
    Ok(Point::new(x, y))
}

/// Parses a photo offset in cell widths and heights: `x,y`, two comma-separated
/// numbers.
///
/// The comma is the surface's own convention for a pair (`--at`), so a caller that
/// has read one flag has read both. The range is [`CropTransform`]'s own: past half
/// a cell the photo's centre leaves the cell and no clamp can cover it again, so the
/// components are bounded by 1 here and by `validate` on the way into a document.
///
/// [`CropTransform`]: pixlay_core::CropTransform
fn parse_offset(value: &OsString) -> Result<(f64, f64), Failure> {
    let text = value
        .to_str()
        .ok_or_else(|| Failure::Usage("--offset must be valid UTF-8".to_string()))?;
    let parsed = text
        .split_once(',')
        .ok_or_else(|| Failure::Usage(format!("--offset must be x,y, got {text}")))?;
    let component = |what: &str, raw: &str| -> Result<f64, Failure> {
        let value = raw
            .trim()
            .parse::<f64>()
            .map_err(|_| Failure::Usage(format!("--offset {what} must be a number, got {raw}")))?;
        if !value.is_finite() || value.abs() > 1.0 {
            return Err(Failure::Usage(format!(
                "--offset {what} {value} is outside -1..=1: the offset is in cell widths and heights"
            )));
        }
        Ok(value)
    };
    Ok((component("x", parsed.0)?, component("y", parsed.1)?))
}

/// Parses `--region <x>,<y>,<w>,<h>`: one rectangle of a photo, in its own pixels.
///
/// The comma convention is the surface's own for a pair (`--at`), extended to the four
/// numbers a rectangle needs. The origin is not range-checked here — whether a rectangle
/// is inside the photo is a fact about the *file*, which is only known once the file is
/// decoded — so that half is the command's own `RegionOutside` (exit 2, with the photo's
/// size in the message). A rectangle with no area is a usage error (exit 1), the same
/// kind of mistake as `--px 0`.
fn parse_region(value: &OsString) -> Result<Rect, Failure> {
    let text = value
        .to_str()
        .ok_or_else(|| Failure::Usage("--region must be valid UTF-8".to_string()))?;
    let parts: Vec<&str> = text.split(',').collect();
    let [x, y, width, height] = parts.as_slice() else {
        return Err(Failure::Usage(format!(
            "--region must be x,y,w,h, got {text}"
        )));
    };
    let component = |what: &str, raw: &str| -> Result<u32, Failure> {
        raw.trim().parse::<u32>().map_err(|_| {
            Failure::Usage(format!(
                "--region {what} must be a whole number of pixels, got {raw}"
            ))
        })
    };
    let region = Rect {
        x: component("x", x)?,
        y: component("y", y)?,
        width: component("w", width)?,
        height: component("h", height)?,
    };
    if region.width == 0 || region.height == 0 {
        return Err(Failure::Usage(format!(
            "--region {text} has no area: w and h are at least 1"
        )));
    }
    Ok(region)
}

/// Parses `--swap <i>,<j>`: two cell indexes, the same comma convention as
/// `--offset` and `--at`.
///
/// The indexes are *not* range-checked here. Whether cell 7 exists is a fact about
/// the project, not about the command line, so the command's own `NoSuchSlot` answers
/// it — reported as a failure (exit 2), with the layout's count in the message.
/// `i == j` is likewise the command's `SameSlot` (exit 2) rather than a usage error.
fn parse_swap(value: &OsString) -> Result<(usize, usize), Failure> {
    let text = value
        .to_str()
        .ok_or_else(|| Failure::Usage("--swap must be valid UTF-8".to_string()))?;
    let (left, right) = text
        .split_once(',')
        .ok_or_else(|| Failure::Usage(format!("--swap must be i,j, got {text}")))?;
    let index = |side: &str, raw: &str| -> Result<usize, Failure> {
        raw.trim()
            .parse::<usize>()
            .map_err(|_| Failure::Usage(format!("--swap {side} must be a cell index, got {raw}")))
    };
    Ok((index("i", left)?, index("j", right)?))
}

/// Parses `w`x`h`, each 1..=[`MAX_PREVIEW_PX`], as a widget's size.
///
/// The `x` is the one GTK itself prints a size with (`GdkRectangle`), so a size
/// copied out of a window's own report reads back unchanged.
fn parse_size(value: &OsString, flag: &str) -> Result<(i32, i32), Failure> {
    let text = value
        .to_str()
        .ok_or_else(|| Failure::Usage(format!("--{flag} must be valid UTF-8")))?;
    let (width, height) = text
        .split_once('x')
        .ok_or_else(|| Failure::Usage(format!("--{flag} must be <w>x<h>, got {text}")))?;
    let side = |which: &str, raw: &str| -> Result<i32, Failure> {
        let pixels = raw
            .trim()
            .parse::<i32>()
            .map_err(|_| Failure::Usage(format!("--{flag} {which} must be a number, got {raw}")))?;
        if !(1..=MAX_PREVIEW_PX).contains(&pixels) {
            return Err(Failure::Usage(format!(
                "--{flag} {which} {pixels} is outside 1..={MAX_PREVIEW_PX}"
            )));
        }
        Ok(pixels)
    };
    Ok((side("width", width)?, side("height", height)?))
}

/// Parses `r,g,b`, each 0..=255, as an opaque [`Rgba8`].
///
/// Opaque because the backdrop is painted rather than blended (see the frame's
/// documentation): a translucent canvas would make a preview depend on what is
/// behind the widget, which is exactly what the export cannot know.
fn parse_color(value: &OsString) -> Result<Rgba8, Failure> {
    let text = value
        .to_str()
        .ok_or_else(|| Failure::Usage("--border-color must be valid UTF-8".to_string()))?;
    let parts: Vec<&str> = text.split(',').collect();
    let [r, g, b] = parts.as_slice() else {
        return Err(Failure::Usage(format!(
            "--border-color must be r,g,b, got {text}"
        )));
    };
    let channel = |what: &str, raw: &str| -> Result<u8, Failure> {
        let value: u32 = raw.trim().parse().map_err(|_| {
            Failure::Usage(format!("--border-color {what} must be 0..=255, got {raw}"))
        })?;
        u8::try_from(value).map_err(|_| {
            Failure::Usage(format!("--border-color {what} {value} is outside 0..=255"))
        })
    };
    Ok(Rgba8::rgb(
        channel("red", r)?,
        channel("green", g)?,
        channel("blue", b)?,
    ))
}

/// A finite floating-point flag value.
///
/// `NaN` and the infinities are refused here rather than carried into the document:
/// the message names the flag, and there is nothing a caller can do with a
/// non-finite framing that it could not do with a finite one.
fn float(value: &OsString, name: &str) -> Result<f64, Failure> {
    let text = value
        .to_str()
        .ok_or_else(|| Failure::Usage(format!("--{name} must be a number")))?;
    let parsed = text
        .parse::<f64>()
        .map_err(|_| Failure::Usage(format!("--{name} must be a number, got {text}")))?;
    if !parsed.is_finite() {
        return Err(Failure::Usage(format!(
            "--{name} must be a finite number, got {text}"
        )));
    }
    Ok(parsed)
}

/// Parses an aspect ratio: `W:H` with positive numbers, or a bare decimal.
/// `W:H` is the form a caller reads off a canvas, and the form the report prints
/// back (`crate::cli::ratio_label`), so the round trip needs no conversion.
fn parse_aspect(value: &OsString) -> Result<f64, Failure> {
    let text = value
        .to_str()
        .ok_or_else(|| Failure::Usage("--aspect must be valid UTF-8".to_string()))?;
    let aspect = match text.split_once(':') {
        Some((width, height)) => {
            let width = ratio_part(width, "width")?;
            let height = ratio_part(height, "height")?;
            width / height
        }
        None => text.parse::<f64>().map_err(|_| {
            Failure::Usage(format!("--aspect must be W:H or a decimal, got {text}"))
        })?,
    };
    if !aspect.is_finite() || !(0.1..=10.0).contains(&aspect) {
        return Err(Failure::Usage(format!(
            "--aspect {text} is outside 0.1..=10.0"
        )));
    }
    Ok(aspect)
}

fn ratio_part(text: &str, what: &str) -> Result<f64, Failure> {
    let value = text
        .trim()
        .parse::<f64>()
        .map_err(|_| Failure::Usage(format!("--aspect {what} must be a number, got {text}")))?;
    if !value.is_finite() || value <= 0.0 {
        return Err(Failure::Usage(format!(
            "--aspect {what} must be positive, got {text}"
        )));
    }
    Ok(value)
}

/// The one file `--photo` names, for the commands that read a single photo.
///
/// `--photo` is repeatable because `init` takes one per cell; a command that
/// reads one file says so instead of quietly using the first argument, and a
/// second one is a usage error rather than a silent drop.
fn one_photo(name: &str, flags: &Flags) -> Result<PathBuf, Failure> {
    match flags.photos.as_slice() {
        [photo] => Ok(photo.clone()),
        [] => Err(Failure::Usage(format!("{name} needs --photo <file>"))),
        photos => Err(Failure::Usage(format!(
            "{name} reads one photo; --photo was given {} times",
            photos.len()
        ))),
    }
}

fn set_once<T>(slot: &mut Option<T>, value: T, name: &str) -> Result<(), Failure> {
    if slot.is_some() {
        return Err(Failure::Usage(format!("--{name} given more than once")));
    }
    *slot = Some(value);
    Ok(())
}

fn number(value: &OsString, name: &str) -> Result<i64, Failure> {
    let text = value
        .to_str()
        .ok_or_else(|| Failure::Usage(format!("--{name} must be a number")))?;
    text.parse::<i64>()
        .map_err(|_| Failure::Usage(format!("--{name} must be a number, got {text}")))
}
