//! The shared harness of the GUI tests.
//!
//! **One test per binary, on purpose.** GTK is single-threaded, and libtest runs
//! the tests of one binary in parallel threads, so a binary that needs the toolkit
//! has exactly one `#[test]` — several of them would initialise GTK from several
//! threads at once and the first warning would be a crash nobody could read. Each
//! of these files therefore has one test, and it collects the checks it can.
//!
//! **A display of our own, not the session's.** GTK lays out, allocates and caches render
//! nodes only on a frame, so every check below is really a statement about frames. A
//! session's compositor has no reason to keep drawing a test window: it is not activated
//! (focus stealing is refused while the session is in use), it is behind everything else,
//! and then it gets almost no frames at all — measured 2026-09-24 on the session's own
//! Wayland display: **0–1 frames in 2 s** in the foreground and **0 in 5 s** behind
//! another window, where the same window inside a private headless mutter ticked at
//! **121 frames in 2 s** and **303 in 5 s** while `active false`. So [`start`] runs the
//! whole test binary inside a private headless mutter.
//!
//! **A tick is not evidence about the compositor.** The count these waits read is this
//! harness's own ([`watch`], [`frames`]), and GTK's frame clock then runs on its own timer
//! for as long as that callback is installed: measured 2026-09-24, a window ticked at
//! **180 frames in 3 s with `is_mapped() == false`** — hidden, and still ticking. So the
//! counter answers "is the clock running, so will layout and allocation advance", which is
//! what a wait needs; it cannot answer "is this window on screen", and nothing here claims
//! it does. What a display that never drives the clock costs is a wait that times out, and
//! every wait says what it saw ([`frame_state`]) — that, and the window's own mapping
//! ([`window`]), is the whole of what a test here can say about the display.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use pixlay::{EditorWindow, canvas, i18n};

/// An image in memory: width, height, and straight RGB, three bytes per pixel.
pub type Image = (i32, i32, Vec<u8>);

/// How long a check waits for the background pipeline before giving up.
///
/// It has to cover two different waits, and both are "something is coming, not
/// something is broken": the decoding thread's reply, and — in [`snapshot`] — the
/// frame a widget needs before GSK can hand its pixels over. Measured 2026-09-22:
/// with a release build of the A0 render tests running at the same time, **60 s is
/// not enough for the second of those** — `mainpath.rs` timed out in `snapshot`
/// twice while such a build ran alongside it, and the same test finishes in 10-12 s
/// on an idle machine. Three minutes is the fix for that; a timeout that fires must
/// still mean "hung", and on this machine it does.
pub const WAIT: Duration = Duration::from_secs(180);

/// The private compositor's child guard: set on the re-executed binary, so the re-execution
/// happens exactly once — and set by hand to run the GUI tests on the display this process
/// already has (a session, or any compositor of your own) instead of a private one.
const CHILD: &str = "PIXLAY_TEST_CHILD";

/// Makes sure this process talks to a display that keeps drawing, re-running the whole
/// test binary inside a private headless mutter when it does not.
///
/// **mutter, because this is a GNOME application** (`AGENTS.md`, the module boundary:
/// GTK4 + libadwaita), so mutter is the platform's own compositor and the one whose frame
/// behaviour these tests are about. It is the *environment* a GUI test needs and nothing
/// else: the product's manifests name no compositor, and neither does this harness — it
/// starts one program, and `CHILD=1` is how a run says "use the display I have instead"
/// (the failure below spells that out, with the invocations of the alternatives).
///
/// **The parent's own arguments are forwarded** to the re-executed child, so `--nocapture`,
/// a test filter or `--test-threads` mean for a GUI binary what they mean everywhere else:
/// without them the child runs libtest with its defaults and a debugging run shows nothing.
///
/// Every test that calls this runs on exactly one thread — libtest's worker for the single
/// `#[test]` its binary has — because GTK has to be initialised and used from one thread,
/// and a second test in the same binary would be a second thread.
pub fn start() {
    if std::env::var_os(CHILD).is_none() {
        // Not the re-executed child yet: become one, inside a display of our own.
        let exe = std::env::current_exe().expect("the test binary's own path");
        let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
        let status = Command::new("mutter")
            // A headless compositor with no monitor never draws anything, so a window in
            // it never gets a surface (measured: the test binary hung for its whole 120 s
            // timeout with `--headless` alone). The virtual monitor is the display those
            // frames are drawn on, sized above the app's own default window (1100x760) so
            // the window is never the thing being constrained.
            .args([
                "--wayland",
                "--headless",
                "--no-x11",
                "--virtual-monitor",
                "1920x1200@60.0",
                "--",
            ])
            .arg(&exe)
            .args(&args)
            .env(CHILD, "1")
            // Without a session there is no ibus to talk to, and GTK's `im-ibus` module
            // *recurses into itself* trying to reach one (measured: SIGSEGV on a 64 MiB
            // stack, with `libim-ibus.so` alternating with `g_type_create_instance` in the
            // backtrace). The built-in simple input context is what a headless run wants,
            // and the accessibility bridge has no bus either.
            .env("GTK_IM_MODULE", "gtk-im-context-simple")
            .env("NO_AT_BRIDGE", "1")
            .status();
        match status {
            Ok(status) if status.success() => std::process::exit(0),
            Ok(status) => std::process::exit(status.code().unwrap_or(1)),
            Err(error) => panic!(
                "the GUI tests run inside a private headless mutter, and mutter could not \
                 be started ({error}). Run them on a display of your own instead — any \
                 headless compositor will do, e.g. `PIXLAY_TEST_CHILD=1 xvfb-run -a cargo \
                 test` — with {CHILD}=1 set, so the harness uses that display rather than \
                 starting its own"
            ),
        }
    }
    // First run or re-executed child, GTK is initialised exactly once — and inside the
    // private compositor rather than against whatever display this process found. That
    // call is also what sets the locale the strings are looked up in (`i18n`'s module:
    // `g_gettext` answers in English before it and in the catalog's language after it), so
    // it is not skipped for a child that already has its display.
    if gtk::init().is_err() {
        panic!("the GUI tests need a display, and the private mutter did not provide one");
    }
}

/// The application, built the way `main` builds it but without an event loop of
/// its own.
pub fn app() -> adw::Application {
    i18n::init();
    let app = pixlay::app::build();
    // Several test processes exist at once (the language checks spawn children),
    // and a D-Bus `GApplication` name can only be owned by one of them: without
    // this, a second process waits out `register()` trying to reach the first,
    // which is not running a main loop. `NON_UNIQUE` keeps every test process
    // local, and the window it builds is the production one either way.
    app.set_flags(app.flags() | gtk4::gio::ApplicationFlags::NON_UNIQUE);
    app.register(None::<&gtk4::gio::Cancellable>)
        .expect("the application registers");
    app
}

thread_local! {
    /// Frames each watched window's own frame clock has ticked, by window.
    ///
    /// A thread-local, not a global: GTK lives on one thread (the module's first
    /// paragraph) and so do the tests. Keyed by window because a frame clock belongs to a
    /// surface — `mainpath.rs` holds two windows at once, and one count shared between
    /// them would let a wait on one window be satisfied by the other's frames.
    static FRAMES: std::cell::RefCell<std::collections::HashMap<usize, u64>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Counts frames for the waits: installs the tick callback that keeps `window`'s frame
/// clock running, and counts its ticks.
///
/// **The harness's own, never the product's.** A callback that returns
/// `ControlFlow::Continue` keeps GTK's frame clock running for as long as it is installed —
/// which is exactly what these waits want (layout and allocation advance with nothing
/// having to invalidate, so a wait for "the frame that carries the change" cannot deadlock
/// on a change that was already drawn) — and it is a cost the shipped app must not pay for
/// a test's sake. Measured 2026-09-24, an idle window in a private headless mutter:
/// **1218 context switches in 20 s** with one installed, **2** without. So `EditorWindow`
/// installs nothing, and the tests install this.
pub fn watch(window: &EditorWindow) {
    let key = window.as_ptr() as usize;
    FRAMES.with(|frames| frames.borrow_mut().insert(key, 0));
    window.add_tick_callback(move |_, _| {
        FRAMES.with(|frames| {
            if let Some(count) = frames.borrow_mut().get_mut(&key) {
                *count += 1;
            }
        });
        glib::ControlFlow::Continue
    });
}

/// Frames `window`'s own frame clock has ticked since [`watch`] installed the counter.
///
/// Zero for a window the harness never watched: the count is the harness's, so a window it
/// did not build has none. What a count of zero does and does not mean is the module doc's
/// second paragraph — it is a fact about the frame clock, not about the compositor.
pub fn frames(window: &EditorWindow) -> u64 {
    let key = window.as_ptr() as usize;
    FRAMES.with(|frames| frames.borrow().get(&key).copied().unwrap_or(0))
}

/// A second window on the same application: one process can only export one
/// `org.gtk.Application` object, so a second window is the way to open a project
/// twice.
pub fn second_window(app: &adw::Application) -> EditorWindow {
    let window = EditorWindow::new(app);
    watch(&window);
    window.present();
    window.pump(Duration::from_millis(300));
    window
}

/// How long [`window`] gives a presented window to be mapped.
///
/// Two seconds is several frames at any refresh rate, so "not mapped" is not a slow
/// machine: it is a compositor that did not take the window.
const FRAME_PROBE: Duration = Duration::from_secs(2);

/// The window of a registered application, activated and presented.
pub fn window(app: &adw::Application) -> EditorWindow {
    app.activate();
    let window = pixlay::app::active_window(app).expect("activating built a window");
    watch(&window);
    window.present();
    // **What a test here can say about the display, and the whole of it**: the compositor
    // mapped the window. It is a condition with a budget and then an assert, because every
    // check below reads an allocation, a placement or a snapshot, and a window that was
    // never mapped has none of those. The frame count cannot make this claim — the clock
    // ticks on its own timer while `watch`'s callback is installed, hidden or not (the
    // module doc) — so a check on it would be a check that can never fail.
    let deadline = Instant::now() + FRAME_PROBE;
    while !window.is_mapped() && Instant::now() < deadline {
        window.pump(Duration::from_millis(50));
    }
    assert!(
        window.is_mapped(),
        "the window was presented and the compositor did not map it in {FRAME_PROBE:?}, so \
         every check would read a window that was never laid out: {}",
        frame_state(&window)
    );
    // And one more round for the frame that draws it: a mapped window is allocated on the
    // frame *after* the one that mapped it.
    window.pump(Duration::from_millis(400));
    window
}

/// The CLI's fixtures, which is where the photos the GUI opens live.
pub fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../pixlay-cli/tests/fixtures")
        .canonicalize()
        .expect("the fixtures are in the repository")
}

pub fn verify_project() -> PathBuf {
    fixtures().join("verify.pixlay")
}

pub fn photo(name: &str) -> PathBuf {
    fixtures().join("photos").join(name)
}

/// Where this run's artifacts go: a disk path, never `/tmp`, which is tmpfs here
/// (`AGENTS.md`, "Measurement rules").
pub fn out_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/var/tmp"));
    let dir = base.join("pixlay-s7");
    std::fs::create_dir_all(&dir).expect("the artifact directory can be created");
    dir
}

pub fn artifact(name: &str) -> PathBuf {
    let path = out_dir().join(name);
    if path.exists() {
        std::fs::remove_file(&path).expect("a previous artifact can be removed");
    }
    path
}

/// Pumps the main context for a while, so a presented widget gets drawn.
pub fn pump(duration: Duration) {
    let context = glib::MainContext::default();
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        while context.pending() {
            context.iteration(false);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Waits for `widget` to be allocated, pumping frames.
///
/// A widget that has just been shown, or added to the tree by a reply from a worker
/// thread, is allocated on the display's *next* frame — and the harness's other waits
/// stop as soon as the reply they were about has arrived, which can be one frame before
/// the widgets it created are laid out. Measured 2026-09-23: the HIG walk read candidate
/// cells of 0x0 on two runs of four.
pub fn allocated(widget: &gtk::Widget, window: &EditorWindow) -> bool {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if widget.width() > 0 && widget.height() > 0 {
            return true;
        }
        window.pump(Duration::from_millis(20));
    }
    false
}

/// The canvas's own size, once it has one **and nothing is in flight**.
///
/// Everything the per-cell controls do is computed from the canvas's allocation *and*
/// from the grid its bitmaps were decoded for (`placement` is expressed in that grid),
/// and a window whose editor page has just been pushed has neither: a `sync` against a
/// 0x0 canvas — or against a stale 1x1 grid, whose placement stretches the sheet to the
/// canvas's own height — leaves the buttons with margins GTK answers with a 0x0
/// allocation, and no later frame fixes them. Measured 2026-09-23: `tests/layout.rs`
/// waited its full 180 s for a `+` that could not appear. So a test that measures the
/// controls asks for both, and reads the size after the waits: this one for the
/// allocation, [`canvas_bitmaps`] for the grid — "nothing is in flight" is also true of a
/// window that has not asked for a decode at all, so it is not the same claim.
pub fn canvas_size(window: &EditorWindow) -> (i32, i32) {
    let area = window.canvas_widget();
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if area.width() > 0 && area.height() > 0 {
            break;
        }
        window.pump(Duration::from_millis(20));
    }
    assert!(
        area.width() > 0 && area.height() > 0,
        "the canvas was never allocated: {}x{}, {}",
        area.width(),
        area.height(),
        frame_state(window),
    );
    let (width, height) = (area.width(), area.height());
    assert!(
        window.wait_for_idle(WAIT),
        "the canvas's own decode never finished: canvas {width}x{height}, images grid \
         {:?}, requested {:?}, band pending {}, {} source decodes so far, {}",
        window.images().0,
        window.requested_grid(),
        window.gallery_decodes(),
        window.decoded_sources(),
        frame_state(window),
    );
    (width, height)
}

/// Waits until the canvas's own bitmaps are in hand **for the grid it rests at**.
///
/// [`canvas_size`] answers "the canvas has a size and nothing is in flight", which is
/// not the same claim: the bitmaps in hand can still be the window's `1x1` placeholder
/// — the state `open_document` starts from — and a control placed against a placeholder
/// sheet is one GTK answers with a `0x0` allocation (`CellControls::sync_in`). A test
/// that measures the canvas's controls asks for this, and the wait is the *condition*
/// rather than a longer settling loop (measured 2026-09-24: a full-suite run failed
/// here with a strip that had stayed `0x0` for the whole 180 s wait).
pub fn canvas_bitmaps(window: &EditorWindow) -> (i32, i32) {
    let area = window.canvas_widget();
    let resting = || {
        let doc = window.document();
        canvas::preferred_grid(doc.template.aspect, area.width(), area.height())
    };
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if area.width() > 0
            && area.height() > 0
            && window.images().0 == resting()
            && window.requested_grid().is_none()
        {
            break;
        }
        window.pump(Duration::from_millis(20));
    }
    let (width, height) = (area.width(), area.height());
    assert!(
        width > 0 && height > 0 && window.images().0 == resting(),
        "the canvas never got its own bitmaps: canvas {width}x{height}, bitmaps for \
         {:?}, resting {:?}, requested {:?}, {}",
        window.images().0,
        resting(),
        window.requested_grid(),
        frame_state(window),
    );
    assert!(
        window.wait_for_idle(WAIT),
        "the background pipeline never finished: {}",
        frame_state(window),
    );
    (width, height)
}

/// Pumps until the window's own frame clock has ticked at least `count` more times.
///
/// A pixel probe reads what is on screen, and what is on screen is what the last frame
/// drew: GTK hands a snapshot the widgets' **cached** render nodes, so a probe taken
/// before the frame that carries the change sees the previous one. Measured 2026-09-24:
/// the picker's pick-then-clear probe compared two identical images (RMSE 0.000) because
/// neither had been drawn after its change, and a fixed 200 ms of pumping is a guess,
/// not a condition. Returns the frames that arrived, so a caller that cares can say so.
///
/// The clock is the harness's own doing ([`watch`]), so this waits on GTK's layout rather
/// than on the compositor (the module doc) — and it cannot deadlock on a change that was
/// already drawn, which is why the callback never stops.
pub fn after_frames(window: &EditorWindow, count: u64, timeout: Duration) -> u64 {
    let start = frames(window);
    let deadline = Instant::now() + timeout;
    while frames(window) < start + count {
        if Instant::now() >= deadline {
            return frames(window) - start;
        }
        window.pump(Duration::from_millis(20));
    }
    frames(window) - start
}

/// Pumps until `observe` reports it has seen what it is waiting for, and hands back
/// what it last saw.
///
/// A pixel probe reads the widgets' **cached** render nodes, and the frame that carries a
/// change is not the probe's to schedule: measured 2026-09-24, a probe read the junction's
/// pre-frame pixels because two window frames had passed without the canvas being redrawn.
/// Waiting for the *observation* is what makes such a probe a condition rather than a
/// guess — and the caller still asserts on what came back, so an observation that never
/// arrives fails exactly as it did before.
pub fn settle_by<T>(
    window: &EditorWindow,
    timeout: Duration,
    mut observe: impl FnMut() -> (T, bool),
) -> T {
    let deadline = Instant::now() + timeout;
    loop {
        let (value, settled) = observe();
        if settled || Instant::now() >= deadline {
            return value;
        }
        window.pump(Duration::from_millis(50));
    }
}

/// How long a pixel probe waits for the observation it wants.
///
/// Generous — a round re-snapshots the whole canvas — and far below the harness's own
/// 180 s ceiling: a probe that has not seen its observation in a minute is not going to,
/// and the assert that follows says so.
pub const PROBE_WAIT: Duration = Duration::from_secs(60);

/// Dismisses a dialog the harness has finished with, and pumps a frame.
///
/// The dialogs' own buttons are what a test clicks, and those handlers call
/// [`adw::Dialog::close`] — which starts libadwaita's close transition **and never
/// finishes it on a headless X server** (measured 2026-09-23: the dialog was still
/// `is_visible()` 180 s after an accepted `close()` under `xvfb-run`). Two facts follow,
/// and both are why this helper exists rather than an assertion of "it is gone":
///
/// * a *presented* dialog (measured: `close()` on one that had already accepted a close
///   is refused — "Trying to close AdwDialog … that's not presented") owns the frame it
///   is over, and **a widget behind it snapshots to nothing** — so a pixel probe of the
///   canvas needs the dialog off the tree, and that is what [`adw::Dialog::force_close`]
///   is for;
/// * whether the *widget* is still mapped afterwards is libadwaita's transition and not
///   this product's contract, so no test asserts it.
pub fn close_dialog(dialog: &adw::Dialog, window: &EditorWindow) {
    dialog.force_close();
    // The wait is a **condition**, not a delay: a dialog that is still presented owns
    // the frame it is over, and a widget behind it snapshots to nothing, so a pixel
    // probe taken too early reads an empty surface rather than a wrong one. Bounded,
    // because libadwaita's own close transition is not this product's contract and a
    // headless X server has been measured to leave a closed dialog visible for as long
    // as anyone waited (2026-09-23) — the loop still pumps frames, which is what lets the
    // transition finish where it can.
    let deadline = Instant::now() + DIALOG_WAIT;
    while dialog.is_visible() && Instant::now() < deadline {
        window.pump(Duration::from_millis(20));
    }
    window.pump(Duration::from_millis(50));
}

/// How long a dialog dismissal is waited for before the test carries on regardless.
const DIALOG_WAIT: Duration = Duration::from_secs(5);

/// Whether this window's frame clock was ticking while a wait ran, in one line.
///
/// The waits in this harness fail on a timeout, and a timeout that cannot say *why* is
/// the flake they exist to end: the frame count separates "the clock never ran, so nothing
/// was laid out" from "frames arrived and the widget still painted nothing", and the
/// canvas's own last `render` refusal is the second of those (measured 2026-09-24: a
/// full-suite run failed on a canvas that had produced no render node for its whole
/// 180 s wait, and the message named neither).
pub fn frame_state(window: &EditorWindow) -> String {
    format!(
        "{} frames, active {}, mapped {}, last canvas draw refusal: {:?}",
        frames(window),
        window.is_active(),
        window.is_mapped(),
        window.last_draw_error()
    )
}

/// The window a widget belongs to, when it is one of the editor's.
fn owner_window(widget: &impl IsA<gtk::Widget>) -> Option<EditorWindow> {
    widget
        .root()
        .and_then(|root| root.downcast::<EditorWindow>().ok())
}

/// The pixels a widget draws, as straight RGB, through a real render node.
///
/// This is the window's own drawing path — the widget's snapshot, rendered by GSK
/// — and not a second call to the drawing function, so what the test compares is
/// what the window shows.
pub fn snapshot(widget: &impl IsA<gtk::Widget>) -> Image {
    let (width, height) = (widget.width(), widget.height());
    assert!(
        width > 0 && height > 0,
        "the widget was not allocated ({}x{})",
        width,
        height
    );
    // A widget that has not been drawn yet snapshots to an empty node, so this
    // waits for the frame that contains it rather than assuming one has happened. The
    // nudge after the first second is what turns "nothing was invalidated, so no frame
    // was going to come" into a frame: the test has just changed something, and a
    // `queue_resize` is the same request the change itself would have made.
    let owner = owner_window(widget);
    let frames_at_start = owner.as_ref().map(frames).unwrap_or(0);
    let start = Instant::now();
    let deadline = start + WAIT;
    let node = loop {
        let paintable = gtk::WidgetPaintable::new(Some(widget));
        let snapshot = gtk::Snapshot::new();
        paintable.snapshot(&snapshot, f64::from(width), f64::from(height));
        match snapshot.to_node() {
            Some(node) => break node,
            None if Instant::now() < deadline => {
                widget.queue_draw();
                if start.elapsed() > Duration::from_secs(1) {
                    widget.queue_resize();
                    if let Some(window) = owner.as_ref() {
                        window.queue_draw();
                    }
                }
                pump(Duration::from_millis(20));
            }
            None => {
                let arrived = owner.as_ref().map(frames).unwrap_or(0) - frames_at_start;
                panic!(
                    "the widget never produced a render node: a {} of {}x{}, visible {}, \
                     mapped {}, {} child(ren); waited {:.1}s, {arrived} frames arrived{}",
                    widget.type_().name(),
                    widget.width(),
                    widget.height(),
                    widget.is_visible(),
                    widget.is_mapped(),
                    widget.first_child().is_some() as u8,
                    start.elapsed().as_secs_f64(),
                    owner
                        .as_ref()
                        .map(|window| format!(", {}", frame_state(window)))
                        .unwrap_or_default(),
                )
            }
        }
    };
    let surface = widget
        .native()
        .and_then(|native| native.surface())
        .expect("the widget is realized and has a surface");
    let renderer = gtk::gsk::Renderer::for_surface(&surface).expect("a renderer for the surface");
    let rect = gtk::graphene::Rect::new(0.0, 0.0, width as f32, height as f32);
    let texture = renderer.render_texture(&node, Some(&rect));
    let (texture_width, texture_height) = (texture.width(), texture.height());
    let mut data = vec![0u8; (texture_width * texture_height * 4) as usize];
    texture.download(&mut data, (texture_width * 4) as usize);
    renderer.unrealize();
    // `download` gives the texture's own layout, BGRA premultiplied; the sheet is
    // opaque, so dropping alpha and swapping is exact.
    let mut rgb = Vec::with_capacity((texture_width * texture_height * 3) as usize);
    for pixel in data.as_chunks::<4>().0 {
        rgb.push(pixel[2]);
        rgb.push(pixel[1]);
        rgb.push(pixel[0]);
    }
    (texture_width, texture_height, rgb)
}

/// One pixel of a `(width, height, rgb)` image.
pub fn pixel(image: &Image, x: i32, y: i32) -> [u8; 3] {
    let (width, _, data) = image;
    let index = (y as usize * *width as usize + x as usize) * 3;
    [data[index], data[index + 1], data[index + 2]]
}

/// The root-mean-square difference per channel between two images of the same
/// size, the metric S1–S6 use for "the same picture".
pub fn rmse(a: &Image, b: &Image) -> f64 {
    assert_eq!((a.0, a.1), (b.0, b.1), "the two images must have one size");
    let count = a.2.len().max(1) as f64;
    let sum: f64 =
        a.2.iter()
            .zip(b.2.iter())
            .map(|(left, right)| {
                let difference = f64::from(*left) - f64::from(*right);
                difference * difference
            })
            .sum();
    (sum / count).sqrt()
}

/// The action a widget activates, for any widget that implements `GtkActionable`
/// (`GtkWidget` itself does not, which is why this goes through the property).
pub fn action_name(widget: &gtk::Widget) -> Option<String> {
    widget.find_property("action-name")?;
    widget
        .property::<Option<glib::GString>>("action-name")
        .map(|name| name.to_string())
}

/// Writes an RGB image as a PNG, through the product's own encoder, so that a
/// human can look at what the tests looked at.
pub fn save_png(path: &Path, image: &Image) {
    let (width, height, data) = image;
    pixlay_imaging::encode::write(
        path,
        &pixlay_imaging::encode::Export {
            format: pixlay_imaging::encode::Format::Png,
            image: pixlay_imaging::Rgb8View {
                width: *width,
                height: *height,
                data,
            },
        },
    )
    .expect("the artifact can be written");
}

/// Every widget below `root`, root included.
pub fn descendants(root: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut found = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(widget) = stack.pop() {
        found.push(widget.clone());
        let mut child = widget.first_child();
        while let Some(child_widget) = child {
            stack.push(child_widget.clone());
            child = child_widget.next_sibling();
        }
    }
    found
}
