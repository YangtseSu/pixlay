# Contributing to Pixlay

What a checkout needs, how to build and test it, and where the project's rules live. Users install a
release — the app, the PKGBUILD and the prebuilt tarballs are described in [`README.md`](README.md).

## What is where

|Path|What it is|
|---|---|
|`crates/pixlay-core`|`CollageDoc`, templates, geometry, framing, history. No GTK, no Cairo|
|`crates/pixlay-imaging`|Decoding (glycin), resampling, colour, encoding (PNG/JPEG). No GTK, no Cairo|
|`crates/pixlay-render`|The one `draw(doc, images, target)`, on Cairo|
|`crates/pixlay-cli`|`pixlay-render`: the windowless entry point and the probe surface|
|`crates/pixlay`|`pixlay`: the GTK4 + libadwaita shell|
|`data/`, `po/`, `packaging/arch/`|The installed desktop files, the translation catalogs, the PKGBUILD|
|`docs/`|The contract, the HIG walk, the roadmap and the plan|
|`meson.build`|The build and the install, the way GNOME's own applications have one|

## Build and install

Needs GTK 4.12+, libadwaita 1.8+, glycin 2, libseccomp, glib and gio (the runtime libraries), plus a
Rust toolchain, meson, ninja and gettext. `meson setup` names anything that is missing:

    meson setup build --prefix=/usr
    meson compile -C build
    sudo meson install -C build

`packaging/arch/PKGBUILD` wraps exactly this, which is how Arch installs the app:

    cd packaging/arch
    makepkg -si

## The verification entry

Every change the product can see — code, shipped data, a template, a constant the render reads —
runs these four:

    cargo fmt --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test
    cargo run --release -p pixlay-cli -- render --project crates/pixlay-cli/tests/fixtures/verify.pixlay --long-edge 14043 --stats --out /var/tmp/a.jpg

The last one is not a unit test: it writes a real 14043 px image from a document of eight photos
(JPEG, PNG, 16-bit PNG, HEIC, EXIF Orientation=6, a date) and prints its measurements, so it
exercises decode, resample, the clamp, `draw` and the encoder in one run. **Judge it by looking at
the image**, and write artifacts to a disk path (`/var/tmp`, `$XDG_CACHE_HOME`) rather than `/tmp`,
which is tmpfs on many machines.

A change the product cannot see — docs, `.gitignore`, the repository layout — runs only the command
that reads what changed, if one exists. The four are also what CI runs, minus the GUI suite, plus the
complete build and a `meson install` into a staging root; the Arch package and the GUI suite are the
machine's.

## The GUI suite and the display it runs on

`cargo test` builds and runs the shell's tests too, which need a display. The harness starts its own
private headless `mutter` for them — a session bus and a machine-id, no GPU node, no compositor in
the product's own dependencies. Where mutter cannot run, hand it the display the process already has:

    PIXLAY_TEST_CHILD=1 xvfb-run -a cargo test

An Xvfb is not mutter's equal: with no window manager GTK frames the window inside its own surface,
so every window geometry the suite reads comes out 10 px smaller in each direction (1090x584 against
1100x594) — a fallback run, whose numbers are that display's.

## Where the rules are

[`AGENTS.md`](AGENTS.md) is the project's rulebook — the hard constraints, the module boundaries, the
step and session discipline, the version policy and the AUR discipline — and it is the authority when
this file and it disagree.

- [`docs/CONTRACT.md`](docs/CONTRACT.md) — the frozen shapes: the `CollageDoc` JSON, the invariants,
  the encoder's per-format fields, the paths an install writes.
- [`docs/steps/`](docs/steps/) — the plan: one file per step, `<S-number>-<slug>-<status>.md`, the
  status in the name and the `**Progress**` line inside it the only authority on where that step
  stands. The S-numbering continues across plans.
- [`docs/HIG-REVIEW.md`](docs/HIG-REVIEW.md) — the GNOME HIG walk, item by item.
- [`docs/ROADMAP.md`](docs/ROADMAP.md) — directions that are not steps.
- [`CHANGELOG.md`](CHANGELOG.md) — the release history; a release's notes are that version's own
  section, written before the tag is pushed.

## Conventions worth knowing before your first patch

- **Everything machine-facing is English**: code, comments, identifiers, logs, commit messages,
  documents. Conversation may be another language; the repository is not.
- **Commit once per completed step**, with `<step>: <what changed>` (`docs:` / `chore:` for one-off
  work), and end the title line with 🤖 — the marker for AI-written or AI-modified content, which at
  this stage is all of it. Pushing needs the human's explicit permission.
- **`Cargo.lock` is committed** and dependencies track the latest release with no upper caps: the
  target is Arch, and a version that pins is a version that has to be justified. `cargo update
  --workspace` starts a step; breakage is fixed, not shimmed.
- **`pixlay-cli` is the only machine-operable surface.** A capability must exist as a subcommand
  before the GUI has it — zero interaction, machine-readable stdout, diagnostics on stderr, fixed
  exit codes, output unaffected by locale. Nothing may be possible only in the GUI.
- **A step is done when its `**Progress**` line is rewritten and committed**, not when its tests are
  green; a human ruling is written to disk in the same turn it is given. Session boundaries line up
  with gates — a human criterion, an irreversible contract, or a technology choice that could be
  overturned — so a gate's next action is a new session.

Releasing is a tag plus four steps on the machine: the notes are the new `CHANGELOG.md` section,
`vX.Y.Z` must equal `meson.build`'s `project(version:)`, `release.yml` attaches the Linux binaries,
and then the PKGBUILD's `pkgver` / `sha256sums`, `makepkg` and the upload follow. The full procedure
is in `AGENTS.md`, "AUR discipline".

## License

GPL-3.0-or-later — see [`LICENSE`](LICENSE). Contributions are under the same license.
