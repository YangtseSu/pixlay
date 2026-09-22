# GNOME HIG conformance checklist

The "GNOME HIG" section of `AGENTS.md` is a **hard constraint**, and this file is its execution checklist: it sorts
every HIG section into the three tiers "machine-checkable criteria / visual criteria / not applicable", and records
clearly which chapters **have already been read page by page** and which have not.

- Spec: <https://developer.gnome.org/hig/> (no version number, **not frozen**; cite URLs and section names)
- The most recent page-by-page read of this file: **2026-09-21 (S7)**. At the start of every UI step (S7, S8), re-read the relevant chapters before updating this file.
- **Re-routed 2026-09-22**: the main path became `open → pick 2–9 photos → pick a layout → adjust →
  export` (`docs/2026-09-22-UX-DIRECTION.md`; the rulings are in `docs/2026-09-22-STEPS.md`). Two things
  in this file changed because of it — **`selection-mode` now applies** (the picker is a collection view)
  and the phone-style deviation is narrower (the chrome, not the capability). The chapters the new stages
  need are still listed in section 4 and are read when the picker stage starts.
- As soon as a chapter is read, write that chapter's criteria into the table above: whatever can be computed goes into the tests (`crates/pixlay/tests/hig.rs`),
  whatever can only be looked at goes into "section 2". **Do not let it pile up** — HIG changes, and letting it pile up is the same as re-reading it next time.

## 1. Read chapters -> criteria

Chapters read page by page for S7: `reference/keyboard`, `guidelines/adaptive`, `guidelines/ui-styling`,
`guidelines/accessibility`, `guidelines/writing-style`, `patterns/containers/utility-panes`, `patterns/feedback`
(index). The rest of `patterns/containers` and `patterns/feedback` are still only their index pages (section 4).

| HIG chapter | Landing point | Criteria |
|---|---|---|
| `index` (platform definition: GTK4 + libadwaita) | `AGENTS.md`, `crates/pixlay/Cargo.toml` | GUI only in the `pixlay` crate; no other crate pulls in GTK — `cargo tree` shows gtk4 under `pixlay` alone, and `pixlay-core`/`-imaging`/`-render`/`-cli` must not name it |
| `guidelines/ui-styling` | `window.rs` (libadwaita containers and rows only), `canvas.rs`, test `tests/hig.rs::check_colour_schemes` | no hard-coded colours anywhere in the shell: the canvas overlays are drawn with the widget's own theme colour (`Widget::color()`), everything else is a libadwaita style class or a stock widget. The app follows the system style (`AdwStyleManager` untouched), starts under both forced schemes, and **the canvas pixels are byte-identical under both** (asserted) |
| `guidelines/accessibility` | test `tests/hig.rs::check_accessible_names` + "section 2" of this file | every interactive control has an accessible name — set explicitly, or derived by GTK from the control's own label; the check walks the widget tree and accepts both, since GTK names a `GtkButton` from its `GtkLabel`. High contrast / large text / screen reader / OSK are visual steps (section 2) |
| `guidelines/keyboard` | `app.rs` (`ACCELERATORS`), `canvas.rs` (arrow keys, `+`/`-`, `0`, `Enter`, `Delete`), test `tests/hig.rs::check_shortcuts` | every action has a keyboard path: the actions the table binds are checked against `GtkApplication::accels_for_action`, and every other action is on a focusable control. The canvas is focusable and pans, zooms, resets, chooses a photo and clears a slot from the keyboard |
| `reference/keyboard` | same test | the required set for this product (`Ctrl+Q`, `Ctrl+W`, `Ctrl+O`, `Ctrl+S`, `Shift+Ctrl+S`, `Ctrl+Z`, `Shift+Ctrl+Z`, `Ctrl+?`, `Ctrl+N`) is present — `F9` left with the pane (ruling 18, S13) — and nothing binds the system's own combinations (`Alt+*`, `Super+*`, `Ctrl+Alt+*`) — both asserted against the one table the dialog and the bindings share |
| `guidelines/adaptive` | `window.rs` (the shell), test `tests/hig.rs::check_adaptive_minimum` | no utility pane since ruling 18 (S13): at the minimum window size the sheet is still drawn in full inside the canvas — asserted; the `AdwOverlaySplitView` overlay behaviour the old row described left with the pane |
| `guidelines/writing-style` | every string in `crates/pixlay/src`, `po/pixlay.pot` | header capitalization on buttons, menu items and tooltips; sentence capitalization on row, slider and combo labels; an ellipsis exactly where the action asks for more input (`Open…`, `Save as…`, `Choose photo…`); no `i.e.`/`e.g.`; no pronouns; no trailing periods outside explanatory body text. The wording itself is a visual step (section 2), and whether a string missed its `gettext` call is only checkable by eye (the extractor cannot see what nobody wrapped) |
| `patterns/containers/utility-panes` | — | **not applicable since ruling 18** (S13 removes the pane): a linear three-minute flow owns its controls per stage — tray, gallery, floating buttons, dialogs — and a permanent panel would be a second surface for every one of those decisions |
| `patterns/feedback` (index) | `window.rs`, `export.rs` | reversible feedback goes through `AdwToast` ("Saved …", "Exported …", a failed decode); the missing-photo case is an `AdwBanner` with a button that selects the slot; export progress is a `GtkProgressBar` in the bottom bar, never a modal dialog; the dialogs are the unsaved-changes confirmation on close plus `Export…` and `Frame…` (ruling 18), both `AdwDialog` rows rather than permanent controls |
| `patterns/containers/selection-mode` | the picker (the plan's S13), test `tests/hig.rs` | **Applies from the picker stage on** (flipped 2026-09-22): the photo picker *is* a collection view with multi-select batch operations, so it is built in selection mode — a cell's selection is a toggle with the platform's own checkmark, `Ctrl+A` selects all, a selection past the cap is reported rather than truncated, the batch action is the header bar's Next button (carrying the count), and `Esc` leaves selection mode. The canvas is unaffected: this page's own advice ("when editing is the primary interaction there should be no separate edit mode") still governs it, and it has no mode of its own |

## 2. Visual steps

Follow the "Testing for Accessibility" of HIG `guidelines/accessibility` item by item. **The second step of every item is an additional criterion of this product**:
the canvas is **content** and the interface is **styling**, and the two must not affect each other (see `AGENTS.md` "Composite onto opaque white").

1. **High contrast mode** (GTK Inspector or the system accessibility settings): every UI element renders normally; canvas pixels are unchanged.
2. **Large text** (system accessibility settings): every label stays readable and is not truncated; canvas pixels are unchanged (the canvas is document content and does not scale with the interface's font size).
3. **Keyboard-only**: walk "pick photos → pick a layout → adjust framing → export from the dialog" with the keyboard alone; the focus order is logical;
   `F10` opens the menu, `Esc` closes overlays, `Tab` covers every control.
4. **Screen reader**: every control is read out, the accessible name is accurate and short; it stays operable with the monitor off.
5. **Touch / on-screen keyboard (OSK)**: the project name and the export path can be typed entirely with the OSK.
6. **S15 additions** (the step that walks the re-routed path):
   - the "three-minute main path": the scripted step list below, timed by hand;
   - the copy: is any label wrapped, truncated or unclear, and does a squeezed row read as intended? (The strings themselves are checked mechanically; the look is not.);
   - the picker: is the cell size right at a normal window size, and does the selection state read at a glance;
   - the layout gallery: does a candidate thumbnail read as "my photos in *that* layout";
   - the floating buttons: do they land where the hand expects, and does the frame look right at both ends of its radius range.

### The re-routed main path's scripted step list (the three-minute walk)

Written for `open → pick 2–9 photos → pick a layout → adjust → export`, and walked once, at the plan's
S15. Run the app (`cargo run --release -p pixlay`, or the installed `pixlay` after S16) and time the whole list:

1. *Pick photos*: the window opens on the library; select two to five photos in the grid (the tray along the bottom shows them in order).
2. *Next*: the header bar's Next button carries the count; press it.
3. *Pick a layout*: every candidate is the user's own photos in that layout; pick one, and add or remove a photo if the count is wrong.
4. *Adjust framing*: select a cell, drag inside it to move the photo, scroll to zoom, rotate by any angle (there is no cap), replace or clear a cell from the floating buttons, and set the frame's gap / radius / colour.
5. *Export*: press `Export…`, pick format and quality, choose the path once, and the progress bar in the bottom bar runs to the toast with the file's name and size.

The retired plan's script — `pick a template → place photos → adjust framing → export`, which began on a
template list and an empty sheet — is in `docs/archive/2026-09-20-STEPS.md`. Its walk was **voided** on
2026-09-22 (ruling 6): that path is not the product's any more, so walking it would have been a
rehearsal for a screen that no longer exists.

What is already machine-checked, and therefore not what this walk is for: the path works at all
(`crates/pixlay/tests/mainpath.rs` walks the list in the same calls the widgets make — the list above,
once S13–S15 have landed), the copy is English under any locale, the shortcuts are bound, and the canvas
matches the CLI's render. **What only a person can judge** is whether the walk is short, whether the
picker reads, whether the framing and rotation gestures feel right, and whether the copy reads well.

## 3. Deliberate deviations (the same list as in `AGENTS.md`, do not fix)

| HIG item | Decision | Reason |
|---|---|---|
| GNOME Shell search provider, notification workflow | Not doing | the same discipline as the "Not doing" list: add no feature that does not serve the main path |
| Phone-style layout — the *chrome*, not the capability (2026-09-22) | the capability is built with desktop idioms; the chrome is not copied | the picker-first flow came from mobile galleries, and `GtkGridView` + selection mode + a header-bar Next is the same capability read on the Arch desktop. The ordered tray along the bottom stays because selection *order* is cell order and re-ordering it is a desktop need — it is the one thing the phone's strip and this tray genuinely share |
| Per-app style preference (light / dark / system, pick one of the three) | Not doing | the shortest main path; "follow the system" already covers how a user expresses "I want dark" |
| Large-text mode acting on the canvas | Not applied | preview and export must be from the same source, pixel by pixel; the canvas is document content, not interface |
| access keys (`Alt+` mnemonics) | Not doing | this application has no menu bar |

## 4. Chapters not yet read page by page (read them and backfill section 1 as the plan's UI steps start)

Re-read at the start of the picker stage (the plan's S13) because that is the first step whose widgets are new:

- `patterns/containers/selection-mode` — **now applicable** (it was read as "not applicable" in S7 and the
  row is flipped above), together with the rest of `patterns/containers/*` beyond `utility-panes`
- `patterns/nav`, `guidelines/navigation` (the picker pushing the editor is a navigation shape)
- `guidelines/pointer-touch` (the grid's and the preview's gestures)

Still unread, and read when their steps need them:

- `principles`, `resources`
- `guidelines`: `app-naming` (S16), `app-icons` (S16), `ui-icons`, `typography`
- `patterns/controls/*`, and `patterns/containers/dialogs` (the `Export…`/`Frame…` dialogs of ruling 18)
- the per-page details of `patterns/feedback/*` (only the index has been read)
- the UI colors under `reference/`
