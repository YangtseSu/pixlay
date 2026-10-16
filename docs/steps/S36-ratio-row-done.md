<!--
SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>

SPDX-License-Identifier: GPL-3.0-or-later
-->

# S36 · The sheet's ratio is a control

**Progress**: done and passed (2026-10-16, human) → next: **S37 — Arrangement depth: the poster family
and irregular cells** (`docs/steps/S37-arrangement-depth-todo.md`), in a new session, because a gate's next
action is a new session (`AGENTS.md`, "Session and persistence discipline"). The work is in `66b60bc` (the
dependency defect it exposed is fixed in `9c68b0a`); both are **local, not pushed**; this gate is its own
commit.

**Goal**: the canvas ratio becomes the layout stage's first choice — Xiaomi's 布局 model — so that a
person says "square" or "portrait" and then picks among that shape's arrangements, and the sheet's shape
survives adding photos.

**Work**

- The band (`crates/pixlay/src/layout.rs`): a **ratio row** in the band, ahead of the strip — the ratios
  the current count offers, in a fixed documented order, with the document's own ratio marked. Activating one
  switches the document to that ratio's layout (`SetTemplate`, one undo step), and every surviving cell
  keeps its photo and framing. The strip then lists only that ratio's candidates. The row follows the
  document — an undo, a project load, the count control — the way the highlight does; a count offering a
  single ratio (count 1) shows no row, because there is nothing to choose. *Proposed model*: the row
  **switches** the sheet (the reference's own behaviour); the alternative — a filter that changes nothing
  until a sketch is clicked — leaves two sources of truth and is not planned.
- **One rule, in core**: the query "the template with this count *and this aspect*, or nothing"
  (`crates/pixlay-core/src/selection.rs`, beside `layout_for`; it is `layout_for`'s answer narrowed to an
  exact aspect match within `ASPECT_TOLERANCE`). `layout_for` stays the count rule's (nearest-aspect)
  answer — a *request* for a ratio the library does not have must refuse rather than silently land on
  another shape — and the GUI and the CLI read this one function so they cannot disagree (S14's rule).
- The CLI: `edit --aspect <W:H|decimal>`, reusing `templates --aspect`'s parser — the same switch, and a
  refusal where no layout of that count has the ratio (exit 2, a message naming the count and the ratio,
  nothing written). `switch --aspect` joins `switch --template` if the band's own measure (S18's
  `--band`) should cover the row's rebuild.
- The strip's query (`EditorWindow::candidate_templates`) narrows to the document's ratio. The band's
  sketch cache is keyed by template name as it is, and a ratio switch may build candidates not drawn yet
  (measure it).
- `docs/CONTRACT.md`: §3 (the ratio vocabulary and the query), the `edit` flag table, §5 if a report
  field is added; `docs/HIG-REVIEW.md` §2's walk item; any copy the row carries goes through gettext
  (`po/`), and a bare ratio label is not copy.

**Machine-checkable exit**

- `crates/pixlay/tests/layout.rs`: the row lists exactly the current count's ratios in the documented
  order and marks the document's own; activating one lands on the core query's answer for (count, ratio,
  family); every surviving cell's photo and framing is unchanged; **one** `Ctrl+Z` restores the previous
  template and photos; the strip then holds only that ratio's candidates; the mark follows the document
  across an undo, a project load and a growth (the count control).
- `crates/pixlay-cli/tests/cli.rs`: `edit --aspect 1:1` writes a project whose template equals the GUI
  test's landing for the same document; a ratio with no layout at that count refuses with exit 2, stdout
  empty and nothing written.
- `crates/pixlay-core/tests/selection.rs`: the query's exactness (never a nearest-aspect fallback) and
  its refusal (`None` outside the library's ratios).
- The band's rebuild measured with the row in place (`switch --band`), and the verification entry green.

**Implemented as** (2026-10-16) — one query, two surfaces, and a defect the step exposed:

- **Three functions in core, and the doc holding them.** `selection::layout_with_aspect(count, aspect,
  family)` is `layout_for` with the nearest-aspect rung removed: the layout with this count and **exactly**
  this shape (`templates::with_slots_and_aspect`'s group), the document's own recipe family preferred where
  the shape has several members and library order where nothing separates, `None` when the count has no
  member of that shape. `templates::ratios_for(slots)` is the row's content — the shapes a count offers,
  **ascending** (2:3, 3:4, 1:1, 4:3, 3:2, 16:9), which keeps each shape's place as the count moves.
  `templates::ratio_label` moved here from the CLI, so the row's labels and every report's `aspect` are one
  function rather than two that agree by hand — and it is a bare ratio, not copy: no translation, no noun.
- **The band.** `Gallery` gained the row (`ratio_cell`: a `GtkToggleButton::with_label`, joined by
  `set_group` so GTK's own radio behaviour is the mechanism, the document's shape carrying the same
  `.picked` accent border the strip's candidate does) and `show_ratios` rebuilds it only when the **set**
  changes, marking from the document — so an undo, a project load or a count change cannot leave the mark
  lying. The band is `[count control | ratio row over strip]`; a count offering one shape draws no row.
  `EditorWindow::candidate_templates` narrowed to `with_slots_and_aspect`, and `select_ratio` is the
  click's one `Command::SetTemplate`. **The toggle-off branch is not the strip's**: GTK's group
  bookkeeping deactivates the marked member *before* it emits the new member's own signal, so a re-mark
  there would switch the row straight back — it restores the mark only when the row is left unmarked.
- **The row's own look, and one thing the machine check got wrong first.** The row is homogeneous and
  packed at its column's start (a stretched row is a segmented control, not a radio set): five shapes in
  **359x36**, buttons 73 px apart, so every shape is one size. The HIG check's first reading said `1:1`
  was 18 px wide and "below the 24 px target" — a **false alarm**: `Widget::width()` reports the *content*
  box, and a text button's content box is its label (31 px for `4:3`) inside a 67 px painted button, which
  is where the theme's own padding lives. The check reads `measure` (the theme's request, which the
  homogeneous packing only grows) and the label's own fit now; no floor and no size request was needed,
  and neither is in the code.
- **The CLI.** `edit --aspect <W:H|decimal>` (one parser with `templates --aspect`), mutually exclusive
  with `--template`, refusing a shape the count has none of with exit **2** and the count, the shape and
  the shapes it *does* offer named — where `layout_for`'s nearest-aspect fallback would have quietly
  landed elsewhere. `edit`'s report carries `aspect` now (a template's name does not carry its ratio, S35,
  so this is where the flag's answer is readable), `templates --slots --aspect` is the band's own list,
  and `switch --band` measures the narrowed rebuild.
- **A dependency defect the step exposed, fixed in the dependency.** `mainpath`'s save-and-reopen failed
  by **one ULP**: serde_json's default `f64` parser reads `3.7586652467163644` as
  `3.758665246716365`, so a saved crop was not the crop that was written and a save → load → save was not
  byte-identical — a `.pixlay` contract, not a cosmetic difference. The fix is the crate's own
  `float_roundtrip` feature (`serde_json` 1.0.154) enabled in `pixlay-core`, its own `chore:` commit, and
  `crates/pixlay-core/tests/project.rs`'s `document()` now carries that 17-digit crop so the round trip is
  asserted rather than assumed. Its registry row and the reason are `AGENTS.md`.

**Result** (2026-10-16): the row is `crates/pixlay/src/layout.rs`'s (cells in `style.css`), the query is
`pixlay-core`'s, and the tests are the four the exit named — `crates/pixlay-core/tests/selection.rs`
(exactness, the tolerance boundary, the refusal, the family preference over the whole library),
`crates/pixlay-core/tests/templates.rs` (a new census: the row's order, one entry per shape, every entry
with a member behind it, and every member reachable from the row), `crates/pixlay/tests/layout.rs` (the row
against every document this test opens: its content and order, the click's landing and the strip it narrows
to, one `Ctrl+Z`, and the mark following an undo, a load and a growth — plus the sketch and caption criteria
now walked **shape by shape**, since the strip holds one shape at a time), and `crates/pixlay-cli/tests/cli.rs`
(`--aspect`'s landing read from the core query, the report's `aspect`, the exit-2 refusal, the mutual
exclusion, and idempotence). Three measurement tests moved with the band rather than being relaxed:
`switch.rs` measures the **row's** clicks (the shape is what a user clicks first, and the verification
document has one 4:3 arrangement), `mainpath` picks a *shape*, and the CLI's `switch --band` asserts the
narrowed count. The numbers are `docs/CONTRACT.md` §8, "S36" — 1 candidate at 0.09–0.10 ms for the
verification document and 2 at 0.11–0.14 ms for the library's only two-member cell, against a `switch_ms`
of 27.8–43.0 on the same project — and the whole entry is green — `cargo test` 378 s wall with the GUI suite included, and the band's own test 71.4 s of it.

**Ruling (2026-10-16, human) — the gate**: **passed** — the human walked the band's new form and ruled it
with no findings ("没啥问题"): the ratio row's placement and size against the canvas, the count control and
the row reading as one stage, the selected pair legible, and the row read as a radio set. **No shape changes
with the ruling**: `docs/CONTRACT.md` §3 already carries the ratio vocabulary, the order and the three
queries, §5 the flag and the report, §9 the band's two controls, and §8 the numbers, so the gate closes with
that half of the synchronization already done — and the walk's own record is written into
`docs/HIG-REVIEW.md` §2's ratio-row item.

**Human**

- The walk of the band's new form (`docs/HIG-REVIEW.md` §2): the ratio row's placement and size against
  the canvas, the count control and the row reading as one stage, the selected pair legible, and the row
  read as a radio set (a selection always marked, and a small set — `patterns/controls/radio-buttons`).
- **Passed** (2026-10-16): the human walked the band's new form on the running editor and ruled it
  "没啥问题" (no problems), with no findings to act on. The pictures the walk rested on are
  `/var/tmp/pixlay-s7/layout-band.png` — the 8-photo verification document's band as the window draws it,
  the 4:3 row above a one-arrangement strip — and `/var/tmp/pixlay-s7/layout-band-gutter.png`, the same for
  the four-cell square pair (`crates/pixlay/tests/layout.rs` writes both).
