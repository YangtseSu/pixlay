//! The shared harness of the GUI tests.
//!
//! **One test per binary, on purpose.** GTK is single-threaded, and libtest runs
//! the tests of one binary in parallel threads, so a binary that needs the toolkit
//! has exactly one `#[test]` — several of them would initialise GTK from several
//! threads at once and the first warning would be a crash nobody could read. Each
//! of these files therefore has one test, and it collects the checks it can.
//!
//! **A display, one way or another.** On a desktop the tests run in the session's
//! own display; where there is none (a build box, a packaging chroot) [`start`]
//! re-runs the whole test binary under `xvfb-run`, which is how a headless
//! verification of a GUI is possible at all. That path is deliberate: a test that
//! silently passes without a display would be a check that checks nothing.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;

use pixlay::{EditorWindow, i18n};

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

/// Makes sure this process can talk to a display, re-running the whole binary
/// under `xvfb-run` when it cannot.
///
/// Every test that calls this runs on exactly one thread — libtest's worker for
/// the single `#[test]` its binary has — because GTK has to be initialised and
/// used from one thread, and a second test in the same binary would be a second
/// thread.
pub fn start() {
    if gtk::init().is_ok() {
        return;
    }
    if std::env::var_os("PIXLAY_XVFB_CHILD").is_some() {
        panic!("the GUI tests need a display: xvfb-run could not provide one either");
    }
    let exe = std::env::current_exe().expect("the test binary's own path");
    let status = Command::new("xvfb-run")
        .arg("-a")
        .arg(&exe)
        .env("PIXLAY_XVFB_CHILD", "1")
        .env_remove("WAYLAND_DISPLAY")
        // Without a session there is no ibus to talk to, and GTK's `im-ibus`
        // module *recurses into itself* trying to reach one (measured: SIGSEGV on
        // a 64 MiB stack, with `libim-ibus.so` alternating with
        // `g_type_create_instance` in the backtrace). The built-in simple input
        // context is what a headless run wants, and the accessibility bridge has
        // no bus either.
        .env("GTK_IM_MODULE", "gtk-im-context-simple")
        .env("NO_AT_BRIDGE", "1")
        .status();
    match status {
        Ok(status) if status.success() => std::process::exit(0),
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(error) => panic!(
            "no display and xvfb-run is not available ({error}); \
             the GUI tests need one or the other"
        ),
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

/// A second window on the same application: one process can only export one
/// `org.gtk.Application` object, so a second window is the way to open a project
/// twice.
pub fn second_window(app: &adw::Application) -> EditorWindow {
    let window = EditorWindow::new(app);
    window.present();
    window.pump(Duration::from_millis(300));
    window
}

/// The window of a registered application, activated and presented.
pub fn window(app: &adw::Application) -> EditorWindow {
    app.activate();
    let window = pixlay::app::active_window(app).expect("activating built a window");
    window.present();
    // One round of the main context gets the window mapped and drawn.
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
    // waits for the frame that contains it rather than assuming one has happened.
    let deadline = Instant::now() + WAIT;
    let node = loop {
        let paintable = gtk::WidgetPaintable::new(Some(widget));
        let snapshot = gtk::Snapshot::new();
        paintable.snapshot(&snapshot, f64::from(width), f64::from(height));
        match snapshot.to_node() {
            Some(node) => break node,
            None if Instant::now() < deadline => {
                widget.queue_draw();
                pump(Duration::from_millis(20));
            }
            None => panic!("the widget never produced a render node"),
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
            dpi: 96.0,
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
