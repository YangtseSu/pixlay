# GNOME HIG conformance checklist

The "GNOME HIG" section of `AGENTS.md` is a **hard constraint**, and this file is its execution checklist: it sorts
every HIG section into the three tiers "machine-checkable criteria / visual criteria / not applicable", and records
clearly which chapters **have already been read page by page** and which have not.

- Spec: <https://developer.gnome.org/hig/> (no version number, **not frozen**; cite URLs and section names)
- The most recent page-by-page read of this file: **2026-09-23 (S13c)**, for the ruling's own UI step
  ("the picker, as gthumb has it"): `patterns/containers/header-bars` (the header bar's slots and its
  primary menu), `guidelines/ui-styling` (dark by default, and the media area on the theme's background),
  `guidelines/adaptive` (the three bands' proportions), `patterns/containers/selection-mode` and
  `guidelines/pointer-touch` (the picked list's click switching the preview, `Enter`/`Space`/`Ctrl+A`), and
  `guidelines/writing-style` (the status bar's four fields). Before that: **2026-09-22 (S13)**, and the same
  day for **S13b** — for `patterns/containers/selection-mode`, `guidelines/pointer-touch` and
  `reference/keyboard`, which are the three chapters its changes touched. At the start of every UI step
  (S7, S8, S13, S13b, S13c, …), re-read the relevant chapters before updating this file.
- **Re-routed 2026-09-22**: the main path became `open → pick 2–9 photos → pick a layout → adjust →
  export` (`docs/2026-09-22-UX-DIRECTION.md`; the rulings are in `docs/2026-09-22-STEPS.md`). Two things
  in this file changed because of it — **`selection-mode` now applies** (the picker is a collection view)
  and the phone-style deviation is narrower (the chrome, not the capability).
- **S13 (2026-09-22) landed the picker stage and read the chapters it needed**: `patterns/containers/selection-mode`,
  `patterns/nav`, `guidelines/navigation` and `guidelines/pointer-touch`. Their criteria are in the table below,
  and the utility pane's row is now "not applicable" rather than "the chapters are unread" — the pane left the
  shell in this step (ruling 18).
- As soon as a chapter is read, write that chapter's criteria into the table above: whatever can be computed goes into the tests (`crates/pixlay/tests/hig.rs`),
  whatever can only be looked at goes into "section 2". **Do not let it pile up** — HIG changes, and letting it pile up is the same as re-reading it next time.

## 1. Read chapters -> criteria

Chapters read page by page for S7: `reference/keyboard`, `guidelines/adaptive`, `guidelines/ui-styling`,
`guidelines/accessibility`, `guidelines/writing-style`, `patterns/containers/utility-panes`, `patterns/feedback`
(index). Read for S13, when the picker stage (the first step whose widgets are new) landed:
`patterns/containers/selection-mode`, `patterns/nav`, `guidelines/navigation`, `guidelines/pointer-touch`.
The rest of `patterns/containers` and `patterns/feedback` are still only their index pages (section 4).

*One reading note, so the next reader does not repeat the work:* `patterns/nav` and `guidelines/navigation`
returned their page furniture but no prose when fetched on 2026-09-22 (the diagrams and the tables are rendered
client-side), so the navigation criteria below come from `selection-mode`, `pointer-touch` and the libadwaita
widget documentation (`AdwNavigationView`, `AdwHeaderBar`) instead of from that chapter's text.

| HIG chapter | Landing point | Criteria |
|---|---|---|
| `index` (platform definition: GTK4 + libadwaita) | `AGENTS.md`, `crates/pixlay/Cargo.toml` | GUI only in the `pixlay` crate; no other crate pulls in GTK — `cargo tree` shows gtk4 under `pixlay` alone, and `pixlay-core`/`-imaging`/`-render`/`-cli` must not name it |
| `guidelines/ui-styling` | `app.rs` (the colour scheme at startup), `window.rs` (libadwaita containers and rows only), `canvas.rs`, test `tests/hig.rs::check_colour_schemes` | no hard-coded colours anywhere in the shell: the canvas overlays are drawn with the widget's own theme colour (`Widget::color()`), everything else is a libadwaita style class or a stock widget. **The app is dark by default** — ruled 2026-09-22 on this page's own sentence, "apps can alternatively choose to use the dark style by default … primarily recommended for apps which display rich visual content like images or video" (`guidelines/ui-styling.rst:13`), and both reference apps force it (gthumb `Adw.ColorScheme.FORCE_DARK`; loupe `PreferDark`) — superseding the earlier "never force light or dark". The canvas is still verified under **both** forced schemes and its pixels must stay byte-identical (asserted), because the document is content and not styling. **S13c implements it** (`app.rs` sets `FORCE_DARK` at `startup`) and adds the media area's own half: it sits on the theme's background rather than on a literal, checked in `tests/hig.rs::check_picker_theme` as "the pane's backdrop equals the status bar's under a forced light *and* a forced dark scheme, and the two differ". The app's stylesheet touches exactly two things: the picker cell's outline/highlight (`--border-color`, `--accent-bg-color`) and the grid item's padding, which the theme sets to 3 px a side and which is what makes the strip 136 px instead of 130 (`gridview > child { padding: 3px }`, GTK 4.24's base stylesheet; styling that node is what the reference's own stylesheet does, `gthumb/data/css/style.css:26-42`) |
| `guidelines/accessibility` | test `tests/hig.rs::check_accessible_names` + "section 2" of this file | every interactive control has an accessible name — set explicitly, or derived by GTK from the control's own label; the check walks the widget tree and accepts both, since GTK names a `GtkButton` from its `GtkLabel`. High contrast / large text / screen reader / OSK are visual steps (section 2) |
| `guidelines/keyboard` | `app.rs` (`ACCELERATORS`), `canvas.rs` (arrow keys, `+`/`-`, `0`, `Enter`, `Delete`), `picker.rs` (`Ctrl+A`, `Esc`, and the picked list's `Ctrl+Up`/`Ctrl+Down`), test `tests/hig.rs::check_shortcuts` | every action has a keyboard path: the actions the table binds are checked against `GtkApplication::accels_for_action`, and every other action is on a focusable control. The canvas is focusable and pans, zooms, resets, chooses a photo and clears a slot from the keyboard. Since S13c the picker's `Enter` goes through GTK's own `list.activate-item` (the grid's `activate` signal) and `Space` through the list item's `listitem.select`, both ending in the same toggle — S13 claimed that in a comment and answered neither — and `Ctrl+A` is bound exactly once, by `GtkListBase`'s `list.select-all` (`gtklistbase.c:1389`; S13's own handler made one press fire twice). The picked list's re-order is a key pair on the *widget* rather than in `ACCELERATORS`, because it acts on whichever row has focus and an application-wide accelerator would fire it in the editor too (`Ctrl+Up`/`Ctrl+Down` are in neither HIG's standard set nor its reserved set) |
| `reference/keyboard` | same test | the required set for this product (`Ctrl+Q`, `Ctrl+W`, `Ctrl+O`, `Ctrl+S`, `Shift+Ctrl+S`, `Ctrl+Z`, `Shift+Ctrl+Z`, `Ctrl+?`, `Ctrl+N`) is present — `F9` left with the pane (ruling 18, S13) — and nothing binds the system's own combinations (`Alt+*`, `Super+*`, `Ctrl+Alt+*`) — both asserted against the one table the dialog and the bindings share |
| `guidelines/adaptive` | `window.rs` (the shell), tests `tests/hig.rs::check_picker_minimum` and `::check_editor_minimum` | no utility pane since ruling 18 (S13), so the two stages are checked instead: at the minimum window size the picker's preview, its grid and its picked list are all allocated, and the sheet is still drawn in full inside the canvas — both asserted. The arrangement inside the picker is asserted too, because the 2026-09-22 ruling moved it and S13b implemented it: the thumbnails sit below the preview and the picked list to the right of it, and the check compares the three widgets' actual positions (`compute_point` against the page's own root), not the widget tree. The `AdwOverlaySplitView` overlay behaviour the old row described left with the pane. **S13c replaced the arrangement with the ruling's three bands** and `tests/picker.rs` holds it to the measured numbers: at 1100x760 the content band above the status bar is 686 logical px, the media area 552 of them (80.5 %, the reference's own being 88 %), the strip 130 and the status bar 24; the picked list's height and top edge equal the pane's, the strip runs from the pane's left edge to the window's right edge, and the status bar is the last band |
| `guidelines/writing-style` | every string in `crates/pixlay/src`, `po/pixlay.pot` | header capitalization on buttons, menu items and tooltips; sentence capitalization on row, slider and combo labels; an ellipsis exactly where the action asks for more input (`Open…`, `Save as…`, `Choose photo…`); no `i.e.`/`e.g.`; no pronouns; no trailing periods outside explanatory body text. The wording itself is a visual step (section 2), and whether a string missed its `gettext` call is only checkable by eye (the extractor cannot see what nobody wrapped) |
| `patterns/containers/utility-panes` | — | **not applicable since ruling 18** (S13 removes the pane): a linear three-minute flow owns its controls per stage — picked list, gallery, floating buttons, dialogs — and a permanent panel would be a second surface for every one of those decisions |
| `patterns/feedback` (index) | `window.rs`, `export.rs` | reversible feedback goes through `AdwToast` ("Saved …", "Exported …", a failed decode); the missing-photo case is an `AdwBanner` with a button that selects the slot; export progress is a `GtkProgressBar` in the bottom bar, never a modal dialog; the dialogs are the unsaved-changes confirmation on close plus `Export…` and `Frame…` (ruling 18), both `AdwDialog` rows rather than permanent controls |
| `patterns/containers/selection-mode` | `picker.rs`, tests `tests/hig.rs::check_picker` and `tests/picker.rs` | **Applies from the picker stage on** (flipped 2026-09-22), and S13 landed it (S13b re-ruled the cell's own state): a `GtkGridView` over the folder with a `GtkMultiSelection`, a cell whose click *toggles* it (the picker owns the click, because GTK's own row handling *replaces* a multi-selection on a plain click — measured in `gtklistfactorywidget.c`), `Ctrl+A` selecting all, a selection past the cap **reported** rather than truncated (exactly once per press, S13c: `tests/hig.rs` counts the window's toasts), the batch action being the header bar's Next button (carrying the count, insensitive below two), and `Esc` clearing the pick — which rebuilds the picked list, because since S13c a row is a control that switches the pane (ruling 21) and a row left behind by a cleared pick would be a control that lies. The list's "nothing picked yet" hint is the list's own placeholder (`GtkListBox::set_placeholder`), and the rebuild removes rows one at a time: `remove_all` takes the placeholder — a child of the box — and forgets it (`gtklistbox.c`), so the hint would never come back. **The page's check box is the one part not taken**: the 2026-09-22 ruling shows a picked cell by a highlight instead (`S13 · Ruling`, and the deviation row in §3), so the criterion became the reverse — the stage contains no `.selection-mode` check button and a picked cell carries its highlight class. Two further clauses of that page are deliberately wider than it asks: it wants the check box "on hover" (nothing here is revealed on hover alone) and it wants "at least three" actions on a selection (this stage's batch action is Next). The canvas is unaffected: this page's own advice ("when editing is the primary interaction there should be no separate edit mode") still governs it, and it has no mode of its own |
| `patterns/containers/header-bars` | `picker.rs`, `window.rs`, `app.rs` (the menu), tests `tests/hig.rs` | read on 2026-09-22 for the ruling "the picker, as gthumb has it": primary and navigation actions go in the **start** slot, the heading in the **centre**, and menus in the **end** slot; group related buttons with spacers rather than linked buttons; every primary control carries a tooltip; a header bar holds few controls and no text-only or `suggested-action` buttons (`patterns/containers/header-bars.rst`, "Button Style" and "Button Grouping"). S13b put every control in the end slot and styled Next `suggested-action`; **S13c corrected both and added the primary menu the picker never had**, and `tests/hig.rs::check_header_chrome` holds the header to the three slots by geometry (the folder button left of the heading, the menu right of it) and to the ruled menu (`[New collage, Open…] · [Choose folder…] · [Keyboard shortcuts, About Pixlay]`), while `check_picker_input` checks the label defect S13b shipped — `GtkButton::set_label` *replaces* a non-label child (`gtkbutton.c`), so Next lost its `AdwButtonContent` on the first `update_next`; the label now goes on the content. Both references agree: every gthumb header ends in `open-menu-symbolic` (`data/ui/browser.ui:243-315`, `data/ui/viewer.ui:96-110`), loupe's ends in its primary menu (`src/widgets/image_window.ui:85-134`) |
| `patterns/nav` + `guidelines/navigation` | `window.rs` (`AdwNavigationView`), `picker.rs`, `tests/mainpath.rs` | **a sequence, not two modes** (the 2026-09-22 ruling): the picker is the root page and the editor is pushed on it, so `AdwHeaderBar` gets the back button from the view itself, `Back` returns to the photos, and the window title follows the visible stage. `tests/mainpath.rs` asserts the stage after Next and after `Open…` |
| `guidelines/pointer-touch` | `picker.rs`, `canvas.rs`, `tests/picker.rs` | click targets are `TILE_SIZE` squares, well past the minimum — 128 px as S13 landed it, 256 px from S13b's reading of gthumb's `thumbnail-size`, and **128 logical px again since the 2026-09-22 ruling "the picker, as gthumb has it"** (that setting's 256 is in *device* pixels: the reference's own cells measure 125 logical on a 2× display); the picked list's rows switch the pane when clicked (ruling 21), through the list's own `row-selected`, so a click and a keystroke take one path; the picker's preview is a *pan* view by this page's own table (its picture is drawn `Contain`-fitted, so it never needs a zoom gesture in this stage); every pointer action has a keyboard path — arrows move the grid's focus, `Enter`/`Space` toggle through the platform's bindings, `Ctrl+A` and `Esc` are the picker's own keys, and the picked list's row drag is `Ctrl+Up`/`Ctrl+Down` on the focused row (asserted through the list's own `GtkShortcutController`, so the trigger and the action are both the real ones), because this page requires that "all actions which can be accomplished with a pointing device should also be possible with a keyboard"; nothing is revealed on hover alone; and `Esc` cancels a drag in progress, which is this page's own clause and is `GtkDragSource`'s behaviour |

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
   - the picker: is the arrangement right at a normal window size — the media area the majority, the
     thumbnails one full-width row under it, the picked list beside the media area at its height, the status
     bar's four fields readable — are the 128 px cells the right size, does the selection highlight read
     at a glance, does a drag in the picked list land where the hand expects it to, and does the preview stay
     sharp when the window is resized (it is decoded at the pane's own pixels since S13b);
   - the layout gallery: does a candidate thumbnail read as "my photos in *that* layout";
   - the floating buttons: do they land where the hand expects, and does the frame look right at both ends of its radius range.

### The re-routed main path's scripted step list (the three-minute walk)

Written for `open → pick 2–9 photos → pick a layout → adjust → export`, and walked once, at the plan's
S15. Run the app (`cargo run --release -p pixlay`, or the installed `pixlay` after S16) and time the whole list:

1. *Pick photos*: the window opens on the library; select two to five photos in the thumbnail row (the picked list down the right edge shows them in order, and clicking one of its rows previews that photo; the status bar reads picked/total · pixels · size · zoom).
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
| Phone-style layout — the *chrome*, not the capability (2026-09-22) | the capability is built with desktop idioms; the chrome is not copied | the picker-first flow came from mobile galleries, and `GtkGridView` + selection mode + a header-bar Next is the same capability read on the Arch desktop. The ordered list of picked photos stays — down the right edge since the 2026-09-22 ruling — because selection *order* is cell order and re-ordering it is a desktop need: that is a desktop need, not a copied control |
| Per-app style preference (light / dark / system, pick one of the three) | Not doing | amended 2026-09-22: the app is **dark by default**, which is what this page's `ui-styling` guidance recommends for one that displays rich visual content, and neither reference app (gthumb, loupe) offers the switch. A stored preference would also need a settings file, which ruling 8 forbids. Dark is not a substitute for high contrast, which stays a separate system mode and is checked in §2 |
| Large-text mode acting on the canvas | Not applied | preview and export must be from the same source, pixel by pixel; the canvas is document content, not interface |
| access keys (`Alt+` mnemonics) | Not doing | this application has no menu bar |
| The selection-mode check box (HIG `patterns/containers/selection-mode`) | Not used: a picked cell is shown by a highlight — an accent border or background on the cell | the 2026-09-22 human ruling (`docs/2026-09-22-STEPS.md`, `S13 · Ruling`). A check box is drawn *over* the photo, which is the thing being chosen, and the platform's own `.selection-mode` check is an indicator rather than a control, so it costs a widget per cell to say what a border already says. This is a state, not an action, so HIG `guidelines/pointer-touch`'s "every pointer action has a keyboard path" is untouched |

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
