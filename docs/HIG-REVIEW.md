# GNOME HIG conformance checklist

The "GNOME HIG" section of `AGENTS.md` is a **hard constraint**, and this file is its execution checklist: it sorts
every HIG section into the three tiers "machine-checkable criteria / visual criteria / not applicable", and records
clearly which chapters **have already been read page by page** and which have not.

- Spec: <https://developer.gnome.org/hig/> (no version number, **not frozen**; cite URLs and section names)
- The most recent page-by-page read of this file: **2026-09-26 (S25)**, for the preferences surface the
  export's settings moved into (ruling 36): `patterns/controls/menus` re-read for its "Standard Primary Menu
  Items" — *Preferences*, *Keyboard Shortcuts*, *Help*, *About App* "should be placed in a group at the end
  of the menu", which is the group the app's own actions already had — `patterns/containers/windows` for the
  shape of a preferences surface ("About Windows and Preferences Windows are both types of secondary window",
  they "should always belong on a primary window", and they are "typically modal to their parent primary
  window"), `reference/keyboard` for `Ctrl+,` ("Preferences … Opens the preferences window"), and
  `patterns/feedback/dialogs` re-read for the dialogs that are left: the app's own action dialog is `Frame…`
  only now, and the export's replace confirmation is the platform's own save dialog's. **A finding of that
  read**: the chapter the plan expected to re-read, `patterns/containers/preferences`, **does not exist in the
  current HIG** — the containers family is windows / header bars / popovers / utility panes / boxed lists /
  grid views / list & column views / selection & edit modes (`patterns/containers.rst`'s own toctree), and
  what governs a preferences surface is the three pages above plus libadwaita's own `AdwPreferencesDialog`.
  **S25b then merged the two surfaces** the same day (the human's ruling of 2026-09-26), and that page's own
  sentence is what allows it: a secondary window "can contain information and preferences that are relevant
  to the entire app, **or** … information and options for a single content item, such as a document Properties
  Window" — the frame's rows are the document's, the export's are the app's, and they are one dialog.
  Before that:
  **2026-09-26 (S22)**, for the entry the new shell
  gives the main path: `patterns/containers/header-bars` re-read for the window's one header bar and its
  start / centre / end slots with **no Save button** (ruling 37), `reference/keyboard` for the table
  after the picker's keys left it (`Ctrl+I` is `Add photos…`; `Ctrl+Shift+O` and `Z` went with the
  stage), `guidelines/adaptive` for the one page at the minimum window size, and
  `patterns/containers/selection-mode` for putting the chapter back to "not applicable" (the app has no
  multi-select collection view any more). Before that:
  **2026-09-25 (S15j)**, for the picker's preview
  zoom: `guidelines/pointer-touch` re-read for the pane's two gestures (its own table for "views which
  pan rather than scroll" is **Pan: click+drag**, which is what a drag in the pane is; and its
  "Additional Guidelines" — "Actions which are physically challenging to accomplish, such as
  double-clicking … should be avoided", and "all actions which can be accomplished with a pointing
  device should also be possible with a keyboard" — are what the deviation in §3 and the `Z` binding
  answer), `reference/keyboard` re-read for the new accelerator (`Z` is in neither the standard nor the
  legacy system-reserved set; that page's "View Options" are `Ctrl++` / `Ctrl+-` / `Ctrl+0`, and
  `Ctrl+0` is this app's framing reset, so the pane's own toggle takes its own key) and
  `patterns/feedback/tooltips` re-read for the pane's tooltip (short, and if one control in a container
  has one they all should; the sentence case is the recorded deviation). Before that:
  **2026-09-24 (S15c)**, for the export's two
  refusals: `patterns/feedback/dialogs` re-read (a **confirmation dialog** "has two buttons: one to
  confirm … and one to cancel"; a **destructive action** "should always be accompanied by either a
  confirmation dialog or an offer to undo"; the cancel button comes **first**; "assign the return key to
  activate the affirmative button. However, this should not be done if its action is irreversible,
  destructive or otherwise inconvenient"; "dialogs should always have a parent window"), and
  `patterns/feedback` re-read for the other half — an error dialog "should be avoided where possible …
  for simple non-critical errors, toasts can be a good alternative", which is why the source-alias
  refusal is a toast rather than a dialog. Before that:
  **2026-09-23 (S15)**, for the compose stage's controls
  and its two dialogs: `patterns/feedback/dialogs` (alert vs action dialogs — an action dialog "have a
  header bar, a heading which describes the action, and two primary buttons"; the cancel button comes
  **first**, the affirmative carries the verb — *Export* — and Esc/Return are bound to them),
  `patterns/controls/buttons` (a button outside a header bar holds "either an icon or a label, and not
  both"; imperative verbs with header capitalization; **circular buttons** are for exactly this step's case,
  "a number of smaller buttons … positioned in close proximity") and `patterns/feedback/tooltips`
  (header-bar controls all carry one; tooltip labels are short; if one control in a container has a tooltip,
  they all should). `patterns/containers/header-bars` was re-read for the editor's own header, and the
  container family's index (`patterns/containers.rst`) confirmed what the retired plan assumed wrongly:
  **there is no `patterns/containers/dialogs` chapter** — the dialogs chapter lives under
  `patterns/feedback/dialogs`. Before that:
  **2026-09-23 (S14)**, for the layout band:
  `patterns/controls/radio-buttons` (a set of mutually exclusive choices — "one button in the set should be
  selected at all times", "most appropriate for small sets of options", sentence capitalization),
  `patterns/containers/grid-views` (a collection of image cells: "each grid item should have a unique
  thumbnail", order the items usefully, and **outline each cell where the images have irregular shapes or
  inconsistent appearance** — which is what the candidates' own aspects are), `guidelines/pointer-touch`
  (the cells' target size and the keyboard path), `guidelines/adaptive` (the band shares the page with the
  canvas at the minimum window size) and `guidelines/writing-style` (the band's labels). **2026-09-26 (S21)**
  re-read the same chapters for the band's change to sketches — `grid-views` for "each grid item should have
  a unique thumbnail" (each layout's sketch *is* unique: it is that layout's geometry), `ui-styling` for the
  two colours a sketch now has, and `accessibility` for the positional name ruling 40 requires. Before that:
  **2026-09-23 (S13c)**, for the ruling's own UI step — for `patterns/containers/selection-mode`, `guidelines/pointer-touch` and
  `reference/keyboard`, which are the three chapters its changes touched. At the start of every UI step
  (S7, S8, S13, S13b, S13c, …), re-read the relevant chapters before updating this file.
- **Re-routed 2026-09-22** (superseded 2026-09-25): the main path briefly became `open → pick 2–9 photos →
  pick a layout → adjust → export` (`docs/archive/2026-09-22-UX-DIRECTION.md`; the rulings are in
  `docs/archive/2026-09-22-STEPS.md`). Two things in this file changed because of it then, and both are
  history now: `selection-mode` applied (the picker was a collection view) and the phone-style deviation was
  narrower. The bullet below is what replaced them.
- **Re-routed again 2026-09-25, and landed 2026-09-26 (S22)**: the picker stage is gone
  (`docs/2026-09-25-STEPS.md`, ruling 31) and the window opens on the editor, so this file's picker rows are
  rewritten: §1's `selection-mode` is **not applicable again** (the app has no multi-select collection view),
  `patterns/nav` + `guidelines/navigation` leave the table with the `AdwNavigationView` that carried them, and
  §3's check-box, phone-chrome and preview-pane deviations go with the stage they described. The walk the
  retired plan was waiting on was replaced by the human's own pass of 2026-09-25, whose findings are that
  plan's steps; the new path's script is in §2.
- **S13 (2026-09-22) landed the picker stage and read the chapters it needed** —
  `patterns/containers/selection-mode`, `patterns/nav`, `guidelines/navigation` and
  `guidelines/pointer-touch` — and the utility pane's row became "not applicable" in the same step
  (ruling 18). Those chapters' criteria for the stage are history now and are not restated: what the
  stage *was* is in `docs/archive/2026-09-22-STEPS.md`, and what its two surfaces left behind is
  recorded by the rows below.
- As soon as a chapter is read, write that chapter's criteria into the table above: whatever can be computed goes into the tests (`crates/pixlay/tests/hig.rs`),
  whatever can only be looked at goes into "section 2". **Do not let it pile up** — HIG changes, and letting it pile up is the same as re-reading it next time.

## 1. Read chapters -> criteria

Chapters read page by page for S7: `reference/keyboard`, `guidelines/adaptive`, `guidelines/ui-styling`,
`guidelines/accessibility`, `guidelines/writing-style`, `patterns/containers/utility-panes`, `patterns/feedback`
(index). Read for S13, when the picker stage (the step whose widgets were new) landed:
`patterns/containers/selection-mode`, `patterns/nav`, `guidelines/navigation`, `guidelines/pointer-touch`; the
last three of those left the table again in S22, which deleted the stage and with it the navigation stack that
was their only subject. S22 re-read `patterns/containers/header-bars`, `reference/keyboard`,
`guidelines/adaptive` and `patterns/containers/selection-mode` for the single-page shell. The rest of
`patterns/containers` and `patterns/feedback` are still only their index pages (section 4).

*One reading note, so the next reader does not repeat the work:* `patterns/nav` and `guidelines/navigation`
returned their page furniture but no prose when fetched on 2026-09-22 (the diagrams and the tables are rendered
client-side), and since S22 there is no navigation in this app for them to describe.

| HIG chapter | Landing point | Criteria |
|---|---|---|
| `index` (platform definition: GTK4 + libadwaita) | `AGENTS.md`, `crates/pixlay/Cargo.toml` | GUI only in the `pixlay` crate; no other crate pulls in GTK — `cargo tree` shows gtk4 under `pixlay` alone, and `pixlay-core`/`-imaging`/`-render`/`-cli` must not name it |
| `guidelines/ui-styling` | `app.rs` (the colour scheme at startup), `window.rs` (libadwaita containers and rows only), `canvas.rs`, test `tests/hig.rs::check_colour_schemes` | no hard-coded colours anywhere in the shell: the canvas overlays are drawn with the widget's own theme colour (`Widget::color()`), everything else is a libadwaita style class or a stock widget. **S21 added the one colour pair that is not a style class**: the layout band's sketch draws *pixels* (paper and ink), so `style.css` gives the two classes `.sketch-paper` / `.sketch-ink` the theme's `@view_fg_color` / `@view_bg_color` and the band reads them back through `Widget::color()` on two invisible probes — the same API, with the colours still declared in CSS (`crates/pixlay/src/layout.rs::sketch_style`) **S24 gave the canvas's selection mark its colour the same way — from the theme, not from a literal**: `canvas::accent()` reads `Adw.StyleManager:accent-color-rgba`, the system accent, and `tests/selection.rs` holds it to the value the stylesheet's own `--accent-bg-color` resolves to — the variable `style.css` borders the band's chosen cell with — in the dark and the light style. **The app is dark by default** — ruled 2026-09-22 on this page's own sentence, "apps can alternatively choose to use the dark style by default … primarily recommended for apps which display rich visual content like images or video" (`guidelines/ui-styling.rst:13`), and both reference apps force it (gthumb `Adw.ColorScheme.FORCE_DARK`; loupe `PreferDark`) — superseding the earlier "never force light or dark". The canvas is still verified under **both** forced schemes, because the document is content and not styling — **and S14b had to correct *how*** (2026-09-23): the check compared the whole canvas widget between the two schemes and required no difference, which a canvas painted white edge to edge satisfies trivially — the very defect the human reported as "the dark theme still has not been applied". The canvas clipped its `draw` to the sheet in S14b, and the check now asserts the three things the product claims: the frame *beside* the sheet is the theme's (measured `#222226` under dark, `#FAFAFB` under light), it is dark under the app's own scheme (luma 34), and the sheet's own pixels are identical at three probes inside it. **S13c implements it** (`app.rs` sets `FORCE_DARK` at `startup`). The app's stylesheet touches exactly two things since S22: the layout band's cells (`.layout-cell`'s outline and its accent `.picked` border, which is the app's own way of saying which candidate is chosen) and the sketch's two colours (`.sketch-paper` / `.sketch-ink`, S21) — the picker cell's own rules left with the stage |
| `guidelines/accessibility` | test `tests/hig.rs::check_accessible_names` + "section 2" of this file | every interactive control has an accessible name — set explicitly, or derived by GTK from the control's own label; the check walks the widget tree and accepts both, since GTK names a `GtkButton` from its `GtkLabel`. **S15 added the other tree**: a presented dialog is not a child of its window, so `check_compose` walks each dialog's own tree — the one dialog's two groups — the frame's three rows and the export's two, S25b — and the strip's six controls are all named, with the unit in the spin rows' names ("Gap in per cent") because a bare number announces nothing. High contrast / large text / screen reader / OSK are visual steps (section 2). **S14b moved a name**: the count control draws the number alone (ruled 2026-09-23), so "Photos in the collage" is now its accessible name and its two tooltips rather than a caption beside it — the noun is not drawn, and what the number counts is still announced. **S21 made a candidate's name positional** (ruling 40): the cell shows no caption and carries no template name in copy, so its accessible name is `Layout 3 of 5` — what a screen reader needs, and the one thing a user cannot read off a sketch |
| `guidelines/keyboard` | `app.rs` (`ACCELERATORS`), `canvas.rs` (arrows choose a cell, `Shift`/`Ctrl`+arrow pans, `+`/`-`, `0`, `Enter`, `Delete`, `Ctrl+Shift+Arrow`, `Esc` for a marked swap), `window.rs` (the actions the header and the menu carry, the clipboard's three among them), test `tests/hig.rs::check_shortcuts` | every action has a keyboard path: the actions the table binds are checked against `GtkApplication::accels_for_action`, and every other action is on a focusable control. The canvas is focusable and — since S15h, PIX-017's ruling of 2026-09-24 — **the arrow keys choose a cell**: the selection follows the focus, the canvas outlines it, and its accessible name says `Collage canvas, cell <n> of <cells>`, which is what makes the main path walkable without a pointer (`tests/keyboard.rs` drives the binding through the canvas's own `key-pressed` and reads the name back). The framing the arrows used to be kept panning under a modifier (`Shift`+arrow, `Ctrl`+arrow for a coarse step), and the canvas still zooms, resets (`0`), chooses a photo (`Enter`), clears a slot (`Delete`) and — since S14b — **swaps the selected cell with its neighbour** (`Ctrl+Shift+Left/Right/Up/Down`; which cell an arrow names is geometric, `Template::neighbour`, so the library's grids and columns behave the way the eye expects, and the sheet's edge answers *nothing* rather than clamping). **S23 gave the swap a second keyboard path and a state**: the strip's swap control is a toggle that marks the selected cell, the arrows move the selection to the other cell, `Return` exchanges them and `Esc` takes the mark off — the sequence is announced in the canvas's own name, and the dashed outline on the marked cell is what makes it visible before anything is pressed. The picker's own keys — `Ctrl+A`, `Esc`, the grid's `Enter`/`Space`, the picked list's `Ctrl+Up`/`Ctrl+Down` — left with the stage in S22, and the row's claim is narrower for it: the one page's actions are the accelerated ones plus the controls a Tab reaches, and **every pointer path over the canvas has a key** (a cell's `+` and the strip's five controls are focusable, and the framing they set is also on the canvas's own keys) |
| `reference/keyboard` | same test | the required set for this product (`Ctrl+Q`, `Ctrl+W`, `Ctrl+O`, `Ctrl+S`, `Shift+Ctrl+S`, `Ctrl+Z`, `Shift+Ctrl+Z`, `Ctrl+?`, `Ctrl+N`, and — since S25 gave the app a preferences surface — `Ctrl+,`) is present — `F9` left with the pane (ruling 18, S13) — and nothing binds the system's own combinations (`Alt+*`, `Super+*`, `Ctrl+Alt+*`) — both asserted against the one table the dialog and the bindings share. **S22 re-cut the rest of the table for the one-page shell**: `win.add-photos` takes `Ctrl+I` (the entry point the picker's `Next` used to be), and `Ctrl+Shift+O` (the folder) and `Z` (the preview zoom) left with the stage that owned them. `Ctrl+0` stays this app's framing reset, which is why that page's "Normal Size" binding is not available to a view control. **S23b added the three standard editing keys**: `Ctrl+X` / `Ctrl+C` / `Ctrl+V` (`win.cut` / `win.copy` / `win.paste`) act on the selected cell's photo, they are in the menu's Edit group as well, and their sensitivity is the state the window can really act in — copy and cut need a cell whose photo is a file that is there, paste needs a cell and a clipboard holding a file list or an image |
| `guidelines/adaptive` | `window.rs` (the shell), test `tests/hig.rs::check_editor_minimum` | no utility pane since ruling 18 (S13) and one page since S22, so there is one thing to check at the minimum window size: the sheet is still drawn in full inside the canvas, and the band above the window's own bottom edge exists — both asserted. **S14 added the layout band to the editor's page**, so the editor's check now includes it: the band has a height and its strip is allocated at the minimum size, and the canvas's height is the same before and after the candidates land (the placeholder is a candidate cell, so the band cannot resize the canvas under a background build — `tests/layout.rs` asserts the two heights are equal) The picker's own arrangement (the preview, the strip of thumbnails below it, the picked list at its right edge, the status bar) left with the stage in S22, and the numbers that measured it went with it |
| `guidelines/writing-style` | every string in `crates/pixlay/src`, `po/pixlay.pot` | header capitalization on buttons, menu items and tooltips; sentence capitalization on row, slider and combo labels; an ellipsis exactly where the action asks for more input (`Open…`, `Save as…`, `Choose photo…`); no `i.e.`/`e.g.`; no pronouns; no trailing periods outside explanatory body text. The wording itself is a visual step (section 2), and whether a string missed its `gettext` call is only checkable by eye (the extractor cannot see what nobody wrapped) |
| `patterns/containers/utility-panes` | — | **not applicable since ruling 18** (S13 removes the pane): a linear three-minute flow owns its controls per stage — picked list, gallery, floating buttons, dialogs — and a permanent panel would be a second surface for every one of those decisions |
| `patterns/feedback` (index) | `window.rs`, `export.rs` | reversible feedback goes through `AdwToast` ("Saved …", "Exported …", a failed decode, and — since S15c — a refusal to write an export over one of the document's own photos, which is a non-critical error and therefore a toast rather than an error dialog); the missing-photo case is an `AdwBanner` with a button that selects the slot; export progress is a `GtkProgressBar` in the bottom bar, never a modal dialog; the dialogs are the unsaved-work confirmation — **one question for every boundary that can end the document, since S15d**: closing the window, `New` and `Open` — plus **`Frame…`, landed in S15** (ruling 18) as an `AdwDialog` of rows rather than permanent controls, which writes live and is closed rather than confirmed. **The export's own dialog left in S25** (ruling 36): the export is the platform's own save dialog now, so the refusal it can raise is a toast (the source-image rule) and its replace confirmation is the platform's — this app draws no dialog for either |
| `patterns/containers/selection-mode` | — | **not applicable again since S22** (ruling 31): the picker was this app's only multi-select collection view and it is gone, so there is no selection mode, no batch action and no check box for the chapter to govern. The canvas keeps the page's own advice as it always has — "when editing is the primary interaction there should be no separate edit mode" — and the layout band is a single-choice set (`radio-buttons`, below). What the stage built under this chapter (the grid over the folder, `Ctrl+A`, the cap's own report, the highlight instead of the check box) is in `docs/archive/2026-09-22-STEPS.md`, and its one surviving product rule is the order: photos enter in the order they are given, and that order is cell order (`Selection`, `edit --photo`, `pixlay a.jpg b.jpg …`) |
| `patterns/controls/radio-buttons` | `layout.rs`, tests `tests/layout.rs` and `tests/hig.rs::check_gallery` | read for S14: the layout gallery is "a selection made from a set of options", and the page's three guidelines are the ones the band answers. **One of the set is selected at all times** — a click on the current candidate cannot leave the document with no layout, and `Gallery::highlight` writes the state from the *document* rather than from the click, so an undo, a project opened or a layout changed from elsewhere cannot leave the highlight lying. **Small sets only** — the library ships at most four layouts for a count (S10's ≥3 per count, and the most any count has), which is why a strip can show them all rather than a drop-down. **Sentence capitalization** applies to the controls (`Add a photo`, `Remove the last photo`); **a candidate carries no caption at all** since S21 (ruling 40 made a template's name machine identity — `edit --template`, `templates` — rather than copy), so the cell is the sketch and nothing else, and what a screen reader announces is the position (`Layout 3 of 5`) |
| `patterns/containers/grid-views` | `layout.rs` | read for S14, and it is the chapter the band is: a collection of image cells from which the user selects. **Each grid item has a unique thumbnail** — a candidate is a *sketch* of that template's geometry (S21, ruling 32), so two candidates never look alike: a layout is its cell outlines, and two layouts with the same geometry are the same layout. The caption left with the same ruling (a template's name is machine identity, `edit --template`), so the cell is the sketch and nothing else. **Order the items usefully** — `Selection::layouts()` is library order (by slot count, then recipe), which is stable across sessions rather than "most recently used". **"In cases where grid images have irregular shapes or inconsistent appearance, it may be necessary to outline each grid cell"** — this is exactly the gallery's case, because each candidate is drawn at its own aspect inside one fixed box, and it is the citation for `.layout-cell`'s border. **Tested at the range of window sizes** — `check_editor_minimum` and `tests/layout.rs` |
| `patterns/feedback/dialogs` | `dialogs.rs`, tests `tests/compose.rs`, `tests/hig.rs::check_compose`, `tests/export.rs` | **Read for S15** and re-read for S15c, and it is the shape ruling 18 gave the two document-level questions: an **action dialog**, which has a header bar, a heading naming the action, and the affirmative labelled with the verb the action is — not *OK* — with the cancel button **before** it (left, in this locale) and Esc bound to cancel. **S25 left the app with one of them** (ruling 36): the export's own dialog is gone, and with it the alert this chapter is really about — replacing a file that is already there is a *destructive* action with no undo, and the confirmation is now the **platform's own save dialog's** (it asks before it returns a path that names a file that is there, and the app adds no second question of its own — which is what `tests/export.rs` asserts instead of pressing *Replace*). **S25b merged what was left into the app's one dialog** (the human's ruling of 2026-09-26): the frame's group keeps this chapter's live half — its rows apply as they move, the canvas redraws behind it, the settled value is one undo step, and `Ctrl+Z` is the way back — and a value the document refuses is reported by a toast *inside* the dialog (`AdwPreferencesDialog::add_toast`), which is this chapter's own "for simple non-critical errors, toasts can be a good alternative" without the old banner's problem (a window toast would be behind the modal). The machine check walks each presented dialog's own tree: every control named, each dialog with its heading and with either its affirmative or its way out. **S15d made that unsaved-work alert the question for every boundary**: closing the window, `New` and `Open` all present it (`EditorWindow::ask_to_save`), the pending edit is committed *before* it is asked, and `Save` continues the boundary only once the file was written — so the chapter's "never pop up a dialog unexpectedly" holds for three deliberate actions rather than one, and the alert itself is unchanged: the same Cancel-first order, the same affirmative, Esc to cancel |
| `patterns/controls/buttons` | `canvas.rs` (the empty cell's `+` and the selected cell's strip), `dialogs.rs` (the affirmative), tests `tests/compose.rs`, `tests/hig.rs::check_compose` | **Read for S15**, and its own example is this step's: "circular buttons … can be useful in situations where a number of smaller buttons are positioned in close proximity" — which is the strip of six over the selected cell, `osd circular` like the empty cell's `+` (libadwaita style classes only; a literal colour would fail in high contrast over a photo). Outside a header bar a button holds an icon or a label and not both, which is why the strip and the `+` are icon-only with a tooltip and an accessible name, and the dialogs' buttons are label-only. **The `suggested-action` style is used in the dialogs' header bars only** — this page's `header-bars` guidance forbids it for primary *window* header bars, and the reference apps keep it out of theirs |
| `patterns/feedback/tooltips` | `window.rs`, `canvas.rs`, `layout.rs`, `dialogs.rs` | Read for S15 (the chapter was only its index before), and re-read by S15j for the preview pane's own tooltip: "controls in the header bars of primary windows should all have tooltips", and if one control in a container has one the others should too. **S22 leaves one header bar to hold to that** — `tests/hig.rs::check_header_chrome` — plus the strip's six controls, the empty cell's `+` and the band's two count buttons; the pane's own tooltip went with the pane. **One deviation, recorded rather than fixed**: HIG asks tooltips to use header capitalization (*Select a File*), and every tooltip in this window has been a sentence since S7 (*Export the collage*, *Zoom in*). The window is internally consistent and the pass that re-capitalizes them belongs with the translation work (S16), which re-reads every string anyway |
| `patterns/containers/header-bars` | `window.rs`, `app.rs` (the menu), test `tests/hig.rs::check_header_chrome` | read on 2026-09-22, re-read by S22 for the window's one header bar: primary actions go in the **start** slot (undo, redo, a spacer, the frame's settings), the heading in the **centre** (`AdwWindowTitle`, the document's name with its dirty marker) and the primary menu at the **end**, with the export beside it; group related buttons with spacers rather than linked buttons; every primary control carries a tooltip; a header bar holds few controls and no text-only or `suggested-action` buttons (`patterns/containers/header-bars.rst`, "Button Style" and "Button Grouping") — the export button is the one `suggested-action`, which is the product's primary action. **S22 removed the Save button** (ruling 37: it sat beside Export and read as the same action) and the check asserts its absence, the three slots by geometry and the ruled menu (`[New collage, Open…, Save, Save as…, Export…] · [Add photos…, Reset the framing] · [Preferences, Keyboard shortcuts, About Pixlay]`, the last group being HIG `patterns/controls/menus`' standard items, completed by S25). Both references agree: every gthumb header ends in `open-menu-symbolic` (`data/ui/browser.ui:243-315`, `data/ui/viewer.ui:96-110`), loupe's ends in its primary menu (`src/widgets/image_window.ui:85-134`) |
| `patterns/controls/menus` | `window.rs` (`main_menu`), test `tests/hig.rs::check_header_chrome` | **Read for S25.** The primary menu's own guidelines: the menu button's tooltip and accessible name refer to it as 'Main Menu' (this app's says `Main menu`, the sentence-case deviation in §3); items are "grouped by relatedness"; and **"Standard Primary Menu Items … should be placed in a group at the end of the menu"** — *Preferences*, *Keyboard Shortcuts*, *Help*, *About App*. The app's last group is exactly that list minus *Help* (there is no user documentation to open; that is the "not doing" list), and S25 completed it by adding *Preferences*, which is why the group is asserted item by item rather than as a count |
| `patterns/containers/windows` | `app.rs` (`app.settings`), `crates/pixlay/src/settings.rs`, tests `tests/settings.rs` and `tests/hig.rs::check_compose` | **Read for S25.** "About Windows and Preferences Windows are both types of secondary window"; a secondary window "should always belong on a primary window, so that closing the primary also closes the secondary", and it is "typically modal to their parent primary window". The app's one dialog is therefore presented over the active window (`app.settings` → `EditorWindow::show_settings` → `AdwDialog::present(Some(&window))`, and `win.frame` from the header bar) and never opens one of its own; the modality is libadwaita's `AdwPreferencesDialog` default. **S25b put the document's frame rows in the same dialog** (the human's ruling of 2026-09-26), which is that page's other half: a secondary window may carry "information and options for a single content item, such as a document Properties Window" — one surface holds the collage's frame above the app's export settings. That page's own API reference names `AdwPreferencesWindow`, which libadwaita deprecated in 1.6 — a deprecated constructor is what `cargo clippy -- -D warnings` refuses, so the surface is the 1.5+ `AdwPreferencesDialog` (the same reasoning the export's research recorded for `GtkFileChooserWidget`) |
| `guidelines/pointer-touch` | `canvas.rs`, `layout.rs`, `tests/layout.rs`, `tests/hig.rs::check_gallery` | re-read by S22 with the picker gone, and the row is now only about the one page's controls: **click targets are well past the minimum** — the layout band's candidates are 128x96 cells (HIG's floor is 24x24, and `check_gallery` asserts it), **the empty cell's own `+` is 32x32** and sits at the cell's centre (S14b, ruled 2026-09-23: `+` in the count control grows the layout, and the *cell* is the control that asks for a photo — measured: 34x34 with its border, inside the cell's own rectangle), and **the selected cell's six controls are 32x32** in one strip over the cell: a row at its bottom edge, or a column at its right edge on a cell too narrow for the row (S15; measured 2026-09-23: 186x34 inside a 3/8 x 3/8 cell, 34x186 inside a 1/16 pane). Every pointer action has a keyboard path — the `+`, the strip's five controls and the band's cells are focusable, the canvas's own keys nudge and reset the framing, and the layout's count control is `+`/`−` buttons — which is this page's "all actions which can be accomplished with a pointing device should also be possible with a keyboard". The picker's own pointer rules (the grid's tiles and their click-to-toggle, the preview pane's double-click zoom and its 1:1 pan, the picked list's row drag) left with the stage in S22, and the two §3 deviations they needed went with them |

## 2. Visual steps

Follow the "Testing for Accessibility" of HIG `guidelines/accessibility` item by item. **The second step of every item is an additional criterion of this product**:
the canvas is **content** and the interface is **styling**, and the two must not affect each other (see `AGENTS.md` "Composite onto opaque white").

1. **High contrast mode** (GTK Inspector or the system accessibility settings): every UI element renders normally; canvas pixels are unchanged. **Where the selection mark's colour comes from, so this item has an answer** (S24): `Adw.StyleManager:accent-color-rgba` — the *system* accent, which the app neither names nor can override (libadwaita's own property doc: "This cannot be overridden by applications"), so the mark follows the platform's accent and contrast settings the way every themed control does. Measured 2026-09-26 with the toolkits' HighContrast theme set on the settings object: libadwaita's accent and the stylesheet's `--accent-bg-color` both stay `#3584e4`, and `tests/selection.rs` reads the same pair in the dark and the light style. The document's own pixels are unaffected the other way round: the mark is drawn *over* the sheet (the export and the CLI have no mark at all), and the snapshot probe finds no accent pixel on a canvas with nothing selected.
2. **Large text** (system accessibility settings): every label stays readable and is not truncated; canvas pixels are unchanged (the canvas is document content and does not scale with the interface's font size).
3. **Keyboard-only**: walk "open → add photos → pick a layout → adjust framing → export from the dialog" with the keyboard alone; the focus order is logical;
   `F10` opens the menu, `Esc` closes overlays, `Tab` covers every control. (Since S22 the window opens on the collage, so this walk starts there: `Ctrl+I`, a cell's `+`, or the command line.) **Since S23 two cells are swapped without a pointer**: the strip's swap toggle marks the selected cell, the arrows choose the other one, `Return` exchanges them and `Esc` takes the mark back — the sequence the keyboard criterion asks a user to be able to finish. **Since S23b the clipboard is on that path too**: `Ctrl+C` / `Ctrl+X` on the selected cell's photo, `Ctrl+V` into another one — and the same three items are in the menu (`F10`), so a user who does not know the keys can still find them.
4. **Screen reader**: every control is read out, the accessible name is accurate and short; it stays operable with the monitor off. The canvas is one control rather than nine (S15h, PIX-017), so its **name carries the state a user needs before pressing anything** — `Collage canvas, cell 3 of 8` — and it changes as the keyboard's focus moves. **S23 added the state that only exists while a sequence is half-done**: with a swap marked, the same name says which cell it is coming from. A layout candidate's name is its position (S21, ruling 40), because a template's name is machine identity and not copy.
5. **Touch / on-screen keyboard (OSK)**: the project name and the export path can be typed entirely with the OSK.
6. **S22 additions** (the step that landed the editor-as-the-app shell; S15's own list, which walked the re-routed path, is below in its own subsection):
   - the "three-minute main path": the scripted step list below, timed by hand;
   - the copy: is any label wrapped, truncated or unclear, and does a squeezed row read as intended? (The strings themselves are checked mechanically; the look is not.);
   - **the way in**: does the window open on something that reads as "start here" — a one-cell collage with an empty cell whose `+` is visible — is `Add photos…` (`Ctrl+I`) where the hand expects it, does a drop from the file manager land on the cell it is aimed at, and does `pixlay a.jpg b.jpg …` open the photos in the order they were given? Is a cell replaceable (the strip's `Replace`, a double click on an occupied cell), and does the header bar read as one action (Export) without a second Save beside it?
   - the layout gallery: **is a 128x96 sketch legible** — can a person tell two layouts apart, and read a
    cell's shape, at that size against the band's own height — and does the highlight make the current one
    obvious? (S21, which rewrote the question: the candidate is the template's geometry as lines, not the
    user's photos in it, and the caption is gone — the template's name is machine identity, ruling 40 —
    so what a person has to read is the drawing itself. S14's own list, now superseded, was "does each
    thumbnail read as 'my photos in *that* layout' at a glance, is below-the-canvas the right place for the
    strip, and is the thumbnail's own size the right one");
   - the floating buttons: do they land where the hand expects, and does the frame look right at both ends of its radius range;
   - the dialogs: does the frame's group read as the three things the document holds — with its live rows
     making the canvas redraw behind it — and (S25's own Human line, item 10 below) does the app's one dialog
     read at a glance, with the frame above the export's settings, and does the export's flow land where the
     platform's dialog said it would;
   - the narrow panes: the strip turns into a **column** on a cell too narrow for the row (the library's
     narrow panes are narrower than the row: the row requests 212 px for its six 32-px controls and measures
     224 once the theme's button borders are counted, while this run's `strip-9-9x1` panes measure 126).
     Whether a vertical stack reads as well as the row is a look — the geometry is settled and asserted
     (measured 2026-09-26, six controls: the row is 224x34 inside a 3/8 x 3/8 cell and the column 34x224
     inside a 126x570 pane).

7. **S23 additions: the swap's own interactions** (ruling 33; S24's list joins them at the same gate):
   - **the `Shift`+drag**: does a `Shift`+drag from one cell onto another read as *carrying* the cell? The
     cell under the pointer is filled while the pointer is over it — that highlight is the promise that this
     is where the release lands — and a release outside every cell, or on the source itself, changes nothing
     (does that read as "nothing happened" rather than as a failed attempt?);
   - **the `Shift`+click**: the one-press form, on the cell the hand points at;
   - **the keyboard sequence**: does the strip's swap toggle's checked state read as "this cell is being
     moved", together with the dashed outline the canvas draws around the same cell — and is the dashed mark
     distinguishable from the selection's solid outline at a glance, on a photo as well as on white?
   - **the icon**: does `mail-send-receive-symbolic` read as "exchange these two" at 32 px?
   - **the refit**: a photo dropped into a differently shaped cell is re-fitted at draw — does it look
     intended (it keeps its stored framing, and what changes is the crop the cell shows)?

8. **S24 additions: the selected cell's own look** (finding 6 of 2026-09-25 — "not distinguishable enough;
   give it a colour"; the step's own Human line is this look):
   - **the mark over a photo**: the selected cell is outlined on its own path in the theme's accent — a 2 px
     line, `#3584e4` under this session's accent — and the question is whether that reads at a glance on
     the eight-photo verification project's cells, where the photos behind it are the fixture bands'
     saturated colours. The test's own picture: `/var/tmp/pixlay-s7/selection.png`;
   - **the dashed mark, which S24 had to separate from the solid one**: with a swap marked, the same
     outline is *dashed* and the solid outline is no longer drawn under it — before S24 the two were drawn
     on the same path and the dashes were invisible. The picture: `/var/tmp/pixlay-s7/selection-swap.png`;
   - **the pair side by side**: does a person tell "this cell is selected" from "this cell is being moved"
     at a glance, in the dark style and in the light one?

9. **S23b additions: what arrives from outside** (ruling 41; the step's own Human line, and it joins the
   gate after S22–S24):
   - **the drop that lands where it is aimed**: drop one file on a cell that holds a photo — does it read as
     "this cell took it" rather than as a surprise somewhere else? Drop three on a full collage and the
     toast says two did not fit: is that report enough to understand why nothing else moved?
   - **the drop on an empty cell, and the wrap**: a drop that fills several cells takes them in reading
     order, and the last one wraps around the sheet — does the result still land where the hand expects?
   - **the clipboard inside the window**: `Ctrl+C` in one cell and `Ctrl+V` in another copies; `Ctrl+X`
     then `Ctrl+V` moves and leaves the source cell empty — does the difference between the two read at all
     (nothing shows a cut on the canvas)?
   - **the clipboard out of and into the window**: `Ctrl+C` on a cell and paste into the file manager, a
     text editor or an image application; a file copied in the file manager pasted into a cell; an image
     copied from another application pasted into a cell — does the last one land as a photo, and does the
     file it wrote stay put across a save and a reopen?
   - **the sensitivity**: with nothing selected, or with a clipboard holding text, the menu's Cut / Copy /
     Paste read as unavailable rather than as commands that do nothing.

10. **S25 additions: the settings surface and the export's one dialog** (rulings 36 and 39; the step's own
   Human line, and it joins the gate after S22–S24):
   - **the settings surface read at a glance**: `Ctrl+,` (or the menu's *Preferences*) opens a dialog titled
     *Preferences* with one group of two rows — the format (JPEG/PNG) and the long edge in pixels. Is the
     unit obvious without a screen reader, does either row wrap or truncate at the default window size, and
     does the row's own value read as the size an export will be (4000 px by default)?
   - **the export's flow**: `Ctrl+E` (or the header bar's button) opens the **platform's own save dialog**,
     not ours — is it seeded with the folder the last export used, and with a name that carries the settings'
     format's extension? Does the file land where the dialog said, is the progress bar in the bottom bar the
     only feedback while it runs, and does the toast name the file and its size?
   - **the platform's replace confirmation**: choosing a name that is already there — does *its* question
     come up (the app asks nothing of its own), and does cancelling leave the file alone?
   - **the name's extension against the settings' row** (S25c): with PNG selected, type a `.jpg` name —
     the file that lands is a JPEG at the settings' long edge, and the settings' row still says PNG. Is that
     understandable, or does the row now look like it lies? (The other half: a name with an extension this
     build does not write, e.g. `.tiff`, is refused with a toast and nothing is written.)
   - **the remembered folder across runs**: export once, quit, start again — does the next save dialog open
     where the last export landed?

11. **S26–S28 additions: the walk of 2026-09-26's four findings** (rulings 42 and 43; the three steps' own
   Human lines, and they join the gate after S26–S28):
   - **the way in, as a button** (finding 1): with the window as it opens, does `Add photos…` in the header
     bar's top-left read as the thing to press first — at the default size, and at the minimum one (560x420),
     where the label may have shrunk to its icon: does the header bar still read as three aligned groups?
   - **the pointer drag** (finding 2): with a photo in a cell, drag inside it — does the photo follow the
     hand, does the pan stop at the cell's edge rather than uncovering it, and is the release one `Ctrl+Z`?
     The same with the keyboard's own pan, so the two are one feel;
   - **the swap's pointer path** (finding 4): press the strip's *Swap with another cell* — does the cell mark
     itself (the dashed accent outline, and the canvas's own name saying `swapping with cell K`), and does a
     click on the target exchange the two without touching the keyboard? `Shift`+drag and `Esc` still behave
     as §2 item 7 asks;
   - **a layout change keeps its photos** (finding 3): three photos, `−` to two cells, `+` back to three —
     does the third photo come back in its own cell with its framing, does the one report read as *kept*
     rather than *lost*, and does the `+`'s own hint make a kept photo findable? Then: `Delete` on a cell
     still deletes, and a kept photo does not come back after a real delete.

### The main path's scripted step list (the three-minute walk)

Written for `open → add photos → pick a layout → adjust → export` — the path ruling 31 re-routed to on
2026-09-25 and S22 landed — and **not yet walked** at the time of writing: the gate after S22–S24 is where a
human walks it (`docs/2026-09-25-STEPS.md`, "Where humans must step in"). Run the app
(`cargo run --release -p pixlay`, the installed `pixlay` after S16, or `pixlay a.jpg b.jpg` for the second
entry) and time the whole list:

1. *Start*: the window opens on a one-cell collage with an empty cell; `Add photos…` (`Ctrl+I`) or a drop from
   the file manager brings the photos in — the order given is the order of the cells — and `Ctrl+Z` takes the
   whole arrival back as one step. The command-line entry (`pixlay a.jpg b.jpg …`) opens the same window with
   the same photos in the same order.
2. *Pick a layout*: the band under the canvas lists every layout with the cell count the document has, each as
   a sketch of its geometry; pick one, and move the count with `+` / `−` if it is wrong (the new cell is empty
   on purpose).
3. *Adjust framing*: select a cell, drag inside it to move the photo, scroll to zoom, rotate by any angle
   (there is no cap), replace or clear a cell from the strip over it, and set the frame's gap / radius /
   colour from `Frame…` in the header.
4. *Export*: press `Export…` (or `Ctrl+E`), and the platform's own save dialog asks for the folder and the
   name — the format and the size are the settings' (`Ctrl+,`), and a file that is already there is confirmed
   by that dialog itself (ruling 36) — and the progress bar in the bottom bar runs to the toast with the
   file's name and size. `Ctrl+S` (or the menu's *Save*) writes the `.pixlay` project.

The picker era's script — `open → pick 2–9 photos → Next → pick a layout → adjust → export`, which the human
actually walked on 2026-09-25 — is the record of that pass, and its findings are
`docs/2026-09-25-STEPS.md`'s six steps. The plan before that one, `pick a template → place photos → adjust
framing → export`, is in `docs/archive/2026-09-20-STEPS.md`; its walk was **voided** on 2026-09-22 (ruling 6),
because that path is not the product's any more.

What is already machine-checked, and therefore not what this walk is for: the path works at all
(`crates/pixlay/tests/mainpath.rs` walks the list in the same calls the widgets make — including the
command-line entry, which the same test drives through `app::open_files`), the copy is English under any
locale, the shortcuts are bound, the canvas matches the CLI's render, and — since S15h — **a cell can be
chosen without a pointer** (`tests/keyboard.rs` presses the canvas's own keys), the export's rows resolve the
name the format row owns, and a frame value the document refuses is reported instead of silently kept.
**What only a person can judge** is whether the walk is short, whether the entry reads as one, whether the
framing and rotation gestures feel right, and whether the copy reads well.

## 3. Deliberate deviations (the same list as in `AGENTS.md`, do not fix)

| HIG item | Decision | Reason |
|---|---|---|
| GNOME Shell search provider, notification workflow | Not doing | the same discipline as the "Not doing" list: add no feature that does not serve the main path |
| Phone-style layout — the *chrome*, not the capability (2026-09-22; amended 2026-09-25) | the capability is built with desktop idioms; the chrome is not copied | the picker-first flow came from mobile galleries and **went with the picker** (ruling 31): the editor is the app, and photos enter from the platform. What the mobile references still give the product is the layout band's own form — **a sketch of the geometry rather than a sample image**, the way Xiaomi's layout strip draws it — because a pixlay template carries geometry and no style, where Google's strip shows sample photos for the opposite reason (ruling 32; the research is in `docs/2026-09-25-STEPS.md`) |
| Double-clicking to toggle the preview's zoom, and the preview pane's pan (HIG `guidelines/pointer-touch`) | Removed with the pane (S22) | S15j had recorded both as deviations: a double click toggled fit ↔ 1:1 and the pan had no keyboard path. The pane was the picker's, the stage is gone (ruling 31), and the deviations are moot rather than kept |
| The selection-mode check box (HIG `patterns/containers/selection-mode`) | Not used, and the chapter is not applicable again | the 2026-09-22 ruling showed a picked cell by a highlight instead (`docs/archive/2026-09-22-STEPS.md`, `S13 · Ruling`); S22 deleted the stage, so the app has no multi-select collection view and the chapter governs nothing (the §1 row) |
| Per-app style preference (light / dark / system, pick one of the three) | Not doing | amended 2026-09-22: the app is **dark by default**, which is what this page's `ui-styling` guidance recommends for one that displays rich visual content, and neither reference app (gthumb, loupe) offers the switch. A stored preference would also need a settings file: ruling 39 of 2026-09-25 allows one for the export's settings and the last export folder and **not** for this (the file carries exactly those three fields). Dark is not a substitute for high contrast, which stays a separate system mode and is checked in §2 |
| Large-text mode acting on the canvas | Not applied | preview and export must be from the same source, pixel by pixel; the canvas is document content, not interface |
| access keys (`Alt+` mnemonics) | Not doing | this application has no menu bar |

## 4. Chapters not yet read page by page (read them and backfill section 1 as the plan's UI steps start)

Read for the picker stage (the plan's S13) and **closed again by S22**, which deleted the stage:

- `patterns/containers/selection-mode` — read for the picker, back to "not applicable" since S22 (§1)
- `patterns/nav`, `guidelines/navigation` — no navigation stack to describe; the chapters were only page
  furniture when fetched (the note above), and S22 removed the last thing that could have needed them
- `guidelines/pointer-touch` — read for the picker's gestures and re-read by S14 (the band's target size)
  and S22 (the one page's controls); its criteria are the §1 row

Still unread, and read when their steps need them:

- `principles`, `resources`
- `guidelines`: `app-naming` (S16), `app-icons` (S16), `ui-icons`, `typography`
- the rest of `patterns/controls/*` (`radio-buttons` was read for S14 and `buttons` for S15)
- the per-page details of `patterns/feedback/*` (`dialogs` and `tooltips` were read for S15; the index
  was read for S7)
- the UI colors under `reference/`

**A second correction, found by S25's read** (2026-09-26): the live plan expected S25 to re-read
`patterns/containers/preferences` for the settings surface, and **that chapter does not exist in the current
HIG** — the containers family's toctree is windows / header bars / popovers / utility panes / boxed lists /
grid views / list & column views / selection & edit modes. What governs a preferences surface is
`patterns/controls/menus` (its standard item and its place in the menu), `patterns/containers/windows` (a
preferences window is a secondary window), `reference/keyboard` (`Ctrl+,`) and libadwaita's own
`AdwPreferencesDialog`; those three pages are §1 rows.

**One correction to the retired plan's own list** (`docs/archive/2026-09-20-STEPS.md`, and the "still
unread" list above before 2026-09-23): it named `patterns/containers/dialogs` as the chapter for the
`Export…`/`Frame…` dialogs of ruling 18. There is no such chapter — the containers family is
windows / header bars / popovers / utility panes / boxed lists / grid views / list & column views /
selection & edit modes (`patterns/containers.rst`'s own toctree), and the dialogs chapter is
`patterns/feedback/dialogs`, which S15 read.
