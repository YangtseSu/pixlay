<!--
SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>

SPDX-License-Identifier: GPL-3.0-or-later
-->

# S36 · The sheet's ratio is a control

**Progress**: todo — written 2026-10-08 as the plan's second step; it assumes S35's ratio coverage, since
the row is not a choice until a count carries several ratios.

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

**Human**

- The walk of the band's new form (`docs/HIG-REVIEW.md` §2): the ratio row's placement and size against
  the canvas, the count control and the row reading as one stage, the selected pair legible, and the row
  read as a radio set (a selection always marked, and a small set — `patterns/controls/radio-buttons`).
