//! The CLI contract, as tests: the machine surface is what every later step's
//! verification loop depends on, so its promises are pinned here.
//!
//! These run the built binary as a subprocess, because that is the only way to
//! observe the things the contract is about: stdout purity, exit codes, the
//! absence of a TTY, and locale independence.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use pixlay_core::{CanvasSpec, Cell, CollageDoc, Polygon, Project, Slot, Template};

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
        };
    }
    let path = dir.join(name);
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
    assert_eq!(field(&output, "occupied"), "8");
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
    // Slot 0 is filled, slot 1 is not: one interior sample, one seam to check.
    assert_eq!(field(&output, "slot.0.match"), "true");
    assert_eq!(field(&output, "slot.0.expected"), "200,30,40");
    assert_eq!(field(&output, "slot.0.actual"), "200,30,40");
    assert!(field(&output, "slot.0.depth_px").parse::<f64>().unwrap() > 100.0);
    assert_eq!(field(&output, "seam.0.clean"), "true");
    assert!(field(&output, "seam.0.rows").parse::<i64>().unwrap() > 400);
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
fn a_failing_probe_still_prints_its_numbers() {
    // A probe that fails is a *result*, not an inability to produce one: the
    // numbers are the whole point, so stdout carries them and the exit code is
    // the verdict. A crop below the covering zoom leaves white inside the slot,
    // which is exactly what the interior probe exists to catch.
    let dir = out_dir("probe-fail");
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
    assert_eq!(code(&output), 2, "a failed probe exits 2");
    assert!(
        !stdout(&output).is_empty(),
        "the numbers are still the result"
    );
    assert_eq!(field(&output, "status"), "failed");
    assert_eq!(field(&output, "slot.0.match"), "false");
    assert_eq!(field(&output, "slot.0.expected"), "200,30,40");
    assert_eq!(field(&output, "slot.0.actual"), "255,255,255");
    assert!(
        stderr(&output).contains("probe failed"),
        "{}",
        stderr(&output)
    );
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
    assert_eq!(field(&with_stats, "icc"), "none");

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
        vec!["probe", "--project", "x.pixlay", "--content", "flat"],
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
        "--content",
        "flat",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));

    let loaded = Project::load(&project).expect("loads");
    let sources = loaded.sources().expect("sources");
    let images = pixlay_cli::content::images(
        loaded.doc(),
        pixlay_cli::content::Fill::NamedCells(&sources),
        pixlay_cli::content::Mode::Flat,
        96,
    )
    .expect("placeholder content");
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
/// The example is expected to be a *valid document*: the only reason it cannot be
/// rendered today is that `draw` refuses text layers until S5.
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
