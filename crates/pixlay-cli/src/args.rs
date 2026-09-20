//! Argument parsing for the machine surface.
//!
//! No argument-parsing crate: the surface is five subcommands and a dozen
//! flags, and the contract (exit codes, stdout purity, locale independence)
//! needs exact control over every message.
//!
//! Exit codes are part of the contract:
//!
//! * `0` success
//! * `1` usage error — unknown flags, out-of-range values, unknown template
//! * `2` the document could not be read, decoded, written or rendered

use std::ffi::OsString;
use std::path::PathBuf;

use pixlay_core::{MAX_DPI, MIN_DPI};

use crate::cli::Failure;
use crate::content::Mode;

/// Largest preview edge in pixels; a preview larger than this cannot be reviewed
/// by eye anyway.
pub const MAX_PREVIEW_PX: i32 = 20000;

/// File extension a project written by `init` must have. A `.pixlay` is a
/// document, and a mistyped extension is more likely a typo than an intention.
pub const PROJECT_EXTENSION: &str = "pixlay";

pub const USAGE: &str = "\
pixlay-render - headless renderer and probe for Pixlay projects

USAGE:
    pixlay-render render --project <file.pixlay> --out <file> [OPTIONS]
    pixlay-render render --template <name> --dpi <n> --out <file> [OPTIONS]
    pixlay-render probe  --project <file.pixlay> [OPTIONS]
    pixlay-render templates [--aspect <ratio>] [--json]
    pixlay-render init --template <name> --out <file.pixlay> [--json]
    pixlay-render --help | --version

RENDER OPTIONS:
    --project <file>    Project to render. Paths inside it are relative to it.
    --template <name>   Render a template with no photos (see `templates`).
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

TEMPLATES OPTIONS:
    --aspect <ratio>    List only the templates authored for this canvas shape,
                        as W:H (4:3) or a decimal (1.333333). Omit to list all.
                        A template and a canvas must share an aspect ratio, so
                        this is the query to run before picking one.

INIT OPTIONS:
    --template <name>   Template of the project to create. Required.
    --out <file>        Project to write, .pixlay. Required, and never
                        overwritten: `init` refuses to replace an existing file.

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
    2  project, decode, render or write failure, or a probe verdict of `failed`
       (the failing path or the failing check is on stderr)
";

/// What the command line asked for.
pub enum Command {
    Render(RenderArgs),
    Probe(ProbeArgs),
    Templates(TemplatesArgs),
    Init(InitArgs),
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

pub struct TemplatesArgs {
    /// Canvas aspect ratio to filter by, already parsed. `None` lists the library.
    pub aspect: Option<f64>,
    pub json: bool,
}

pub struct InitArgs {
    pub template: String,
    pub out: PathBuf,
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
    aspect: Option<f64>,
    stats: bool,
    json: bool,
}

/// The first flag a subcommand does not accept, with the reason to quote back.
///
/// A flag missing from a subcommand's list is refused there rather than ignored:
/// a silently dropped `--dpi 300` on `templates` looks like it was honored.
fn first_rejected(name: &str, flags: &Flags) -> Option<(&'static str, &'static str)> {
    let valid: &[&str] = match name {
        "render" => &[
            "project",
            "template",
            "out",
            "dpi",
            "preview-px",
            "content",
            "stats",
        ],
        "probe" => &["project", "dpi", "stats"],
        "templates" => &["aspect"],
        "init" => &["template", "out"],
        _ => &[],
    };
    let present: [(&'static str, bool); 8] = [
        ("project", flags.project.is_some()),
        ("template", flags.template.is_some()),
        ("out", flags.out.is_some()),
        ("dpi", flags.dpi.is_some()),
        ("preview-px", flags.preview_px.is_some()),
        ("content", flags.content.is_some()),
        ("aspect", flags.aspect.is_some()),
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
        ("probe", "template") => "probe reads a project",
        ("probe", "out") => "probe writes no file",
        ("probe", "preview-px") => "probe always renders at full size",
        ("probe", "content") => "probe renders flat content on purpose",
        ("probe", "aspect") => "probe filters no template list",
        ("templates", _) => "templates only lists the library",
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
        "render" | "probe" | "templates" | "init" => head,
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
            "aspect" => {
                let raw = value("aspect")?;
                let aspect = parse_aspect(&raw)?;
                set_once(&mut flags.aspect, aspect, "aspect")?;
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
                dpi: flags.dpi.unwrap_or(300),
                stats: flags.stats,
                json: flags.json,
            }))
        }
        "templates" => Ok(Command::Templates(TemplatesArgs {
            aspect: flags.aspect,
            json: flags.json,
        })),
        "init" => {
            let template = flags
                .template
                .ok_or_else(|| Failure::Usage("init needs --template <name>".to_string()))?;
            let out = flags
                .out
                .ok_or_else(|| Failure::Usage("init needs --out <file.pixlay>".to_string()))?;
            let extension = out.extension().and_then(|value| value.to_str());
            if extension != Some(PROJECT_EXTENSION) {
                return Err(Failure::Usage(format!(
                    "--out {}: expected a .{PROJECT_EXTENSION} file",
                    out.display()
                )));
            }
            Ok(Command::Init(InitArgs {
                template,
                out,
                json: flags.json,
            }))
        }
        _ => {
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
    }
}

/// Parses an aspect ratio: `W:H` with positive numbers, or a bare decimal.
///
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
