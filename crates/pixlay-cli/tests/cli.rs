//! The CLI contract, as tests: the machine surface is what every later step's
//! verification loop depends on, so its promises are pinned here.
//!
//! These run the built binary as a subprocess, because that is the only way to
//! observe the things the contract is about: stdout purity, exit codes, the
//! absence of a TTY, and locale independence.

use std::ffi::OsString;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use pixlay_core::{
    Cell, CollageDoc, CropTransform, PixelSize, Point, Polygon, Project, Selection, Slot, Template,
    templates,
};

/// `CARGO_BIN_EXE_<name>` is set by Cargo for integration tests.
const BIN: &str = env!("CARGO_BIN_EXE_pixlay-render");

fn out_dir(name: &str) -> PathBuf {
    // Artifacts go to disk, never to tmpfs (`AGENTS.md`, measurement rules).
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/var/tmp"));
    let dir = base.join(format!("pixlay-cli-tests/{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create test directory");
    dir
}

/// Runs the binary with a closed stdin, no TTY, and a fixed environment. The
/// environment is fixed so that a stray `LANG`-dependent message would show up
/// as a difference between this run and the locale cases.
fn run(args: &[&str]) -> Output {
    run_in(args, None, None)
}

fn run_in(args: &[&str], cwd: Option<&Path>, locale: Option<(&str, &str)>) -> Output {
    let mut command = Command::new(BIN);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("TZ", "UTC")
        .env("LANG", locale.map(|(lang, _)| lang).unwrap_or("C"))
        .env("LC_ALL", locale.map(|(_, all)| all).unwrap_or("C"))
        // A font-free, cache-free environment: nothing here may need either.
        .env("HOME", "/nonexistent");
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    command.output().expect("run pixlay-render")
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout is UTF-8")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr is UTF-8")
}

fn code(output: &Output) -> i32 {
    output.status.code().expect("exit code")
}

/// One `key = value` field from the default output shape.
fn field(output: &Output, key: &str) -> String {
    stdout(output)
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(" = ")?;
            (name == key).then(|| value.to_string())
        })
        .unwrap_or_else(|| panic!("field {key} missing from:\n{}", stdout(output)))
}

/// A two-slot project whose cells point at real files, so `Project::sources`
/// resolves without a decoder having to exist.
fn write_project(dir: &Path, name: &str, fill: bool) -> PathBuf {
    let left = Polygon::rect(0.0, 0.0, 0.5, 1.0);
    let right = Polygon::rect(0.5, 0.0, 1.0, 1.0);
    let template = Template {
        name: "test-2".to_string(),
        version: 1,
        aspect: 4.0 / 3.0,
        slots: vec![
            Slot {
                area: left.area(),
                outline: left,
            },
            Slot {
                area: right.area(),
                outline: right,
            },
        ],
    };
    let mut doc = CollageDoc::new(template);
    if fill {
        let photo = dir.join("photo.png");
        std::fs::write(&photo, include_bytes!("fixtures/photos/square.png")).expect("write photo");
        doc.cells[0] = Cell {
            source: Some(PathBuf::from("photo.png")),
            crop: Default::default(),
        };
    }
    let path = dir.join(name);
    std::fs::write(&path, doc.to_json().expect("serializes")).expect("write project");
    path
}

/// The same shape with **both** cells filled, so a shared edge is a seam between
/// two painted slots rather than between a slot and the white canvas.
///
/// The two sides get different photos: identical content on both sides would make
/// a blended seam pixel indistinguishable from the content itself.
fn write_full_project(dir: &Path, name: &str) -> PathBuf {
    let path = write_project(dir, name, true);
    let mut doc = Project::load(&path).expect("loads").doc().clone();
    std::fs::write(
        dir.join("second.jpg"),
        include_bytes!("fixtures/photos/landscape.jpg"),
    )
    .expect("write photo");
    doc.cells[1] = Cell {
        source: Some(PathBuf::from("second.jpg")),
        crop: Default::default(),
    };
    std::fs::write(&path, doc.to_json().expect("serializes")).expect("write project");
    path
}

#[test]
fn help_and_version_succeed_on_stdout() {
    let help = run(&["--help"]);
    assert_eq!(code(&help), 0);
    assert!(stdout(&help).contains("USAGE:"), "{}", stdout(&help));
    for flag in [
        "--project",
        "--template",
        "--out",
        "--long-edge",
        "--preview-px",
        "--photo",
        "--dir",
        "--recursive",
        "--px",
        "--region",
        "--slot",
        "--rotate",
        "--zoom",
        "--offset",
        "--clear",
        "--gap",
        "--radius",
        "--border-color",
        "--sketch",
        "--paper",
        "--ink",
        "--stroke",
        "--json",
        "--stats",
    ] {
        assert!(stdout(&help).contains(flag), "{flag} missing from --help");
    }
    for subcommand in [
        "render",
        "probe",
        "image",
        "scan",
        "thumb",
        "templates",
        "init",
        "edit",
        "hit",
        "save",
    ] {
        assert!(
            stdout(&help).contains(&format!("pixlay-render {subcommand}")),
            "{subcommand} missing from --help"
        );
    }
    for exit in ["0  success", "1  usage error", "2  project"] {
        assert!(stdout(&help).contains(exit), "{exit} missing from --help");
    }
    assert!(stderr(&help).is_empty());

    let version = run(&["--version"]);
    assert_eq!(code(&version), 0);
    assert_eq!(
        stdout(&version),
        format!("pixlay-render {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn the_template_smoke_path_renders_without_a_project() {
    let dir = out_dir("smoke");
    let out = dir.join("smoke.png");
    let output = run(&[
        "render",
        "--template",
        "mosaic-8-s14",
        "--long-edge",
        "3370",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "status"), "ok");
    assert_eq!(field(&output, "cells"), "8");
    // A template carries no photos, so every cell is empty and the sheet is white:
    // the smoke path checks that the geometry loads and the output path works, not
    // what a photo looks like in a slot.
    assert_eq!(field(&output, "occupied"), "0");
    assert!(stderr(&output).is_empty(), "{}", stderr(&output));

    // The smoke template has a 4:3 aspect, so a 3370 px long edge is 3370 x 2528 px.
    assert_eq!(field(&output, "out_w"), "3370");
    assert_eq!(field(&output, "out_h"), "2528");
    let image = image::open(&out).expect("output is a readable image");
    assert_eq!((image.width(), image.height()), (3370, 2528));
    assert_eq!(
        std::fs::metadata(&out).expect("stat").len().to_string(),
        field(&output, "bytes")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_sketch_draws_a_templates_geometry_in_the_callers_colours() {
    let dir = out_dir("sketch");
    let out = dir.join("sketch.png");
    let output = run(&[
        "render",
        "--template",
        "mosaic-8-s14",
        "--sketch",
        "--long-edge",
        "128",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "status"), "ok");
    assert_eq!(field(&output, "sketch"), "true");
    assert_eq!(field(&output, "slots"), "8");
    assert_eq!(field(&output, "out_w"), "128");
    assert_eq!(field(&output, "out_h"), "96");
    assert_eq!(field(&output, "long_edge"), "128");
    // The defaults, which are the renderer's: white paper, black ink, one pixel.
    assert_eq!(field(&output, "paper"), "255,255,255");
    assert_eq!(field(&output, "ink"), "0,0,0");
    assert_eq!(field(&output, "stroke"), "1.000000");
    // A sketch is not a document: the render's own rows (its cell count, its
    // occupancy, its frame) do not exist, and a caller comparing field sets sees
    // which kind of render it got from `sketch` alone.
    for absent in ["cells", "occupied", "gap", "radius", "border"] {
        assert!(
            !stdout(&output).contains(&format!("{absent} = ")),
            "{absent} has no meaning in a sketch"
        );
    }
    assert!(stderr(&output).is_empty(), "{}", stderr(&output));

    // The pixels: the sheet's own edge is a solid line, every cell's outline is
    // one, and the middle of a cell is the paper.
    let image = image::open(&out).expect("readable").to_rgb8();
    assert_eq!(image.get_pixel(0, 48).0, [0, 0, 0], "the sheet's left edge");
    assert_eq!(
        image.get_pixel(127, 48).0,
        [0, 0, 0],
        "the sheet's right edge"
    );
    assert_eq!(image.get_pixel(64, 0).0, [0, 0, 0], "the sheet's top edge");
    assert_eq!(
        image.get_pixel(64, 95).0,
        [0, 0, 0],
        "the sheet's bottom edge"
    );
    assert_eq!(image.get_pixel(20, 20).0, [255, 255, 255], "a cell's paper");

    // The two colours and the width are the caller's, and a sketch is an opaque
    // drawing: the same geometry written as a JPEG (which has no alpha at all)
    // shows the same two colours.
    let coloured = dir.join("coloured.jpg");
    let output = run(&[
        "render",
        "--template",
        "mosaic-8-s14",
        "--sketch",
        "--long-edge",
        "128",
        "--paper",
        "20,24,28",
        "--ink",
        "240,240,240",
        "--stroke",
        "3",
        "--out",
        coloured.to_str().unwrap(),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "format"), "jpeg");
    assert_eq!(field(&output, "paper"), "20,24,28");
    assert_eq!(field(&output, "ink"), "240,240,240");
    assert_eq!(field(&output, "stroke"), "3.000000");
    let image = image::open(&coloured).expect("readable").to_rgb8();
    // A JPEG is lossy, so this is "the caller's colours arrived", not exact bytes:
    // [20,24,28] and [240,240,240] are far enough apart that eight levels of
    // tolerance cannot confuse one with the other or with the default palette.
    let near = |got: [u8; 3], want: [u8; 3]| {
        got.iter()
            .zip(want)
            .all(|(got, want)| i32::from(*got).abs_diff(i32::from(want)) <= 8)
    };
    assert!(
        near(image.get_pixel(0, 48).0, [240, 240, 240]),
        "the caller's ink, at the sheet's edge: {:?}",
        image.get_pixel(0, 48).0
    );
    assert!(
        near(image.get_pixel(20, 20).0, [20, 24, 28]),
        "the caller's paper: {:?}",
        image.get_pixel(20, 20).0
    );

    // Same input, same pixels: the band's parity with this command is a
    // comparison of two runs of one renderer.
    let again = dir.join("sketch-again.png");
    let repeat = run(&[
        "render",
        "--template",
        "mosaic-8-s14",
        "--sketch",
        "--long-edge",
        "128",
        "--out",
        again.to_str().unwrap(),
    ]);
    assert_eq!(code(&repeat), 0, "{}", stderr(&repeat));
    assert_eq!(
        std::fs::read(&out).expect("read the first"),
        std::fs::read(&again).expect("read the second"),
        "two sketches of one template differ"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_sketch_refuses_what_it_cannot_draw() {
    let dir = out_dir("sketch-refusals");
    let project = write_project(&dir, "sketch.pixlay", true);
    let out = dir.join("x.png");
    let out = out.to_str().unwrap();
    let project = project.to_str().unwrap();
    for (args, what) in [
        // A sketch is a template's geometry: a project's photos and its frame are
        // not part of it, and a preview of a document neither.
        (
            vec!["render", "--project", project, "--sketch", "--out", out],
            "a project",
        ),
        (
            vec![
                "render",
                "--project",
                project,
                "--sketch",
                "--template",
                "mosaic-8-s14",
                "--out",
                out,
            ],
            "a project beside a template",
        ),
        (
            vec![
                "render",
                "--template",
                "mosaic-8-s14",
                "--sketch",
                "--preview-px",
                "800",
                "--out",
                out,
            ],
            "a preview",
        ),
        (
            vec![
                "render",
                "--template",
                "mosaic-8-s14",
                "--sketch",
                "--gap",
                "0.05",
                "--out",
                out,
            ],
            "a frame",
        ),
        // The three parameters are the sketch's, and a render has nothing to do
        // with them: taken silently they would be a drop.
        (
            vec![
                "render",
                "--template",
                "mosaic-8-s14",
                "--paper",
                "1,2,3",
                "--out",
                out,
            ],
            "a paper without a sketch",
        ),
        (
            vec![
                "render",
                "--template",
                "mosaic-8-s14",
                "--sketch",
                "--stroke",
                "0",
                "--out",
                out,
            ],
            "a stroke of zero",
        ),
    ] {
        let output = run(&args);
        assert_eq!(code(&output), 1, "{what}: {}", stderr(&output));
        assert!(stdout(&output).is_empty(), "{what}: wrote to stdout");
        assert!(!stderr(&output).is_empty(), "{what}: said nothing");
    }
    // A sketch writes nothing when it is refused, so the refusal costs a caller
    // nothing.
    assert!(!std::path::Path::new(out).exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_project_renders_and_names_its_output() {
    let dir = out_dir("project");
    let project = write_project(&dir, "two.pixlay", true);
    let out = dir.join("two.jpg");
    let output = run(&[
        "render",
        "--project",
        project.to_str().unwrap(),
        "--long-edge",
        "708",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "format"), "jpeg");
    assert_eq!(field(&output, "occupied"), "1");
    // The test document has a 4:3 aspect, so a 708 px long edge is 708 x 531 px.
    assert_eq!(field(&output, "out_w"), "708");
    assert_eq!(field(&output, "out_h"), "531");
    assert!(out.is_file());

    // Slot 0 (the left half) carries placeholder content; slot 1, which names no
    // photo, stays white.
    let image = image::open(&out)
        .expect("output is a readable image")
        .to_rgb8();
    assert_ne!(image.get_pixel(100, 250).0, [255, 255, 255]);
    assert_eq!(image.get_pixel(600, 250).0, [255, 255, 255]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn preview_px_sets_the_long_edge() {
    let dir = out_dir("preview");
    let out = dir.join("preview.png");
    let output = run(&[
        "render",
        "--template",
        "mosaic-8-s14",
        "--preview-px",
        "800",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "preview_px"), "800");
    // `preview_px` is the request; `long_edge` is the edge the file was written
    // at, which is what a caller reading one number has to compare against
    // (S15h, PIX-019). This grid is exact, so the two agree here.
    assert_eq!(field(&output, "long_edge"), "800");
    let width: i32 = field(&output, "out_w").parse().unwrap();
    let height: i32 = field(&output, "out_h").parse().unwrap();
    assert_eq!(width.max(height), 800, "{width}x{height}");
    assert_eq!(field(&output, "long_edge"), width.max(height).to_string());
    // 4:3 canvas.
    assert_eq!((width, height), (800, 600));
    let _ = std::fs::remove_dir_all(&dir);
}

/// The PNG `pHYs` chunk, if the file has one. Read from the file's own chunk
/// stream, so an assertion about the export is about the file and not about the
/// report the same command printed.
fn png_pixel_dimensions(path: &Path) -> Option<(u32, u32, u8)> {
    let bytes = std::fs::read(path).expect("read the PNG");
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    let mut at = 8;
    while at + 12 <= bytes.len() {
        let length = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        if &bytes[at + 4..at + 8] == b"pHYs" {
            let data = &bytes[at + 8..at + 8 + length];
            return Some((
                u32::from_be_bytes(data[0..4].try_into().unwrap()),
                u32::from_be_bytes(data[4..8].try_into().unwrap()),
                data[8],
            ));
        }
        at += 12 + length;
    }
    None
}

/// The component sampling factors a JPEG declares in its `SOF0` — the record of
/// what the encoder actually did, which is what the two-pass metadata trap
/// destroys.
fn jpeg_sampling(path: &Path) -> Vec<(u8, u8)> {
    let bytes = std::fs::read(path).expect("read the JPEG");
    assert_eq!(&bytes[..2], &[0xff, 0xd8]);
    let mut at = 2;
    while at + 4 <= bytes.len() {
        let marker = bytes[at + 1];
        if marker == 0xda {
            break;
        }
        let length = usize::from(u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]));
        if marker == 0xc0 {
            let frame = &bytes[at + 4..at + 2 + length];
            return (0..frame[5] as usize)
                .map(|index| {
                    let sampling = frame[6 + 3 * index + 1];
                    (sampling >> 4, sampling & 0x0f)
                })
                .collect();
        }
        at += 2 + length;
    }
    panic!("no SOF0 in {}", path.display());
}

#[test]
fn long_edge_is_exact_and_carries_the_resolution_it_works_out_to() {
    let dir = out_dir("long-edge");
    // `mosaic-8-s14` has a 4:3 aspect, so the long edge is the width and the
    // short one is exactly 3/4 of it.
    let out = dir.join("long.png");
    let output = run(&[
        "render",
        "--template",
        "mosaic-8-s14",
        "--long-edge",
        "9000",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "long_edge"), "9000");
    assert_eq!(field(&output, "out_w"), "9000");
    assert_eq!(field(&output, "out_h"), "6750");
    let image = image::open(&out).expect("a readable PNG").to_rgb8();
    assert_eq!((image.width(), image.height()), (9000, 6750));

    // The file carries no resolution (S12d): the PNG has no `pHYs`, and the
    // report names no resolution for the 9000 px grid.
    assert!(png_pixel_dimensions(&out).is_none(), "no pHYs is written");
    assert!(
        stdout(&output)
            .lines()
            .all(|line| !line.starts_with("dpi = "))
    );

    // A portrait canvas puts its exact edge on the other axis.
    let portrait = dir.join("portrait.png");
    let output = run(&[
        "render",
        "--template",
        "strip-2-1x2",
        "--long-edge",
        "1234",
        "--out",
        portrait.to_str().unwrap(),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "out_h"), "1234");
    // 2:3 canvas: 1234 * 2/3 = 822.67, rounded half away from zero.
    assert_eq!(field(&output, "out_w"), "823");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_export_format_is_written_with_its_metadata() {
    let dir = out_dir("formats");
    for (name, format) in [
        ("out.png", "png"),
        ("out.jpg", "jpeg"),
        ("out.jpeg", "jpeg"),
    ] {
        let out = dir.join(name);
        let output = run(&[
            "render",
            "--template",
            "grid-4-2x2",
            "--long-edge",
            "2835",
            "--out",
            out.to_str().unwrap(),
        ]);
        assert_eq!(code(&output), 0, "{name}: {}", stderr(&output));
        assert_eq!(field(&output, "format"), format);
        // Decodable by another implementation, at the size the report claims.
        let image = image::open(&out).expect("a readable image");
        assert_eq!(
            (image.width(), image.height()),
            (
                field(&output, "out_w").parse::<u32>().unwrap(),
                field(&output, "out_h").parse::<u32>().unwrap()
            ),
            "{name}"
        );
        assert_eq!(
            std::fs::metadata(&out).expect("stat").len().to_string(),
            field(&output, "bytes"),
            "{name}"
        );

        // The profile and the sampling factors are in the file — no resolution
        // since S12d. PNG deflates the profile into iCCP, so its presence is what
        // this level checks; what the bytes decode to is `pixlay-imaging`'s test.
        let bytes = std::fs::read(&out).expect("read back");
        let holds = |needle: &[u8]| bytes.windows(needle.len()).any(|window| window == needle);
        match format {
            "png" => {
                // No `pHYs` (S12d): the file's size is its pixels.
                assert!(
                    png_pixel_dimensions(&out).is_none(),
                    "{name} claims a resolution"
                );
                assert!(holds(b"iCCP"), "{name} carries no profile");
            }
            _ => {
                assert!(holds(b"ICC_PROFILE"), "{name} carries no profile");
                assert!(holds(b"JFIF"), "{name} is not JFIF");
                // 4:4:4 in the file's own frame header, unrequested since S12c
                // removed `--chroma`: the sampling factor is a property of the
                // encoder now, and this is the assertion a metadata-patching
                // second pass would fail.
                assert_eq!(
                    jpeg_sampling(&out),
                    vec![(1, 1), (1, 1), (1, 1)],
                    "{name} is not 4:4:4"
                );
            }
        }
    }

    // An extension nothing writes is a usage error, and the message names the
    // formats this build has — two since S12c removed TIFF, so `.tif` is refused
    // like any other unknown extension rather than falling back to PNG.
    for extension in ["gif", "tif", "tiff"] {
        let out = dir.join(format!("out.{extension}"));
        let output = run(&[
            "render",
            "--template",
            "grid-4-2x2",
            "--out",
            out.to_str().unwrap(),
        ]);
        assert_eq!(code(&output), 1, "{extension}");
        assert!(
            stderr(&output).contains(".png, .jpg or .jpeg"),
            "{extension}: {}",
            stderr(&output)
        );
        assert!(!out.exists(), "a refused render writes nothing");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_export_modes_are_mutually_exclusive() {
    let dir = out_dir("modes");
    let out = dir.join("out.png").to_str().unwrap().to_string();
    fn with_template<'a>(out: &'a str, flags: &[&'a str]) -> Vec<&'a str> {
        let mut args = vec!["render", "--template", "grid-4-2x2", "--out", out];
        args.extend_from_slice(flags);
        args
    }
    for flags in [
        // A preview is a smaller render of the export; a long edge *is* the size.
        &["--long-edge", "1000", "--preview-px", "400"][..],
        // Out of range on both ends, and zero.
        &["--long-edge", "0"][..],
        &["--long-edge", "30001"][..],
        &["--long-edge", "wide"][..],
        // A flag this build no longer has (S12c removed the JPEG chroma request).
        &["--chroma", "444"][..],
        // Two more this build no longer has (S12d removed paper: no resolution to
        // ask for, no sheet to set).
        &["--dpi", "300"][..],
        &["--sheet", "420"][..],
    ] {
        let output = run(&with_template(&out, flags));
        assert_eq!(code(&output), 1, "{flags:?}: {}", stderr(&output));
        assert!(stdout(&output).is_empty(), "{flags:?} wrote to stdout");
        assert!(!stderr(&output).is_empty(), "{flags:?} said nothing");
    }
    assert!(
        !Path::new(out.as_str()).exists(),
        "a usage error writes nothing"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The canvas pixel budget at the boundaries a size is actually *asked for*
/// (S15e, PIX-003).
///
/// `--preview-px` and `--grid` are long edges, and the grid they derive is what
/// gets allocated: the flag's own range (`1..=20000`) is a range of long edges,
/// and 20000 on a square canvas is 400 MP. The refusal is the typed one — exit 2,
/// the same `CanvasTooLarge` `--long-edge` raises — and it happens before the
/// first decode, so a refused render costs nothing and writes nothing.
#[test]
fn a_grid_past_the_canvas_budget_is_refused_before_it_is_allocated() {
    let dir = out_dir("budget");
    // A square template: 20000x20000 = 400 MP, well past the 200 MP budget.
    let out = dir.join("preview.png");
    let output = run(&[
        "render",
        "--template",
        "grid-4-2x2",
        "--preview-px",
        "20000",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code(&output), 2, "{}", stderr(&output));
    assert!(
        stderr(&output).contains("canvas would be 400000000 pixels"),
        "{}",
        stderr(&output)
    );
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
    assert!(!out.exists(), "a refused render writes nothing");

    // The same limit on the export path, where it has always applied: the flag is
    // inside `MAX_LONG_EDGE_PX` and the *grid* it derives is not.
    let out = dir.join("long.png");
    let output = run(&[
        "render",
        "--template",
        "grid-4-2x2",
        "--long-edge",
        "30000",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code(&output), 2, "{}", stderr(&output));
    assert!(
        stderr(&output).contains("canvas would be 900000000 pixels"),
        "{}",
        stderr(&output)
    );
    assert!(!out.exists(), "a refused render writes nothing");

    // And on `gesture`, whose grid used to be derived by a second implementation
    // with no budget at all: a 4:3 canvas at 20000 is 300 MP.
    let project = write_full_project(&dir, "two.pixlay");
    let output = run(&[
        "gesture",
        "--project",
        project.to_str().unwrap(),
        "--grid",
        "20000",
    ]);
    assert_eq!(code(&output), 2, "{}", stderr(&output));
    assert!(
        stderr(&output).contains("canvas would be 300000000 pixels"),
        "{}",
        stderr(&output)
    );
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
    let _ = std::fs::remove_dir_all(&dir);
}

/// The other half of the same budget: the bitmap boundary (S15e, PIX-003).
///
/// A bitmap holds the part of the photo a slot can show, which is the slot's own
/// extent plus the axis-aligned box a framing rotation needs — so a half-canvas
/// slot at 45 degrees asks for more than the canvas it sits on, and the pipeline
/// refuses it naming the slot and the memory the conversion would have held.
/// Without the refusal this render would try to hold 3.9 GB of one cell's bitmap.
#[test]
fn a_bitmap_past_the_budget_is_refused_naming_the_slot() {
    let dir = out_dir("bitmap-budget");
    let photo = dir.join("photo.png");
    std::fs::write(&photo, include_bytes!("fixtures/photos/ratio-4-3.png")).expect("write photo");
    let project = dir.join("square.pixlay");
    let created = run(&[
        "init",
        "--template",
        "strip-2-2x1g",
        "--photo",
        photo.to_str().unwrap(),
        "--photo",
        photo.to_str().unwrap(),
        "--out",
        project.to_str().unwrap(),
    ]);
    assert_eq!(code(&created), 0, "{}", stderr(&created));
    let rotated = dir.join("rotated.pixlay");
    let edited = run(&[
        "edit",
        "--project",
        project.to_str().unwrap(),
        "--slot",
        "0",
        "--rotate",
        "45",
        "--out",
        rotated.to_str().unwrap(),
    ]);
    assert_eq!(code(&edited), 0, "{}", stderr(&edited));

    let out = dir.join("a0.png");
    let output = run(&[
        "render",
        "--project",
        rotated.to_str().unwrap(),
        "--long-edge",
        "14043",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code(&output), 2, "{}", stderr(&output));
    assert!(
        stderr(&output).contains("slot 0: bitmap needs"),
        "{}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("the limit is 200000000 pixels"),
        "{}",
        stderr(&output)
    );
    assert!(!out.exists(), "a refused render writes nothing");

    // The same document at a grid where the box fits still renders, so the check
    // is a bound rather than a refusal of the layout.
    let small = dir.join("small.png");
    let output = run(&[
        "render",
        "--project",
        rotated.to_str().unwrap(),
        "--long-edge",
        "2000",
        "--out",
        small.to_str().unwrap(),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(small.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A **square** sheet with two cells side by side, both filled, under `frame_gap`.
///
/// The square aspect is what makes the frame's number the same arithmetic in both
/// axes, and at a 4000 px long edge the canvas is 4000 px tall — so a 4% gap is
/// exactly 160 px, the number S20's criterion names.
fn write_square_project(dir: &Path, name: &str, frame_gap: f64) -> PathBuf {
    let left = Polygon::rect(0.0, 0.0, 0.5, 1.0);
    let right = Polygon::rect(0.5, 0.0, 1.0, 1.0);
    let template = Template {
        name: "test-2-square".to_string(),
        version: 1,
        aspect: 1.0,
        slots: vec![
            Slot {
                area: left.area(),
                outline: left,
            },
            Slot {
                area: right.area(),
                outline: right,
            },
        ],
    };
    let mut doc = CollageDoc::new(template);
    doc.frame.gap_rel = frame_gap;
    let photos = ["a.png", "b.jpg"];
    let bytes: [&[u8]; 2] = [
        include_bytes!("fixtures/photos/square.png"),
        include_bytes!("fixtures/photos/landscape.jpg"),
    ];
    for ((cell, photo), data) in doc.cells.iter_mut().zip(photos).zip(bytes) {
        std::fs::write(dir.join(photo), data).expect("write photo");
        cell.source = Some(PathBuf::from(photo));
    }
    let path = dir.join(name);
    std::fs::write(&path, doc.to_json().expect("serializes")).expect("write project");
    path
}

#[test]
fn probe_reports_numbers_the_renderer_can_be_judged_by() {
    let dir = out_dir("probe");
    let project = write_project(&dir, "two.pixlay", true);
    let output = run(&[
        "probe",
        "--project",
        project.to_str().unwrap(),
        "--long-edge",
        "1500",
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "status"), "ok");
    assert_eq!(field(&output, "slots"), "2");
    assert_eq!(field(&output, "occupied"), "1");
    assert_eq!(field(&output, "bg_off_backdrop"), "0");
    // A probe that sampled nothing would pass vacuously: the two-slot project
    // leaves a margin around both slots.
    assert!(
        field(&output, "bg_samples").parse::<i64>().unwrap() > 1000,
        "background sampler found nothing to check"
    );
    // The probe paints the slots itself, so the interior samples must be the
    // palette colors exactly — that is what makes a swapped cell or a misplaced
    // bitmap fail rather than merely look odd.
    assert_eq!(field(&output, "slot.0.match"), "true");
    assert_eq!(field(&output, "slot.0.expected"), "200,30,40");
    assert_eq!(field(&output, "slot.0.actual"), "200,30,40");
    assert!(field(&output, "slot.0.depth_px").parse::<f64>().unwrap() > 100.0);
    assert_eq!(field(&output, "seam.0.clean"), "true");
    // The right cell is empty, so this edge is between a slot and the white
    // canvas: there is no blend to measure and the row count says so.
    assert_eq!(field(&output, "seam.0.rows"), "0");

    // With both cells filled the same edge is a real seam between two painted
    // slots, and now it has to be measured: one antialiased pixel per seam pixel,
    // nothing foreign, and both interiors on their own palette color.
    let full = write_full_project(&dir, "full.pixlay");
    let output = run(&[
        "probe",
        "--project",
        full.to_str().unwrap(),
        "--long-edge",
        "1500",
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "occupied"), "2");
    assert_eq!(field(&output, "slot.1.match"), "true");
    assert_eq!(field(&output, "slot.1.expected"), "30,160,60");
    assert_eq!(field(&output, "bg_off_backdrop"), "0");
    // Walked end to end: the walker keeps a guard band at both ends where the
    // seam meets a corner, so the row count is the seam length minus 4.
    let rows: i64 = field(&output, "seam.0.rows").parse().unwrap();
    let length: f64 = field(&output, "seam.0.length_px").parse().unwrap();
    assert!(
        (length - rows as f64).abs() <= 5.0,
        "{rows} rows of {length} px"
    );
    assert_eq!(field(&output, "seam.0.foreign"), "0");
    // The seam is the shared edge: the full height of the canvas (the probe runs
    // at a 1500 px long edge on a 4:3 document, so the grid is 1500x1125).
    assert!((field(&output, "seam.0.length_px").parse::<f64>().unwrap() - 1125.0).abs() <= 1.0);
    // One antialiased edge: about one blended pixel per seam pixel.
    let per_px: f64 = field(&output, "seam.0.per_px").parse().unwrap();
    assert!(per_px <= 2.0, "seam blend {per_px} px per seam px");
    assert!(
        field(&output, "seam.0.max_residual")
            .parse::<f64>()
            .unwrap()
            <= 3.0
    );
    assert_eq!(field(&output, "seam.0.foreign"), "0");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_gap_is_the_distance_between_two_photos() {
    // S20, ruling 35: the frame's number is the visible stripe — between two photos
    // *and* between a photo and the sheet's edge. At a 4000 px long edge on a square
    // sheet a 4% gap is 160 px, and it has to measure 160 px in all five places: the
    // seam between the two cells and the four borders of the sheet. Before S20 the
    // borders measured 80 px (half the number, with the seam the whole one) and this
    // probe reported `passed = false`.
    let dir = out_dir("gap");
    let project = write_square_project(&dir, "square.pixlay", 0.04);
    let output = run(&[
        "probe",
        "--project",
        project.to_str().unwrap(),
        "--long-edge",
        "4000",
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "out_h"), "4000");
    assert_eq!(field(&output, "gap_px"), "160.000000");
    assert_eq!(field(&output, "gap_ok"), "true");
    for (index, side) in ["top", "right", "bottom", "left"].into_iter().enumerate() {
        let prefix = format!("border.{index}");
        assert_eq!(field(&output, &format!("{prefix}.side")), side);
        assert_eq!(
            field(&output, &format!("{prefix}.gap_min_px")),
            "160",
            "{side}"
        );
        assert_eq!(
            field(&output, &format!("{prefix}.gap_max_px")),
            "160",
            "{side}"
        );
        assert!(
            field(&output, &format!("{prefix}.samples"))
                .parse::<i64>()
                .unwrap()
                >= 1,
            "{side} was not sampled at all"
        );
        assert_eq!(
            field(&output, &format!("{prefix}.gap_ok")),
            "true",
            "{side}"
        );
    }
    // One seam: the two cells share the full-height edge at the middle of the sheet.
    let prefix = "seam.0";
    assert_eq!(
        field(&output, &format!("{prefix}.length_px")),
        "4000.000000"
    );
    assert_eq!(
        field(&output, &format!("{prefix}.gap_min_px")),
        "160",
        "{prefix}"
    );
    assert_eq!(
        field(&output, &format!("{prefix}.gap_max_px")),
        "160",
        "{prefix}"
    );
    assert!(
        field(&output, &format!("{prefix}.gap_rows"))
            .parse::<i64>()
            .unwrap()
            > 100,
        "{prefix} was measured on too few rows to mean anything"
    );
    // Whole-pixel geometry on a square sheet, so the stripe is exact and the
    // deviation from what the geometry leaves is zero.
    assert_eq!(field(&output, &format!("{prefix}.gap_dev_px")), "0.000000");
    assert_eq!(
        field(&output, &format!("{prefix}.gap_ok")),
        "true",
        "{prefix}"
    );
    assert_eq!(field(&output, "passed"), "true");

    // The other two shapes S20's criterion names, on the library's own templates so
    // the whole machine path is exercised rather than a hand-made document: a rounded
    // rectangle (the corners are cut, the stripes at the middles are not) and a
    // single-slot document, which has no seam at all and one uniform border. The
    // expected width is the probe's own `gap_px`, because these templates are not
    // square: it is the same number in pixels whatever the aspect is.
    let photo = dir.join("round.png");
    std::fs::write(&photo, include_bytes!("fixtures/photos/square.png")).expect("write photo");
    let second = dir.join("round2.jpg");
    std::fs::write(&second, include_bytes!("fixtures/photos/landscape.jpg")).expect("write photo");
    for (template, photos, seams) in [
        ("strip-2-1x2", vec![&photo, &second], 1usize),
        ("grid-1-1x1", vec![&photo], 0usize),
    ] {
        let plain = dir.join(format!("{template}.pixlay"));
        let framed = dir.join(format!("{template}-framed.pixlay"));
        let mut args = vec!["init", "--template", template];
        for photo in &photos {
            args.push("--photo");
            args.push(photo.to_str().unwrap());
        }
        args.push("--out");
        args.push(plain.to_str().unwrap());
        let output = run(&args);
        assert_eq!(code(&output), 0, "{}", stderr(&output));

        let output = run(&[
            "edit",
            "--project",
            plain.to_str().unwrap(),
            "--gap",
            "0.04",
            "--radius",
            "0.03",
            "--out",
            framed.to_str().unwrap(),
        ]);
        assert_eq!(code(&output), 0, "{}", stderr(&output));

        let output = run(&[
            "probe",
            "--project",
            framed.to_str().unwrap(),
            "--long-edge",
            "1000",
        ]);
        assert_eq!(code(&output), 0, "{template}: {}", stderr(&output));
        let expected: f64 = field(&output, "gap_px").parse().unwrap();
        assert!(expected > 0.0);
        let rows = stdout(&output)
            .lines()
            .filter(|line| line.starts_with("seam.") && line.ends_with(".gap_max_px = 0"))
            .count();
        assert_eq!(rows, 0, "{template}: a measured stripe came out at zero px");
        let measured: Vec<f64> = stdout(&output)
            .lines()
            .filter_map(|line| {
                let (key, value) = line.split_once(" = ")?;
                (key.ends_with(".gap_min_px") || key.ends_with(".gap_max_px"))
                    .then(|| value.parse::<f64>().ok())?
            })
            .collect();
        assert_eq!(measured.len(), 2 * (4 + seams), "{template}: {measured:?}");
        for stripe in measured {
            // The probe's own tolerance: the stripe is measured as a pixel span, so
            // a fractional boundary adds up to a pixel at each end.
            assert!(
                (stripe - expected).abs() <= 2.0,
                "{template}: a {stripe} px stripe against the frame's {expected} px"
            );
        }
        assert_eq!(field(&output, "passed"), "true", "{template}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_crop_below_the_covering_zoom_is_clamped_instead_of_leaving_white() {
    // A crop is a *request*; what `draw` paints is its fit (S3). `zoom: 0.5`
    // cannot cover this slot, and the probe — whose first question is whether the
    // slot's own content is where the geometry says it is — passes anyway,
    // because the clamp raised the zoom before painting. S1 asserted the opposite
    // (that this document left white inside the slot and failed the probe); that
    // was a property of a renderer with no clamp, and the probe's ability to fail
    // its interior criterion is now pinned in `tests/probe.rs`, which hands it an
    // image that is wrong by construction.
    let dir = out_dir("probe-clamped");
    let project = write_project(&dir, "underzoomed.pixlay", true);
    let json = std::fs::read_to_string(&project).expect("read");
    assert!(
        json.contains("\"zoom\": 1.0"),
        "the fixture's zoom changed shape:\n{json}"
    );
    std::fs::write(&project, json.replace("\"zoom\": 1.0", "\"zoom\": 0.5")).expect("write");

    let output = run(&[
        "probe",
        "--project",
        project.to_str().unwrap(),
        "--long-edge",
        "1500",
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "status"), "ok");
    assert_eq!(field(&output, "passed"), "true");
    assert_eq!(field(&output, "slot.0.match"), "true");
    assert_eq!(field(&output, "slot.0.expected"), "200,30,40");
    assert_eq!(field(&output, "slot.0.actual"), "200,30,40");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_probe_with_nothing_to_probe_fails_instead_of_passing_vacuously() {
    // Every question the probe answers is about an occupied cell. A project whose
    // cells are all empty renders a blank sheet, so "all checks passed" would be
    // a verdict about content that was never drawn: measured before this floor,
    // it reported status = ok, occupied = 0, exit 0.
    let dir = out_dir("probe-vacuous");
    let project = write_project(&dir, "empty.pixlay", false);
    let output = run(&[
        "probe",
        "--project",
        project.to_str().unwrap(),
        "--long-edge",
        "1500",
    ]);
    assert_eq!(code(&output), 2, "an empty project must not pass the probe");
    assert_eq!(field(&output, "occupied"), "0");
    assert_eq!(field(&output, "passed"), "false");
    assert_eq!(field(&output, "status"), "failed");
    assert!(
        stderr(&output).contains("no cell is occupied"),
        "{}",
        stderr(&output)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn json_output_parses_and_is_byte_stable() {
    let dir = out_dir("json");
    let output = run(&[
        "render",
        "--template",
        "mosaic-8-s14",
        "--long-edge",
        "3370",
        "--out",
        dir.join("json.png").to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    let value: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("valid JSON");
    assert_eq!(value["status"], "ok");
    assert_eq!(value["cells"], 8);
    assert_eq!(value["out_w"], 3370); // --long-edge 3370 on a 4:3 template

    // Same input, same bytes. The result carries no timestamps and no durations.
    let repeat = run(&[
        "render",
        "--template",
        "mosaic-8-s14",
        "--long-edge",
        "3370",
        "--out",
        dir.join("json-again.png").to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(stdout(&output), stdout(&repeat));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn stats_adds_measurements_without_changing_the_rest() {
    let dir = out_dir("stats");
    let plain_out = dir.join("plain.png");
    let stats_out = dir.join("stats.png");
    let render = |out: &Path, stats: bool| {
        let mut args = vec![
            "render".to_string(),
            "--template".to_string(),
            "mosaic-8-s14".to_string(),
            "--long-edge".to_string(),
            "3370".to_string(),
            "--out".to_string(),
            out.display().to_string(),
        ];
        if stats {
            args.push("--stats".to_string());
        }
        let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
        run(&borrowed)
    };
    let plain = render(&plain_out, false);
    let with_stats = render(&stats_out, true);
    assert_eq!(code(&plain), 0, "{}", stderr(&plain));
    assert_eq!(code(&with_stats), 0, "{}", stderr(&with_stats));

    let measured = field(&with_stats, "peak_rss_mb").parse::<f64>().unwrap();
    assert!(measured > 10.0, "peak_rss_mb {measured}");
    assert!(field(&with_stats, "ms").parse::<f64>().unwrap() > 0.0);
    // `icc` is the profile the written file carries, not a promise (S6).
    assert_eq!(field(&with_stats, "icc"), pixlay_imaging::icc::DESCRIPTION);

    // Everything except the measured fields is identical, so the ruler adds
    // fields rather than changing the report.
    let strip = |text: &str| {
        text.lines()
            .filter(|line| {
                !line.starts_with("ms = ")
                    && !line.starts_with("peak_rss_mb = ")
                    && !line.starts_with("encode_ms = ")
            })
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    let plain_fields = strip(&stdout(&plain));
    let stats_fields = strip(&stdout(&with_stats));
    for line in &plain_fields {
        assert!(stats_fields.contains(line), "{line} missing with --stats");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn locale_never_changes_stdout_or_stderr() {
    let dir = out_dir("locale");
    let out = dir.join("locale.png");

    let cases = [
        ("C", "C"),
        ("en_US.UTF-8", "en_US.UTF-8"),
        ("zh_CN.UTF-8", "zh_CN.UTF-8"),
        ("de_DE.UTF-8", "de_DE.UTF-8"),
    ];
    let mut reference: Option<(Vec<u8>, Vec<u8>)> = None;
    for (lang, all) in cases {
        let output = run_in(
            &[
                "render",
                "--template",
                "mosaic-8-s14",
                "--long-edge",
                "3370",
                "--out",
                out.to_str().expect("path"),
                "--json",
            ],
            None,
            Some((lang, all)),
        );
        assert_eq!(code(&output), 0, "{lang}: {}", stderr(&output));
        match &reference {
            None => reference = Some((output.stdout.clone(), output.stderr.clone())),
            Some((stdout_ref, stderr_ref)) => {
                assert_eq!(
                    &output.stdout, stdout_ref,
                    "stdout changed under LANG={lang}"
                );
                assert_eq!(
                    &output.stderr, stderr_ref,
                    "stderr changed under LANG={lang}"
                );
            }
        }
    }

    // The error branch is locale-independent too. A relative path that does not
    // exist keeps the message free of machine-specific text.
    let mut messages = Vec::new();
    for (lang, all) in cases {
        let output = run_in(
            &["render", "--project", "missing.pixlay", "--out", "x.png"],
            Some(&dir),
            Some((lang, all)),
        );
        assert_eq!(code(&output), 2);
        assert!(
            stdout(&output).is_empty(),
            "stdout must stay empty on failure"
        );
        messages.push(stderr(&output));
    }
    assert!(
        messages.windows(2).all(|pair| pair[0] == pair[1]),
        "{messages:?}"
    );
    assert!(messages[0].contains("missing.pixlay"), "{}", messages[0]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn exit_codes_are_fixed() {
    let dir = out_dir("exits");

    // 1: usage errors, all of them, with an empty stdout.
    let usage_cases: [&[&str]; 8] = [
        &[],
        &["nonsense"],
        &["--nonsense"],
        &["render"],
        &["render", "--template", "mosaic-8-s14"],
        &["render", "--out", "/var/tmp/should-not-exist.png"],
        &["render", "--template", "mosaic-8-s14", "--out", "x.gif"],
        // A flag this build no longer has: S12d removed paper, so there is no
        // resolution to ask for.
        &[
            "render",
            "--template",
            "mosaic-8-s14",
            "--out",
            "x.png",
            "--dpi",
            "300",
            // ^ S12d: no resolution to ask for; the flag is gone.
        ],
    ];
    for args in usage_cases {
        let output = run(args);
        assert_eq!(code(&output), 1, "{args:?}: {}", stderr(&output));
        assert!(stdout(&output).is_empty(), "{args:?} wrote to stdout");
        assert!(
            !stderr(&output).is_empty(),
            "{args:?} wrote nothing to stderr"
        );
    }

    // An unknown template is a usage error: the name is the caller's typo.
    let unknown = run(&["render", "--template", "nope", "--out", "x.png"]);
    assert_eq!(code(&unknown), 1);
    assert!(
        stderr(&unknown).contains("mosaic-8-s14"),
        "{}",
        stderr(&unknown)
    );

    // 2: the input exists but cannot be used, and the path is named.
    let broken = dir.join("broken.pixlay");
    std::fs::write(&broken, "{ not json").expect("write broken project");
    let output = run(&[
        "render",
        "--project",
        broken.to_str().unwrap(),
        "--out",
        "x.png",
    ]);
    assert_eq!(code(&output), 2, "{}", stderr(&output));
    assert!(stdout(&output).is_empty());
    // The exact file, serde's own detail, and nothing invented in between
    // (S15h, PIX-021): the message has to say which project was wrong.
    let message = stderr(&output);
    assert!(
        message.contains(broken.to_str().unwrap()),
        "the failing project must be named exactly: {message}"
    );
    assert!(
        message.contains("project JSON"),
        "serde's detail is kept: {message}"
    );
    assert!(
        message.contains("at line 1"),
        "serde's position is kept: {message}"
    );

    let missing_source = dir.join("dangling.pixlay");
    let project = write_project(&dir, "dangling.pixlay", true);
    let json = std::fs::read_to_string(&project)
        .expect("read")
        .replace("photo.png", "gone.png");
    std::fs::write(&missing_source, json).expect("write");
    let output = run(&[
        "render",
        "--project",
        missing_source.to_str().unwrap(),
        "--out",
        "x.png",
    ]);
    assert_eq!(code(&output), 2);
    assert!(
        stderr(&output).contains("gone.png"),
        "the missing path must be named: {}",
        stderr(&output)
    );

    // 2: the output directory does not exist. The message names the file.
    let output = run(&[
        "render",
        "--template",
        "mosaic-8-s14",
        "--long-edge",
        "3370",
        "--out",
        "/var/tmp/pixlay-does-not-exist/out.png",
    ]);
    assert_eq!(code(&output), 2);
    assert!(stderr(&output).contains("out.png"), "{}", stderr(&output));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn probe_refuses_flags_that_belong_to_render() {
    for args in [
        vec!["probe", "--template", "mosaic-8-s14"],
        vec!["probe", "--project", "x.pixlay", "--out", "y.png"],
        vec!["probe", "--project", "x.pixlay", "--preview-px", "100"],
        vec!["image", "--photo", "x.jpg", "--out", "y.png"],
        vec!["image", "--project", "x.pixlay"],
    ] {
        let output = run(&args);
        assert_eq!(code(&output), 1, "{args:?}");
        assert!(stdout(&output).is_empty(), "{args:?}");
    }
}

#[test]
fn render_never_reads_stdin_or_needs_a_tty() {
    // The harness already closes stdin and runs without a TTY; this makes the
    // requirement explicit and checks that a pipe of data makes no difference.
    let dir = out_dir("no-stdin");
    let mut command = Command::new(BIN);
    command
        .args([
            "render",
            "--template",
            "mosaic-8-s14",
            "--long-edge",
            "3370",
            "--out",
            dir.join("out.png").to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("spawn");
    {
        use std::io::Write as _;
        let stdin = child.stdin.as_mut().expect("stdin");
        // Data on stdin that a prompt would have consumed: the command must
        // ignore it entirely.
        let _ = stdin.write_all(b"yes\n");
    }
    let output = child.wait_with_output().expect("wait");
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "status"), "ok");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_rendered_output_matches_what_draw_produces() {
    // The CLI is a thin wrapper: whatever it writes must be exactly what
    // `pixlay_render::draw` produces for the same document. This is the check
    // that keeps preview and export on one path.
    let dir = out_dir("one-path");
    let project = write_project(&dir, "two.pixlay", true);
    let out = dir.join("two.png");
    let output = run(&[
        "render",
        "--project",
        project.to_str().unwrap(),
        "--long-edge",
        "454",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));

    // The same document through the same two libraries: the CLI must be a thin
    // wrapper over `pixlay_imaging` + `pixlay_render::draw`, not a second path.
    let loaded = Project::load(&project).expect("loads");
    let sources = loaded.sources().expect("sources");
    let canvas = PixelSize::for_long_edge(loaded.doc().template.aspect, 454).expect("canvas size");
    let bitmaps = pixlay_imaging::slot_bitmaps(loaded.doc(), canvas, &sources).expect("decode");
    let mut images = pixlay_render::Images::new();
    for bitmap in &bitmaps {
        images.insert(
            bitmap.slot,
            pixlay_render::Bitmap::from_argb32_region(
                bitmap.width as i32,
                bitmap.height as i32,
                bitmap.origin,
                bitmap.display,
                bitmap.pixels.clone(),
            )
            .expect("bitmap"),
        );
    }
    let expected =
        pixlay_render::render_rgb8(loaded.doc(), &images, canvas, 1.0, None).expect("draw");
    let actual = image::open(&out).expect("output").to_rgb8();
    assert_eq!(
        (actual.width(), actual.height()),
        (expected.width as u32, expected.height as u32)
    );
    let differing = expected
        .data
        .chunks(3)
        .zip(actual.pixels())
        .filter(|(want, got)| want != &got.0)
        .count();
    assert_eq!(differing, 0, "{differing} pixels differ from draw()");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The example in `docs/CONTRACT.md` §1 is the first thing a reader copies. It
/// once declared a 297x210 canvas against a template whose aspect is 4:3, which
/// this build refuses as a hard error — the contract contradicting itself.
///
/// Rather than restate the example here (a copy would drift), this test extracts
/// the `jsonc` block from the document, strips its `//` comments, and loads it.
/// The example is expected to be a *valid document* under the current shape:
/// `docVersion` 3, no `text`/`filter`/`grade`/`textFallback`/`canvas` key, a frame.
/// The whole path with a real photo: decode, resample in linear light, place,
/// clip, encode.
///
/// A flat photo makes this exact rather than statistical. Every pixel more than a
/// few pixels inside a slot must be *the photo's color*: a white sliver, a region
/// that stops short, an offset that drifted or a channel swap anywhere in the
/// pipeline shows up as a pixel that is not. The slot boundary itself is excluded
/// because Cairo antialiases the clip, which legitimately blends with the white
/// base underneath.
#[test]
fn a_decoded_photo_fills_its_slot() {
    let dir = out_dir("decode-fills");
    let color = [30u8, 140, 200];
    let photo = dir.join("flat.png");
    image::RgbImage::from_fn(1200, 900, |_, _| image::Rgb(color))
        .save(&photo)
        .expect("write photo");

    // The two-slot cut template, both cells filled with the same flat photo, and
    // framings that magnify, pan and rotate it.
    let path = write_project(&dir, "flat.pixlay", true);
    let mut doc =
        CollageDoc::from_json(&std::fs::read_to_string(&path).expect("read")).expect("loads");
    // Both cells point at the flat photo; the helper's own fixture photo stays
    // unreferenced in the directory.
    doc.cells[0].source = Some(photo.clone());
    doc.cells[1] = Cell {
        source: Some(photo.clone()),
        crop: CropTransform {
            zoom: 2.2,
            offset: (0.3, -0.4),
            rotation_deg: 25.0,
        },
    };
    std::fs::write(&path, doc.to_json().expect("serializes")).expect("write project");

    let out = dir.join("flat.png.out.png");
    let output = run(&[
        "render",
        "--project",
        path.to_str().unwrap(),
        "--long-edge",
        "1500",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));

    let canvas = PixelSize::for_long_edge(doc.template.aspect, 1500).expect("canvas size");
    let image = image::open(&out).expect("output").to_rgb8();
    assert_eq!(
        (image.width() as i32, image.height() as i32),
        (canvas.width, canvas.height)
    );
    let mut checked = 0;
    for y in 0..canvas.height {
        for x in 0..canvas.width {
            let point = Point::new(
                f64::from(x) / f64::from(canvas.width),
                f64::from(y) / f64::from(canvas.height),
            );
            let depth = doc
                .template
                .slots
                .iter()
                .map(|slot| slot.outline.distance_to_boundary(point) * f64::from(canvas.height))
                .fold(f64::INFINITY, f64::min);
            if depth <= 3.0 {
                continue;
            }
            checked += 1;
            let got = image.get_pixel(x as u32, y as u32).0;
            assert_eq!(
                got, color,
                "({x}, {y}) is {depth:.1} px inside a slot but is {got:?}"
            );
        }
    }
    assert!(checked > 100_000, "only {checked} pixels were checked");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_contract_example_is_a_valid_document() {
    let doc_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/CONTRACT.md");
    let text = std::fs::read_to_string(&doc_path).expect("read docs/CONTRACT.md");

    let block = text
        .split_once("```jsonc\n")
        .and_then(|(_, rest)| rest.split_once("\n```"))
        .map(|(block, _)| block)
        .expect("docs/CONTRACT.md has a jsonc example");

    // Strip the inline `//` comments the example uses to annotate fields. The
    // example contains no string with `//` in it, so a split is exact.
    let json: String = block
        .lines()
        .map(|line| line.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");

    let doc = CollageDoc::from_json(&json).expect("the contract example is a valid document");

    // The template must be a cut template: the slots tile the canvas exactly.
    let area: f64 = doc.template.slots.iter().map(|slot| slot.area).sum();
    assert!(
        (area - 1.0).abs() < 1e-12,
        "the example's slot areas sum to {area}, not 1.0"
    );
    assert_eq!(doc.cells.len(), doc.template.slots.len());
}

#[test]
fn templates_lists_the_library_and_filters_by_aspect() {
    let all = run(&["templates"]);
    assert_eq!(code(&all), 0, "{}", stderr(&all));
    assert!(stderr(&all).is_empty(), "{}", stderr(&all));
    assert_eq!(field(&all, "status"), "ok");

    // Every slot count of the product range appears, which is S2's coverage
    // criterion seen from the outside.
    let count: usize = field(&all, "count").parse().unwrap();
    let mut slot_counts: Vec<usize> = Vec::new();
    for index in 0..count {
        let name = field(&all, &format!("template.{index}.name"));
        let slots: usize = field(&all, &format!("template.{index}.slots"))
            .parse()
            .unwrap();
        let aspect = field(&all, &format!("template.{index}.aspect"));
        assert!(!name.is_empty());
        assert!((1..=9).contains(&slots), "{name}: {slots} slots");
        assert!(
            aspect.contains(':'),
            "{name}: aspect {aspect} is not in W:H form"
        );
        if !slot_counts.contains(&slots) {
            slot_counts.push(slots);
        }
    }
    for wanted in 1..=9 {
        assert!(
            slot_counts.contains(&wanted),
            "no template with {wanted} slots in {slot_counts:?}"
        );
    }

    // Filtering by canvas shape returns only that group, and the same entries
    // the unfiltered list holds.
    for (ratio, expected) in [
        ("4:3", "mosaic-8-s14"),
        ("1:1", "grid-4-2x2"),
        ("16:9", "strip-3-3x1"),
    ] {
        let filtered = run(&["templates", "--aspect", ratio]);
        assert_eq!(code(&filtered), 0, "{ratio}: {}", stderr(&filtered));
        assert_eq!(field(&filtered, "aspect"), ratio);
        let filtered_count: usize = field(&filtered, "count").parse().unwrap();
        assert!(filtered_count > 0, "{ratio}: no template");
        let mut found = false;
        for index in 0..filtered_count {
            assert_eq!(field(&filtered, &format!("template.{index}.aspect")), ratio);
            if field(&filtered, &format!("template.{index}.name")) == expected {
                found = true;
            }
        }
        assert!(found, "{ratio}: {expected} missing from the group");
        assert!(filtered_count < count, "{ratio}: filter matched everything");
    }

    // A decimal is the same query as the ratio it names.
    let decimal = run(&["templates", "--aspect", "1.3333333333333333"]);
    assert_eq!(code(&decimal), 0, "{}", stderr(&decimal));
    assert_eq!(
        field(&decimal, "count"),
        field(&run(&["templates", "--aspect", "4:3"]), "count")
    );

    // A shape nothing was authored for is an empty list, not an error: the
    // picker asks with whatever the canvas is.
    let none = run(&["templates", "--aspect", "7:5"]);
    assert_eq!(code(&none), 0, "{}", stderr(&none));
    assert_eq!(field(&none, "count"), "0");
    assert!(stdout(&none).contains("status = ok"));

    // Same input, same bytes, and `--json` is the same data: the group the filter
    // returns is the library's own 4:3 group, in library order. The expectation is
    // read from `templates` rather than written as a literal, so adding a layout
    // does not invalidate this test (S10 added four 4:3 ones).
    assert_eq!(stdout(&all), stdout(&run(&["templates"])));
    let group: Vec<&str> = templates::names()
        .into_iter()
        .filter(|name| templates::get(name).is_some_and(|template| template.aspect == 4.0 / 3.0))
        .collect();
    assert!(
        group.contains(&"mosaic-8-s14"),
        "the smoke template's own group: {group:?}"
    );
    let json = run(&["templates", "--aspect", "4:3", "--json"]);
    let value: serde_json::Value = serde_json::from_str(&stdout(&json)).expect("valid JSON");
    assert_eq!(value["count"], group.len());
    for (index, name) in group.iter().enumerate() {
        assert_eq!(
            value[format!("template.{index}.name").as_str()],
            *name,
            "the JSON group drifted from the library"
        );
    }

    // Out-of-range and malformed ratios are usage errors with an empty stdout.
    for bad in ["0", "20", "4:0", "x:y", "", "4:3:2"] {
        let output = run(&["templates", "--aspect", bad]);
        assert_eq!(code(&output), 1, "--aspect {bad:?}");
        assert!(
            stdout(&output).is_empty(),
            "--aspect {bad:?} wrote to stdout"
        );
    }
}

#[test]
fn templates_filters_by_slot_count_which_is_the_gallery_s_query() {
    // S14: the layout gallery offers every layout with the photo count, and
    // `Selection::layouts` is that query. This flag is the same query from the
    // outside — `AGENTS.md`: nothing may be possible only in the GUI — so the
    // expectation is read from the core policy rather than written as a literal.
    for slots in 2..=9usize {
        let output = run(&["templates", "--slots", &slots.to_string()]);
        assert_eq!(code(&output), 0, "{slots}: {}", stderr(&output));
        assert_eq!(field(&output, "slots"), slots.to_string());
        let count: usize = field(&output, "count").parse().unwrap();
        assert!(count >= 3, "{slots} photos have {count} layouts");
        let listed: Vec<String> = (0..count)
            .map(|index| {
                let name = field(&output, &format!("template.{index}.name"));
                assert_eq!(
                    field(&output, &format!("template.{index}.slots")),
                    slots.to_string(),
                    "{name} is not a {slots}-slot layout"
                );
                name
            })
            .collect();
        // The order is the library's, and the set is `Selection::layouts`'s: the
        // picker's own query and this report cannot drift.
        let expected: Vec<String> = Selection::new(
            (0..slots)
                .map(|index| PathBuf::from(format!("/photos/{index}.jpg")))
                .collect(),
        )
        .expect("a selection inside the clamp")
        .layouts()
        .into_iter()
        .map(|template| template.name)
        .collect();
        assert_eq!(listed, expected, "{slots} slots");
    }

    // Count 1 is the exception to the "at least three layouts" rule and the reason
    // the loop above starts at two: since S19 a single photo is a legal collage and
    // its layout is the sheet itself, so the filter answers with exactly one name
    // (ruling 34).
    let one = run(&["templates", "--slots", "1"]);
    assert_eq!(code(&one), 0, "{}", stderr(&one));
    assert_eq!(field(&one, "slots"), "1");
    assert_eq!(field(&one, "count"), "1");
    assert_eq!(field(&one, "template.0.name"), "grid-1-1x1");
    assert_eq!(field(&one, "template.0.aspect"), "4:3");

    // The two filters combine, and the shape filter is still the library's own.
    let both = run(&["templates", "--slots", "5", "--aspect", "4:3"]);
    assert_eq!(code(&both), 0, "{}", stderr(&both));
    assert_eq!(field(&both, "count"), "1");
    assert_eq!(field(&both, "template.0.name"), "mosaic-5-hero");

    // A count outside the format's range is a usage error with an empty stdout: no
    // layout has zero slots, and ten left the library with S12c.
    for bad in ["0", "10", "-2", "x"] {
        let output = run(&["templates", "--slots", bad]);
        assert_eq!(code(&output), 1, "--slots {bad}");
        assert!(stdout(&output).is_empty(), "--slots {bad} wrote to stdout");
    }
    // And the filter is `templates`'s alone.
    let misplaced = run(&["scan", "--dir", ".", "--slots", "5"]);
    assert_eq!(code(&misplaced), 1);
    assert!(
        stderr(&misplaced).contains("--slots"),
        "{}",
        stderr(&misplaced)
    );
}

#[test]
fn init_writes_a_project_that_loads_back() {
    let dir = out_dir("init");
    let path = dir.join("new.pixlay");
    let output = run(&[
        "init",
        "--template",
        "mosaic-8-s14",
        "--out",
        path.to_str().unwrap(),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(stderr(&output).is_empty(), "{}", stderr(&output));
    assert_eq!(field(&output, "status"), "ok");
    assert_eq!(field(&output, "template"), "mosaic-8-s14");
    assert_eq!(field(&output, "cells"), "8");
    assert_eq!(
        std::fs::metadata(&path).expect("stat").len().to_string(),
        field(&output, "bytes")
    );

    // The file is a real project, not a shape that merely looks like one: it
    // loads through the same loader `render --project` uses, its template is the
    // one the library ships, and every cell is empty.
    let project = Project::load(&path).expect("the written project loads");
    let doc = project.doc();
    let shipped = templates::get("mosaic-8-s14").expect("registered");
    assert_eq!(doc.template, shipped);
    assert_eq!(doc.cells.len(), doc.template.slots.len());
    assert!(doc.cells.iter().all(|cell| cell.source.is_none()));
    assert_eq!(doc.doc_version, pixlay_core::DOC_VERSION);
    assert!(
        project
            .sources()
            .expect("no missing files")
            .iter()
            .all(Option::is_none)
    );

    // And it renders: an empty project renders a blank sheet, which is the
    // documented behavior of an empty cell.
    let rendered = run(&[
        "render",
        "--project",
        path.to_str().unwrap(),
        "--long-edge",
        "708",
        "--out",
        dir.join("new.png").to_str().unwrap(),
    ]);
    assert_eq!(code(&rendered), 0, "{}", stderr(&rendered));
    assert_eq!(field(&rendered, "occupied"), "0");

    // `init` never overwrites: the second run fails with the path named, and the
    // file is untouched.
    let before = std::fs::read_to_string(&path).expect("read");
    let again = run(&[
        "init",
        "--template",
        "mosaic-8-s14",
        "--out",
        path.to_str().unwrap(),
    ]);
    assert_eq!(code(&again), 2, "{}", stderr(&again));
    assert!(
        stdout(&again).is_empty(),
        "stdout must stay empty on failure"
    );
    assert!(stderr(&again).contains("new.pixlay"), "{}", stderr(&again));
    assert_eq!(std::fs::read_to_string(&path).expect("read"), before);

    // Same input, same bytes: init carries no timestamp.
    let other = dir.join("other.pixlay");
    let first = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    run(&[
        "init",
        "--template",
        "mosaic-8-s14",
        "--out",
        path.to_str().unwrap(),
    ]);
    assert_eq!(std::fs::read_to_string(&path).expect("read"), first);

    // A different template is a different project, and every template the
    // library lists inits successfully.
    run(&[
        "init",
        "--template",
        "grid-4-2x2",
        "--out",
        other.to_str().unwrap(),
    ]);
    let other_doc = CollageDoc::load(&other).expect("loads");
    assert_eq!(other_doc.template.slots.len(), 4);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn init_and_templates_keep_the_usage_and_locale_rules() {
    let dir = out_dir("init-usage");

    // Usage errors: unknown template and a non-.pixlay output, both exit 1 with
    // an empty stdout. The unknown name lists what the build knows.
    let unknown = run(&[
        "init",
        "--template",
        "nope",
        "--out",
        dir.join("x.pixlay").to_str().unwrap(),
    ]);
    assert_eq!(code(&unknown), 1);
    assert!(stdout(&unknown).is_empty());
    assert!(
        stderr(&unknown).contains("mosaic-8-s14"),
        "{}",
        stderr(&unknown)
    );

    let bad_extension = run(&[
        "init",
        "--template",
        "mosaic-8-s14",
        "--out",
        dir.join("x.json").to_str().unwrap(),
    ]);
    assert_eq!(code(&bad_extension), 1);
    assert!(stdout(&bad_extension).is_empty());
    assert!(
        stderr(&bad_extension).contains("x.json"),
        "{}",
        stderr(&bad_extension)
    );

    // Missing required flags, and a flag that belongs to another subcommand.
    for args in [
        vec!["init", "--out", "x.pixlay"],
        vec!["init", "--template", "mosaic-8-s14"],
        vec!["templates", "--out", "x.png"],
        vec!["templates", "--dpi", "300"],
        vec![
            "init",
            "--template",
            "mosaic-8-s14",
            "--out",
            "x.pixlay",
            "--stats",
        ],
    ] {
        let output = run(&args);
        assert_eq!(code(&output), 1, "{args:?}: {}", stderr(&output));
        assert!(stdout(&output).is_empty(), "{args:?} wrote to stdout");
        assert!(!stderr(&output).is_empty(), "{args:?} said nothing");
    }

    // stdout and stderr are byte-identical under any locale, on the success and
    // the failure branch alike.
    let out = dir.join("locale.pixlay");
    for (lang, all) in [
        ("C", "C"),
        ("zh_CN.UTF-8", "zh_CN.UTF-8"),
        ("de_DE.UTF-8", "de_DE.UTF-8"),
    ] {
        let listed = run_in(&["templates", "--json"], None, Some((lang, all)));
        assert_eq!(code(&listed), 0, "{lang}: {}", stderr(&listed));
        assert!(!listed.stdout.is_empty());
        if lang == "C" {
            std::fs::write(dir.join("reference.json"), &listed.stdout).expect("write");
        } else {
            assert_eq!(
                listed.stdout,
                std::fs::read(dir.join("reference.json")).expect("read"),
                "templates changed under LANG={lang}"
            );
        }
        let _ = std::fs::remove_file(&out);
        let created = run_in(
            &[
                "init",
                "--template",
                "strip-2-2x1",
                "--out",
                out.to_str().unwrap(),
            ],
            None,
            Some((lang, all)),
        );
        assert_eq!(code(&created), 0, "{lang}: {}", stderr(&created));
        assert!(created.stderr.is_empty());
    }
    // The failure branch: a relative path keeps the message free of machine text.
    let mut messages = Vec::new();
    for (lang, all) in [("C", "C"), ("zh_CN.UTF-8", "zh_CN.UTF-8")] {
        let output = run_in(
            &["init", "--template", "mosaic-8-s14", "--out", "x.png"],
            Some(&dir),
            Some((lang, all)),
        );
        assert_eq!(code(&output), 1);
        assert!(stdout(&output).is_empty());
        messages.push(stderr(&output));
    }
    assert_eq!(messages[0], messages[1], "{messages:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The directory the committed fixtures live in (`photos/`).
fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

// ---------------------------------------------------------------------------
// S6.5: hit testing and saving a project
// ---------------------------------------------------------------------------

/// Writes a project through `init`, so the tests use the geometry the library
/// actually ships rather than one they build themselves.
fn init_project(dir: &Path, name: &str, template: &str) -> PathBuf {
    let path = dir.join(name);
    let output = run(&[
        "init",
        "--template",
        template,
        "--out",
        path.to_str().expect("utf-8 path"),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    path
}

#[test]
fn hit_answers_which_slot_a_point_falls_in() {
    let dir = out_dir("hit");
    let project = init_project(&dir, "hit.pixlay", "mosaic-8-s14");
    let template = templates::get("mosaic-8-s14").expect("registered");

    // The declared slot centroids, computed here rather than asked of the hit
    // test, so the answer is checked against the geometry and not itself.
    let centroid = |index: usize| {
        let points = &template.slots[index].outline.points;
        let (mut area, mut x, mut y) = (0.0, 0.0, 0.0);
        for i in 0..points.len() {
            let a = points[i];
            let b = points[(i + 1) % points.len()];
            let cross = a.x * b.y - b.x * a.y;
            area += cross;
            x += (a.x + b.x) * cross;
            y += (a.y + b.y) * cross;
        }
        (x / (3.0 * area), y / (3.0 * area))
    };

    for index in 0..template.slots.len() {
        let (x, y) = centroid(index);
        let at = format!("{x:.4},{y:.4}");
        let output = run(&[
            "hit",
            "--project",
            project.to_str().expect("utf-8 path"),
            "--at",
            &at,
        ]);
        assert_eq!(code(&output), 0, "{}", stderr(&output));
        assert_eq!(field(&output, "slot"), index.to_string(), "centroid {at}");
        assert_eq!(field(&output, "hit"), "true");
        assert_eq!(field(&output, "slots"), "8");
        assert_eq!(field(&output, "template"), "mosaic-8-s14");
        assert_eq!(field(&output, "at"), at);
    }

    // The concave slot is the case a bounding box would get wrong: slot 6 is an L
    // whose notch belongs to slot 7, and (0.9, 0.9) is inside that notch.
    let notch = run(&[
        "hit",
        "--project",
        project.to_str().expect("utf-8 path"),
        "--at",
        "0.9,0.9",
    ]);
    assert_eq!(code(&notch), 0);
    assert_eq!(field(&notch, "slot"), "7");

    // The gutter template's middle belongs to no slot, and that is an answer with
    // exit code 0, not a failure.
    let gutter = run(&["hit", "--template", "grid-4-2x2g", "--at", "0.5,0.5"]);
    assert_eq!(code(&gutter), 0, "{}", stderr(&gutter));
    assert_eq!(field(&gutter, "hit"), "false");
    assert_eq!(field(&gutter, "slot"), "none");
    assert!(stderr(&gutter).is_empty());
    let cell = run(&["hit", "--template", "grid-4-2x2g", "--at", "0.2,0.2"]);
    assert_eq!(field(&cell, "slot"), "0");

    // `--json` is the same data in one object.
    let json = run(&[
        "hit",
        "--template",
        "strip-2-2x1",
        "--at",
        "0.25,0.5",
        "--json",
    ]);
    assert_eq!(code(&json), 0);
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&json)).expect("parses");
    assert_eq!(parsed["slot"], 0);
    assert_eq!(parsed["hit"], true);
    assert_eq!(parsed["slots"], 2);

    // Hit testing is geometry: a project whose photos have moved still answers,
    // because nothing here decodes one.
    let mut doc = CollageDoc::load(&project).expect("loads");
    doc.cells[0].source = Some(PathBuf::from("photos/nowhere.png"));
    let moved = dir.join("moved.pixlay");
    std::fs::write(&moved, doc.to_json().expect("serializes")).expect("write");
    let (x, y) = centroid(0);
    let answer = run(&[
        "hit",
        "--project",
        moved.to_str().expect("utf-8 path"),
        "--at",
        &format!("{x:.4},{y:.4}"),
    ]);
    assert_eq!(code(&answer), 0, "{}", stderr(&answer));
    assert_eq!(field(&answer, "slot"), "0");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn save_writes_the_project_that_was_read() {
    let dir = out_dir("save");
    let project = init_project(&dir, "a.pixlay", "mosaic-8-s14");

    // A copy beside the original is the same bytes: same directory, so no path
    // inside it needs rewriting, and the writer is the document's one writer.
    let copy = dir.join("b.pixlay");
    let output = run(&[
        "save",
        "--project",
        project.to_str().expect("utf-8 path"),
        "--out",
        copy.to_str().expect("utf-8 path"),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(stderr(&output).is_empty());
    assert_eq!(field(&output, "status"), "ok");
    assert_eq!(field(&output, "template"), "mosaic-8-s14");
    assert_eq!(field(&output, "version"), "1");
    assert_eq!(field(&output, "aspect"), "4:3");
    assert_eq!(field(&output, "cells"), "8");
    assert_eq!(
        std::fs::metadata(&copy).expect("stat").len().to_string(),
        field(&output, "bytes")
    );
    assert_eq!(
        std::fs::read(&copy).expect("read"),
        std::fs::read(&project).expect("read"),
        "a copy beside the original is byte-identical"
    );

    // Save, load, save: the second generation is still the same bytes.
    let again = dir.join("c.pixlay");
    let second = run(&[
        "save",
        "--project",
        copy.to_str().expect("utf-8 path"),
        "--out",
        again.to_str().expect("utf-8 path"),
    ]);
    assert_eq!(code(&second), 0, "{}", stderr(&second));
    assert_eq!(
        std::fs::read(&again).expect("read"),
        std::fs::read(&project).expect("read")
    );

    // Saving in place replaces the file with the same document.
    let in_place = run(&[
        "save",
        "--project",
        project.to_str().expect("utf-8 path"),
        "--out",
        project.to_str().expect("utf-8 path"),
    ]);
    assert_eq!(code(&in_place), 0, "{}", stderr(&in_place));
    assert!(Project::load(&project).is_ok());
    assert_eq!(
        std::fs::read(&project).expect("read"),
        std::fs::read(&again).expect("read")
    );

    // Unlike `init`, `save` replaces what is there: that is what saving is.
    let replaced = run(&[
        "save",
        "--project",
        copy.to_str().expect("utf-8 path"),
        "--out",
        project.to_str().expect("utf-8 path"),
    ]);
    assert_eq!(code(&replaced), 0, "{}", stderr(&replaced));
    assert!(Project::load(&project).is_ok());

    // A save into another directory keeps the photos findable: the relative source
    // is rewritten against the new home.
    let photos = dir.join("photos");
    std::fs::create_dir_all(&photos).expect("create photos");
    std::fs::write(
        photos.join("p.png"),
        include_bytes!("fixtures/photos/square.png"),
    )
    .expect("write photo");
    let mut doc = CollageDoc::load(&project).expect("loads");
    doc.cells[0].source = Some(PathBuf::from("photos/p.png"));
    std::fs::write(&project, doc.to_json().expect("serializes")).expect("write");
    let elsewhere = dir.join("copies");
    std::fs::create_dir_all(&elsewhere).expect("create copies");
    let moved = elsewhere.join("moved.pixlay");
    let rebased = run(&[
        "save",
        "--project",
        project.to_str().expect("utf-8 path"),
        "--out",
        moved.to_str().expect("utf-8 path"),
    ]);
    assert_eq!(code(&rebased), 0, "{}", stderr(&rebased));
    let copy = Project::load(&moved).expect("the copy loads");
    assert_eq!(
        copy.doc().cells[0].source,
        Some(PathBuf::from("../photos/p.png"))
    );
    assert_eq!(
        std::fs::canonicalize(
            copy.sources()
                .expect("the copy's photos resolve")
                .first()
                .expect("a cell")
                .as_ref()
                .expect("a source")
        )
        .expect("canonicalize"),
        std::fs::canonicalize(photos.join("p.png")).expect("canonicalize")
    );

    // No temporary file is left behind anywhere.
    for entry in std::fs::read_dir(&dir).expect("read") {
        let name = entry
            .expect("entry")
            .file_name()
            .to_string_lossy()
            .into_owned();
        assert!(!name.ends_with(".tmp"), "a temporary file survived: {name}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn save_reports_a_missing_project_and_a_newer_version() {
    let dir = out_dir("save-errors");

    let missing = run(&[
        "save",
        "--project",
        dir.join("absent.pixlay").to_str().expect("utf-8 path"),
        "--out",
        dir.join("out.pixlay").to_str().expect("utf-8 path"),
    ]);
    assert_eq!(code(&missing), 2, "{}", stderr(&missing));
    assert!(stdout(&missing).is_empty(), "stdout must stay empty");
    assert!(
        stderr(&missing).contains("absent.pixlay"),
        "the message must name the file: {}",
        stderr(&missing)
    );
    assert!(!dir.join("out.pixlay").exists(), "nothing was written");

    // A document from a newer version is refused, not guessed at or rewritten.
    let project = init_project(&dir, "newer.pixlay", "strip-2-2x1");
    let json = std::fs::read_to_string(&project)
        .expect("read")
        .replace("\"docVersion\": 3", "\"docVersion\": 4");
    std::fs::write(&project, json).expect("write");
    let newer = run(&[
        "save",
        "--project",
        project.to_str().expect("utf-8 path"),
        "--out",
        dir.join("other.pixlay").to_str().expect("utf-8 path"),
    ]);
    assert_eq!(code(&newer), 2, "{}", stderr(&newer));
    assert!(stdout(&newer).is_empty());
    assert!(
        stderr(&newer).contains("newer than the supported version"),
        "{}",
        stderr(&newer)
    );
    // A version refusal is read from the file too, so it names the file (S15h,
    // PIX-021) — not only serde's failures do.
    assert!(
        stderr(&newer).contains(project.to_str().expect("utf-8 path")),
        "the refusal must name the project: {}",
        stderr(&newer)
    );
    assert!(!dir.join("other.pixlay").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

/// S15g: a project whose *own* geometry is broken is refused wherever a document is
/// read, before anything is decoded or written (PIX-007, ruled 2026-09-24).
#[test]
fn a_project_whose_own_geometry_is_broken_is_refused_with_the_reason() {
    let dir = out_dir("topology");
    // Slot 1 moved onto slot 0: the overlap a hand edit leaves behind. The two
    // areas still sum to 1.0, so it is the geometry that refuses this project and
    // not the sum.
    let mut doc = CollageDoc::new(templates::get("strip-2-2x1").expect("registered"));
    doc.template.slots[1] = doc.template.slots[0].clone();
    let project = dir.join("overlap.pixlay");
    std::fs::write(&project, doc.to_json().expect("serializes")).expect("write");
    let path = project.to_str().expect("utf-8 path");
    let out = dir.join("out.png");
    let out_path = out.to_str().expect("utf-8 path");

    let hit = run(&["hit", "--project", path, "--at", "0.5,0.5"]);
    assert_eq!(code(&hit), 2, "{}", stderr(&hit));
    assert!(stdout(&hit).is_empty(), "stdout must stay empty");
    assert!(
        stderr(&hit).contains("slots 0 and 1 overlap"),
        "the reason must name the pair: {}",
        stderr(&hit)
    );

    let render = run(&[
        "render",
        "--project",
        path,
        "--long-edge",
        "64",
        "--out",
        out_path,
    ]);
    assert_eq!(code(&render), 2, "{}", stderr(&render));
    assert!(stdout(&render).is_empty(), "stdout must stay empty");
    assert!(
        stderr(&render).contains("slots 0 and 1 overlap"),
        "{}",
        stderr(&render)
    );
    assert!(!out.exists(), "a refused render writes nothing");
    let _ = std::fs::remove_dir_all(&dir);
}

/// `hit` and `save` keep the S1 rules: flags that belong elsewhere are refused,
/// and no locale changes a byte of either stream.
#[test]
fn hit_and_save_keep_the_usage_and_locale_rules() {
    let dir = out_dir("hit-save-usage");
    let project = init_project(&dir, "u.pixlay", "strip-2-2x1");
    let path = project.to_str().expect("utf-8 path");

    for args in [
        vec!["hit", "--project", path],
        vec!["hit", "--at", "0.5,0.5"],
        vec![
            "hit",
            "--project",
            path,
            "--template",
            "strip-2-2x1",
            "--at",
            "0.5,0.5",
        ],
        vec!["hit", "--template", "nope", "--at", "0.5,0.5"],
        vec!["hit", "--project", path, "--at", "0.5"],
        vec!["hit", "--project", path, "--at", "0.5,1.5"],
        vec!["hit", "--project", path, "--at", "nan,0.5"],
        vec!["hit", "--project", path, "--at", "0.5,0.5", "--dpi", "300"],
        vec!["hit", "--project", path, "--at", "0.5,0.5", "--stats"],
        vec![
            "hit",
            "--project",
            path,
            "--at",
            "0.5,0.5",
            "--out",
            "x.png",
        ],
        vec!["save"],
        vec!["save", "--project", path],
        vec![
            "save",
            "--project",
            path,
            "--out",
            path,
            "--json",
            "--stats",
        ],
        vec![
            "save",
            "--project",
            path,
            "--out",
            path,
            "--template",
            "strip-2-2x1",
        ],
        vec!["save", "--template", "strip-2-2x1", "--out", path],
        vec!["save", "--project", path, "--out", "x.json"],
        vec![
            "save",
            "--project",
            path,
            "--out",
            "x.pixlay",
            "--long-edge",
            "300",
        ],
        vec!["templates", "--at", "0.5,0.5"],
        vec![
            "render",
            "--template",
            "strip-2-2x1",
            "--long-edge",
            "1000",
            "--out",
            "x.png",
            "--at",
            "0.5,0.5",
        ],
    ] {
        let output = run(&args);
        assert_eq!(code(&output), 1, "{args:?}: {}", stderr(&output));
        assert!(stdout(&output).is_empty(), "{args:?} wrote to stdout");
        assert!(!stderr(&output).is_empty(), "{args:?} said nothing");
    }
    // The unknown-template message lists what this build knows.
    let unknown = run(&["hit", "--template", "nope", "--at", "0.5,0.5"]);
    assert!(
        stderr(&unknown).contains("mosaic-8-s14"),
        "{}",
        stderr(&unknown)
    );

    // Byte-identical under any locale, on the success and the failure branch.
    let copy = dir.join("locale.pixlay");
    let mut success = Vec::new();
    let mut failure = Vec::new();
    for (lang, all) in [
        ("C", "C"),
        ("zh_CN.UTF-8", "zh_CN.UTF-8"),
        ("de_DE.UTF-8", "de_DE.UTF-8"),
    ] {
        let hit = run_in(
            &["hit", "--project", path, "--at", "0.25,0.5", "--json"],
            None,
            Some((lang, all)),
        );
        assert_eq!(code(&hit), 0, "{lang}: {}", stderr(&hit));
        success.push(hit.stdout.clone());

        let _ = std::fs::remove_file(&copy);
        let saved = run_in(
            &[
                "save",
                "--project",
                path,
                "--out",
                copy.to_str().expect("utf-8 path"),
            ],
            None,
            Some((lang, all)),
        );
        assert_eq!(code(&saved), 0, "{lang}: {}", stderr(&saved));

        // The failure branch: a relative path keeps the message free of machine
        // text.
        let broken = run_in(
            &["hit", "--project", "absent.pixlay", "--at", "0.5,0.5"],
            Some(&dir),
            Some((lang, all)),
        );
        assert_eq!(code(&broken), 2);
        assert!(stdout(&broken).is_empty());
        failure.push(broken.stderr.clone());
    }
    assert!(
        success.iter().all(|stdout| stdout == &success[0]),
        "hit changed under a locale"
    );
    assert!(
        failure.iter().all(|stderr| stderr == &failure[0]),
        "hit's failure message changed under a locale"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// S11: the free rotation, and the frame
// ---------------------------------------------------------------------------

/// A two-cell project with real photos, so `edit` has a photo aspect to fit against.
fn framing_project(dir: &Path, name: &str) -> PathBuf {
    let photos = dir.join("photos");
    std::fs::create_dir_all(&photos).expect("create photos");
    std::fs::write(
        photos.join("wide.jpg"),
        include_bytes!("fixtures/photos/landscape.jpg"),
    )
    .expect("write photo");
    std::fs::write(
        photos.join("tall.jpg"),
        include_bytes!("fixtures/photos/portrait.jpg"),
    )
    .expect("write photo");
    let project = dir.join(name);
    let output = run(&[
        "init",
        "--template",
        "strip-2-2x1",
        "--photo",
        photos.join("wide.jpg").to_str().expect("utf-8"),
        "--photo",
        photos.join("tall.jpg").to_str().expect("utf-8"),
        "--out",
        project.to_str().expect("utf-8"),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(project.is_file());
    project
}

/// One cell's crop as the file now holds it.
fn stored_crop(project: &Path, slot: usize) -> CropTransform {
    CollageDoc::load(project)
        .unwrap_or_else(|error| panic!("{}: {error}", project.display()))
        .cells[slot]
        .crop
}

#[test]
fn edit_stores_the_fit_of_what_was_asked_for() {
    // A crop is a request; what gets drawn is what covers the cell. `edit` is the
    // command that writes a framing, so it writes the *fit*: the file says what the
    // renderer will draw, and a rotation is stored with the zoom it needs.
    let dir = out_dir("edit-fit");
    let project = framing_project(&dir, "a.pixlay");
    let out = dir.join("b.pixlay");
    let path = project.to_str().expect("utf-8 path");
    let output = run(&[
        "edit",
        "--project",
        path,
        "--slot",
        "0",
        "--rotate",
        "25",
        "--zoom",
        "3.5",
        "--offset",
        "0.2,0",
        "--out",
        out.to_str().expect("utf-8"),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(stderr(&output).is_empty());
    assert_eq!(field(&output, "command"), "edit");
    assert_eq!(field(&output, "template"), "strip-2-2x1");
    assert_eq!(field(&output, "cells"), "2");
    assert_eq!(field(&output, "photos"), "2");
    assert_eq!(field(&output, "slot"), "0");
    assert_eq!(field(&output, "occupied"), "true");
    assert_eq!(field(&output, "gap"), "0.000000");
    assert_eq!(field(&output, "radius"), "0.000000");
    assert_eq!(field(&output, "border"), "255,255,255");
    assert_eq!(field(&output, "rotation_deg"), "25.000000");

    let crop = stored_crop(&out, 0);
    // The angle is kept exactly — and this cell is the worst case for that: a
    // portrait slot with a landscape photo, where the *upright* floor alone is
    // 2.37x and 25 degrees asks for 2.90x.
    assert_eq!(crop.rotation_deg, 25.0);
    // A request above the floor is kept as it stands, never pulled back to it.
    assert_eq!(crop.zoom, 3.5, "the user's own zoom must survive");
    // And the pan survives as given: at 3.5x the photo has the room for it. The
    // clamp only acts where it has to — `edit_is_idempotent_on_the_fit` asks for a
    // pan that does not fit and gets pulled back.
    assert_eq!(crop.offset, (0.2, 0.0));
    let reported = field(&output, "zoom").parse::<f64>().expect("a number");
    assert!(
        (crop.zoom - reported).abs() <= 1e-6,
        "the report says {reported}, the file says {}",
        crop.zoom
    );
    assert_eq!(stored_crop(&out, 1), CropTransform::IDENTITY);

    // The written file renders: a document that says one thing and draws another
    // would be the failure this test exists for.
    let image = dir.join("out.png");
    let render = run(&[
        "render",
        "--project",
        out.to_str().expect("utf-8"),
        "--long-edge",
        "708",
        "--out",
        image.to_str().expect("utf-8"),
    ]);
    assert_eq!(code(&render), 0, "{}", stderr(&render));
    assert_eq!(field(&render, "occupied"), "2");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn edit_is_idempotent_on_the_fit() {
    // The property S3 established for the clamp, re-asserted through the new entry
    // point: fitting a fit returns it bit for bit, so the same edit twice writes the
    // same bytes. A second pass that moved the numbers would make every re-run of a
    // tool a new revision of the user's project.
    let dir = out_dir("edit-idempotent");
    let project = framing_project(&dir, "a.pixlay");
    let once = dir.join("once.pixlay");
    let twice = dir.join("twice.pixlay");
    let edit = |from: &Path, to: &Path, extra: &[&str]| {
        let mut args = vec![
            "edit".to_string(),
            "--project".to_string(),
            from.display().to_string(),
            "--out".to_string(),
            to.display().to_string(),
        ];
        args.extend(extra.iter().map(|s| s.to_string()));
        let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
        let output = run(&borrowed);
        assert_eq!(code(&output), 0, "{extra:?}: {}", stderr(&output));
    };

    // A rotation that has to be paid for, a pan that has to be clamped, and a zoom
    // below the floor: every lever the clamp owns.
    let framing = [
        "--slot", "1", "--rotate", "137.5", "--zoom", "0.4", "--offset", "-0.9,0.9",
    ];
    edit(&project, &once, &framing);
    edit(&once, &twice, &framing);
    assert_eq!(
        std::fs::read(&once).expect("read"),
        std::fs::read(&twice).expect("read"),
        "the second edit moved the framing"
    );
    // And the numbers really are the fit's: the pan was clamped, the zoom raised.
    let crop = stored_crop(&once, 1);
    assert_eq!(crop.rotation_deg, 137.5);
    assert!(crop.zoom >= 0.4, "the fit raised the zoom: {}", crop.zoom);
    assert_ne!(
        crop.offset,
        (-0.9, 0.9),
        "a pan this far out cannot survive, so the fit had to move it"
    );
    assert!(crop.offset.0.abs() <= 0.9 && crop.offset.1.abs() <= 0.9);

    // The same for a frame: setting the same frame twice is the same document.
    let framed = dir.join("framed.pixlay");
    let framed_again = dir.join("framed-again.pixlay");
    let frame = [
        "--gap",
        "0.03",
        "--radius",
        "0.04",
        "--border-color",
        "10,20,30",
    ];
    edit(&project, &framed, &frame);
    edit(&framed, &framed_again, &frame);
    assert_eq!(
        std::fs::read(&framed).expect("read"),
        std::fs::read(&framed_again).expect("read")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_combined_frame_and_framing_edit_is_fitted_against_the_frame_it_writes() {
    // PIX-009. `edit` fitted a crop against the document's **old** frame and applied
    // the requested frame afterwards, so `--slot i --zoom z --gap g` wrote a crop
    // fitted for a frame the file does not carry: `draw` refits it against the frame
    // that *is* in the file, and the file stops holding the fit it claims. Both
    // consequences are checkable, and both are checked here: the stored crop is the
    // fit of the frame in the same file, and the same edit twice writes the same
    // bytes (with the old order the second run fitted against the frame the first had
    // written, and moved the crop).
    let dir = out_dir("edit-frame-fit");
    let project = framing_project(&dir, "a.pixlay");
    let once = dir.join("once.pixlay");
    let twice = dir.join("twice.pixlay");
    let combined = [
        "--slot", "0", "--zoom", "0.8", "--rotate", "30", "--gap", "0.06", "--radius", "0.02",
    ];
    let edit = |from: &Path, to: &Path| {
        let mut args = vec![
            "edit",
            "--project",
            from.to_str().expect("utf-8"),
            "--out",
            to.to_str().expect("utf-8"),
        ];
        args.extend(combined);
        let output = run(&args);
        assert_eq!(code(&output), 0, "{combined:?}: {}", stderr(&output));
    };
    edit(&project, &once);
    edit(&once, &twice);
    assert_eq!(
        std::fs::read(&once).expect("read"),
        std::fs::read(&twice).expect("read"),
        "the second combined edit moved the framing"
    );

    // The fit in the file is the fit of the frame in the same file: recomputed here
    // through the same reference `draw` uses (`CollageDoc::fit_crop`), from the
    // request the command was given.
    let doc = CollageDoc::load(&once).expect("the edited project loads");
    assert_eq!(doc.frame.gap_rel, 0.06, "the frame came with the edit");
    let sources = Project::load(&once)
        .expect("loads")
        .sources()
        .expect("resolves");
    let photo = sources[0].clone().expect("cell 0 has a photo");
    let source = pixlay_imaging::Source::decode(&photo).expect("decode");
    let request = CropTransform {
        zoom: 0.8,
        offset: (0.0, 0.0),
        rotation_deg: 30.0,
    }
    .normalized();
    let framed = doc
        .fit_crop(0, request, doc.template.aspect, source.aspect())
        .expect("fits")
        .transform;
    assert_eq!(
        doc.cells[0].crop, framed,
        "the file does not hold the fit of its own frame"
    );
    // And the unframed fit is a *different* transform — the one the old order
    // stored — so this test means something only while the gap moves the fit.
    let mut unframed = doc.clone();
    unframed.frame.gap_rel = 0.0;
    let bare = unframed
        .fit_crop(0, request, unframed.template.aspect, source.aspect())
        .expect("fits")
        .transform;
    assert_ne!(
        framed, bare,
        "the gap has to move the fit for this test to be about anything"
    );
    assert_ne!(doc.cells[0].crop, bare);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn edit_wraps_a_free_rotation_into_one_turn() {
    // A dial does not accumulate turns: any finite angle is accepted, and what the
    // document stores is the equivalent one in `(-180, 180]`, so a project never
    // says 450 degrees.
    let dir = out_dir("edit-wrap");
    let project = framing_project(&dir, "a.pixlay");
    let path = project.to_str().expect("utf-8 path");

    for (given, expected) in [
        ("450", 90.0),
        ("-450", -90.0),
        ("180", 180.0),
        ("-180", 180.0),
        ("181", -179.0),
        ("-179.9", -179.9),
    ] {
        let out = dir.join("wrapped.pixlay");
        let _ = std::fs::remove_file(&out);
        let output = run(&[
            "edit",
            "--project",
            path,
            "--slot",
            "0",
            "--rotate",
            given,
            "--out",
            out.to_str().expect("utf-8"),
        ]);
        assert_eq!(code(&output), 0, "{given}: {}", stderr(&output));
        assert_eq!(
            stored_crop(&out, 0).rotation_deg,
            expected,
            "{given} degrees"
        );
    }

    // A project that says a far-out angle is wrapped when it is loaded, so a load →
    // save round trip normalizes it without touching the picture (the angle is
    // periodic, and `fit` uses its sine and cosine).
    let json = std::fs::read_to_string(&project).expect("read");
    let poked = dir.join("poked.pixlay");
    std::fs::write(
        &poked,
        json.replace("\"rotationDeg\": 0.0", "\"rotationDeg\": 730.5"),
    )
    .expect("write");
    let poked_doc = CollageDoc::load(&poked).expect("730.5 degrees loads");
    assert_eq!(
        poked_doc.cells[0].crop.rotation_deg, 10.5,
        "loading wraps the angle, so the document never holds 730.5"
    );
    let out = dir.join("normalized.pixlay");
    let output = run(&[
        "edit",
        "--project",
        poked.to_str().expect("utf-8"),
        "--gap",
        "0.01",
        "--out",
        out.to_str().expect("utf-8"),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    let doc = CollageDoc::load(&out).expect("loads");
    for cell in &doc.cells {
        assert_eq!(cell.crop.rotation_deg, 10.5, "730.5 wraps to 10.5");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn edit_clears_a_cell_and_keeps_the_others() {
    let dir = out_dir("edit-clear");
    let project = framing_project(&dir, "a.pixlay");
    let out = dir.join("cleared.pixlay");
    let before = CollageDoc::load(&project).expect("loads");
    let output = run(&[
        "edit",
        "--project",
        project.to_str().expect("utf-8"),
        "--slot",
        "0",
        "--clear",
        "--out",
        out.to_str().expect("utf-8"),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "occupied"), "false");
    assert_eq!(field(&output, "photos"), "1");
    let after = CollageDoc::load(&out).expect("loads");
    assert_eq!(
        after.cells[0],
        Cell::default(),
        "the cell is empty and reset"
    );
    assert_eq!(
        after.cells[1].source, before.cells[1].source,
        "the other cell is untouched"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn edit_grows_and_shrinks_the_layout_one_cell_at_a_time() {
    // S14b's count control, from the outside: `--add-cell` takes the layout with one
    // slot more and leaves the new cell empty; `--remove-cell` takes the layout with
    // one slot fewer, dropping the last cell whatever it holds. Both are the window's
    // own `Command`s, so the two produce the same document.
    let dir = out_dir("edit-cells");
    let project = framing_project(&dir, "a.pixlay");
    let before = CollageDoc::load(&project).expect("loads");
    assert_eq!(before.cells.len(), 2);

    let grown = dir.join("grown.pixlay");
    let output = run(&[
        "edit",
        "--project",
        project.to_str().expect("utf-8"),
        "--add-cell",
        "--out",
        grown.to_str().expect("utf-8"),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "command"), "edit");
    assert_eq!(field(&output, "cells"), "3");
    assert_eq!(
        field(&output, "photos"),
        "2",
        "the new cell has no photo: `+` is a layout edit"
    );
    let after = CollageDoc::load(&grown).expect("loads");
    assert_eq!(
        after.cells[..2],
        before.cells[..],
        "the cells that survived keep their photo and framing"
    );
    assert!(
        after.cells[2].source.is_none(),
        "the appended cell is empty: {:?}",
        after.cells[2].source
    );
    assert_eq!(after.cells[2].crop, CropTransform::IDENTITY);
    assert_eq!(
        after.template.slots.len(),
        3,
        "and the layout has one more cell"
    );

    // `-` takes it away again, and the document is what it was.
    let shrunk = dir.join("shrunk.pixlay");
    let output = run(&[
        "edit",
        "--project",
        grown.to_str().expect("utf-8"),
        "--remove-cell",
        "--out",
        shrunk.to_str().expect("utf-8"),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "cells"), "2");
    let back = CollageDoc::load(&shrunk).expect("loads");
    assert_eq!(
        back.cells, before.cells,
        "add then remove lands on the document it started from"
    );

    // The floor: the two-cell layout drops to the one-cell sheet, and only that
    // refuses — one cell is the smallest layout the library has since S19.
    let floor = dir.join("floor.pixlay");
    let output = run(&[
        "edit",
        "--project",
        project.to_str().expect("utf-8"),
        "--remove-cell",
        "--out",
        floor.to_str().expect("utf-8"),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "cells"), "1");
    assert_eq!(field(&output, "template"), "grid-1-1x1");

    let refused = dir.join("refused.pixlay");
    let output = run(&[
        "edit",
        "--project",
        floor.to_str().expect("utf-8"),
        "--remove-cell",
        "--out",
        refused.to_str().expect("utf-8"),
    ]);
    assert_eq!(code(&output), 2, "there is no zero-cell layout");
    assert!(
        stderr(&output).contains("at least 1 cell"),
        "{}",
        stderr(&output)
    );
    assert!(!refused.exists());

    // The two are opposites: one edit, one intent.
    let both = run(&[
        "edit",
        "--project",
        project.to_str().expect("utf-8"),
        "--add-cell",
        "--remove-cell",
        "--out",
        dir.join("both.pixlay").to_str().expect("utf-8"),
    ]);
    assert_eq!(code(&both), 1, "{}", stderr(&both));
    assert!(stderr(&both).contains("--add-cell"), "{}", stderr(&both));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn edit_exchanges_two_cells_whole() {
    // S14b: "two photos must be swappable". The whole cell moves, so a photo keeps
    // the framing that made it look right where it was.
    let dir = out_dir("edit-swap");
    let project = framing_project(&dir, "a.pixlay");
    let framed = dir.join("framed.pixlay");
    // Framing on cell 0 only, so the swap has something to carry.
    let output = run(&[
        "edit",
        "--project",
        project.to_str().expect("utf-8"),
        "--slot",
        "0",
        "--rotate",
        "12",
        "--zoom",
        "1.8",
        "--out",
        framed.to_str().expect("utf-8"),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    let before = CollageDoc::load(&framed).expect("loads");

    let out = dir.join("swapped.pixlay");
    let output = run(&[
        "edit",
        "--project",
        framed.to_str().expect("utf-8"),
        "--swap",
        "0,1",
        "--out",
        out.to_str().expect("utf-8"),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "command"), "edit");
    assert_eq!(field(&output, "cells"), "2", "a swap changes no count");
    assert_eq!(field(&output, "photos"), "2");
    let after = CollageDoc::load(&out).expect("loads");
    assert_eq!(after.cells[0], before.cells[1], "cell 0 is what cell 1 was");
    assert_eq!(after.cells[1], before.cells[0], "and the other way round");
    assert_ne!(
        after.cells[1].crop,
        CropTransform::IDENTITY,
        "the framing travelled with its photo"
    );

    // Swapping the same pair back is the identity, which is what makes it a swap
    // rather than a rotation of the list.
    let back = dir.join("back.pixlay");
    let output = run(&[
        "edit",
        "--project",
        out.to_str().expect("utf-8"),
        "--swap",
        "0,1",
        "--out",
        back.to_str().expect("utf-8"),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    let round = CollageDoc::load(&back).expect("loads");
    assert_eq!(round.cells, before.cells, "two swaps are no swap");
    assert_eq!(
        CollageDoc::load(&framed)
            .expect("loads")
            .to_json()
            .expect("serializes"),
        round.to_json().expect("serializes"),
        "and the bytes agree, not just the fields"
    );

    // The two pairs that cannot be a swap: the command's own refusals, with the
    // document untouched.
    let same = run(&[
        "edit",
        "--project",
        framed.to_str().expect("utf-8"),
        "--swap",
        "1,1",
        "--out",
        dir.join("same.pixlay").to_str().expect("utf-8"),
    ]);
    assert_eq!(code(&same), 2, "{}", stderr(&same));
    assert!(
        stderr(&same).contains("swapped with itself"),
        "{}",
        stderr(&same)
    );

    let missing = run(&[
        "edit",
        "--project",
        framed.to_str().expect("utf-8"),
        "--swap",
        "0,9",
        "--out",
        dir.join("missing.pixlay").to_str().expect("utf-8"),
    ]);
    assert_eq!(code(&missing), 2, "{}", stderr(&missing));
    assert!(
        stderr(&missing).contains("slot 9 does not exist"),
        "{}",
        stderr(&missing)
    );

    // A malformed pair is a usage error, before anything is read.
    let malformed = run(&[
        "edit",
        "--project",
        framed.to_str().expect("utf-8"),
        "--swap",
        "0",
        "--out",
        dir.join("malformed.pixlay").to_str().expect("utf-8"),
    ]);
    assert_eq!(code(&malformed), 1, "{}", stderr(&malformed));
    assert!(stderr(&malformed).contains("i,j"), "{}", stderr(&malformed));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn edit_appends_a_photo_and_grows_the_layout_with_the_count() {
    // S14's `+`: the photo goes to the first empty cell, and if there is none the
    // layout grows by one slot — the same command the window's count control sends,
    // so the two produce the same document (the GUI test compares them).
    let dir = out_dir("edit-add");
    let project = framing_project(&dir, "a.pixlay");
    let extra = dir.join("photos/wide.jpg");
    assert!(
        extra.is_file(),
        "the fixture photo the project was built from"
    );

    let out = dir.join("added.pixlay");
    let output = run(&[
        "edit",
        "--project",
        project.to_str().expect("utf-8"),
        "--add-photo",
        extra.to_str().expect("utf-8"),
        "--out",
        out.to_str().expect("utf-8"),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "command"), "edit");
    assert_eq!(
        field(&output, "cells"),
        "3",
        "the layout grew with the count"
    );
    assert_eq!(field(&output, "photos"), "3");
    // `strip-2-2x1` is 3:2; the three-slot layouts are 16:9, 2:3 and 4:3, so the
    // count rule keeps the *family* — this is the same `layout_for` the window
    // uses, and the two are one implementation.
    assert_eq!(field(&output, "template"), "strip-3-3x1");

    let before = CollageDoc::load(&project).expect("loads");
    let after = CollageDoc::load(&out).expect("loads");
    assert_eq!(
        after.cells[..2],
        before.cells[..],
        "the cells that survived keep their photo and framing"
    );
    assert!(
        after.cells[2]
            .source
            .as_deref()
            .is_some_and(|source| source.ends_with("photos/wide.jpg")),
        "the appended cell shows the new photo: {:?}",
        after.cells[2].source
    );
    assert_eq!(after.template.slots.len(), 3);

    // Two photos at once, and the growth is per photo; a second photo that cannot
    // be read is refused before anything is written.
    let two = dir.join("two.pixlay");
    let output = run(&[
        "edit",
        "--project",
        project.to_str().expect("utf-8"),
        "--add-photo",
        extra.to_str().expect("utf-8"),
        "--add-photo",
        extra.to_str().expect("utf-8"),
        "--out",
        two.to_str().expect("utf-8"),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "cells"), "4");
    assert_eq!(field(&output, "template"), "strip-4-4x1");

    let missing = dir.join("missing.pixlay");
    let output = run(&[
        "edit",
        "--project",
        project.to_str().expect("utf-8"),
        "--add-photo",
        dir.join("nope.jpg").to_str().expect("utf-8"),
        "--out",
        missing.to_str().expect("utf-8"),
    ]);
    assert_eq!(
        code(&output),
        2,
        "a photo that is not there is not written in"
    );
    assert!(stderr(&output).contains("nope.jpg"), "{}", stderr(&output));
    assert!(!missing.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn edit_shrinks_the_layout_one_cell_at_a_time() {
    // The other half of the count control (S14b): `−` takes the layout with one
    // cell fewer, so the last *cell* goes whether or not it holds a photo — and
    // never below one, the floor `Selection` has had since S19.
    let dir = out_dir("edit-remove");
    let photos = dir.join("photos");
    std::fs::create_dir_all(&photos).expect("create photos");
    for (name, source) in [
        ("a.jpg", "landscape.jpg"),
        ("b.jpg", "portrait.jpg"),
        ("c.jpg", "square.png"),
        ("d.jpg", "dated.jpg"),
    ] {
        let bytes: &[u8] = match source {
            "landscape.jpg" => include_bytes!("fixtures/photos/landscape.jpg"),
            "portrait.jpg" => include_bytes!("fixtures/photos/portrait.jpg"),
            "square.png" => include_bytes!("fixtures/photos/square.png"),
            _ => include_bytes!("fixtures/photos/dated.jpg"),
        };
        std::fs::write(photos.join(name), bytes).expect("write photo");
    }
    let project = dir.join("four.pixlay");
    let mut init = vec![
        "init",
        "--template",
        "strip-4-4x1",
        "--out",
        project.to_str().expect("utf-8"),
    ];
    let photo_args: Vec<String> = ["a.jpg", "b.jpg", "c.jpg", "d.jpg"]
        .iter()
        .map(|name| photos.join(name).to_str().expect("utf-8").to_string())
        .collect();
    for path in &photo_args {
        init.push("--photo");
        init.push(path);
    }
    let output = run(&init);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    let before = CollageDoc::load(&project).expect("loads");
    assert_eq!(before.cells.len(), 4);

    let out = dir.join("three.pixlay");
    let output = run(&[
        "edit",
        "--project",
        project.to_str().expect("utf-8"),
        "--remove-cell",
        "--out",
        out.to_str().expect("utf-8"),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "cells"), "3");
    assert_eq!(field(&output, "photos"), "3");
    assert_eq!(field(&output, "template"), "strip-3-3x1", "16:9 stays 16:9");
    let after = CollageDoc::load(&out).expect("loads");
    assert_eq!(
        after.cells,
        before.cells[..3],
        "the survivors are the first three cells, unchanged"
    );

    // Two cells — whatever their *photo* count: the control moves the layout, and
    // a cell that holds no photo is still a cell of it. One of the two is emptied
    // on its own to say exactly that.
    let two = dir.join("two.pixlay");
    let output = run(&[
        "edit",
        "--project",
        out.to_str().expect("utf-8"),
        "--remove-cell",
        "--out",
        two.to_str().expect("utf-8"),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "template"), "strip-2-2x1");

    let holed = dir.join("holed.pixlay");
    let output = run(&[
        "edit",
        "--project",
        two.to_str().expect("utf-8"),
        "--slot",
        "1",
        "--clear",
        "--out",
        holed.to_str().expect("utf-8"),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "photos"), "1");

    // The floor (S19): the two-cell layout still drops to the one-cell sheet, and
    // it is the sheet that refuses a further removal — one cell is the smallest
    // layout the library has.
    let sheet = dir.join("sheet.pixlay");
    let output = run(&[
        "edit",
        "--project",
        holed.to_str().expect("utf-8"),
        "--remove-cell",
        "--out",
        sheet.to_str().expect("utf-8"),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "cells"), "1");
    assert_eq!(field(&output, "template"), "grid-1-1x1");
    assert_eq!(
        field(&output, "photos"),
        "1",
        "the surviving cell keeps its photo"
    );

    let refused = dir.join("refused.pixlay");
    let output = run(&[
        "edit",
        "--project",
        sheet.to_str().expect("utf-8"),
        "--remove-cell",
        "--out",
        refused.to_str().expect("utf-8"),
    ]);
    assert_eq!(
        code(&output),
        2,
        "a one-cell layout has no smaller one: {}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("at least 1 cell"),
        "{}",
        stderr(&output)
    );
    assert!(!refused.exists());

    // `--remove-cell` and `--add-photo` are *not* opposites any more (S14b): one
    // moves the layout, the other places a photo, and an edit that does both is a
    // legal edit. The documented order decides the result: the cell goes first, so
    // the photo lands in the layout that is left.
    let both = dir.join("both.pixlay");
    let output = run(&[
        "edit",
        "--project",
        project.to_str().expect("utf-8"),
        "--remove-cell",
        "--add-photo",
        photos.join("a.jpg").to_str().expect("utf-8"),
        "--out",
        both.to_str().expect("utf-8"),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(
        field(&output, "cells"),
        "4",
        "three cells after the removal, then the layout grew to hold the photo"
    );
    assert_eq!(field(&output, "photos"), "4");
    let composed = CollageDoc::load(&both).expect("loads");
    assert_eq!(composed.cells.len(), 4);
    assert!(
        composed.cells[3]
            .source
            .as_deref()
            .is_some_and(|source| source.ends_with("a.jpg")),
        "the photo took the cell the growth made: {:?}",
        composed.cells[3].source
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn edit_switches_the_layout_and_sets_one_cell_s_photo() {
    // S14's other two capabilities, which the gallery and the canvas's own
    // click-to-replace reach from the window: switch the layout keeping the
    // surviving cells, and point one cell at another photo.
    let dir = out_dir("edit-layout");
    let project = framing_project(&dir, "a.pixlay");
    let other = dir.join("photos/tall.jpg");
    let before = CollageDoc::load(&project).expect("loads");

    // A layout with more slots: the cells that survive keep what they hold, and
    // the new ones are empty.
    let grown = dir.join("grown.pixlay");
    let output = run(&[
        "edit",
        "--project",
        project.to_str().expect("utf-8"),
        "--template",
        "mosaic-4-hero",
        "--out",
        grown.to_str().expect("utf-8"),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "template"), "mosaic-4-hero");
    assert_eq!(field(&output, "cells"), "4");
    assert_eq!(field(&output, "photos"), "2");
    let after = CollageDoc::load(&grown).expect("loads");
    assert_eq!(after.cells[..2], before.cells[..]);
    assert_eq!(after.cells[2], Cell::default());
    assert_eq!(after.cells[3], Cell::default());

    // A layout with *fewer* slots drops the tail, which is the documented rule and
    // the one the gallery's own thumbnails show before the click.
    let shrunk = dir.join("shrunk.pixlay");
    let output = run(&[
        "edit",
        "--project",
        grown.to_str().expect("utf-8"),
        "--template",
        "strip-2-2x1",
        "--out",
        shrunk.to_str().expect("utf-8"),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "cells"), "2");
    assert_eq!(field(&output, "photos"), "2");

    // One cell's photo, with the framing it already had: `SetSource` never resets
    // the crop, because the zoom is absolute (docs/CONTRACT.md §1).
    let framed = dir.join("framed.pixlay");
    let output = run(&[
        "edit",
        "--project",
        project.to_str().expect("utf-8"),
        "--slot",
        "1",
        "--rotate",
        "12",
        "--out",
        framed.to_str().expect("utf-8"),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    let crop = stored_crop(&framed, 1);
    let replaced = dir.join("replaced.pixlay");
    let output = run(&[
        "edit",
        "--project",
        framed.to_str().expect("utf-8"),
        "--slot",
        "1",
        "--photo",
        other.to_str().expect("utf-8"),
        "--out",
        replaced.to_str().expect("utf-8"),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "occupied"), "true");
    let after = CollageDoc::load(&replaced).expect("loads");
    assert!(
        after.cells[1]
            .source
            .as_deref()
            .is_some_and(|source| source.ends_with("photos/tall.jpg")),
        "{:?}",
        after.cells[1].source
    );
    assert_eq!(
        after.cells[1].crop.rotation_deg, crop.rotation_deg,
        "the new photo keeps the framing the cell had"
    );
    assert_eq!(
        after.cells[0], before.cells[0],
        "the other cell is untouched"
    );

    // The flag relationships: `--photo` needs a cell, and it is not a way to empty
    // one.
    for args in [
        vec!["--photo", other.to_str().expect("utf-8")],
        vec!["--slot", "0", "--clear", "--photo", "x.jpg"],
        vec!["--slot", "9", "--photo", other.to_str().expect("utf-8")],
    ] {
        let bad = dir.join("bad.pixlay");
        let mut argv = vec!["edit", "--project", project.to_str().expect("utf-8")];
        argv.extend(&args);
        argv.push("--out");
        argv.push(bad.to_str().expect("utf-8"));
        let output = run(&argv);
        assert_eq!(code(&output), 1, "{args:?}: {}", stderr(&output));
        assert!(!dir.join("bad.pixlay").exists());
    }
    // An unknown layout names the library, like every other unknown-template
    // refusal this build has.
    let unknown = run(&[
        "edit",
        "--project",
        project.to_str().expect("utf-8"),
        "--template",
        "nope",
        "--out",
        dir.join("nope.pixlay").to_str().expect("utf-8"),
    ]);
    assert_eq!(code(&unknown), 1);
    assert!(
        stderr(&unknown).contains("mosaic-8-s14"),
        "{}",
        stderr(&unknown)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn edit_refuses_the_sheet_flag_it_no_longer_has() {
    // S12d removed the document's whole physical size (ruling 17: the product
    // has no concept of paper), so `--sheet` is a usage error — and one that
    // names the flag it no longer takes, rather than an empty silence.
    let dir = out_dir("edit-sheet");
    let project = framing_project(&dir, "a.pixlay");
    let out = dir.join("a4.pixlay");
    for sheet in ["0", "420", "2001"] {
        let output = run(&[
            "edit",
            "--project",
            project.to_str().expect("utf-8"),
            "--sheet",
            sheet,
            "--out",
            out.to_str().expect("utf-8"),
        ]);
        assert_eq!(code(&output), 1, "--sheet {sheet}: {}", stderr(&output));
        assert!(stderr(&output).contains("--sheet"), "{}", stderr(&output));
    }
    assert!(!out.exists(), "a usage error writes nothing");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn edit_stores_the_frame_and_render_overrides_it() {
    // `edit` writes the frame into the document; `render`'s flags are a render-time
    // override that leaves the file alone. Both have to be visible in the report,
    // because "which frame did that render use" is not answerable from the pixels
    // without counting them.
    let dir = out_dir("frame");
    let project = framing_project(&dir, "a.pixlay");
    let path = project.to_str().expect("utf-8 path");
    let stored = dir.join("framed.pixlay");

    let output = run(&[
        "edit",
        "--project",
        path,
        "--gap",
        "0.02",
        "--radius",
        "0.03",
        "--border-color",
        "12,200,240",
        "--out",
        stored.to_str().expect("utf-8"),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "gap"), "0.020000");
    assert_eq!(field(&output, "radius"), "0.030000");
    assert_eq!(field(&output, "border"), "12,200,240");

    let doc = CollageDoc::load(&stored).expect("loads");
    assert_eq!(doc.frame.gap_rel, 0.02);
    assert_eq!(doc.frame.radius_rel, 0.03);
    assert_eq!(
        doc.frame.color,
        pixlay_core::Rgba8 {
            r: 12,
            g: 200,
            b: 240,
            a: 255
        }
    );
    // Nothing else moved: the frame is a document field, not a relayout.
    assert_eq!(doc.template.name, "strip-2-2x1");
    assert_eq!(doc.cells.len(), 2);

    // A render of the stored frame: the report agrees, and the canvas border is the
    // frame's colour (the same claim `pixlay-render`'s own frame tests measure).
    let png = dir.join("framed.png");
    let render = run(&[
        "render",
        "--project",
        stored.to_str().expect("utf-8"),
        "--long-edge",
        "1000",
        "--out",
        png.to_str().expect("utf-8"),
    ]);
    assert_eq!(code(&render), 0, "{}", stderr(&render));
    assert_eq!(field(&render, "gap"), "0.020000");
    assert_eq!(field(&render, "border"), "12,200,240");
    let image = image::open(&png).expect("a readable PNG").to_rgb8();
    assert_eq!(image.get_pixel(1, 1).0, [12, 200, 240], "the corner");

    // `render --gap` overrides for that render only: the project on disk keeps the
    // frame `edit` wrote (and an unnamed flag keeps the document's own value).
    let over = dir.join("over.png");
    let render = run(&[
        "render",
        "--project",
        stored.to_str().expect("utf-8"),
        "--long-edge",
        "1000",
        "--gap",
        "0.06",
        "--out",
        over.to_str().expect("utf-8"),
    ]);
    assert_eq!(code(&render), 0, "{}", stderr(&render));
    assert_eq!(field(&render, "gap"), "0.060000");
    assert_eq!(
        field(&render, "radius"),
        "0.030000",
        "the radius was not named"
    );
    assert_eq!(field(&render, "border"), "12,200,240");
    assert_eq!(
        CollageDoc::load(&stored).expect("loads").frame.gap_rel,
        0.02,
        "the override reached the file"
    );

    // A gap that empties a cell is refused where it is asked for, naming the cell:
    // the same check `validate` makes, reached through the command line.
    let broken = run(&[
        "render",
        "--project",
        stored.to_str().expect("utf-8"),
        "--long-edge",
        "1000",
        "--gap",
        "0.99",
        "--out",
        dir.join("broken.png").to_str().expect("utf-8"),
    ]);
    assert_eq!(code(&broken), 2, "{}", stderr(&broken));
    assert!(stdout(&broken).is_empty());
    assert!(stderr(&broken).contains("slot 0"), "{}", stderr(&broken));
    assert!(!dir.join("broken.png").exists(), "nothing was written");

    // The radius is clamped, not refused: 1.0 is the largest a document may ask for
    // and it rounds the cell into a stadium.
    let stadium = dir.join("stadium.png");
    let render = run(&[
        "render",
        "--project",
        stored.to_str().expect("utf-8"),
        "--long-edge",
        "1000",
        "--radius",
        "1",
        "--out",
        stadium.to_str().expect("utf-8"),
    ]);
    assert_eq!(code(&render), 0, "{}", stderr(&render));
    assert_eq!(field(&render, "radius"), "1.000000");
    let _ = std::fs::remove_dir_all(&dir);
}

/// `edit` and the frame flags keep the S1 rules: flags that belong elsewhere are
/// refused, and no locale changes a byte of either stream.
#[test]
fn edit_keeps_the_usage_and_locale_rules() {
    let dir = out_dir("edit-usage");
    let project = framing_project(&dir, "u.pixlay");
    let path = project.to_str().expect("utf-8 path");
    let out = dir.join("out.pixlay");
    let out_path = out.to_str().expect("utf-8 path").to_string();

    for args in [
        // Nothing to change.
        vec!["edit", "--project", path, "--out", &out_path],
        // A framing flag without a cell.
        vec![
            "edit",
            "--project",
            path,
            "--out",
            &out_path,
            "--rotate",
            "10",
        ],
        vec![
            "edit",
            "--project",
            path,
            "--out",
            &out_path,
            "--zoom",
            "1.2",
            "--clear",
        ],
        // Clearing and framing at once.
        vec![
            "edit",
            "--project",
            path,
            "--out",
            &out_path,
            "--slot",
            "0",
            "--clear",
            "--rotate",
            "10",
        ],
        // Out of range: a cell that does not exist, a zoom of zero, a gap past the
        // canvas, a channel past 255, a pan past the cell.
        vec![
            "edit",
            "--project",
            path,
            "--out",
            &out_path,
            "--slot",
            "7",
            "--rotate",
            "10",
        ],
        vec![
            "edit",
            "--project",
            path,
            "--out",
            &out_path,
            "--slot",
            "0",
            "--zoom",
            "0",
        ],
        vec!["edit", "--project", path, "--out", &out_path, "--gap", "2"],
        vec![
            "edit",
            "--project",
            path,
            "--out",
            &out_path,
            "--border-color",
            "300,0,0",
        ],
        vec![
            "edit",
            "--project",
            path,
            "--out",
            &out_path,
            "--border-color",
            "1,2",
        ],
        vec![
            "edit",
            "--project",
            path,
            "--out",
            &out_path,
            "--slot",
            "0",
            "--offset",
            "2,0",
        ],
        vec![
            "edit",
            "--project",
            path,
            "--out",
            &out_path,
            "--slot",
            "0",
            "--rotate",
            "nan",
        ],
        // Missing what it needs.
        vec!["edit"],
        vec!["edit", "--project", path],
        vec!["edit", "--out", &out_path, "--gap", "0.01"],
        vec![
            "edit",
            "--project",
            path,
            "--out",
            "out.json",
            "--gap",
            "0.01",
        ],
        // Flags that belong elsewhere.
        vec![
            "edit",
            "--project",
            path,
            "--out",
            &out_path,
            "--gap",
            "0.01",
            "--long-edge",
            "1000",
        ],
        vec![
            "edit",
            "--project",
            path,
            "--out",
            &out_path,
            "--gap",
            "0.01",
            "--photo",
            "p.png",
        ],
        vec![
            "edit",
            "--project",
            path,
            "--out",
            &out_path,
            "--gap",
            "0.01",
            "--stats",
        ],
        vec![
            "render",
            "--template",
            "strip-2-2x1",
            "--long-edge",
            "1000",
            "--out",
            "x.png",
            "--slot",
            "0",
        ],
        vec!["probe", "--project", path, "--gap", "0.01"],
        vec![
            "save",
            "--project",
            path,
            "--out",
            &out_path,
            "--radius",
            "0.1",
        ],
        vec![
            "init",
            "--template",
            "strip-2-2x1",
            "--out",
            &out_path,
            "--clear",
        ],
    ] {
        let output = run(&args);
        assert_eq!(code(&output), 1, "{args:?}: {}", stderr(&output));
        assert!(stdout(&output).is_empty(), "{args:?} wrote to stdout");
        assert!(!stderr(&output).is_empty(), "{args:?} said nothing");
    }
    assert!(!out.exists(), "a refused edit wrote a file");

    // Byte-identical under any locale, on the success and the failure branch.
    let mut success = Vec::new();
    let mut failure = Vec::new();
    for (lang, all) in [
        ("C", "C"),
        ("zh_CN.UTF-8", "zh_CN.UTF-8"),
        ("de_DE.UTF-8", "de_DE.UTF-8"),
    ] {
        let copy = dir.join("locale.pixlay");
        let _ = std::fs::remove_file(&copy);
        let edited = run_in(
            &[
                "edit",
                "--project",
                path,
                "--slot",
                "1",
                "--rotate",
                "-33.5",
                "--gap",
                "0.02",
                "--out",
                copy.to_str().expect("utf-8"),
                "--json",
            ],
            None,
            Some((lang, all)),
        );
        assert_eq!(code(&edited), 0, "{lang}: {}", stderr(&edited));
        success.push(edited.stdout.clone());

        let broken = run_in(
            &[
                "edit",
                "--project",
                "absent.pixlay",
                "--out",
                "x.pixlay",
                "--gap",
                "0.01",
            ],
            Some(&dir),
            Some((lang, all)),
        );
        assert_eq!(code(&broken), 2);
        assert!(stdout(&broken).is_empty());
        failure.push(broken.stderr.clone());
    }
    assert!(
        success.iter().all(|stdout| stdout == &success[0]),
        "edit changed under a locale"
    );
    assert!(
        failure.iter().all(|stderr| stderr == &failure[0]),
        "edit's failure message changed under a locale"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// S9: the library, the preview and the selection
// ---------------------------------------------------------------------------

/// The folder `scan` is pointed at: one file per case a picker has to survive.
///
/// Built from the committed photos rather than committed again — the fixtures
/// already are the decodable files this step needs — plus the three things no
/// photo fixture can be: a file with an image extension that is not an image, a
/// file the decoder cannot open at all, and a file that is not a photo.
fn library_dir(dir: &Path) {
    std::fs::create_dir_all(dir).expect("create the library");
    let photos = fixture_dir().join("photos");
    for name in [
        "square.png",
        "landscape.jpg",
        "photo.heic",
        "photo-16bit.png",
        "oriented-6.jpg",
        "dated.jpg",
    ] {
        std::fs::copy(photos.join(name), dir.join(name))
            .unwrap_or_else(|error| panic!("{name}: {error}"));
    }
    // A non-image: the decoder refuses it, and the row has to carry the reason.
    std::fs::write(dir.join("broken.png"), b"this is not a PNG\n").expect("write broken.png");
    // An unreadable file. A dangling symlink rather than a permission bit: a test
    // that happens to run as root would not notice `chmod 000`.
    std::os::unix::fs::symlink(dir.join("absent.jpg"), dir.join("unreadable.jpg"))
        .expect("symlink unreadable.jpg");
    // Not a photo at all: no image extension, so it is not listed.
    std::fs::write(dir.join("notes.txt"), b"not a photo\n").expect("write notes.txt");
    // A subdirectory, which only `--recursive` descends into.
    std::fs::create_dir_all(dir.join("nested")).expect("nested");
    std::fs::copy(photos.join("portrait.jpg"), dir.join("nested/portrait.jpg"))
        .expect("copy nested/portrait.jpg");
}

/// `scan`'s rows as one map per file, so a test asserts facts rather than line
/// order (`file.<n>.<field>`).
fn scan_rows(output: &Output) -> Vec<std::collections::BTreeMap<String, String>> {
    let mut rows: Vec<std::collections::BTreeMap<String, String>> = Vec::new();
    for line in stdout(output).lines() {
        let Some((key, value)) = line.split_once(" = ") else {
            continue;
        };
        let Some(rest) = key.strip_prefix("file.") else {
            continue;
        };
        let Some((index, field)) = rest.split_once('.') else {
            continue;
        };
        let index: usize = index.parse().expect("a row index");
        while rows.len() <= index {
            rows.push(std::collections::BTreeMap::new());
        }
        rows[index].insert(field.to_string(), value.to_string());
    }
    rows
}

/// One library row, looked up by the file name it ends with.
fn row<'a>(
    rows: &'a [std::collections::BTreeMap<String, String>],
    name: &str,
) -> &'a std::collections::BTreeMap<String, String> {
    rows.iter()
        .find(|row| row["path"].ends_with(name))
        .unwrap_or_else(|| panic!("no scan row for {name}"))
}

/// Decodes the report's escape rule back into the bytes a field was made from:
/// `\\`, `\n`, `\r`, `\t` and `\xNN`.
///
/// It is the inverse of `report.rs`'s writer, and running it here is the
/// statement that the rule *is* invertible — which is what makes a name that is
/// not UTF-8 a name a consumer can still use.
fn unescape(value: &str) -> Vec<u8> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != b'\\' {
            // A UTF-8 sequence passes through unchanged, so its bytes are copied
            // one at a time.
            out.push(bytes[at]);
            at += 1;
            continue;
        }
        match bytes[at + 1] {
            b'\\' => out.push(b'\\'),
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b't' => out.push(b'\t'),
            b'x' => {
                let hex = std::str::from_utf8(&bytes[at + 2..at + 4]).expect("two hex digits");
                out.push(u8::from_str_radix(hex, 16).expect("hex"));
                at += 4;
                continue;
            }
            other => panic!("unknown escape \\{} in {value:?}", other as char),
        }
        at += 2;
    }
    out
}

/// S15h (PIX-018): a filename is bytes, and the report carries the bytes it is.
///
/// The names this uses are the ones a Linux filesystem allows and a machine
/// surface has to survive: a newline (which used to end a field line early and
/// start another one), a second control byte, the escape's own backslash, and one
/// that is not UTF-8 at all. Each row's `path` has to decode back to the exact
/// bytes the file was created with.
#[test]
fn scan_preserves_a_newline_a_control_byte_and_a_non_utf8_name() {
    let dir = out_dir("scan-bytes");
    let library = dir.join("library");
    std::fs::create_dir_all(&library).expect("create the library");
    let photo = include_bytes!("fixtures/photos/square.png");
    let names: [OsString; 5] = [
        OsString::from("plain.png"),
        OsString::from("line\nbreak.png"),
        OsString::from("bell\x01.png"),
        OsString::from("back\\slash.png"),
        // Not UTF-8 at all: `Path::display` would print a replacement character
        // here, and two different byte names could then print the same line.
        OsString::from_vec(b"\xff\xfe.png".to_vec()),
    ];
    for name in &names {
        std::fs::write(library.join(name), photo).expect("write a photo");
    }

    let path = library.to_str().expect("a UTF-8 path");
    let output = run(&["scan", "--dir", path]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    let text = stdout(&output);

    // One line per field: every line is a `key = value`, and no key is printed
    // twice. A raw newline inside a value would surface here as a line without
    // ` = `, or as a second line for a key that already had one.
    let mut keys = std::collections::BTreeSet::new();
    for line in text.lines() {
        let (key, _) = line
            .split_once(" = ")
            .unwrap_or_else(|| panic!("a value injected a line: {line:?}"));
        assert!(keys.insert(key.to_string()), "{key} was printed twice");
    }

    // The escapes are visible in the output itself, so the assertion is about the
    // bytes on stdout and not only about the decoder above.
    for escaped in [
        r"line\nbreak.png",
        r"bell\x01.png",
        r"back\\slash.png",
        r"\xff\xfe.png",
    ] {
        assert!(text.contains(escaped), "no {escaped} in:\n{text}");
    }
    assert!(
        !text.contains("line\nbreak.png"),
        "a filename's newline reached a line of its own"
    );

    // An ordinary path is unchanged: the escaping only touches what it must.
    assert_eq!(field(&output, "dir"), path);

    // `--json` carries the same escaped string as the value of the field, so a
    // consumer reads a path out of either shape by one rule — and the object still
    // parses, which the raw escaped form would not (`\xNN` is not a JSON escape).
    let json = run(&["scan", "--dir", path, "--json"]);
    assert_eq!(code(&json), 0, "{}", stderr(&json));
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout(&json)).expect("--json is one JSON object");
    let object = parsed.as_object().expect("one object");
    assert_eq!(object.len(), keys.len(), "a value injected a JSON key");
    let values: Vec<&str> = object
        .iter()
        .filter(|(key, _)| key.as_str() == "dir" || key.ends_with(".path"))
        .filter_map(|(_, value)| value.as_str())
        .collect();
    for name in &names {
        let mut expected = library.as_os_str().as_bytes().to_vec();
        expected.push(b'/');
        expected.extend_from_slice(name.as_bytes());
        assert!(
            values.iter().any(|value| unescape(value) == expected),
            "no JSON value decodes back to {name:?}: {values:?}"
        );
    }

    let rows = scan_rows(&output);
    assert_eq!(rows.len(), names.len(), "{rows:?}");
    for name in &names {
        // The row's path is the path `--dir` was given joined with the name, so
        // the expectation is that whole path as bytes.
        let mut expected = library.as_os_str().as_bytes().to_vec();
        expected.push(b'/');
        expected.extend_from_slice(name.as_bytes());
        let found = rows
            .iter()
            .find(|row| unescape(&row["path"]) == expected)
            .unwrap_or_else(|| panic!("no row decodes back to {name:?}: {rows:?}"));
        assert_eq!(
            found["status"], "ok",
            "{name:?} was listed but not read: {found:?}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn scan_lists_a_folder_of_photos_one_row_per_file() {
    let dir = out_dir("scan");
    let library = dir.join("library");
    library_dir(&library);
    let path = library.to_str().expect("utf-8 path");

    let listed = run(&["scan", "--dir", path]);
    assert_eq!(code(&listed), 0, "{}", stderr(&listed));
    assert!(stderr(&listed).is_empty(), "{}", stderr(&listed));
    assert_eq!(field(&listed, "status"), "ok");
    assert_eq!(field(&listed, "dir"), path);
    assert_eq!(field(&listed, "recursive"), "false");
    // Eight rows: six photos and the two refusals.
    assert_eq!(field(&listed, "count"), "8");
    assert_eq!(field(&listed, "failed"), "2");

    let rows = scan_rows(&listed);
    assert_eq!(rows.len(), 8);
    // The order is lexical by path, and the same run twice is byte-identical:
    // that is what makes a listing diffable.
    let paths: Vec<&String> = rows.iter().map(|row| &row["path"]).collect();
    let mut sorted = paths.clone();
    sorted.sort();
    assert_eq!(paths, sorted, "scan's order is not lexical by path");
    let again = run(&["scan", "--dir", path]);
    assert_eq!(stdout(&listed), stdout(&again), "scan is not stable");

    // The facts a grid tile is laid out against, per file.
    let square = row(&rows, "square.png");
    assert_eq!(square["status"], "ok");
    assert_eq!(square["mime"], "image/png");
    assert_eq!(square["width"], "640");
    assert_eq!(square["height"], "640");
    assert_eq!(square["date"], "");
    let mtime = std::fs::metadata(library.join("square.png"))
        .expect("stat")
        .modified()
        .expect("mtime")
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after the epoch")
        .as_secs();
    assert_eq!(square["mtime"], mtime.to_string());

    assert_eq!(row(&rows, "landscape.jpg")["mime"], "image/jpeg");
    // A HEIC is recognised as a HEIF-family image (glycin reports the family; the
    // imaging crate's decode test pins the same string).
    assert_eq!(row(&rows, "photo.heic")["mime"], "image/heif");
    assert_eq!(row(&rows, "photo-16bit.png")["width"], "800");
    assert_eq!(row(&rows, "dated.jpg")["date"], "2019:07:14 10:32:00");
    // The size is the size **after** EXIF rotation: the file stores 600x1200 and
    // displays 1200x600, which is the size a preview is laid out against.
    assert_eq!(row(&rows, "oriented-6.jpg")["width"], "1200");
    assert_eq!(row(&rows, "oriented-6.jpg")["height"], "600");

    // Neither a text file nor a subdirectory is a row.
    assert!(rows.iter().all(|row| !row["path"].ends_with("notes.txt")));
    assert!(
        rows.iter()
            .all(|row| !row["path"].ends_with("portrait.jpg"))
    );

    // `--recursive` is the only way in, and it says so in its own report.
    let deep = run(&["scan", "--dir", path, "--recursive"]);
    assert_eq!(code(&deep), 0, "{}", stderr(&deep));
    assert_eq!(field(&deep, "recursive"), "true");
    assert_eq!(field(&deep, "count"), "9");
    assert!(row(&scan_rows(&deep), "nested/portrait.jpg")["width"] == "600");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn scan_reports_a_file_it_cannot_decode_as_a_row() {
    let dir = out_dir("scan-refusals");
    let library = dir.join("library");
    library_dir(&library);

    let output = run(&["scan", "--dir", library.to_str().unwrap()]);
    // The listing is the result, so a file this build cannot read does not fail
    // the command — the row says which file and why, and the grid can show it.
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    let rows = scan_rows(&output);

    let broken = row(&rows, "broken.png");
    assert_eq!(broken["status"], "failed");
    assert!(
        broken["reason"].len() > 10,
        "the decoder's reason has to be quoted: {broken:?}"
    );
    assert!(
        !broken.contains_key("width") && !broken.contains_key("height"),
        "a failed row has no size to report: {broken:?}"
    );

    let unreadable = row(&rows, "unreadable.jpg");
    assert_eq!(unreadable["status"], "failed");
    assert!(
        unreadable["reason"].contains("unreadable.jpg"),
        "the failing path must be named: {unreadable:?}"
    );

    // The healthy files are still there, in the same listing.
    assert_eq!(row(&rows, "square.png")["status"], "ok");
    assert_eq!(field(&output, "failed"), "2");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn thumb_writes_a_preview_at_the_requested_long_edge() {
    let dir = out_dir("thumb");
    // A flat photo makes every pixel expectation exact: a preview of a solid
    // colour is that colour, wherever it is sampled (the imaging crate's own test
    // covers the ramp and the alpha cases).
    let color = [30u8, 140, 200];
    let photo = dir.join("flat.png");
    image::RgbImage::from_fn(400, 200, |_, _| image::Rgb(color))
        .save(&photo)
        .expect("write the flat photo");

    let out = dir.join("small.png");
    let output = run(&[
        "thumb",
        "--photo",
        photo.to_str().unwrap(),
        "--px",
        "100",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(stderr(&output).is_empty(), "{}", stderr(&output));
    assert_eq!(field(&output, "status"), "ok");
    assert_eq!(field(&output, "format"), "png");
    assert_eq!(field(&output, "mime"), "image/png");
    assert_eq!(field(&output, "src_w"), "400");
    assert_eq!(field(&output, "src_h"), "200");
    assert_eq!(field(&output, "px"), "100");
    // The long edge is exact and the other keeps the photo's ratio.
    assert_eq!(field(&output, "out_w"), "100");
    assert_eq!(field(&output, "out_h"), "50");
    // The grid, read back from the file rather than from the report.
    let preview = image::open(&out).expect("open the preview").to_rgb8();
    assert_eq!(preview.dimensions(), (100, 50));
    // And no resolution is claimed: the preview PNG has no `pHYs` (S12d).
    assert!(png_pixel_dimensions(&out).is_none());
    assert_eq!(
        std::fs::metadata(&out).expect("stat").len().to_string(),
        field(&output, "bytes")
    );
    for pixel in preview.pixels() {
        for channel in 0..3 {
            assert!(
                pixel[channel].abs_diff(color[channel]) <= 1,
                "a flat photo previewed to {pixel:?}"
            );
        }
    }

    // The size a preview reports is the size after EXIF rotation, so a tile and a
    // preview agree about a photo that a camera stored sideways.
    let rotated = dir.join("rotated.png");
    let output = run(&[
        "thumb",
        "--photo",
        fixture_dir()
            .join("photos/oriented-6.jpg")
            .to_str()
            .unwrap(),
        "--px",
        "256",
        "--out",
        rotated.to_str().unwrap(),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "src_w"), "1200");
    assert_eq!(field(&output, "src_h"), "600");
    assert_eq!(
        image::open(&rotated).expect("open").to_rgb8().dimensions(),
        (256, 128),
        "a preview of the 600x1200 file is laid out the way it displays"
    );

    // This is the picker's budget number (`AGENTS.md`: a visual conclusion has to
    // become a number), so `--stats` is part of the surface rather than a bonus.
    let measured = run(&[
        "thumb",
        "--photo",
        photo.to_str().unwrap(),
        "--px",
        "256",
        "--out",
        dir.join("measured.png").to_str().unwrap(),
        "--stats",
    ]);
    assert_eq!(code(&measured), 0, "{}", stderr(&measured));
    assert!(field(&measured, "ms").parse::<f64>().expect("ms") > 0.0);
    assert!(
        field(&measured, "encode_ms")
            .parse::<f64>()
            .expect("encode_ms")
            > 0.0
    );
    assert!(
        field(&measured, "peak_rss_mb")
            .parse::<f64>()
            .expect("peak")
            > 10.0,
        "peak_rss_mb is the ruler's own definition"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn init_photo_writes_the_arguments_as_cells_in_order() {
    let dir = out_dir("init-photo");
    let photos = ["square.png", "landscape.jpg", "portrait.jpg"];
    for name in photos {
        std::fs::copy(fixture_dir().join("photos").join(name), dir.join(name))
            .unwrap_or_else(|error| panic!("{name}: {error}"));
    }
    let path = dir.join("three.pixlay");
    let output = run(&[
        "init",
        "--template",
        "strip-3-3x1",
        "--out",
        path.to_str().unwrap(),
        "--photo",
        dir.join("square.png").to_str().unwrap(),
        "--photo",
        dir.join("landscape.jpg").to_str().unwrap(),
        "--photo",
        dir.join("portrait.jpg").to_str().unwrap(),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "photos"), "3");
    assert_eq!(field(&output, "cells"), "3");

    let project = Project::load(&path).expect("the written project loads");
    let stored: Vec<String> = project
        .doc()
        .cells
        .iter()
        .map(|cell| {
            cell.source
                .as_deref()
                .expect("every cell has a photo")
                .display()
                .to_string()
        })
        .collect();
    // A photo beside the project is stored relative to it, so the project stays
    // movable — the same rule `save_as` applies to a copy.
    assert_eq!(
        stored,
        photos.map(str::to_string).to_vec(),
        "argument order is cell order"
    );
    assert!(
        project
            .sources()
            .expect("every photo resolves")
            .iter()
            .all(Option::is_some)
    );

    // And it renders: three photos in three cells, not three empty ones.
    let rendered = run(&[
        "render",
        "--project",
        path.to_str().unwrap(),
        "--preview-px",
        "600",
        "--out",
        dir.join("three.jpg").to_str().unwrap(),
    ]);
    assert_eq!(code(&rendered), 0, "{}", stderr(&rendered));
    assert_eq!(field(&rendered, "occupied"), "3");
    assert_eq!(field(&rendered, "cells"), "3");

    // A photo in another directory is stored relative to the project wherever the
    // two share a root, which is what makes a project folder portable.
    let elsewhere = dir.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).expect("elsewhere");
    for name in ["square.png", "landscape.jpg"] {
        std::fs::copy(
            fixture_dir().join("photos").join(name),
            elsewhere.join(name),
        )
        .expect("copy");
    }
    let other = dir.join("other.pixlay");
    let output = run(&[
        "init",
        "--template",
        "strip-2-2x1",
        "--out",
        other.to_str().unwrap(),
        "--photo",
        elsewhere.join("square.png").to_str().unwrap(),
        "--photo",
        elsewhere.join("landscape.jpg").to_str().unwrap(),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    let stored: Vec<String> = Project::load(&other)
        .expect("loads")
        .doc()
        .cells
        .iter()
        .map(|cell| cell.source.clone().expect("occupied").display().to_string())
        .collect();
    assert_eq!(
        stored,
        vec!["elsewhere/square.png", "elsewhere/landscape.jpg"]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn init_refuses_a_photo_count_outside_the_range_or_a_wrong_slot_count() {
    let dir = out_dir("init-refusals");
    let photo = dir.join("p.png");
    std::fs::copy(fixture_dir().join("photos/square.png"), &photo).expect("copy");
    let path = dir.join("out.pixlay");
    let photo_arg = photo.to_str().unwrap().to_string();

    // Ten photos: the 1..=9 clamp names both bounds, exits 1, and writes nothing —
    // a refused command leaves no half-made project behind. One photo left this
    // list in S19: a single photo is a legal collage (ruling 34), and `--template
    // grid-1-1x1` is its layout, which the positive case below exercises.
    let mut ten = vec![
        "init",
        "--template",
        "mosaic-8-s14",
        "--out",
        path.to_str().unwrap(),
    ];
    for _ in 0..10 {
        ten.push("--photo");
        ten.push(&photo_arg);
    }
    let output = run(&ten);
    assert_eq!(code(&output), 1, "ten photos: {}", stderr(&output));
    assert!(stdout(&output).is_empty(), "ten photos wrote to stdout");
    assert!(
        stderr(&output).contains("1..=9"),
        "ten photos: {}",
        stderr(&output)
    );
    assert!(!path.exists(), "a refused init must write nothing");

    // One photo on the one-slot sheet: the collage of a single photo, from the
    // machine surface, and the CLI is not allowed to trim a longer list — it
    // refuses one instead (the window is the surface that trims, with a report).
    let one = run(&[
        "init",
        "--template",
        "grid-1-1x1",
        "--out",
        path.to_str().unwrap(),
        "--photo",
        &photo_arg,
    ]);
    assert_eq!(code(&one), 0, "{}", stderr(&one));
    assert_eq!(field(&one, "cells"), "1");
    assert_eq!(field(&one, "photos"), "1");
    assert_eq!(field(&one, "aspect"), "4:3");
    let single = CollageDoc::load(&path).expect("loads");
    assert_eq!(single.cells.len(), 1);
    assert!(single.cells[0].source.is_some());
    single.validate().expect("a valid document");
    std::fs::remove_file(&path).expect("remove the one-photo project");

    // Three photos into a two-slot template: the numbers are named.
    let output = run(&[
        "init",
        "--template",
        "strip-2-2x1",
        "--out",
        path.to_str().unwrap(),
        "--photo",
        &photo_arg,
        "--photo",
        &photo_arg,
        "--photo",
        &photo_arg,
    ]);
    assert_eq!(code(&output), 1, "{}", stderr(&output));
    assert!(stdout(&output).is_empty());
    assert!(
        stderr(&output).contains("strip-2-2x1"),
        "{}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains('2') && stderr(&output).contains('3'),
        "the template's count and the photo count must both be named: {}",
        stderr(&output)
    );
    assert!(!path.exists());

    // A photo that is not there is a *failure* (exit 2), not a usage error: the
    // arguments were well formed.
    let output = run(&[
        "init",
        "--template",
        "strip-2-2x1",
        "--out",
        path.to_str().unwrap(),
        "--photo",
        &photo_arg,
        "--photo",
        dir.join("absent.png").to_str().unwrap(),
    ]);
    assert_eq!(code(&output), 2, "{}", stderr(&output));
    assert!(
        stderr(&output).contains("absent.png"),
        "{}",
        stderr(&output)
    );
    assert!(!path.exists());

    // The photo-free project S2 shipped is unchanged, and says so.
    let output = run(&[
        "init",
        "--template",
        "strip-2-2x1",
        "--out",
        path.to_str().unwrap(),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "photos"), "0");
    let _ = std::fs::remove_dir_all(&dir);
}

/// `thumb --region` is the window's 1:1 view (S15j): the rectangle's own pixels.
///
/// The comparison is the CLI against itself, which is what makes it a statement about
/// *identity* rather than about a resampler: a whole photo at its own long edge is the
/// photo unchanged (1:1 is the identity kernel), so a rectangle of it at its own size has
/// to be exactly the crop — pixel for pixel, which a re-scaled or half-texel-shifted crop
/// could not be.
#[test]
fn thumb_resamples_one_rectangle_of_a_photo() {
    let dir = out_dir("thumb-region");
    // A checkerboard: the content is at the pixel level, so it is also what a smoothed
    // 1:1 copy would fail on (the resampler's own tests measure the kernel; this one
    // measures the *surface*).
    let photo = dir.join("checker.png");
    image::RgbImage::from_fn(200, 150, |x, y| {
        if (x + y) % 2 == 0 {
            image::Rgb([250, 250, 250])
        } else {
            image::Rgb([5, 5, 5])
        }
    })
    .save(&photo)
    .expect("write the checkerboard");

    let whole = dir.join("whole.png");
    let output = run(&[
        "thumb",
        "--photo",
        photo.to_str().unwrap(),
        "--px",
        "200",
        "--out",
        whole.to_str().unwrap(),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "region"), "0,0,200,150");

    let region = dir.join("region.png");
    let output = run(&[
        "thumb",
        "--photo",
        photo.to_str().unwrap(),
        "--px",
        "64",
        "--region",
        "40,30,64,48",
        "--out",
        region.to_str().unwrap(),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "region"), "40,30,64,48");
    assert_eq!(field(&output, "src_w"), "200");
    assert_eq!(field(&output, "src_h"), "150");
    // The long edge is the output's and still exact; the other keeps the *rectangle's*
    // ratio, not the photo's.
    assert_eq!(field(&output, "out_w"), "64");
    assert_eq!(field(&output, "out_h"), "48");

    let whole = image::open(&whole)
        .expect("open the photo at 1:1")
        .to_rgb8();
    let region = image::open(&region).expect("open the rectangle").to_rgb8();
    assert_eq!(region.dimensions(), (64, 48));
    for y in 0..48u32 {
        for x in 0..64u32 {
            assert_eq!(
                region.get_pixel(x, y),
                whole.get_pixel(x + 40, y + 30),
                "the rectangle differs from the photo at {x},{y}"
            );
        }
    }

    // A rectangle the photo does not contain is a failure (exit 2), not a usage error:
    // its shape is fine, and the file's own size is what refuses it. The message names
    // both.
    let refused = run(&[
        "thumb",
        "--photo",
        photo.to_str().unwrap(),
        "--px",
        "64",
        "--region",
        "160,120,64,48",
        "--out",
        dir.join("outside.png").to_str().unwrap(),
    ]);
    assert_eq!(code(&refused), 2, "{}", stderr(&refused));
    assert!(stdout(&refused).is_empty(), "{}", stdout(&refused));
    let message = stderr(&refused);
    assert!(
        message.contains("the region 160,120 64x48 is not inside the 200x150 photo"),
        "{message}"
    );
    assert!(
        !dir.join("outside.png").exists(),
        "a refusal writes nothing"
    );
}

#[test]
fn scan_and_thumb_keep_the_usage_and_locale_rules() {
    let dir = out_dir("scan-thumb-usage");
    let library = dir.join("library");
    library_dir(&library);
    let path = library.to_str().expect("utf-8 path");

    // Usage errors, all of them: a flag from another subcommand, a missing
    // required flag, an out-of-range size, an output format this build does not
    // write. Exit 1, stdout empty, stderr naming the problem.
    for args in [
        vec!["scan"],
        vec!["scan", "--project", "x.pixlay"],
        vec!["scan", "--out", "x.png"],
        vec!["scan", "--dpi", "300"],
        vec!["scan", "--dir", path, "--recursive", "--dir", path],
        vec!["thumb", "--photo", "x.jpg", "--px", "10"],
        vec!["thumb", "--px", "10", "--out", "x.png"],
        vec!["thumb", "--photo", "x.jpg", "--out", "x.png"],
        vec!["thumb", "--photo", "x.jpg", "--px", "0", "--out", "x.png"],
        vec![
            "thumb", "--photo", "x.jpg", "--px", "9000", "--out", "x.png",
        ],
        vec!["thumb", "--photo", "x.jpg", "--px", "10", "--out", "x.gif"],
        vec![
            "thumb", "--photo", "x.jpg", "--px", "10", "--out", "x.png", "--dir", "/tmp",
        ],
        vec![
            "thumb", "--photo", "x.jpg", "--photo", "y.jpg", "--px", "10", "--out", "x.png",
        ],
        // `--region` is S15j's flag, and it has its own shape: four whole numbers with
        // an area.
        vec![
            "thumb", "--photo", "x.jpg", "--px", "10", "--out", "x.png", "--region", "1,2,3",
        ],
        vec![
            "thumb",
            "--photo",
            "x.jpg",
            "--px",
            "10",
            "--out",
            "x.png",
            "--region",
            "1,2,3,4,5",
        ],
        vec![
            "thumb", "--photo", "x.jpg", "--px", "10", "--out", "x.png", "--region", "-1,2,3,4",
        ],
        vec![
            "thumb", "--photo", "x.jpg", "--px", "10", "--out", "x.png", "--region", "0,0,0,4",
        ],
        vec![
            "thumb",
            "--photo",
            "x.jpg",
            "--px",
            "10",
            "--out",
            "x.png",
            "--region",
            "1,2,3.5,4",
        ],
        // And a flag from another subcommand is still refused on the ones that do not
        // take it.
        vec![
            "render",
            "--template",
            "mosaic-8-s14",
            "--px",
            "10",
            "--region",
            "1,2,3,4",
        ],
        vec![
            "render",
            "--template",
            "mosaic-8-s14",
            "--long-edge",
            "3370",
            "--out",
            "x.png",
            "--px",
            "10",
        ],
        vec!["image", "--photo", "x.jpg", "--dir", "/tmp"],
    ] {
        let output = run(&args);
        assert_eq!(code(&output), 1, "{args:?}: {}", stderr(&output));
        assert!(stdout(&output).is_empty(), "{args:?} wrote to stdout");
        assert!(!stderr(&output).is_empty(), "{args:?} said nothing");
    }

    // A `--dir` that is not a directory exists but cannot be used: exit 2, and
    // the path is named.
    for bad in [dir.join("absent"), dir.join("library/square.png")] {
        let output = run(&["scan", "--dir", bad.to_str().unwrap()]);
        assert_eq!(code(&output), 2, "{bad:?}: {}", stderr(&output));
        assert!(stdout(&output).is_empty());
        assert!(
            stderr(&output).contains(bad.file_name().unwrap().to_str().unwrap()),
            "{}",
            stderr(&output)
        );
    }

    // stdout and stderr are byte-identical under any locale, on the report and on
    // the refusal alike.
    let mut listed = Vec::new();
    for (lang, all) in [
        ("C", "C"),
        ("zh_CN.UTF-8", "zh_CN.UTF-8"),
        ("de_DE.UTF-8", "de_DE.UTF-8"),
    ] {
        let scan = run_in(&["scan", "--dir", path, "--json"], None, Some((lang, all)));
        assert_eq!(code(&scan), 0, "{lang}: {}", stderr(&scan));
        assert!(scan.stderr.is_empty(), "{lang}: {}", stderr(&scan));
        listed.push(scan.stdout.clone());

        let thumb = run_in(
            &[
                "thumb",
                "--photo",
                library.join("square.png").to_str().unwrap(),
                "--px",
                "32",
                "--out",
                dir.join("locale.png").to_str().unwrap(),
                "--json",
            ],
            None,
            Some((lang, all)),
        );
        assert_eq!(code(&thumb), 0, "{lang}: {}", stderr(&thumb));
        assert!(thumb.stderr.is_empty(), "{lang}: {}", stderr(&thumb));
    }
    assert!(
        listed.iter().all(|stdout| stdout == &listed[0]),
        "scan changed under a locale"
    );

    // `--help` documents the extensions `scan` actually accepts: the two lists
    // are the user's only way to find out why a folder came back empty. The list
    // itself lives in `pixlay-imaging` since S13, because the picker's library
    // grid walks the same folder and the two surfaces must agree.
    let help = run(&["--help"]);
    assert_eq!(code(&help), 0);
    for extension in pixlay_imaging::PHOTO_EXTENSIONS {
        assert!(
            stdout(&help).contains(&format!(".{extension}")),
            "--help does not mention .{extension}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn gesture_measures_a_step_without_decoding_it() {
    let dir = out_dir("gesture");
    let project = write_full_project(&dir, "two.pixlay");
    let path = project.to_str().unwrap().to_string();

    // A short sequence: the counts and the relation are what this test is about,
    // and 60 steps would only make it slow (`DEFAULT_GESTURE_STEPS` is what a
    // real measurement uses).
    let output = run(&[
        "gesture",
        "--project",
        &path,
        "--grid",
        "400",
        "--steps",
        "6",
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(stderr(&output).is_empty(), "{}", stderr(&output));

    // The field set is the contract, and the split of the sequence into
    // open/cold/warm/refine is what the four decode counts report.
    let report = stdout(&output);
    let fields: Vec<&str> = report
        .lines()
        .map(|line| line.split_once(" = ").expect("key = value").0)
        .collect();
    assert_eq!(
        fields,
        vec![
            "budget_ms",
            "cold_decodes",
            "command",
            "gesture_h",
            "gesture_w",
            "grid_h",
            "grid_w",
            "occupied",
            "open_decodes",
            "refine_decodes",
            "slot",
            "slots",
            "src_h",
            "src_w",
            "step_deg",
            "steps",
            "template",
            "verdict",
            "version",
            "warm_decodes",
        ]
    );
    assert_eq!(field(&output, "command"), "gesture");
    assert_eq!(field(&output, "template"), "test-2");
    assert_eq!(field(&output, "slots"), "2");
    assert_eq!(field(&output, "occupied"), "2");
    assert_eq!(field(&output, "slot"), "0");
    assert_eq!(field(&output, "steps"), "6");
    assert_eq!(field(&output, "step_deg"), "1.000000");
    // The resting grid is the one asked for, at the template's own aspect, and the
    // grid a gesture draws at is half of it in each direction.
    assert_eq!(field(&output, "grid_w"), "400");
    assert_eq!(field(&output, "grid_h"), "300");
    assert_eq!(field(&output, "gesture_w"), "200");
    assert_eq!(field(&output, "gesture_h"), "150");
    // S12's central claim, as a count: opening the document decodes its photos, and
    // **no step of the gesture after the first frame decodes anything at all**. A
    // warm step that still re-decoded its photo would show a number here, and this
    // is the assertion that would catch a cache that quietly stopped working. The
    // one frame that does decode is the live gesture's first: since S12b each grid
    // has its own preview-grade source, so the coarse grid builds its copies once
    // (two photos, two decodes) and every frame after it — and the release, which
    // is the resting grid again — is served from the cache.
    assert_eq!(field(&output, "open_decodes"), "2");
    assert_eq!(field(&output, "cold_decodes"), "2");
    assert_eq!(field(&output, "warm_decodes"), "0");
    assert_eq!(field(&output, "refine_decodes"), "0");
    // The source the *warm step* resampled (S12b): a preview-grade copy, not the
    // file. `square.png` is 640x640, and a warm step runs at the gesture grid — half
    // of 400x300 — whose own target is 1.25x (`PREVIEW_SOURCE_SCALE`) its 200-px
    // long edge, so 250x250. Before the reduction this field was the photo's own
    // size, and against a 24 MP photo it is the whole difference between a step that
    // reads 24 MP and one that reads a megapixel.
    assert_eq!(field(&output, "src_w"), "250");
    assert_eq!(field(&output, "src_h"), "250");
    assert_eq!(field(&output, "budget_ms"), "16.666667");
    // And the field is about **the cell that step rebuilt**, not about the
    // document's biggest source (S15f, PIX-027C): cell 1 is `landscape.jpg`
    // (960x540), so framing *it* reports its own 250x141 copy — the pair is one
    // copy's two edges and never a width from one beside a height from another.
    let other = run(&[
        "gesture",
        "--project",
        &path,
        "--grid",
        "400",
        "--steps",
        "6",
        "--slot",
        "1",
    ]);
    assert_eq!(code(&other), 0, "{}", stderr(&other));
    assert_eq!(field(&other, "slot"), "1");
    assert_eq!(field(&other, "src_w"), "250");
    assert_eq!(field(&other, "src_h"), "141");
    // The verdict is the warm median against that budget: the test asserts the
    // relation, not a value, so it holds on a slow machine and a fast one.
    assert!(
        ["pipeline_holds", "gpu_preview"].contains(&field(&output, "verdict").as_str()),
        "{}",
        field(&output, "verdict")
    );

    // `--stats` adds the measurements, and the verdict is what the warm *median*
    // is compared against (`warm_max_ms` is reported, not judged).
    let measured = run(&[
        "gesture",
        "--project",
        &path,
        "--grid",
        "400",
        "--steps",
        "6",
        "--stats",
        "--json",
    ]);
    assert_eq!(code(&measured), 0, "{}", stderr(&measured));
    let json: serde_json::Value = serde_json::from_str(&stdout(&measured)).expect("json");
    for key in [
        "ms",
        "open_ms",
        "cold_ms",
        "warm_ms",
        "warm_max_ms",
        "refine_ms",
        "peak_rss_mb",
    ] {
        assert!(
            json[key].as_f64().unwrap_or_default() > 0.0,
            "{key} = {}",
            json[key]
        );
    }
    assert!(
        json["warm_max_ms"].as_f64().expect("warm_max_ms")
            >= json["warm_ms"].as_f64().expect("warm_ms"),
        "the worst step cannot be better than the median"
    );
    let holds = json["warm_ms"].as_f64().expect("warm_ms") <= 16.666_667;
    assert_eq!(
        json["verdict"].as_str().expect("verdict"),
        if holds {
            "pipeline_holds"
        } else {
            "gpu_preview"
        }
    );
    // The counts do not move when the timings are asked for.
    assert_eq!(json["warm_decodes"].as_i64(), Some(0));
    assert_eq!(json["open_decodes"].as_i64(), Some(2));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn gesture_refuses_what_it_cannot_measure() {
    let dir = out_dir("gesture-refusals");
    let project = write_full_project(&dir, "two.pixlay");
    let path = project.to_str().unwrap().to_string();

    // Usage errors: exit 1, stdout empty, stderr naming the problem.
    for args in [
        vec!["gesture"],
        vec!["gesture", "--grid", "400"],
        vec!["gesture", "--project", &path],
        vec!["gesture", "--project", &path, "--grid", "0"],
        vec!["gesture", "--project", &path, "--grid", "20001"],
        vec![
            "gesture",
            "--project",
            &path,
            "--grid",
            "400",
            "--steps",
            "1",
        ],
        vec![
            "gesture",
            "--project",
            &path,
            "--grid",
            "400",
            "--steps",
            "4000",
        ],
        vec![
            "gesture",
            "--project",
            &path,
            "--grid",
            "400",
            "--slot",
            "9",
        ],
        vec![
            "gesture",
            "--project",
            &path,
            "--grid",
            "400",
            "--out",
            "x.png",
        ],
        vec![
            "gesture",
            "--project",
            &path,
            "--grid",
            "400",
            "--long-edge",
            "300",
        ],
        vec![
            "gesture",
            "--project",
            &path,
            "--grid",
            "400",
            "--slots",
            "2",
        ],
        // The flags belong to `gesture` alone.
        vec![
            "render",
            "--template",
            "mosaic-8-s14",
            "--grid",
            "400",
            "--out",
            "x.png",
        ],
        vec!["scan", "--dir", ".", "--grid", "400"],
        vec!["probe", "--project", &path, "--steps", "10"],
    ] {
        let output = run(&args);
        assert_eq!(code(&output), 1, "{args:?}: {}", stderr(&output));
        assert!(stdout(&output).is_empty(), "{args:?} wrote to stdout");
        assert!(!stderr(&output).is_empty(), "{args:?} said nothing");
    }

    // A cell with no photo cannot be framed, and the message names the slots that
    // can.
    let half = write_project(&dir, "half.pixlay", true);
    let output = run(&[
        "gesture",
        "--project",
        half.to_str().unwrap(),
        "--grid",
        "400",
        "--slot",
        "1",
    ]);
    assert_eq!(code(&output), 1, "{}", stderr(&output));
    assert!(stdout(&output).is_empty());
    assert!(stderr(&output).contains("--slot 1"), "{}", stderr(&output));

    // A document with nothing to gesture on, and a project that is not there, are
    // failures to produce a result: exit 2.
    let empty = dir.join("empty.pixlay");
    let created = run(&[
        "init",
        "--template",
        "mosaic-5-hero",
        "--out",
        empty.to_str().unwrap(),
    ]);
    assert_eq!(code(&created), 0, "{}", stderr(&created));
    for args in [
        vec![
            "gesture",
            "--project",
            empty.to_str().unwrap(),
            "--grid",
            "400",
        ],
        vec![
            "gesture",
            "--project",
            dir.join("absent.pixlay").to_str().unwrap(),
            "--grid",
            "400",
        ],
    ] {
        let output = run(&args);
        assert_eq!(code(&output), 2, "{args:?}: {}", stderr(&output));
        assert!(stdout(&output).is_empty(), "{args:?} wrote to stdout");
        assert!(!stderr(&output).is_empty(), "{args:?} said nothing");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn gesture_keeps_the_usage_and_locale_rules() {
    let dir = out_dir("gesture-locale");
    let project = write_full_project(&dir, "two.pixlay");
    let path = project.to_str().unwrap().to_string();

    // The report is the same under every locale. The measured fields are the
    // documented exception (`--stats`), so the byte-identical claim is made on the
    // shape without them — which is also the shape the counts live in.
    let mut listed = Vec::new();
    for (lang, all) in [
        ("C", "C"),
        ("zh_CN.UTF-8", "zh_CN.UTF-8"),
        ("de_DE.UTF-8", "de_DE.UTF-8"),
    ] {
        let gesture = run_in(
            &[
                "gesture",
                "--project",
                &path,
                "--grid",
                "400",
                "--steps",
                "6",
                "--json",
            ],
            None,
            Some((lang, all)),
        );
        assert_eq!(code(&gesture), 0, "{lang}: {}", stderr(&gesture));
        assert!(gesture.stderr.is_empty(), "{lang}: {}", stderr(&gesture));
        listed.push(gesture.stdout.clone());
    }
    assert!(
        listed.iter().all(|stdout| stdout == &listed[0]),
        "gesture changed under a locale: {}",
        String::from_utf8_lossy(&listed[1])
    );

    // `--help` documents the flags this command takes, including the required one.
    let help = run(&["--help"]);
    assert_eq!(code(&help), 0);
    for flag in ["--grid", "--slot", "--steps", "gesture"] {
        assert!(
            stdout(&help).contains(flag),
            "--help does not mention {flag}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// `switch` (S18): one layout change, in phases, at a canvas box.
///
/// The numbers themselves are the step's measurement and live in
/// `docs/CONTRACT.md` §8; what is pinned here is the *shape*: which phases the
/// report splits, that the two grids come from the box (the widget's own rule, less
/// its margin), and that a layout change which moves the preview-grade edge decodes
/// the photos again while one that does not keeps them.
#[test]
fn switch_measures_a_layout_change_at_the_canvas_box() {
    let dir = out_dir("switch");
    let project = write_full_project(&dir, "two.pixlay");
    let path = project.to_str().unwrap().to_string();

    let output = run(&[
        "switch",
        "--project",
        &path,
        "--template",
        "strip-2-1x2",
        "--canvas",
        "400x300",
        "--stats",
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(stderr(&output).is_empty(), "{}", stderr(&output));

    let report = stdout(&output);
    let fields: Vec<&str> = report
        .lines()
        .map(|line| line.split_once(" = ").expect("key = value").0)
        .collect();
    assert_eq!(
        fields,
        vec![
            "budget_ms",
            "canvas_h",
            "canvas_w",
            "command",
            "composite_ms",
            "decodes",
            "from_grid_h",
            "from_grid_w",
            "from_template",
            "grid_h",
            "grid_w",
            "icc",
            "ms",
            "occupied",
            "open_decodes",
            "open_ms",
            "peak_rss_mb",
            "slots",
            "sources_ms",
            "src_h",
            "src_w",
            "switch_ms",
            "template",
            "template_ms",
            "verdict",
            "version",
        ]
    );
    assert_eq!(field(&output, "command"), "switch");
    assert_eq!(field(&output, "from_template"), "test-2");
    assert_eq!(field(&output, "template"), "strip-2-1x2");
    assert_eq!(field(&output, "version"), "1");
    assert_eq!(field(&output, "slots"), "2");
    assert_eq!(field(&output, "occupied"), "2");
    // The canvas box is what both grids come from, and the rule is the window's
    // (`pixlay_core::canvas_grid`): 400x300 less the 12 px margin on every side is
    // 376x276, and the largest 4:3 grid inside it is 368x276 while the largest 2:3
    // one is 184x276 — the first is height-limited (376/276 > 4/3) and the second is
    // too (376/276 > 2/3), which is exactly the asymmetry a layout change hits.
    assert_eq!(field(&output, "canvas_w"), "400");
    assert_eq!(field(&output, "canvas_h"), "300");
    assert_eq!(field(&output, "from_grid_w"), "368");
    assert_eq!(field(&output, "from_grid_h"), "276");
    assert_eq!(field(&output, "grid_w"), "184");
    assert_eq!(field(&output, "grid_h"), "276");
    // The document's own template is the test's 4:3 one and the target is portrait:
    // the preview-grade edge moves with the grid (1.25 x the long edge: 470 → 345),
    // so both photos are decoded again. This is the phase the human's finding is
    // about, and a switch that *keeps* the edge decodes nothing — the second run
    // below.
    assert_eq!(field(&output, "open_decodes"), "2");
    assert_eq!(field(&output, "decodes"), "2");
    // The copy the composite resampled: a 640x640 photo at a 345-px target.
    assert_eq!(field(&output, "src_w"), "345");
    assert_eq!(field(&output, "src_h"), "345");
    // `verdict` is a reading, like `--stats`'s times: it is reported against
    // `budget_ms` and the exit code does not move with it (S18's gate is the human's
    // ruling on the number, so a test that pinned it would pin the machine).

    // A switch whose grids share their long edge keeps the copies and decodes
    // nothing: at a 300x600 box the available 276x576 is taller than both layouts'
    // aspects, so the width limits both — 276x207 for the 4:3 document and 276x184
    // for a 3:2 target — and the preview-grade edge (1.25 x 276 = 345) is the one the
    // open already built. The grids still differ, so the *bitmaps* are rebuilt; the
    // difference between this run and the one above is the whole of `sources_ms`.
    let kept = run(&[
        "switch",
        "--project",
        &path,
        "--template",
        "strip-2-2x1",
        "--canvas",
        "300x600",
        "--stats",
    ]);
    assert_eq!(code(&kept), 0, "{}", stderr(&kept));
    assert_eq!(field(&kept, "from_grid_w"), "276");
    assert_eq!(field(&kept, "from_grid_h"), "207");
    assert_eq!(field(&kept, "grid_w"), "276");
    assert_eq!(field(&kept, "grid_h"), "184");
    assert_eq!(
        field(&kept, "decodes"),
        "0",
        "the same edge means the copies are in hand"
    );
    assert_eq!(field(&kept, "open_decodes"), "2");

    // The band's own rebuild, when it is asked for: every candidate of the new cell
    // count, at the grid the window draws a candidate at. Since S21 a candidate is
    // a *sketch* — its cells' outlines over the sheet's ground — so the band's loop
    // names no photo and decodes nothing; what the row above says about `decodes`
    // is therefore the canvas's own, and this run's `decodes` is the same number
    // with the band's rebuild included.
    let banded = run(&[
        "switch",
        "--project",
        &path,
        "--template",
        "strip-2-1x2",
        "--canvas",
        "400x300",
        "--band",
        "--stats",
    ]);
    assert_eq!(code(&banded), 0, "{}", stderr(&banded));
    assert_eq!(
        field(&banded, "band_candidates"),
        templates::with_slots(2).len().to_string()
    );
    assert!(
        stdout(&banded).contains("band_ms = "),
        "--band prints the band's own time"
    );
    // Without the flag the band is not built, and nothing about it is reported.
    assert!(
        !stdout(&output).contains("band_ms"),
        "a run without --band measured the band"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// `switch` refuses what it cannot measure, and refuses flags that are not its own.
#[test]
fn switch_refuses_what_it_cannot_measure() {
    let dir = out_dir("switch-refusals");
    let project = write_full_project(&dir, "two.pixlay");
    let path = project.to_str().unwrap().to_string();

    for args in [
        vec!["switch"],
        vec!["switch", "--template", "strip-2-1x2"],
        vec!["switch", "--project", &path],
        vec!["switch", "--project", &path, "--template", "nope"],
        vec![
            "switch",
            "--project",
            &path,
            "--template",
            "strip-2-1x2",
            "--canvas",
            "0x300",
        ],
        vec![
            "switch",
            "--project",
            &path,
            "--template",
            "strip-2-1x2",
            "--canvas",
            "400",
        ],
        vec![
            "switch",
            "--project",
            &path,
            "--template",
            "strip-2-1x2",
            "--canvas",
            "400x20001",
        ],
        vec![
            "switch",
            "--project",
            &path,
            "--template",
            "strip-2-1x2",
            "--canvas",
            "400x300",
            "--grid",
            "400",
        ],
        vec![
            "switch",
            "--project",
            &path,
            "--template",
            "strip-2-1x2",
            "--out",
            "x.png",
        ],
        vec![
            "switch",
            "--project",
            &path,
            "--template",
            "strip-2-1x2",
            "--steps",
            "3",
        ],
        vec![
            "switch",
            "--project",
            &path,
            "--template",
            "strip-2-1x2",
            "--long-edge",
            "300",
        ],
        vec![
            "switch",
            "--project",
            &path,
            "--template",
            "strip-2-1x2",
            "--slot",
            "0",
        ],
        // The flags belong to `switch` alone.
        vec![
            "render",
            "--template",
            "mosaic-8-s14",
            "--band",
            "--out",
            "x.png",
        ],
        vec![
            "gesture",
            "--project",
            &path,
            "--grid",
            "400",
            "--canvas",
            "400x300",
        ],
        vec!["probe", "--project", &path, "--band"],
    ] {
        let output = run(&args);
        assert_eq!(code(&output), 1, "{args:?}: {}", stderr(&output));
        assert!(stdout(&output).is_empty(), "{args:?} wrote to stdout");
        assert!(!stderr(&output).is_empty(), "{args:?} said nothing");
    }

    // A project that is not there, and one whose photo is gone: exit 2, with the
    // reason on stderr. A switch that could not draw its photos is not a number.
    let missing = dir.join("missing.pixlay");
    let output = run(&[
        "switch",
        "--project",
        missing.to_str().unwrap(),
        "--template",
        "strip-2-1x2",
    ]);
    assert_eq!(code(&output), 2, "{}", stderr(&output));
    assert!(stdout(&output).is_empty());

    let gone = write_project(&dir, "gone.pixlay", true);
    std::fs::remove_file(project_photo(&dir)).expect("remove the photo");
    let output = run(&[
        "switch",
        "--project",
        gone.to_str().unwrap(),
        "--template",
        "strip-2-1x2",
    ]);
    assert_eq!(code(&output), 2, "{}", stderr(&output));
    assert!(
        stderr(&output).contains("photo.png"),
        "the reason names the file: {}",
        stderr(&output)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// `switch` keeps the usage and locale rules every subcommand shares.
#[test]
fn switch_keeps_the_usage_and_locale_rules() {
    let dir = out_dir("switch-locale");
    let project = write_full_project(&dir, "two.pixlay");
    let path = project.to_str().unwrap().to_string();

    // The report is the same under every locale. The measured fields are the
    // documented exception (`--stats`), and the verdict is a reading of one, so the
    // byte-identical claim is made on the shape without them — which is also the
    // shape the counts and the grids live in.
    let mut listed = Vec::new();
    for (lang, all) in [
        ("C", "C"),
        ("zh_CN.UTF-8", "zh_CN.UTF-8"),
        ("de_DE.UTF-8", "de_DE.UTF-8"),
    ] {
        let switch = run_in(
            &[
                "switch",
                "--project",
                &path,
                "--template",
                "strip-2-1x2",
                "--canvas",
                "400x300",
                "--json",
            ],
            None,
            Some((lang, all)),
        );
        assert_eq!(code(&switch), 0, "{lang}: {}", stderr(&switch));
        assert!(switch.stderr.is_empty(), "{lang}: {}", stderr(&switch));
        listed.push(switch.stdout.clone());
    }
    assert!(
        listed.iter().all(|stdout| stdout == &listed[0]),
        "switch changed under a locale: {}",
        String::from_utf8_lossy(&listed[1])
    );

    // `--help` documents the flags this command takes, including the required one.
    let help = run(&["--help"]);
    assert_eq!(code(&help), 0);
    for flag in ["--canvas", "--band", "switch", "--template <name>"] {
        assert!(
            stdout(&help).contains(flag),
            "--help does not mention {flag}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// The photo `write_project` put in cell 0, as the path the CLI is asked to write over.
fn project_photo(dir: &Path) -> PathBuf {
    dir.join("photo.png")
}

#[test]
fn an_output_that_is_one_of_the_projects_photos_is_refused() {
    // `AGENTS.md`: source images are read-only. `render --out <a photo of this very
    // project>` used to decode the photo and then write the render over it, which is
    // irreversible user-data loss (S15c, PIX-001). Every spelling of "that same file"
    // is refused, and the refusal is a usage error: nothing is rendered, nothing is
    // written, and the photo is byte-identical afterwards.
    let dir = out_dir("render-alias");
    let project = write_project(&dir, "aliased.pixlay", true);
    let photo = project_photo(&dir);
    let original = std::fs::read(&photo).expect("read the photo");

    // A second directory, so `..` has somewhere to come back from.
    std::fs::create_dir(dir.join("nested")).expect("create nested");
    let literal = photo.clone();
    let dotdot = dir.join("nested").join("..").join("photo.png");
    let link = dir.join("link.png");
    std::os::unix::fs::symlink(&photo, &link).expect("symlink to the photo");
    let hard = dir.join("hard.png");
    std::fs::hard_link(&photo, &hard).expect("hard link to the photo");

    for (name, out) in [
        ("literal", literal),
        ("dot-dot", dotdot),
        ("symlink", link),
        ("hard link", hard),
    ] {
        let refused = run(&[
            "render",
            "--project",
            project.to_str().unwrap(),
            "--long-edge",
            "400",
            "--out",
            out.to_str().unwrap(),
        ]);
        assert_eq!(code(&refused), 1, "{name}: {}", stderr(&refused));
        assert!(
            stdout(&refused).is_empty(),
            "{name}: stdout must stay empty"
        );
        assert!(
            stderr(&refused).contains("refusing to write"),
            "{name}: {}",
            stderr(&refused)
        );
        assert!(
            stderr(&refused).contains("photo.png"),
            "{name}: the message names the photo: {}",
            stderr(&refused)
        );
        assert_eq!(
            std::fs::read(&photo).expect("read the photo"),
            original,
            "{name}: the photo changed"
        );
    }

    // A render that fails for another reason does not touch its output either: the
    // target keeps what it had until the whole render has succeeded (S15c, PIX-011).
    let other = dir.join("other.png");
    std::fs::write(&other, b"not a picture yet").expect("write");
    let doc = Project::load(&project).expect("loads");
    let mut doc = doc.doc().clone();
    doc.cells[0].source = Some(PathBuf::from("gone.png"));
    std::fs::write(&project, doc.to_json().expect("serializes")).expect("write");
    let failed = run(&[
        "render",
        "--project",
        project.to_str().unwrap(),
        "--long-edge",
        "400",
        "--out",
        other.to_str().unwrap(),
    ]);
    assert_eq!(code(&failed), 2, "{}", stderr(&failed));
    assert!(
        stderr(&failed).contains("gone.png"),
        "the missing photo is what failed: {}",
        stderr(&failed)
    );
    assert_eq!(
        std::fs::read(&other).expect("read"),
        b"not a picture yet",
        "a failed render replaced the file it was writing"
    );

    // With the photo back, the same command succeeds and replaces the target — the
    // alias rule is about the photos alone, not about "any output that exists".
    std::fs::write(&photo, &original).expect("restore the photo");
    doc.cells[0].source = Some(PathBuf::from("photo.png"));
    std::fs::write(&project, doc.to_json().expect("serializes")).expect("write");
    let rendered = run(&[
        "render",
        "--project",
        project.to_str().unwrap(),
        "--long-edge",
        "400",
        "--out",
        other.to_str().unwrap(),
    ]);
    assert_eq!(code(&rendered), 0, "{}", stderr(&rendered));
    assert_ne!(std::fs::read(&other).expect("read"), b"not a picture yet");
    assert_eq!(field(&rendered, "out_w"), "400");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn thumb_refuses_to_write_over_the_photo_it_reads() {
    let dir = out_dir("thumb-alias");
    let photo = dir.join("photo.png");
    std::fs::write(&photo, include_bytes!("fixtures/photos/square.png")).expect("write photo");
    let original = std::fs::read(&photo).expect("read");

    for out in [photo.clone(), {
        let link = dir.join("link.png");
        std::os::unix::fs::symlink(&photo, &link).expect("symlink");
        link
    }] {
        let refused = run(&[
            "thumb",
            "--photo",
            photo.to_str().unwrap(),
            "--px",
            "128",
            "--out",
            out.to_str().unwrap(),
        ]);
        assert_eq!(code(&refused), 1, "{}", stderr(&refused));
        assert!(stdout(&refused).is_empty());
        assert!(
            stderr(&refused).contains("refusing to write"),
            "{}",
            stderr(&refused)
        );
        assert_eq!(std::fs::read(&photo).expect("read"), original);
    }

    // The spelling rule is the paths' and not the filesystem's: a photo that is not
    // there is still refused as an output rather than reported as a photo that cannot
    // be decoded, because the check comes before the decode (S15c).
    let absent = dir.join("absent.png");
    let refused = run(&[
        "thumb",
        "--photo",
        absent.to_str().unwrap(),
        "--px",
        "128",
        "--out",
        absent.to_str().unwrap(),
    ]);
    assert_eq!(code(&refused), 1, "{}", stderr(&refused));
    assert!(
        stderr(&refused).contains("refusing to write"),
        "{}",
        stderr(&refused)
    );

    // The same command to a path of its own writes a preview, as it always did.
    let preview = dir.join("preview.png");
    let ok = run(&[
        "thumb",
        "--photo",
        photo.to_str().unwrap(),
        "--px",
        "128",
        "--out",
        preview.to_str().unwrap(),
    ]);
    assert_eq!(code(&ok), 0, "{}", stderr(&ok));
    assert_eq!(field(&ok, "out_w"), "128");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn two_concurrent_inits_leave_exactly_one_winner() {
    // `init` never overwrites a project, and the refusal is the creation itself:
    // `create_new` makes "is it there" and "make it" one operation, so two processes
    // racing leave one project rather than both seeing an absent path and one
    // truncating the other's (S15c, PIX-015).
    let dir = out_dir("init-race");
    let path = dir.join("raced.pixlay");
    let spawn = || {
        Command::new(BIN)
            .args([
                "init",
                "--template",
                "mosaic-8-s14",
                "--out",
                path.to_str().unwrap(),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("LANG", "C")
            .env("LC_ALL", "C")
            .env("HOME", "/nonexistent")
            .spawn()
            .expect("spawn pixlay-render")
    };
    let children = [spawn(), spawn()];
    let outcomes: Vec<Output> = children
        .into_iter()
        .map(|child| child.wait_with_output().expect("wait"))
        .collect();
    let codes: Vec<i32> = outcomes.iter().map(code).collect();
    assert_eq!(
        codes.iter().filter(|code| **code == 0).count(),
        1,
        "exactly one init must win, got {codes:?}: {}",
        outcomes.iter().map(stderr).collect::<Vec<_>>().join(" | ")
    );
    let loser = outcomes
        .iter()
        .find(|outcome| code(outcome) != 0)
        .expect("one of them lost");
    assert_eq!(code(loser), 2);
    assert!(
        stderr(loser).contains("never overwrites"),
        "{}",
        stderr(loser)
    );

    // The winner's file is a whole project, not a truncated one.
    let doc = CollageDoc::load(&path).expect("the winner's project loads");
    assert_eq!(doc.template.name, "mosaic-8-s14");
    assert_eq!(doc.cells.len(), doc.template.slots.len());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn init_refuses_a_symbolic_link_at_the_output_path() {
    // A link at `--out` is a path that is already there: `create_new` refuses it
    // whether or not its target exists, so a dangling link cannot be followed into a
    // file `init` was never asked to write (S15c, PIX-015).
    let dir = out_dir("init-symlink");
    let real = dir.join("real.pixlay");
    let created = run(&[
        "init",
        "--template",
        "mosaic-8-s14",
        "--out",
        real.to_str().unwrap(),
    ]);
    assert_eq!(code(&created), 0, "{}", stderr(&created));
    let before = std::fs::read(&real).expect("read");

    let link = dir.join("link.pixlay");
    std::os::unix::fs::symlink(&real, &link).expect("symlink");
    let refused = run(&[
        "init",
        "--template",
        "grid-4-2x2",
        "--out",
        link.to_str().unwrap(),
    ]);
    assert_eq!(code(&refused), 2, "{}", stderr(&refused));
    assert!(
        stderr(&refused).contains("never overwrites"),
        "{}",
        stderr(&refused)
    );
    assert_eq!(
        std::fs::read(&real).expect("read"),
        before,
        "init wrote through the link into the project it points at"
    );

    let dangling = dir.join("dangling.pixlay");
    let nowhere = dir.join("nowhere.pixlay");
    std::os::unix::fs::symlink(&nowhere, &dangling).expect("symlink");
    let refused = run(&[
        "init",
        "--template",
        "mosaic-8-s14",
        "--out",
        dangling.to_str().unwrap(),
    ]);
    assert_eq!(code(&refused), 2, "{}", stderr(&refused));
    assert!(
        !nowhere.exists(),
        "init followed a dangling link and created its target"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
