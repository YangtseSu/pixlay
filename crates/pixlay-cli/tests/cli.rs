//! The CLI contract, as tests: the machine surface is what every later step's
//! verification loop depends on, so its promises are pinned here.
//!
//! These run the built binary as a subprocess, because that is the only way to
//! observe the things the contract is about: stdout purity, exit codes, the
//! absence of a TTY, and locale independence.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use pixlay_core::{
    CanvasSpec, Cell, CollageDoc, CropTransform, Point, Polygon, Project, Slot, Template, templates,
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
    let mut doc = CollageDoc::new(CanvasSpec::new(120.0, 90.0), template);
    if fill {
        let photo = dir.join("photo.png");
        std::fs::write(&photo, include_bytes!("fixtures/photos/square.png")).expect("write photo");
        doc.cells[0] = Cell {
            source: Some(PathBuf::from("photo.png")),
            crop: Default::default(),
            grade: Default::default(),
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
        grade: Default::default(),
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
        "--dpi",
        "--preview-px",
        "--photo",
        "--dir",
        "--recursive",
        "--px",
        "--json",
        "--stats",
    ] {
        assert!(stdout(&help).contains(flag), "{flag} missing from --help");
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
        "--dpi",
        "72",
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

    // The declared canvas is 1189 x 891.75 mm; at 72 dpi that is 3370 x 2528 px.
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
fn a_project_renders_and_names_its_output() {
    let dir = out_dir("project");
    let project = write_project(&dir, "two.pixlay", true);
    let out = dir.join("two.jpg");
    let output = run(&[
        "render",
        "--project",
        project.to_str().unwrap(),
        "--dpi",
        "150",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "format"), "jpeg");
    assert_eq!(field(&output, "occupied"), "1");
    // 120 x 90 mm at 150 dpi.
    assert_eq!(field(&output, "out_w"), "709");
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
        "--dpi",
        "300",
        "--preview-px",
        "800",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "preview_px"), "800");
    let width: i32 = field(&output, "out_w").parse().unwrap();
    let height: i32 = field(&output, "out_h").parse().unwrap();
    assert_eq!(width.max(height), 800, "{width}x{height}");
    // 4:3 canvas.
    assert_eq!((width, height), (800, 600));
    let _ = std::fs::remove_dir_all(&dir);
}

/// The PNG `pHYs` chunk, as `(xppu, yppu, unit)`. Read from the file's own chunk
/// stream, so a `--long-edge` assertion is about the export and not about the
/// report the same command printed.
fn png_pixel_dimensions(path: &Path) -> (u32, u32, u8) {
    let bytes = std::fs::read(path).expect("read the PNG");
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    let mut at = 8;
    while at + 12 <= bytes.len() {
        let length = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        if &bytes[at + 4..at + 8] == b"pHYs" {
            let data = &bytes[at + 8..at + 8 + length];
            return (
                u32::from_be_bytes(data[0..4].try_into().unwrap()),
                u32::from_be_bytes(data[4..8].try_into().unwrap()),
                data[8],
            );
        }
        at += 12 + length;
    }
    panic!("no pHYs chunk in {}", path.display());
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
    // `mosaic-8-s14` is 4:3 on a 1189 x 891.75 mm canvas, so the long edge is the
    // width and the short one is exactly 3/4 of it.
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

    // The resolution the file carries is the one the grid works out to:
    // 9000 px over 1189 mm is 192.2624 dpi, i.e. 7569 px/m.
    let reported: f64 = field(&output, "dpi").parse().unwrap();
    assert!(
        (reported - 9000.0 * 25.4 / 1189.0).abs() < 1e-6,
        "{reported}"
    );
    assert_eq!(png_pixel_dimensions(&out), (7569, 7569, 1));

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
fn chroma_reaches_the_jpeg_it_was_asked_for() {
    let dir = out_dir("chroma");
    for (chroma, expected) in [
        ("444", vec![(1, 1), (1, 1), (1, 1)]),
        ("422", vec![(2, 1), (1, 1), (1, 1)]),
        ("420", vec![(2, 2), (1, 1), (1, 1)]),
    ] {
        let out = dir.join(format!("chroma-{chroma}.jpg"));
        let output = run(&[
            "render",
            "--template",
            "grid-4-2x2",
            "--dpi",
            "72",
            "--chroma",
            chroma,
            "--out",
            out.to_str().unwrap(),
        ]);
        assert_eq!(code(&output), 0, "{}", stderr(&output));
        assert_eq!(field(&output, "chroma"), chroma);
        assert_eq!(jpeg_sampling(&out), expected, "chroma {chroma} in SOF0");
    }

    // 4:4:4 is the default (`AGENTS.md`), and the report names it.
    let plain = dir.join("plain.jpg");
    let output = run(&[
        "render",
        "--template",
        "grid-4-2x2",
        "--dpi",
        "72",
        "--out",
        plain.to_str().unwrap(),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "chroma"), "444");
    assert_eq!(jpeg_sampling(&plain), vec![(1, 1), (1, 1), (1, 1)]);

    // A PNG stores three samples per pixel, so accepting `--chroma` there would
    // drop the flag silently.
    let png = dir.join("chroma.png");
    let output = run(&[
        "render",
        "--template",
        "grid-4-2x2",
        "--chroma",
        "420",
        "--out",
        png.to_str().unwrap(),
    ]);
    assert_eq!(code(&output), 1, "{}", stderr(&output));
    assert!(stdout(&output).is_empty());
    assert!(stderr(&output).contains("--chroma"), "{}", stderr(&output));
    assert!(!png.exists(), "a refused render writes nothing");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_export_format_is_written_with_its_metadata() {
    let dir = out_dir("formats");
    for (name, format) in [
        ("out.png", "png"),
        ("out.jpg", "jpeg"),
        ("out.jpeg", "jpeg"),
        ("out.tif", "tiff"),
        ("out.tiff", "tiff"),
    ] {
        let out = dir.join(name);
        let output = run(&[
            "render",
            "--template",
            "grid-4-2x2",
            "--dpi",
            "72",
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

        // The resolution and the profile are in the file. PNG deflates the
        // profile into iCCP, so its presence is what this level checks; what the
        // bytes decode to is `pixlay-imaging`'s test.
        let bytes = std::fs::read(&out).expect("read back");
        let holds = |needle: &[u8]| bytes.windows(needle.len()).any(|window| window == needle);
        match format {
            "png" => {
                assert_eq!(png_pixel_dimensions(&out), (2835, 2835, 1), "{name}");
                assert!(holds(b"iCCP"), "{name} carries no profile");
            }
            "jpeg" => {
                assert!(holds(b"ICC_PROFILE"), "{name} carries no profile");
                assert!(holds(b"JFIF"), "{name} is not JFIF");
            }
            _ => {
                assert!(holds(b"acsp"), "{name} carries no profile");
                assert!(holds(b"pixl"), "{name} carries no profile");
            }
        }
    }

    // An extension nothing writes is still a usage error, and the message names
    // the formats this build has.
    let output = run(&["render", "--template", "grid-4-2x2", "--out", "out.gif"]);
    assert_eq!(code(&output), 1);
    assert!(stderr(&output).contains(".tiff"), "{}", stderr(&output));
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
        // Both size the output, and they disagree about what the number means.
        &["--dpi", "300", "--long-edge", "1000"][..],
        // A preview is a smaller render of the export; a long edge *is* the size.
        &["--long-edge", "1000", "--preview-px", "400"],
        // Out of range on both ends, and zero.
        &["--long-edge", "0"],
        &["--long-edge", "30001"],
        &["--long-edge", "wide"],
        // A chroma spelling that is not one of the three.
        &["--chroma", "411"],
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

#[test]
fn probe_reports_numbers_the_renderer_can_be_judged_by() {
    let dir = out_dir("probe");
    let project = write_project(&dir, "two.pixlay", true);
    let output = run(&[
        "probe",
        "--project",
        project.to_str().unwrap(),
        "--dpi",
        "150",
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "status"), "ok");
    assert_eq!(field(&output, "slots"), "2");
    assert_eq!(field(&output, "occupied"), "1");
    assert_eq!(field(&output, "bg_non_white"), "0");
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
    let output = run(&["probe", "--project", full.to_str().unwrap(), "--dpi", "150"]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "occupied"), "2");
    assert_eq!(field(&output, "slot.1.match"), "true");
    assert_eq!(field(&output, "slot.1.expected"), "30,160,60");
    assert_eq!(field(&output, "bg_non_white"), "0");
    // Walked end to end: the walker keeps a guard band at both ends where the
    // seam meets a corner, so the row count is the seam length minus 4.
    let rows: i64 = field(&output, "seam.0.rows").parse().unwrap();
    let length: f64 = field(&output, "seam.0.length_px").parse().unwrap();
    assert!(
        (length - rows as f64).abs() <= 5.0,
        "{rows} rows of {length} px"
    );
    assert_eq!(field(&output, "seam.0.foreign"), "0");
    // The seam is the shared edge: the full height of the canvas.
    assert!((field(&output, "seam.0.length_px").parse::<f64>().unwrap() - 531.0).abs() <= 1.0);
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
        "--dpi",
        "150",
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
        "--dpi",
        "150",
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
        "--dpi",
        "72",
        "--out",
        dir.join("json.png").to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    let value: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("valid JSON");
    assert_eq!(value["status"], "ok");
    assert_eq!(value["cells"], 8);
    assert_eq!(value["out_w"], 3370);

    // Same input, same bytes. The result carries no timestamps and no durations.
    let repeat = run(&[
        "render",
        "--template",
        "mosaic-8-s14",
        "--dpi",
        "72",
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
            "--dpi".to_string(),
            "72".to_string(),
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
                "--dpi",
                "72",
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
        &[
            "render",
            "--template",
            "mosaic-8-s14",
            "--out",
            "x.png",
            "--dpi",
            "10",
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
    assert!(
        stderr(&output).contains("project JSON"),
        "{}",
        stderr(&output)
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
        "--dpi",
        "72",
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
            "--dpi",
            "72",
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
        "--dpi",
        "96",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));

    // The same document through the same two libraries: the CLI must be a thin
    // wrapper over `pixlay_imaging` + `pixlay_render::draw`, not a second path.
    let loaded = Project::load(&project).expect("loads");
    let sources = loaded.sources().expect("sources");
    let canvas = loaded.doc().canvas.pixel_size(96).expect("canvas size");
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
    let expected = pixlay_render::render_rgb8(loaded.doc(), &images, 96, 1.0, None).expect("draw");
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
/// The example is expected to be a *valid document*, and its text layer is no
/// longer a reason it could not be rendered: S5 removed `draw`'s text gate.
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
        grade: Default::default(),
    };
    std::fs::write(&path, doc.to_json().expect("serializes")).expect("write project");

    let out = dir.join("flat.png.out.png");
    let output = run(&[
        "render",
        "--project",
        path.to_str().unwrap(),
        "--dpi",
        "150",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));

    let canvas = doc.canvas.pixel_size(150).expect("canvas size");
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
    // The canvas must match the template's declared aspect, which is what makes
    // the example loadable rather than a hard error.
    assert!(
        (doc.canvas.aspect() - doc.template.aspect).abs() < 1e-6,
        "canvas aspect {} vs template aspect {}",
        doc.canvas.aspect(),
        doc.template.aspect
    );
    assert_eq!(doc.cells.len(), doc.template.slots.len());
    assert_eq!(doc.text.len(), 1, "the example shows one text layer");
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
        assert!((2..=10).contains(&slots), "{name}: {slots} slots");
        assert!(
            aspect.contains(':'),
            "{name}: aspect {aspect} is not in W:H form"
        );
        if !slot_counts.contains(&slots) {
            slot_counts.push(slots);
        }
    }
    for wanted in 2..=10 {
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
    assert_eq!(doc.canvas.aspect(), doc.template.aspect);
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
        "--dpi",
        "72",
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

// ---------------------------------------------------------------------------
// S5: text layers
// ---------------------------------------------------------------------------

/// The directory the committed fixtures live in (`photos/`, `fonts/`).
fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Runs the binary with the committed test font as the machine's only font.
///
/// Text needs a font, and a font is the one thing a machine is not guaranteed to
/// have: a clean chroot has none at all. `tests/fixtures/fonts/` holds a subset of
/// Noto Sans CJK generated by `fonts/generate.py`, and this environment makes
/// fontconfig see nothing else, so a render of a text project is the same
/// everywhere.
fn run_with_font(args: &[&str], dir: &Path) -> Output {
    let cache = dir.join("fontcache");
    let config = dir.join("fonts.conf");
    std::fs::create_dir_all(&cache).expect("create font cache directory");
    std::fs::write(
        &config,
        format!(
            "<?xml version=\"1.0\"?>\n\
             <!DOCTYPE fontconfig SYSTEM \"urn:fontconfig:fonts.dtd\">\n\
             <fontconfig>\n\
             \x20 <dir>{fonts}</dir>\n\
             \x20 <cachedir>{cache}</cachedir>\n\
             </fontconfig>\n",
            fonts = fixture_dir().join("fonts").display(),
            cache = cache.display()
        ),
    )
    .expect("write fonts.conf");

    let mut command = Command::new(BIN);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("TZ", "UTC")
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("HOME", "/nonexistent")
        .env("FONTCONFIG_FILE", &config)
        .env("XDG_CACHE_HOME", dir);
    command.output().expect("run pixlay-render")
}

/// `render` on the S5 fixture: a free caption, a free `{date}`, and a tiled
/// watermark, all in one document.
///
/// Not vacuous: the same document with its `text` array emptied has to render to
/// *different* pixels, so this is evidence the layers reached the output and not
/// only the report (`text = 3`).
#[test]
fn rendered_text_reaches_the_output() {
    let dir = out_dir("text-render");
    let project = fixture_dir().join("text.pixlay");
    let with = dir.join("with-text.png");
    let without = dir.join("without-text.png");

    let output = run_with_font(
        &[
            "render",
            "--project",
            project.to_str().unwrap(),
            "--dpi",
            "150",
            "--preview-px",
            "480",
            "--out",
            with.to_str().unwrap(),
        ],
        &dir,
    );
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "text"), "3", "{}", stdout(&output));
    assert_eq!(field(&output, "occupied"), "8");

    // The same document with the layers removed. The copy lives in the scratch
    // directory, so its sources become absolute (a `.pixlay` resolves relative
    // ones against its own directory).
    let mut doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&project).expect("read fixture"))
            .expect("parse fixture");
    doc["text"] = serde_json::json!([]);
    for cell in doc["cells"].as_array_mut().expect("cells") {
        if let Some(source) = cell["source"].as_str() {
            cell["source"] = serde_json::json!(fixture_dir().join(source).display().to_string());
        }
    }
    let bare = dir.join("bare.pixlay");
    std::fs::write(
        &bare,
        serde_json::to_string_pretty(&doc).expect("serialize"),
    )
    .expect("write bare project");
    let stripped = run_with_font(
        &[
            "render",
            "--project",
            bare.to_str().unwrap(),
            "--dpi",
            "150",
            "--preview-px",
            "480",
            "--out",
            without.to_str().unwrap(),
        ],
        &dir,
    );
    assert_eq!(code(&stripped), 0, "{}", stderr(&stripped));
    assert_eq!(field(&stripped, "text"), "0");

    let with = image::open(&with).expect("decode the render").to_rgb8();
    let without = image::open(&without).expect("decode the render").to_rgb8();
    assert_eq!(
        (with.width(), with.height()),
        (without.width(), without.height())
    );
    let different = with
        .pixels()
        .zip(without.pixels())
        .filter(|(a, b)| a.0 != b.0)
        .count();
    assert!(
        different > 1000,
        "the text layers changed only {different} pixels of the render"
    );

    // A preview that is a different size is a different render: `--json` carries
    // the same count, and the report stays machine-readable.
    let json = run_with_font(
        &[
            "render",
            "--project",
            project.to_str().unwrap(),
            "--dpi",
            "150",
            "--preview-px",
            "240",
            "--json",
            "--out",
            dir.join("json.png").to_str().unwrap(),
        ],
        &dir,
    );
    let text: serde_json::Value =
        serde_json::from_str(&stdout(&json)).expect("--json prints one object");
    assert_eq!(text["text"], 3);
    assert_eq!(text["out_w"], 240);
    let _ = std::fs::remove_dir_all(&dir);
}

/// `text` reports what every layer's tokens resolve to, without rendering.
///
/// This is the machine surface for S5's `{date}` criterion: the fixture's date
/// layer names the slot holding `dated.jpg`, whose EXIF says 2019, while the
/// document's stored fallback says 2026 — so the report shows which one won.
#[test]
fn text_reports_resolved_tokens_from_the_projects_photos() {
    let project = fixture_dir().join("text.pixlay");
    let output = run(&["text", "--project", project.to_str().unwrap()]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(field(&output, "count"), "3");
    // No tokens: the caption is reported as written.
    assert_eq!(
        field(&output, "text.0.content"),
        "他说：“今天天气很好。”然后他走了。这是一段用来测试断行的中文文字，标点不应该出现在行首。"
    );
    assert_eq!(field(&output, "text.0.mode"), "free");
    assert_eq!(field(&output, "text.0.anchor"), "topCenter");
    // `{date}` from EXIF `DateTimeOriginal`, verbatim — not the stored fallback.
    assert_eq!(field(&output, "text.1.content"), "2019:07:14 10:32:00");
    assert_eq!(field(&output, "text.1.source_slot"), "5");
    // A tiled layer resolves the same way, and reports the grid it will draw.
    assert_eq!(
        field(&output, "text.2.content"),
        "PIXLAY 2019:07:14 10:32:00"
    );
    assert_eq!(field(&output, "text.2.mode"), "tiled");
    assert_eq!(field(&output, "text.2.tiles"), "15");

    let json = run(&["text", "--project", project.to_str().unwrap(), "--json"]);
    let report: serde_json::Value =
        serde_json::from_str(&stdout(&json)).expect("--json prints one object");
    assert_eq!(report["count"], 3);
    // Flat keys, like every other report: `text.<i>.<field>`.
    assert_eq!(report["text.1.content"], "2019:07:14 10:32:00");
}

/// The fallback path: a photo with no `DateTimeOriginal`, a slot with no photo at
/// all, and no stored date either.
#[test]
fn text_falls_back_to_the_documents_own_date() {
    let dir = out_dir("text-fallback");
    // `square.png` carries no EXIF block; the second slot is empty.
    let mut doc = Project::load(&write_project(&dir, "two.pixlay", true))
        .expect("loads")
        .doc()
        .clone();
    doc.text_fallback.date = "2026-09-21".to_string();
    let layer = |content: &str, source_slot: Option<usize>| pixlay_core::TextLayer {
        content: content.to_string(),
        mode: pixlay_core::TextMode::Free {
            position: Point::new(0.5, 0.5),
            anchor: pixlay_core::Anchor::Center,
        },
        size_rel: 0.05,
        rotation_deg: 0.0,
        color: pixlay_core::Rgba8::BLACK,
        source_slot,
    };
    doc.text = vec![
        layer("{date}|{filename}|{index}", Some(0)),
        layer("{date}", Some(1)),
        layer("{date}", None),
    ];
    let path = dir.join("fallback.pixlay");
    std::fs::write(&path, doc.to_json().expect("serializes")).expect("write project");

    let output = run(&["text", "--project", path.to_str().unwrap()]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    // A photo without a date: the document's stored string, and the file name.
    assert_eq!(field(&output, "text.0.content"), "2026-09-21|photo.png|0");
    // An empty slot: same fallback, and no file name to substitute.
    assert_eq!(field(&output, "text.1.content"), "2026-09-21");
    // The layer does name a slot; that slot's cell is simply empty.
    assert_eq!(field(&output, "text.1.source_slot"), "1");
    // A layer that names no slot: the fallback, and nothing else.
    assert_eq!(field(&output, "text.2.content"), "2026-09-21");

    // No stored date either: the token renders as nothing, never as `{date}`.
    doc.text_fallback.date = String::new();
    std::fs::write(&path, doc.to_json().expect("serializes")).expect("write project");
    let output = run(&["text", "--project", path.to_str().unwrap()]);
    assert_eq!(field(&output, "text.2.content"), "");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A probe cannot judge a document with text on it: its samples are about pixels
/// it painted itself.
#[test]
fn probe_refuses_a_document_with_text_layers() {
    let dir = out_dir("probe-text");
    let mut doc = Project::load(&write_project(&dir, "two.pixlay", true))
        .expect("loads")
        .doc()
        .clone();
    doc.text.push(pixlay_core::TextLayer {
        content: "wm".to_string(),
        mode: pixlay_core::TextMode::Tiled { step: (0.25, 0.25) },
        size_rel: 0.05,
        rotation_deg: 0.0,
        color: pixlay_core::Rgba8::BLACK,
        source_slot: None,
    });
    let path = dir.join("probe-text.pixlay");
    std::fs::write(&path, doc.to_json().expect("serializes")).expect("write project");

    let output = run(&["probe", "--project", path.to_str().unwrap()]);
    assert_eq!(code(&output), 2, "{}", stderr(&output));
    assert!(
        stdout(&output).is_empty(),
        "a document the probe refuses produces no numbers"
    );
    assert!(
        stderr(&output).contains("text layer"),
        "the message must name the reason: {}",
        stderr(&output)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The `text` subcommand takes a project and nothing else.
#[test]
fn text_refuses_flags_that_belong_to_other_subcommands() {
    for args in [
        vec!["text"],
        vec!["text", "--photo", "x.jpg"],
        vec!["text", "--template", "mosaic-8-s14"],
        vec!["text", "--project", "x.pixlay", "--dpi", "300"],
        vec!["text", "--project", "x.pixlay", "--out", "y.png"],
        vec!["text", "--project", "x.pixlay", "--preview-px", "100"],
    ] {
        let output = run(&args);
        assert_eq!(code(&output), 1, "{args:?}: {}", stderr(&output));
        assert!(stdout(&output).is_empty(), "{args:?} wrote to stdout");
        assert!(!stderr(&output).is_empty(), "{args:?} said nothing");
    }
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
    assert_eq!(field(&output, "text"), "0");
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
        .replace("\"docVersion\": 1", "\"docVersion\": 2");
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
    assert!(!dir.join("other.pixlay").exists());
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
            "--dpi",
            "300",
        ],
        vec!["templates", "--at", "0.5,0.5"],
        vec![
            "render",
            "--template",
            "strip-2-2x1",
            "--dpi",
            "72",
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
    // And the resolution the preview carries: 72 dpi as the PNG `pHYs` chunk, i.e.
    // 2835 pixels per metre, unit 1.
    assert_eq!(png_pixel_dimensions(&out), (2835, 2835, 1));
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

    // One photo and ten: the 2..=9 clamp names both bounds, exits 1, and writes
    // nothing — a refused command leaves no half-made project behind.
    for count in [1usize, 10] {
        let mut args = vec![
            "init",
            "--template",
            "mosaic-8-s14",
            "--out",
            path.to_str().unwrap(),
        ];
        for _ in 0..count {
            args.push("--photo");
            args.push(&photo_arg);
        }
        let output = run(&args);
        assert_eq!(code(&output), 1, "{count} photos: {}", stderr(&output));
        assert!(stdout(&output).is_empty(), "{count} wrote to stdout");
        assert!(
            stderr(&output).contains("2..=9"),
            "{count} photos: {}",
            stderr(&output)
        );
    }
    assert!(!path.exists(), "a refused init must write nothing");

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
        vec![
            "render",
            "--template",
            "mosaic-8-s14",
            "--dpi",
            "72",
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
    // are the user's only way to find out why a folder came back empty.
    let help = run(&["--help"]);
    assert_eq!(code(&help), 0);
    for extension in pixlay_cli::args::PHOTO_EXTENSIONS {
        assert!(
            stdout(&help).contains(&format!(".{extension}")),
            "--help does not mention .{extension}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
