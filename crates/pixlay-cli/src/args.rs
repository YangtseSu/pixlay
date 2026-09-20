//! Argument parsing for the machine surface.
//!
//! No argument-parsing crate: the surface is three subcommands and a dozen
//! flags, and the contract (exit codes, stdout purity, locale independence)
//! needs exact control over every message.
//!
//! Exit codes are part of the contract:
//!
//! * `0` success
//! * `1` usage error — unknown flags, out-of-range values, unknown template
//! * `2` the document could not be read, decoded or rendered

use std::ffi::OsString;
use std::path::PathBuf;

use pixlay_core::{MAX_DPI, MIN_DPI};

use crate::cli::Failure;
use crate::content::Mode;

/// Largest preview edge in pixels; a preview larger than this cannot be reviewed
/// by eye anyway.
pub const MAX_PREVIEW_PX: i32 = 20000;

pub const USAGE: &str = "\
pixlay-render - headless renderer and probe for Pixlay projects

USAGE:
    pixlay-render render --project <file.pixlay> --out <file> [OPTIONS]
    pixlay-render render --template <name> --dpi <n> --out <file> [OPTIONS]
    pixlay-render probe  --project <file.pixlay> [OPTIONS]
    pixlay-render --help | --version

RENDER OPTIONS:
    --project <file>    Project to render. Paths inside it are relative to it.
    --template <name>   Render a template with no photos (template library: S2).
    --out <file>        Output file. Format comes from the extension:
                        .png, .jpg, .jpeg. Required.
    --dpi <n>           Export resolution, 72..=600. Default 300.
    --preview-px <n>    Render the long edge at n pixels instead of full size,
                        1..=20000. The same draw, only the scale changes.
    --content <mode>    Placeholder content for cells: detail (default) or flat.
                        Until S4 there is no decoder, so cells are filled with
                        deterministic content instead of the photo they name.

PROBE OPTIONS:
    --project <file>    Project to probe. Required.
    --dpi <n>           Resolution to probe at, 72..=600. Default 300.

COMMON OPTIONS:
    --json              Print one JSON object instead of key = value lines.
    --stats             Add measured fields: ms, peak_rss_mb, icc. `render`
                        adds encode_ms as well; `probe` does not encode.
    -h, --help          Print this help.
    -V, --version       Print the version.

OUTPUT:
    stdout carries the result and nothing else. Diagnostics go to stderr. The
    output is byte-identical for identical input; with --stats the measured
    fields (ms, peak_rss_mb) vary, as measurements do.

EXIT CODES:
    0  success
    1  usage error (unknown flag, out-of-range value, unknown template)
    2  project, decode or render failure, or a probe verdict of `failed`
       (the failing path or the failing check is on stderr)
";

/// What the command line asked for.
pub enum Command {
    Render(RenderArgs),
    Probe(ProbeArgs),
    Help,
    Version,
}

/// Which document a render starts from.
pub enum Source {
    /// A `.pixlay` project, photos included.
    Project(PathBuf),
    /// A template with no photos at all: the smoke path of `AGENTS.md`.
    Template(String),
}

pub struct RenderArgs {
    pub source: Source,
    pub out: PathBuf,
    pub dpi: u32,
    pub preview_px: Option<i32>,
    pub content: Mode,
    pub stats: bool,
    pub json: bool,
}

pub struct ProbeArgs {
    pub project: PathBuf,
    pub dpi: u32,
    pub stats: bool,
    pub json: bool,
}

#[derive(Default)]
struct Flags {
    project: Option<PathBuf>,
    template: Option<String>,
    out: Option<PathBuf>,
    dpi: Option<u32>,
    preview_px: Option<i32>,
    content: Option<Mode>,
    stats: bool,
    json: bool,
}

pub fn parse(argv: &[OsString]) -> Result<Command, Failure> {
    let Some(head) = argv.first() else {
        return Err(Failure::Usage("missing subcommand".to_string()));
    };
    let head = head
        .to_str()
        .ok_or_else(|| Failure::Usage("subcommand must be valid UTF-8".to_string()))?;
    let subcommand = match head {
        "render" | "probe" => head,
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
            "dpi" => {
                let raw = number(&value("dpi")?, "dpi")?;
                let dpi = u32::try_from(raw).map_err(|_| {
                    Failure::Usage(format!("--dpi must be a positive integer, got {raw}"))
                })?;
                if !(MIN_DPI..=MAX_DPI).contains(&dpi) {
                    return Err(Failure::Usage(format!(
                        "--dpi {dpi} is outside {MIN_DPI}..={MAX_DPI}"
                    )));
                }
                set_once(&mut flags.dpi, dpi, "dpi")?;
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
            "content" => {
                let raw = value("content")?;
                let name = raw
                    .to_str()
                    .ok_or_else(|| Failure::Usage("--content must be valid UTF-8".to_string()))?;
                let mode = Mode::parse(name).ok_or_else(|| {
                    Failure::Usage(format!("--content {name} is not one of detail, flat"))
                })?;
                set_once(&mut flags.content, mode, "content")?;
            }
            other => return Err(Failure::Usage(format!("unknown option --{other}"))),
        }
    }

    if subcommand == "probe" {
        for (present, name, hint) in [
            (
                flags.template.is_some(),
                "template",
                "probe reads a project",
            ),
            (flags.out.is_some(), "out", "probe writes no file"),
            (
                flags.preview_px.is_some(),
                "preview-px",
                "probe always renders at full size",
            ),
            (
                flags.content.is_some(),
                "content",
                "probe renders flat content on purpose",
            ),
        ] {
            if present {
                return Err(Failure::Usage(format!(
                    "--{name} is not valid for probe: {hint}"
                )));
            }
        }
        let project = flags
            .project
            .ok_or_else(|| Failure::Usage("probe needs --project".to_string()))?;
        return Ok(Command::Probe(ProbeArgs {
            project,
            dpi: flags.dpi.unwrap_or(300),
            stats: flags.stats,
            json: flags.json,
        }));
    }

    let source = match (flags.project, flags.template) {
        (Some(_), Some(_)) => {
            return Err(Failure::Usage(
                "--project and --template are mutually exclusive".to_string(),
            ));
        }
        (Some(path), None) => Source::Project(path),
        (None, Some(name)) => Source::Template(name),
        (None, None) => {
            return Err(Failure::Usage(
                "render needs --project <file> or --template <name>".to_string(),
            ));
        }
    };
    let out = flags
        .out
        .ok_or_else(|| Failure::Usage("render needs --out <file>".to_string()))?;
    Ok(Command::Render(RenderArgs {
        source,
        out,
        dpi: flags.dpi.unwrap_or(300),
        preview_px: flags.preview_px,
        content: flags.content.unwrap_or(Mode::Detail),
        stats: flags.stats,
        json: flags.json,
    }))
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
