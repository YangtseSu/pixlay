# GNOME HIG conformance checklist

The "GNOME HIG" section of `AGENTS.md` is a **hard constraint**, and this file is its execution checklist: it sorts
every HIG section into the three tiers "machine-checkable criteria / visual criteria / not applicable", and records
clearly which chapters **have already been read page by page** and which have not.

- Spec: <https://developer.gnome.org/hig/> (no version number, **not frozen**; cite URLs and section names)
- The most recent page-by-page read of this file: **2026-09-20**. At the start of every UI step (S7, S8), re-read the relevant chapters before updating this file.
- As soon as a chapter is read, write that chapter's criteria into the table above: whatever can be computed goes into tests (`docs/STEPS.md`, the "S7 · GNOME HIG" patch layer),
  whatever can only be looked at goes into "section 2". **Do not let it pile up** — HIG changes, and letting it pile up is the same as re-reading it next time.

## 1. Read chapters -> criteria

| HIG chapter | Landing point | Criteria |
|---|---|---|
| `index` (platform definition: GTK4 + libadwaita) | `AGENTS.md` | GUI only in the `pixlay` crate; no other crate pulls in GTK |
| `guidelines/ui-styling` | `AGENTS.md` + S7 tests | style classes / CSS variables only, hard-coded colors and spacing forbidden; follow the system dark mode; starts under both styles; **canvas pixels do not change with the style** |
| `guidelines/accessibility` | S7 tests + "section 2" of this file | every interactive control has an accessible name (machine-checkable); high contrast / large text / keyboard-only / screen reader / OSK (visual inspection) |
| `guidelines/keyboard` | S7 tests | every action has a keyboard path; the Tab order covers every control |
| `reference/keyboard` | S7 tests | the accelerator table ⊇ the required set, ∩ the system-reserved set = ∅; do not bind `Alt+*` / `Super+*` |
| `patterns/containers` (windows / header-bars / popovers / utility-panes / boxed-lists / grid-views / list-column-views) | S7 design | use libadwaita containers; once this step has chosen them, backfill the containers actually used into this table |
| `patterns/feedback` (toasts / banners / dialogs / placeholders / spinners / progress-bars / tooltips / notifications) | S7 design | reversible short feedback goes through `AdwToast`; destructive operations go through a dialog; empty states go through `AdwStatusPage`; progress goes through a progress bar rather than a modal |
| `patterns/containers/selection-mode` | **Not applicable** | there are no collection views and no multi-select batch operations; that page itself states that "when editing is the primary interaction there should be no separate edit mode", which points the same way as "do not add mode switching" |

## 2. Visual steps

Follow the "Testing for Accessibility" of HIG `guidelines/accessibility` item by item. **The second step of every item is an additional criterion of this product**:
the canvas is **content** and the interface is **styling**, and the two must not affect each other (see `AGENTS.md` "Composite onto opaque white").

1. **High contrast mode** (GTK Inspector or the system accessibility settings): every UI element renders normally; canvas pixels are unchanged.
2. **Large text** (system accessibility settings): every label stays readable and is not truncated; canvas pixels are unchanged (text layers are document content and do not scale with the font size).
3. **Keyboard-only**: walk "pick a template → place photos → adjust framing → export" with the keyboard alone; the focus order is logical;
   `F10` opens the menu, `Esc` closes overlays, `Tab` covers every control.
4. **Screen reader**: every control is read out, the accessible name is accurate and short; it stays operable with the monitor off.
5. **Touch / on-screen keyboard (OSK)**: text-layer content and the export path can be typed entirely with the OSK.
6. **S7 additions**: the "three-minute main path" timing (a scripted step list + timing, see `docs/STEPS.md`) and copy wording
   (`guidelines/writing-style`'s sentence case, no jargon, no honorifics; omitted wrapping is not a machine-checkable criterion).

## 3. Deliberate deviations (the same one as in `AGENTS.md`, do not fix)

| HIG item | Decision | Reason |
|---|---|---|
| GNOME Shell search provider, notification workflow | Not doing | the same discipline as the "Not doing" list: add no feature that does not serve the main path |
| Phone-style layout | Not doing | the target platform is the Arch desktop; the canvas has physical-size semantics |
| Per-app style preference (light / dark / system, pick one of the three) | Not doing | the shortest main path; "follow the system" already covers how a user expresses "I want dark" |
| Large-text mode acting on canvas text layers | Not applied | preview and export must be from the same source, pixel by pixel; text layers are document content |
| access keys (`Alt+` mnemonics) | Not doing | this application has no menu bar |

## 4. Chapters not yet read page by page (read them and backfill section 1 when S7 / S8 start)

- `principles`, `resources`
- `guidelines`: `app-naming` (S8), `app-icons` (S8), `ui-icons` (S7), `writing-style` (S7),
  `typography` (S7), `navigation` (S7), `pointer-touch` (S7), `adaptive` (S7)
- `patterns/nav`, `patterns/controls/*`
- the **per-page details** of `patterns/containers/*` and `patterns/feedback/*` (this table currently uses only their index pages)
- the UI colors under `reference/`
