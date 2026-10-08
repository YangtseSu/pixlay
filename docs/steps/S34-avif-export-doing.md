<!--
SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>

SPDX-License-Identifier: GPL-3.0-or-later
-->

# S34 · AVIF is the third format, and the default

**Progress**: doing — implemented, measured and machine-checked; the human's walk (`Human`, below) is the
gate. Nothing in the tree is waiting on a machine.

**Goal**: a collage can be exported as AVIF, and AVIF is what a fresh account exports — the format a
collage's hard colour edges and flat colour survive in for the fewest bytes.

**Ruling (2026-10-08, human)**: "增加avif输出方式，并且作为默认。" — add AVIF output, and make it the
default. Basis: AVIF is the modern format for exactly this content (a few large flat areas and hard
edges at a high resolution), the third format is what the product's own export menu was missing, and the
writer costs no new dependency (below). What the ruling does **not** fix, and this step leaves to the
same human: the *quality* number, the *look* of a 4:2:0 AVIF on a collage, and whether a machine that
needs `libheif` for its default export is acceptable (the `Human` line).

**Work**

- `pixlay-imaging` — `Format::Avif` (`.avif`, `name()` → `avif`, `EXTENSIONS`), `AVIF_QUALITY = 90`,
  and the writer: libheif through **glycin's encoder API** (`glycin::Creator`), run as a job on the
  driver thread (`crate::driver`, the one thread that talks to glycin) because a loader future only
  completes while a main context is iterated. Pixels, the sRGB profile and the quality go into one
  `create` call — the one-pass rule of `AGENTS.md` holds for it as it does for PNG and JPEG — and the
  file's bytes are then written through `pixlay_core::atomic` like the other two. A machine without
  the `glycin-heif` loader gets `EncodeError::Avif` with a sentence naming what is missing
  (`avif_reason`), never a fallback to another format.
- `pixlay-cli` — `--out`'s extension takes `.avif` (the rule has always been the extension), the
  usage text names it, and the format test renders and reads an AVIF back through the product's own
  decoder.
- `pixlay` (the shell) — `Settings::default().format` is AVIF and the settings file accepts `"avif"`;
  the format row's table is AVIF / JPEG / PNG (AVIF first, `dialogs.rs`); `export::extension` and the
  save dialog's filter know it. The **name's extension still decides the format** (S25c), so the row
  is a default rather than a constraint.
- `docs/CONTRACT.md` §5 (the output-format row, the per-format metadata row, the quality row, the
  `thumb` row), §8 (the S34 numbers) and §9 (the settings file's `format` and the default);
  `AGENTS.md` (the one-pass constraint, the not-doing list, the module table, the `glycin` registry
  row with `libheif`); `README.md`; `CHANGELOG.md`; `docs/HIG-REVIEW.md` §2's walk.

**Machine-checkable exit**

- `pixlay-render render --out x.avif` writes a file that starts `ftyp`/`avif`, carries the sRGB
  profile in a `colr` box of type `prof`, decodes back through `Source::decode` at the size the
  report claims, and reports `format = avif` — `crates/pixlay-cli/tests/cli.rs`'s
  `every_export_format_is_written_with_its_metadata`.
- The container's structure and the round trip: `crates/pixlay-imaging/tests/encode.rs`'s
  `the_avif_carries_its_profile_and_its_size` and
  `the_avif_decodes_back_to_the_picture_it_was_written_from` (RMSE 345 of 65535 on the test gradient,
  against the threshold of 1028).
- The default and the file's vocabulary: `crates/pixlay/src/settings.rs`'s unit tests,
  `crates/pixlay/tests/settings.rs` (a fresh window holds AVIF; the row's index 2 is PNG),
  `crates/pixlay/tests/export.rs` (the seed suggests `.avif`, and a `.avif` name exports an AVIF
  through the window's own worker).
- The verification entry: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test`, `reuse lint`.

**Measured** (2026-10-08, `--release`, this machine; the fixture project's eight photos through
`render --stats`; the full table is `docs/CONTRACT.md` §8, "S34"):

| 4000 px (4000x3000) | bytes | `encode_ms` | `peak_rss_mb` |
|---|---|---|---|
| AVIF | **566,985** | 303.5 | 186.6 |
| JPEG | 1,359,606 | 157.5 | 163.5 |
| PNG | 5,014,040 | 1633.5 | 163.8 |

- 2.4x smaller than the JPEG at 4000 px, 3.0x at 147.9 MP (3,026,779 against 9,157,670 bytes), for
  about twice the encode time; the A0 export peaks at 1.91 GB, inside the 2.5 GB budget.
- Fidelity against the same render's PNG: AVIF RMSE **0.0131**, JPEG **0.0054** — and raising the
  AVIF's quality barely moves it (q95: 899,743 B at 0.0130; q100: 2,787,175 B at 0.0128), because
  libheif's encoder writes YCbCr **4:2:0** and that is what a collage's hard edges cost. 90 is where
  the quantizer stops being the limit.
- Deterministic: two processes, the same pixels, the same quality → the same 566,985 bytes
  (`481da849…bcf72`).

**Result**

AVIF is the third format and the app's default; the CLI takes it by extension and the shell suggests
it. The writer is the decoder's own loader family, so the product gains no dependency, and the ICC
profile reaches the file in the same pass as the pixels (`colr`/`prof`, 664 bytes, the profile
`pixlay_imaging::icc` builds). What a machine must have is the `glycin-heif` loader with `libheif` —
the same package an AVIF or HEIC *photo* already needs — and a machine without it is told so.

**Human**: the walk of `docs/HIG-REVIEW.md` §2 item 13 — is AVIF the right default *look* (the 4:2:0
chroma against the JPEG's 4:4:4 on a collage's hard colour edges), does a `.avif` export open in the
person's other applications, and is a default that needs `libheif` installed acceptable? The step is
`done` when that is answered and the progress line below says so.
