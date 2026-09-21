# GNOME HIG conformance checklist

The "GNOME HIG" section of `AGENTS.md` is a **hard constraint**, and this file is its execution checklist: it sorts
every HIG section into the three tiers "machine-checkable criteria / visual criteria / not applicable", and records
clearly which chapters **have already been read page by page** and which have not.

- Spec: <https://developer.gnome.org/hig/> (no version number, **not frozen**; cite URLs and section names)
- The most recent page-by-page read of this file: **2026-09-21 (S7)**. At the start of every UI step (S7, S8), re-read the relevant chapters before updating this file.
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
| `reference/keyboard` | same test | the required set for this product (`Ctrl+Q`, `Ctrl+W`, `Ctrl+O`, `Ctrl+S`, `Shift+Ctrl+S`, `Ctrl+Z`, `Shift+Ctrl+Z`, `Ctrl+?`, `Ctrl+N`, `F9`) is present, and nothing binds the system's own combinations (`Alt+*`, `Super+*`, `Ctrl+Alt+*`) — both asserted against the one table the dialog and the bindings share |
| `guidelines/adaptive` | `window.rs` (the shell), test `tests/hig.rs::check_adaptive_minimum` | `AdwOverlaySplitView` overlays the utility pane on the canvas when the window is too narrow; at the minimum window size the sheet is still drawn in full inside the canvas and the pane is still allocated — asserted |
| `guidelines/writing-style` | every string in `crates/pixlay/src`, `po/pixlay.pot` | header capitalization on buttons, menu items and tooltips; sentence capitalization on row, slider and combo labels; an ellipsis exactly where the action asks for more input (`Open…`, `Save as…`, `Choose photo…`); no `i.e.`/`e.g.`; no pronouns; no trailing periods outside explanatory body text. The wording itself is a visual step (section 2), and whether a string missed its `gettext` call is only checkable by eye (the extractor cannot see what nobody wrapped) |
| `patterns/containers/utility-panes` | `sidebar.rs`, `window.rs` | the editing controls live in a utility pane (`AdwOverlaySplitView`), toggled with `F9` so it can be hidden while looking at the collage, and it overlays rather than squeezes the canvas when narrow |
| `patterns/feedback` (index) | `window.rs`, `export.rs` | reversible feedback goes through `AdwToast` ("Saved …", "Exported …", a failed decode); the missing-photo case is an `AdwBanner` with a button that selects the slot; export progress is a `GtkProgressBar` in the bottom bar, never a modal dialog; the one dialog is the unsaved-changes confirmation on close |
| `patterns/containers/selection-mode` | **Not applicable** | there are no collection views and no multi-select batch operations; the canvas selects one slot because editing is the primary interaction, which is what that page itself recommends |

## 2. Visual steps

Follow the "Testing for Accessibility" of HIG `guidelines/accessibility` item by item. **The second step of every item is an additional criterion of this product**:
the canvas is **content** and the interface is **styling**, and the two must not affect each other (see `AGENTS.md` "Composite onto opaque white").

1. **High contrast mode** (GTK Inspector or the system accessibility settings): every UI element renders normally; canvas pixels are unchanged.
2. **Large text** (system accessibility settings): every label stays readable and is not truncated; canvas pixels are unchanged (text layers are document content and do not scale with the font size).
3. **Keyboard-only**: walk "pick a template → place photos → adjust framing → export" with the keyboard alone; the focus order is logical;
   `F10` opens the menu, `Esc` closes overlays, `Tab` covers every control.
4. **Screen reader**: every control is read out, the accessible name is accurate and short; it stays operable with the monitor off.
5. **Touch / on-screen keyboard (OSK)**: text-layer content and the export path can be typed entirely with the OSK.
6. **S7 additions**:
   - the "three-minute main path": the scripted step list below, timed by hand;
   - the copy: is any label wrapped, truncated or unclear, and does a squeezed row read as intended? (The strings themselves are checked mechanically; the look is not.)

### S7's scripted step list (the three-minute walk)

Run the app (`cargo run --release -p pixlay`, or the installed `pixlay` after S8) and time the whole list:

1. *Pick a template*: choose the sheet size (A4), then a layout in the Template list — one click, no dialog.
2. *Place photos*: drop two to five photos on the canvas (or select a slot and press `Ctrl+I`), once each.
3. *Adjust framing*: select a slot, drag inside it to move the photo, scroll to zoom, drag the Straighten slider to line the horizon up against the guides, then press `Ctrl+0` if it needs to go back.
4. *Export*: press `Export` (the path is asked for once), and the progress bar in the bottom bar runs to the toast with the file's name and size.

What is already machine-checked, and therefore not what this walk is for: the path works at all
(`crates/pixlay/tests/mainpath.rs` walks exactly this list in the same calls the widgets make), the copy is English
under any locale, the shortcuts are bound, and the canvas matches the CLI's render. **What only a person can judge**
is whether the walk is short, whether the framing gestures feel right, and whether the copy reads well.

## 3. Deliberate deviations (the same one as in `AGENTS.md`, do not fix)

| HIG item | Decision | Reason |
|---|---|---|
| GNOME Shell search provider, notification workflow | Not doing | the same discipline as the "Not doing" list: add no feature that does not serve the main path |
| Phone-style layout | Not doing | the target platform is the Arch desktop; the canvas has physical-size semantics |
| Per-app style preference (light / dark / system, pick one of the three) | Not doing | the shortest main path; "follow the system" already covers how a user expresses "I want dark" |
| Large-text mode acting on canvas text layers | Not applied | preview and export must be from the same source, pixel by pixel; text layers are document content |
| access keys (`Alt+` mnemonics) | Not doing | this application has no menu bar |

## 4. Chapters not yet read page by page (read them and backfill section 1 when S8 starts)

- `principles`, `resources`
- `guidelines`: `app-naming` (S8), `app-icons` (S8), `ui-icons`, `typography`, `navigation`, `pointer-touch`
- `patterns/nav`, `patterns/controls/*`
- the **per-page details** of `patterns/containers/*` (only `utility-panes` has been read so far) and of
  `patterns/feedback/*` (only the index has been read)
- the UI colors under `reference/`
