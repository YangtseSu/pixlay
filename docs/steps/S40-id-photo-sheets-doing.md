<!--
SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>

SPDX-License-Identifier: GPL-3.0-or-later
-->

# S40 · ID photo sheets

**Progress**: implemented and re-worked to the human's rulings of 2026-10-17 — the model, the six members, the
render/CLI/window paths and the tests are in; the walk passed on the six upright sheets (「人共没有什么问题」) and
the four rulings that followed it are built (the count control stays, each size is printed at the orientation that
holds more copies, the size names are the library's own, and no export preset is added). **→ next: a glance at the
turned sheets and the count control** (the same walk, two items), then `-done`. (The GUI-suite session window the
same runs exposed is a separate record: `S41-session-window-during-gui-tests-done.md`.)

**Goal**: a sheet of one photo repeated at one of the six ID-photo sizes on a 6-inch (4R, 4×6 inch) print — an
*ID photo sheet* — reachable from the layout band's ratio row at one photo, with no second entry point and no
second document kind: `CollageDoc` gains **one cell**, the template carries the copies.

## Ruling (2026-10-17, human)

**The walk passed** — 「人共没有什么问题」 — on the six *upright* sheets, and the same message's four follow-ups
are rulings this step now carries:

- 「尺寸保留，取什么名字，你决定。」 — the six sizes stand (大二寸 included); their template names are the
  library's own vocabulary, and the *copy* a person reads is the size name.
- 「导出不用给提示。1800是下限，只要比例对，高点像素的也无所谓」 — no export preset and no hint: a 6-inch
  print at 300 dpi is a **lower bound** (1800 px on the long edge), and any larger long edge at the same ratio
  prints the same photo. `docs/CONTRACT.md` §8 says so; nothing in the product learned a dpi.
- 「在ID card模式，左边的选择图片张数的组件，不要去掉。加1,就自动退出，到别的模式」 — the count control stays
  on a sheet, and its `+` leaves the sheet for the count rule's two-cell collage (`AddCell` on a `onePhoto`
  document is no longer refused); `−` is the one-cell floor.
- 「按照照片尺寸大小排序，从小到大。」 — the six sizes are listed **smallest photo first**: 小一寸 22×32,
  一寸 25×35, 小二寸 35×45, 大一寸 33×48, 二寸 35×49, 大二寸 35×53 (the height is strictly ascending, and so
  is the area, so the two readings of "size" agree). That is the library's order and the strip's; the row's
  member still switches to **一寸**, the size most forms ask for, which is the second member.
- 「那几张大的图片，有点浪费纸张。这个是以打印为目标的，所以横向纵向无所谓……你看是怎么排版能多打印几张。」 —
  each size is authored on the page orientation that holds more copies: 一寸 12 upright, 小一寸 **18**, and
  大一寸, 二寸 and 小二寸 **8** each turned, 大二寸 4 either way — **58 copies** against the upright page's 50.
  A *mixed* sheet (a landscape row beside a portrait one) is not expressible: every copy draws the document's one
  cell, so they share one framing and one orientation, and the leftovers on these pages are too narrow for a
  turned cell anyway (二寸's widest is 6.4 mm against the 48 mm a turned cell needs).

- 「不做更多的入口。在1张照片的时候，在16：9的右边增加『证件照』。」
- 「我的倾向于，做1个特殊的1 slot模板，而不是12。」 — the document holds **one cell**; the sheet's copies are the
  template's slots, not twelve documents' cells.
- 「相纸一般都是无边框打印」 — the sheet's outer margin is not a constraint; the layouts are the largest that
  fit.
- 「模板名不显示，普通拼图无所谓，在证件照拼图的时候不显示，不知道哪个是具体是多大的尺寸。我倾向于在鼠标悬停或者
  选中某个模板的时候，显示『一寸』。」
- Sizes: the five listed (一寸 25×35, 小一寸 22×32, 大一寸 33×48, 二寸 35×49, 小二寸 35×45), "有必要的可以多做" —
  the sixth (大二寸 35×53) is added on the research's basis.
- 「做plan文件」 — the plan this step was built from.

## Work

- **The sizes, the pages and the counts** (measured, `render --long-edge 1800`, `/var/tmp/s40probe/`):

  | member | cell (mm) | tile at 1800 px | copies | page |
  |---|---|---|---|---|
  | `id-1cun-small-18-3x2` | 22.027×31.988 | 261–262×378–379 | 6×3 = 18 | 1800×1200 |
  | `id-1cun-12-2x3` | 25.003×35.004 | 296×414–415 | 3×4 = 12 | 1200×1800 |
  | `id-2cun-small-8-3x2` | 35.004×45.006 | 414–415×533 | 4×2 = 8 | 1800×1200 |
  | `id-1cun-large-8-3x2` | 32.980×47.982 | 390–391×568 | 4×2 = 8 | 1800×1200 |
  | `id-2cun-8-3x2` | 35.004×49.014 | 414–415×580 | 4×2 = 8 | 1800×1200 |
  | `id-2cun-large-4-2x3` | 35.004×52.983 | 415×627 | 2×2 = 4 | 1200×1800 |

  (The table is the library's order: smallest photo first, per the ruling below.)

  The gap reads 22–23 px against the recipe's 2 mm (23.6 px), and every sheet's run is 42–48 ms and 31–33 MB with
  `probe` reporting `status = ok`, 0 background pixels off the backdrop and `occupied` = its copies.
- **The model: one cell, N slots.** `Template::one_photo` (serde default `false`, `skip_serializing_if`), so
  every project written before S40 is byte-identical and `DOC_VERSION` does not move. A `onePhoto` document
  holds exactly one cell and keeps nothing (`CollageDoc::validate`, `OnePhotoDocument`), and every slot draws it
  (`CollageDoc::cell_index` / `cell` / `cell_mut` — the one slot→cell mapping the renderer, the preview, the
  probe, `hit` and the shell all ask).
- **Commands** (`Command::apply`, one place): `SetSource` / `SetCrop` / `ClearCell` act on the sheet's one cell
  whatever slot they name; `AddPhotos` / `PlacePhotos` take exactly one source; `MovePhoto` and `SwapCells` are
  refused (`OnePhotoCommand` — the two edits that would read a one-cell list by slot); `SetTemplate` **onto** a
  sheet is refused unless the document holds at most one cell and keeps none (`OnePhotoLayout`); switching off
  is the ordinary retention rule, and the count control's `+` (`AddCell`) is that exit.
- **Limits**: `MAX_PHOTOS` / `MAX_SLOTS` stay 9 (a document still holds 1..=9 photos), and a *template's* own
  slot count gets `MAX_TEMPLATE_SLOTS = 16` — the largest sheet, 小一寸's 4×4 — which `Template::validate` and
  `docs/CONTRACT.md` §2 use.
- **Queries**: `with_slots`, `with_slots_and_aspect`, `of_aspect`, `ratios_for`, `selection::layout_for` and the
  window's `candidate_templates` skip `onePhoto` members; `all`, `names`, `get` and the new
  `templates::one_photo()` keep them; `Family::of` reads the `id` prefix as `Family::Id`.
- **Geometry**: the sheets are authored in millimetres on the 4R page (101.6×152.4 mm — the upright `2x3` tag or
  the turned `3x2`, whichever holds more copies) and converted to a **1/2560** lattice (0.04–0.06 mm a step);
  the gap is 2 mm, the copies are congruent, and the counts are the largest that fit (borderless, per the ruling).
  The white between copies is the sheet's backdrop, so `frame.gapRel` stays at its default 0.
- **The window**: the ratio row's **last member** (after 16:9) exists at one cell and switches to
  `id-1cun-12-2x3`; the strip then holds the six sheets; every candidate's size is its tooltip and the front of
  its accessible name; the count control **stays** (its `+` leaves the sheet); the strip's swap is insensitive;
  the strip anchors to the canvas's bottom-start corner on a tile too small for both its row and its column; the
  header's way-in button becomes a single-photo chooser whose label moves between "Add the photo…" and "Replace
  the photo…".
- **The CLI**: `init --template id-… --photo one.jpg`, `edit --slot <any tile>`, the refusals, `templates`
  carrying `one_photo`, and `probe` reading the tiles — with the help text saying all of it.
- **Copy**: *ID photo* / 证件照, the six size names, the chooser strings and the "holds one photo" report, all
  through `gettext` with the `zh_CN` catalog updated in the same change.

## Implemented as (2026-10-17)

- **The model**: `Template::one_photo` (`crates/pixlay-core/src/template.rs`), `CollageDoc::cell_index` /
  `cell` / `cell_mut` and the one-cell validation (`doc.rs`), `MAX_TEMPLATE_SLOTS = 16` (`lib.rs`), four new
  typed errors (`error.rs`: `OnePhotoDocument`, `OnePhotoLayout`, `OnePhotoCommand`, `OnePhotoPhotos`), the
  command rules (`history.rs`) and the query exclusions plus `templates::one_photo()` / `ID_PHOTO_DEFAULT`
  (`templates/mod.rs`).
- **The six members** (`templates/generator.rs` + the regenerated `frozen.rs`): `id-1cun-12-2x3` (12, upright),
  `id-1cun-small-18-3x2` (18), `id-1cun-large-8-3x2` (8), `id-2cun-8-3x2` (8), `id-2cun-large-4-2x3` (4,
  upright), `id-2cun-small-8-3x2` (8) — the recipe in millimetres (`ID_SHEET_MM`, `ID_GAP_MM`, `ID_SHEETS` with
  its `landscape` bit, `ID_GRID`), the frozen table appended after every collage arrangement, and the
  `ONE_PHOTO` name list so that **every pre-S40 entry's bytes are unchanged**.
- **The renderer** (`pixlay-render/src/draw.rs`): the loop is over **slots**, each fitted against its own slot
  with its cell's crop; `Images` stays keyed by cell, so one bitmap is blitted once per copy — sound because the
  copies are congruent, which `an_id_sheet_is_a_grid_of_congruent_cells` asserts.
- **The probe** (`pixlay-imaging/src/probe.rs`): a flat colour per occupied **cell** (as `Images` is keyed), the
  expectation is the cell's colour (`doc.cell_index(slot)`), and `probe_bitmaps` lost its `sources` parameter —
  the CLI resolves the sources as a check instead, which keeps "a photo went away" a failure.
- **The CLI**: `init`'s one-photo rule comes from `Selection::document` (`SelectionError::OnePhoto`, exit 1);
  `edit`'s `--slot` is bounded by the *template's* slots and every cell read goes through `cell_at`;
  `templates` reports `one_photo` per row; `probe` reports `cells`, `photos` and `slots`; the help text carries
  all three rules.
- **The window**: `Gallery::show_ratios` builds the ID member when the document holds one cell and
  `highlight_ratios(aspect, one_photo)` marks the document's own kind, `show_count(!one_photo)`,
  `photo_size_name` + `id_photo_cell` (`layout.rs`), `select_id_photo` / `update_add_label` /
  `candidate_templates` / `select_ratio`'s kind test / `add_photos` / `place_files` (`window.rs`), the empty
  buttons and the strip's occupancy through `doc.cell(slot)`, the corner anchor and the swap's sensitivity
  (`canvas.rs`).
- **The tests**: `crates/pixlay-core/tests/idphoto.rs` (the model, the mapping, the validation, the command
  rules, the layout switch both ways, the fit of a copy and the JSON round trip) and
  `crates/pixlay/tests/idphoto.rs` (the window's half, listed in the exit below), beside the geometry test in
  `templates.rs` and the end-to-end `an_id_photo_sheet_holds_one_photo` in `pixlay-cli/tests/cli.rs`.
- **The docs**: `CONTRACT.md` §1 (the field), §2 (both ceilings), §3 (the six members, their provenance, the
  exclusions, the census), §5 (the CLI rules), §8 (the numbers below); `CHANGELOG.md`; `HIG-REVIEW.md` §2 item
  14 (the walk); `ROADMAP.md`'s census; `po/` (the template and `zh_CN`, the i18n test holding them together).

**Six things the implementation did differently from the plan**, each recorded where it belongs:

1. **`AddPhotos` / `PlacePhotos` with one photo *replace* the sheet's photo** rather than being refused: a sheet
   has no empty cell to append to, and the window's single-photo chooser ("Replace the photo…") is exactly that
   edit. Two names in one call are still refused. (`docs/CONTRACT.md` §5 says so.)
2. **The strip's swap control is insensitive, not hidden**: the strip's length is the six controls' arithmetic
   (`STRIP_LENGTH`), and hiding one would place the rest by the wrong constant.
3. **`probe_bitmaps` paints per cell and the probe expects the cell's colour** — the first attempt painted one
   colour per *slot*, which contradicts `draw`'s own lookup (`images.get(cell_index(slot))`) and made 10 of 12
   tiles mismatch. The seam checks are vacuous on a sheet (its copies share no edge), which is why the probe's
   verdict is still a real one.
4. **The imaging sweep now covers every *slot* rather than every cell** (`pixlay-imaging/tests/layout.rs`):
   its claim — "the region holds what the slot shows" — is about the slot's geometry, and a sheet's twelve
   copies are twelve of them. The canary moved 1522 × 3 → 1580 × 3.
5. **`set_template` sizes a sheet's document to one cell** (`history.rs`): it resized to the template's slot
   count, which is right for a collage and wrong for a sheet — `a_layout_switch_onto_a_sheet_needs_a_one_cell_document`
   caught it as `OnePhotoDocument { cells: 12 }`. The two layout switches now also report the *document's* cell
   count rather than the template's slot count.
6. **Only the first tile carries the empty sheet's `+`** (`canvas.rs`): every slot resolving to the one cell made
   nine buttons all open the same chooser, which `an_id_photo_sheet_is_chosen_from_the_ratio_row_at_one_photo`
   caught (9 visible against 1). The canvas's accessible name also learned the sheet's own shape —
   *ID photo sheet, copy 5 of 12* — because "cell 5 of 12" would claim twelve cells where there is one.

## The canaries this step moved

| Test | Was | Now | Why |
|---|---|---|---|
| `pixlay-core/tests/hit.rs` (twice) | 1522 slots | **1572** | six sheets, 50 copies |
| `pixlay-imaging/tests/layout.rs` | 1522 × 3 framings | **1572 × 3** | and the sweep walks slots |
| `pixlay-core/tests/templates.rs`'s lattice | 1/128 | **1/2560** | the sheets' lattice; the check allows the division's own round-off (1e-9) |
| `templates.rs`'s count range, names, ratio row, aspect group | 1..=9, three families | per kind (`MAX_SLOTS` vs `MAX_TEMPLATE_SLOTS`), the `id` family, `onePhoto` excluded | the candidates are the collage's |
| `templates.rs`'s fingerprint | the whole emitted source | the table alone | the emitter's `ONE_PHOTO` trailer is not geometry |
| `contract.rs`'s slot-count test | 10 and 11 refused | **17 and 24** refused, 16 legal as a sheet | the template ceiling is 16 |
| `pixlay-cli/tests/cli.rs`'s listing and library render | `1..=9` slots, `cells == slots` | `one_photo` rows allow 1..=18, a sheet's `cells` is 1 | the sheets are in the library |
| `history.rs`'s no-lost-cell sweep, `selection.rs`'s three library sweeps and `templates.rs`'s per-count order walk | `templates::all()` / every member of a count | the **collage** library (`!one_photo`) | a sheet is not a layout a collage switches to (the switch is refused, walked in `idphoto.rs`), its count is its copies, and its page is not one of the six shapes a count offers |
| `gesture.rs` / `pending.rs` / `preview.rs`'s one-template predicates | `slots.len() == 2` / `== 4` | the same, plus `!one_photo` | a four-copy sheet has one cell, and those tests fill four |

## Machine-checkable exit

- `cargo run -p pixlay-core --bin pixlay-gen-templates` then `git diff` on `templates/frozen.rs`: **115
  insertions, 2 deletions** — the two deleted lines are the header prose the emitter rewrote (the lattice
  paragraph and the new `ONE_PHOTO` sentence), and every pre-S40 member's name, version, aspect and slots are
  byte-identical. The file is **173,771 bytes** (S39: 163,116).
- `cargo test -p pixlay-core --test templates` (green): the six members' cells are within half a lattice step of
  their nominal millimetres, the copies are congruent on a regular grid, the gaps measure 2 mm, the counts are
  the largest that fit, and the library's invariants hold over them.
- `cargo test -p pixlay-core --test idphoto` (9 tests, green): one cell and twelve slots on load; the identity
  mapping for a collage; a two-cell or keeping document refused; a per-slot edit reaching the one cell; one
  photo what `AddPhotos`/`PlacePhotos` accept (and two refused, and one replacing); the four commands refused
  on a sheet and accepted on a collage; `SetTemplate` onto a sheet from one cell and refused from two or with
  a kept cell; the way back to a collage; a copy's fit being its own slot with the cell's crop; and the JSON
  carrying `onePhoto` only when true.
- `pixlay-render init --template id-1cun-12-2x3 --photo <fixture>` → `probe --long-edge 1800`: `cells = 1`,
  `photos = 1`, `slots = 12`, `occupied = 12`, `out_w = 1200`, `out_h = 1800`, `status = ok`; `edit --slot 7
  --zoom 2` writes one cell with zoom 2; `--add-cell`, `--remove-cell` and `--swap 0,1` are exit 2 naming the
  one-photo rule; a two-photo `init` is exit 1 and writes nothing (`crates/pixlay-cli/tests/cli.rs`,
  `an_id_photo_sheet_holds_one_photo`).
- The GUI suite (mutter 51.1): `crates/pixlay/tests/idphoto.rs` walks the row's member (its presence at one
  cell, its absence at two, its landing on the 一寸 sheet), the strip's six in library order, each sheet's size
  as its tooltip and its accessible name, the count control that stays (and whose `+` leaves the sheet), the
  single empty-cell `+`, a framing step on the fifth
  copy reaching the one cell, the header's label moving between "Add the photo…" and "Replace the photo…", the
  four refusals, and the way back to a collage — green, beside the other seventeen binaries.
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test` (the GUI suite
  included) and `reuse lint` are green.

## Human

**Not passed.** The walk judges, at the same sitting (`docs/HIG-REVIEW.md` §2 item 14):

- the six sizes: hover and focus — do 二寸 and 大二寸 read apart, and is the tooltip enough or does this ask for
  a caption on these six candidates only;
- the entry: does the ratio row read as one control with seven members at one photo, and does the member's
  absence at two photos look deliberate;
- the strip and the chooser copy on a 25 mm tile;
- a **printed** 6-inch sheet (or the 1800-px PNG measured): is each tile the size in the table, and does the
  2 mm gap read as a cut line;
- the main path: open → one photo → the row's 证件照 → a size → adjust → export, under three minutes.

## Result

Not yet — written when the walk is ruled, with the ruling block and the measured numbers.
