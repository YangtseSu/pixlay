<!--
SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>

SPDX-License-Identifier: GPL-3.0-or-later
-->

# S40 · ID photo sheets

**Progress**: planned (2026-10-17) — the human's rulings of 2026-10-17 are written below; the design is
settled and the numbers are measured. **→ next: implement** in a fresh session, then the Human line's walk.

**Goal**: a sheet of one photo repeated at one of the six ID-photo sizes on a 6-inch (4R, 4×6 inch) print — an
*ID photo sheet* — reachable from the layout band's ratio row at one photo, with no second entry point and no
second document kind: `CollageDoc` gains **one cell**, the template carries the copies.

## Ruling (2026-10-17, human)

- 「不做更多的入口。在1张照片的时候，在16：9的右边增加『证件照』。」
- 「我的倾向于，做1个特殊的1 slot模板，而不是12。」 — the document holds **one cell**; the sheet's copies are the
  template's slots, not twelve documents' cells.
- 「相纸一般都是无边框打印」 — the sheet's outer margin is not a constraint; the layouts are the largest that
  fit.
- 「模板名不显示，普通拼图无所谓，在证件照拼图的时候不显示，不知道哪个是具体是多大的尺寸。我倾向于在鼠标悬停或者
  选中某个模板的时候，显示『一寸』。」
- Sizes: the five below, "有必要的可以多做" — the sixth (大二寸) is added on the research's basis, stated below.
- 「做plan文件」 — this file.

## The model: one cell, N slots

- **`Template::one_photo: bool`** (`#[serde(default, skip_serializing_if = "is_false")]`, camelCase `onePhoto`).
  The field only ever appears on the ID sheets, so every project written before S40 reads and writes
  **byte-identically**, and `DOC_VERSION` does not move (S28's `kept` precedent). An older build refuses such a
  project loudly through `deny_unknown_fields` rather than drawing it wrongly.
- **A `onePhoto` document holds exactly one cell**: `cells.len() == 1`, `kept` empty. `CollageDoc::validate`
  enforces both (a hand-edited file is refused with a message naming the one-photo rule). Every slot draws
  cell 0, so one photo, one crop and one undo step are the same thing.
- **One slot→cell accessor.** `CollageDoc::cell(slot)`, `cell_mut(slot)` and `cell_index(slot)` resolve a slot
  to cell 0 on a `onePhoto` template and to the slot itself otherwise. Every **slot-indexed** reader moves to
  it: `pixlay-render`'s `draw` (iterate `template.slots`, fit each slot against its own visible region with the
  *cell's* crop), `pixlay-imaging`'s preview bake, the CLI's `probe`/`edit`/`gesture` reports, `hit`, and the
  shell's canvas, empty-cell buttons and selection. Cell-indexed reads (validation, serialization, the
  selection policy) stay as they are.
- **Commands** (`Command::apply`, one place): on a `onePhoto` document `SetSource`, `SetCrop` and `ClearCell`
  act on the sheet's one cell whatever slot they name; `AddPhotos`/`PlacePhotos` accept exactly one source (a
  second is refused, exit 2, naming the rule); `MovePhoto`, `SwapCells`, `AddCell` and `RemoveCell` are
  refused; `SetTemplate` **onto** a `onePhoto` template is refused unless the document holds at most one photo,
  and its first photo (with its framing) becomes the sheet's. Switching **off** a `onePhoto` template keeps
  that photo in cell 0 and fills the rest empty, exactly as `SetTemplate` already does.
- **Limits**: `MAX_PHOTOS` and `MAX_SLOTS` stay **9** (a document still holds 1..=9 photos, and the
  `cells + kept` ceiling is unchanged — a `onePhoto` document holds one). A template's *slot* count gets its
  own ceiling, `MAX_TEMPLATE_SLOTS = 16` (the largest sheet: 小一寸's 4×4), which is what
  `Template::validate` and `docs/CONTRACT.md` §2 use.
- **Queries**: the ID sheets are **not** collage candidates. `templates::with_slots`,
  `with_slots_and_aspect`, `of_aspect`, `ratios_for`, `layout_for`, `layout_with_aspect` and
  `window::candidate_templates` exclude `one_photo` members; `all`, `names` and `get` keep them (the library
  this build ships, and the CLI's by-name path). New `templates::one_photo()` returns the six in the human's
  order, and `Family::of` learns the `id` prefix (`Family::Id`).

## Geometry

6-inch photo paper is **4R = 4×6 inch = 101.6×152.4 mm**, whose aspect is exactly **2:3** — the library's
existing `2x3` sheet tag, so the shapes need nothing new. The sheet's millimetres exist only in the
**generator's recipe** (which authors the cells in mm) and in the **tests** (which measure the frozen geometry
back); the document, the CLI and the export keep the format's rule — ratios and one long-edge pixel count.

The copies are the **template's geometry** (a gutter layout, the `*g` family's existing mechanism), not
`frame.gapRel`: the white between photos is the sheet's backdrop showing through, and the frame stays at its
defaults. Gap **2 mm**; the counts are the largest that fit (borderless, per the ruling).

Recipe lattice: each arrangement declares its own `grid` (the existing 128 is untouched — no shipped member's
bytes move); the ID sheets use **2560** (1/2560 of the width = 0.0397 mm, 1/2560 of the height = 0.0595 mm),
which lands every cell within **0.03 mm** of nominal and every tile within **0.6 px** of the human's pixel
column at a 1800-px long edge. Measured, `generate()` at that lattice:

| Template | 标称 | px @300dpi | Cell (mm) | px at 1800 | Grid | Gap (mm) | Margins (mm) |
|---|---|---|---|---|---|---|---|
| `id-1cun-12-2x3` | 一寸 25×35 | 295×413 | 25.003×35.004 | 295.3×413.4 | 3×4 = 12 | 1.98/2.02 | 11.3 × 3.2 |
| `id-1cun-small-16-2x3` | 小一寸 22×32 | 260×378 | 21.987×32.028 | 259.7×378.3 | 4×4 = 16 | 1.98/2.02 | 3.9 × 9.1 |
| `id-1cun-large-6-2x3` | 大一寸 33×48 | 390×567 | 32.980×47.982 | 389.5×566.7 | 2×3 = 6 | 1.98/2.02 | 16.8 × 2.2 |
| `id-2cun-6-2x3` | 二寸 35×49 | 413×579 | 35.004×48.994 | 413.4×578.7 | 2×3 = 6 | 1.98/2.02 | 14.8 × 0.7 |
| `id-2cun-large-4-2x3` | 大二寸 35×53 | 413×626 | 35.004×52.983 | 413.4×625.8 | 2×2 = 4 | 1.98/2.02 | 14.8 × 22.2 |
| `id-2cun-small-6-2x3` | 小二寸 35×45 | 413×531 | 35.004×45.006 | 413.4×531.6 | 2×3 = 6 | 1.98/2.02 | 14.8 × 6.7 |

- The five are the human's list. **大二寸 35×53 mm (413×626 px)** is the sixth: the sources call it the
  *standard* two-inch size (百度百科: 3.5×5.3 cm, 413×626) while 35×49 is the other one in wide use, and the
  human's 二寸 is the latter — so the set carries both, named by size and not by rank in the UI.
- The 2-inch sheets' 0.7–2.2 mm margins are why the ruling says borderless; nothing in the layout depends on
  the margin.
- The sheet is the **inch** paper (101.6×152.4); the nominal "6寸 = 102×152" is 0.6% larger, and a print scaled
  to it stretches the cells by that much. Stated in `docs/CONTRACT.md` §3.
- Names are stable machine vocabulary; the **size is copy** and goes through gettext (below). Order in the
  library and the strip: the human's list, 大二寸 after 二寸, 小二寸 last.
- **The generator** grows a recipe that authors a sheet in mm (`cell_mm`, `gap_mm`, the 4R sheet) and converts
  to the 2560 lattice per axis. The frozen artifact gains the six entries **after every collage arrangement**
  (the "a different kind" position), and `thaw` reads the one-photo bit from a committed `ONE_PHOTO` name list
  so **every pre-S40 entry stays byte-identical** — the same audit S39 used.

## The window

- **The entry is the ratio row** (`Gallery`): when the document holds one cell, the row gets a last member —
  `gettext("ID photo")` — to the right of 16:9, in the same radio group. Activating it switches to
  `id-1cun-12-2x3` (一寸, the first), one `Command::SetTemplate`, one undo step; the strip then lists the six
  ID sheets. Activating a shape button leaves the ID sheet for that shape's one-slot collage.
- **The strip's six candidates** are sketches as usual, and each carries the **size name** as its tooltip and
  as its accessible name (`"一寸, layout 2 of 6"`), per the ruling's "悬停或者选中" — no permanent caption, so
  the band's height (and the canvas's) does not move. If the walk says a tooltip is not readable enough, the
  fallback is a caption on the ID candidates only, and that is a walk decision, not this plan's.
  A `photo_size_name(name) -> Option<String>` `match` beside `shortcut_title`'s pattern keeps every string a
  literal `gettext` call.
- On a `onePhoto` document the **count control is hidden** (the sheet has one photo), the **ratio highlight**
  marks the ID member rather than the 2:3 shape, the **cell strip's swap control is hidden** (there is no
  second cell) and the strip is **anchored to the canvas's bottom-start corner** when the selected tile is too
  small for both its row and its column — the ID tiles are the first layouts that reach the "last resort"
  branch `canvas.rs` documents.
- `Add photos…` on a `onePhoto` document is a **single-photo chooser** and its label/accessible name switch to
  "Add the photo…" / "Replace the photo…" with the document's state. The empty sheet's `+` appears on the
  sheet once: only slot 0 resolves to a cell, which the existing `cells.get(slot)` logic already gives.
- The export form is unchanged (a long edge in pixels); a 6-inch sheet at 300 dpi is `--long-edge 1800`, which
  the 2:3 aspect makes exactly 1200×1800. No dpi field, no preset, no physical size in any surface.

## The CLI

- `init --template id-1cun-12-2x3 --photo a.jpg`: on a `onePhoto` template the photo count is **0 or 1**
  (1 fills the sheet); 2+ is a usage error (exit 1) naming the rule, and the existing "the template's slot
  count must equal the photo count" rule is replaced rather than patched for these templates.
- `edit --slot <i>` (any slot of the sheet) frames the sheet's one cell and is applied once; `--swap`,
  `--add-cell`, `--remove-cell`, `--add-photo` are refused (exit 2). `--template` onto an ID sheet from a
  document with one photo works and keeps it; from more, exit 2.
- `templates`: the unfiltered listing includes the six (`--json` gains `template.N.one_photo`); `--slots` and
  `--aspect` keep answering the *gallery's* collage query and exclude them, documented in the help.
- `probe` reports the sheet **per slot** (12 tiles, each saying cell 0's photo) and the gutters as the
  document's own backdrop — the existing checks, with the slot→cell accessor.
- No new subcommand: the machine path is `init` + `render`/`probe`/`save`.

## Copy (gettext, `po/` and `po/zh_CN.po` in the same change)

`"ID photo"`, `"1-inch photo"`, `"Small 1-inch photo"`, `"Large 1-inch photo"`, `"2-inch photo"`,
`"Large 2-inch photo"`, `"Small 2-inch photo"`, `"Add the photo…"`, `"Replace the photo…"` (the last two
replace nothing — the existing "Add photos…" stays for collages). zh_CN: 证件照, 一寸照片, 小一寸照片,
大一寸照片, 二寸照片, 大二寸照片, 小二寸照片, 添加照片…, 更换照片…; `crates/pixlay/tests/i18n.rs` holds the catalog to
the template, so the change is one commit.

## Documents

`docs/CONTRACT.md` §1 (`onePhoto`), §2 (`MAX_TEMPLATE_SLOTS`, the template slot bound), §3 (the six members,
their provenance, the candidate-query exclusion, the census **293 templates / 1572 slots**), §5 (the CLI
rules), §8 (S40's measured numbers), §9 (the row's ID member); `CHANGELOG.md` (Unreleased · Added);
`docs/HIG-REVIEW.md` (the walk's items); the slot canaries move 1522 → 1572 (imaging 4566 → 4716).

## Machine-checkable exit

- **The audit**: `cargo run -p pixlay-core --bin pixlay-gen-templates` and `git diff` on
  `templates/frozen.rs` show **additions only** — every pre-S40 member's name, version, aspect and geometry
  byte-identical.
- `cargo test -p pixlay-core --test templates`: the six members' cells are within **half a lattice unit** of
  the table above (the test carries the mm and the 4R sheet, and asserts the converted-back size), the counts
  and gaps are the table's, no overlap/no hole holds, and the lattice assertion becomes "within 1e-9 of a
  multiple of 1/2560" — a *representation* bound; the 512-sample raster stays exact because a coordinate
  `k/2560` can never equal a sample `(2i+1)/1024` (that would need `2k = 5(2i+1)`, an even number equal to an
  odd one).
- `pixlay-render init --template id-1cun-12-2x3 --photo <fixture> --out /var/tmp/pixlay-s40/id.pixlay` →
  `probe --long-edge 1800`: 12 tiles, every one cell 0's photo, the gutters the backdrop, the render
  **1200×1800**; `save` round-trips the project byte-identically.
- `pixlay-core` unit tests: one cell on load and in `validate`; the command rules (fan-out, one-photo
  AddPhotos/PlacePhotos, the four refusals, `SetTemplate` onto/off); a non-uniform or multi-cell `onePhoto`
  document is refused.
- The GUI suite (mutter): the row's ID member exists **only at one cell**; activating it lands on
  `id-1cun-12-2x3`; the strip holds six; every candidate's tooltip and accessible name is its size; a drag on
  tile 5 changes cell 0's crop only and the document still has one cell; the count control is hidden; the
  single-photo chooser fills the sheet; `Export` at 1800 writes 1200×1800.
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test`, `reuse lint`.

## Human

**Not passed.** The walk judges, at the same sitting:

- the six sheets' **sketches and names** — hover and focus, do `二寸` and `大二寸` read apart;
- a **printed** 6-inch sheet (or the 1800-px PNG measured): is each tile the size in the table above, and does
  the trim read right at 2 mm;
- the **strip on a 25-mm tile** (the corner anchor) and the single-photo chooser's copy;
- the main path: open → one photo → the row's `证件照` → a size → adjust → export, under three minutes.

## Result

Not yet — written when the step lands, with the ruling block of the walk and the measured numbers.
