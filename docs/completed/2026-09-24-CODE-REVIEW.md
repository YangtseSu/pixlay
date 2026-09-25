# Pixlay Whole-Project Code Review — 2026-09-24

> **Closed 2026-09-25.** Every finding below was answered by the S15 series (S15b … S15i, plus S15j for
> PIX-028's ruling) and the answers are recorded in `docs/archive/2026-09-22-STEPS.md`, "The 2026-09-24
> review's remediation". The file moved here with that plan's retirement. Kept as the record of what was
> looked at and what was found — a static review, so each finding is a hypothesis that the remediation
> either reproduced or refuted.

## Review scope and methodology

This is a static, whole-workspace audit of the current Pixlay tree. The review inspected the workspace manifests and lockfile, all production modules in the five crates, the committed fixtures and fixture generator, every integration-test area, GUI test harness, gettext metadata, and the executable-relevant authoritative documents (`AGENTS.md`, `docs/CONTRACT.md`, and `docs/archive/2026-09-22-STEPS.md`). No formatter, linter, build, test suite, or application run was performed; conclusions are based on direct source and test inspection. The repository's status at the time said S15 was waiting for the human walk and S16 packaging had not started (both have since moved on — the plan of 2026-09-22 is retired, see the banner), so the absence of packaging assets was treated as planned scope rather than as a shipped-package defect.

### Architecture and invariant map

| Layer | Responsibility and current boundary | Main invariants |
|---|---|---|
| `pixlay-core` | Versioned `CollageDoc`, embedded template geometry, normalized geometry, frame, crop fitting, selection, command history, project loading/writing | `docVersion = 3`; unknown fields rejected; one cell per slot; normalized finite geometry; frame opacity and visible-region coverage; command transitions validate before replacing state; sources are resolved relative to the project file |
| `pixlay-imaging` | glycin decode, EXIF facts, linear-light resampling, preview reduction/cache, probes, PNG/JPEG encoding, ICC generation | Decode caps at 120 MP/20,000 px/20 s; source alpha is premultiplied in linear light; 16-bit intermediates and final sRGB quantization; opaque output; 4:4:4 JPEG; ICC written during encoding |
| `pixlay-render` | The single Cairo compositor and bitmap/band public API | Opaque `frame.color` base; slot outline clip, then frame rounded-inset clip; partial-bitmap placement at 1:1; no GUI dependency; band partitions output rows exactly |
| `pixlay-cli` | Machine surface: render, probe, image, scan, thumb, templates, init, edit, hit, save, gesture | Zero stdin/prompts; fixed exit codes; stdout results and stderr diagnostics; locale-independent English output; shared core commands and renderer |
| `pixlay` GTK shell | Picker → layout → compose flow, editor state, dialogs, workers, canvas | GTK objects remain on the main thread; plain data crosses worker boundaries; one document/history; controls and accessible names; pending commands become one undo step |
| Packaging/CI | No current CI, `PKGBUILD`, desktop/MIME/metainfo/icons, or language packs | S16 explicitly owns these; the current source has no installed-release surface to validate |

### Test coverage map

- **Core geometry and templates:** very strong for the shipped library: large framing sweeps, frame coverage sweeps, independent hit-test oracle, lattice/topology/gutter tests, deterministic regeneration, and frozen fingerprints.
- **Imaging and rendering:** strong on real decode fixtures, orientation, alpha, ICC conversion, 16-bit depth, Lanczos reference/zone-plate comparisons, partial-bitmap equivalence, frame pixels, alpha, golden image, and band stitching.
- **CLI:** strong observable subprocess coverage for flags, streams, locale, exit classes, metadata, project round trips, and GUI/CLI command parity.
- **GUI:** real GTK windows, workers, render-node snapshots, ordered picker selection, layout thumbnails, frame/export dialogs, gestures, and main-path behavior. Most interaction tests call public helper methods rather than synthesizing every event controller.
- **Material risk gaps:** source/output aliasing, pending edits at close/save/New/Open, malformed embedded geometry, memory-budget boundaries, worker failure paths, real event-controller operation, standard EXIF SubIFD, image-write failure preservation, preview source-reduction identity, timing-sensitive GUI layout tests, and complete end-to-end combinations of decoder classes.

## Executive summary

The repository has a strong architectural foundation. Dependency direction is acyclic and respects the declared crate boundaries; the document format is deliberately versioned and strict; the shipped template library has unusually good invariant and determinism coverage; preview, GUI, gallery, CLI render, probe, and export converge on the same Cairo compositor; GUI workers keep GTK objects on the main thread; and output metadata is tested from actual file bytes.

The current tree nevertheless has several correctness and data-integrity gaps that should block a release. The most serious are an output path that can overwrite a source photo, live GUI edits that can disappear at save/close/document replacement, silent replacement of an existing export, preview caches that can reuse geometry or reduction quality produced for an older request, and accepted requests that can bypass the documented pixel memory budget. These are not style issues: they produce wrong documents, lost work, destroyed inputs or prior exports, or process-level memory failure on valid user paths.

The S15 machine walk and existing tests cover the happy path well, but they do not close the destructive transitions around it. The companion review's full `cargo test` run also failed in the compose GUI test, so the progress document's “built and green” wording is not proof of a clean run. S16 should not begin release packaging until the high-severity findings below have remediation and regression coverage.

## Findings (ordered by severity)

### PIX-001 — High — Reject output paths that alias a source image

- **Confidence:** High
- **Locations:** `AGENTS.md:251-252`; `crates/pixlay-cli/src/cli.rs:534-612, 831-860`; `crates/pixlay/src/window.rs:1589-1623`; `crates/pixlay/src/export.rs:87-142`; `crates/pixlay-imaging/src/encode.rs:87-113`.
- **Evidence:** The repository contract says source images are read-only under all circumstances. `render` resolves project sources, decodes them, and then writes the user-selected `--out`; `thumb` decodes `--photo` and writes the same selected destination; the GUI passes its export path through the same encoder. `encode::write` calls `File::create(path)` before encoding, and no caller compares output identity with any source. Literal equality, relative aliases, symlinks, and hard links all reach the same unguarded writer.
- **Impact:** `pixlay-render render --project p.pixlay --out photos/a.jpg`, `pixlay-render thumb --photo a.jpg --out a.jpg`, or an equivalent GUI export destroys the original source photo after decoding. This is irreversible user-data loss and directly violates the read-only invariant.
- **Recommendation:** Add one preflight helper used by CLI render/thumb and GUI export before decoding or creating output. Reject normalized-path aliases and existing filesystem identity aliases (canonical path, device/inode or an equivalent same-file check). Keep the check in the export/CLI boundary and add literal, `..`, symlink, and hard-link regression cases.

### PIX-002 — High — Centralize pending-edit and document-replacement confirmation

- **Confidence:** High
- **Locations:** `crates/pixlay/src/state.rs:94-155, 211-249`; `crates/pixlay/src/dialogs.rs:135-180`; `crates/pixlay/src/window.rs:499-545, 1376-1428`; `crates/pixlay/src/app.rs:163-182`; `crates/pixlay/tests/compose.rs:321-364`.
- **Evidence:** `Editor::begin` stores a live command separately from history and does not mark the editor dirty. `Editor::save` serializes `self.doc()` (the committed history), not `display_doc()` (the pending command). The Frame dialog's Close button only closes the dialog. The close-request handler checks only the committed dirty boolean. `Ctrl+N`/`new_document` and the Open callback replace the editor/history immediately, without the close-request confirmation. The existing compose test explicitly commits before closing, so it does not cover the boundary.
- **Impact:** A frame or crop change visible on the canvas can be lost if the user saves, closes the window, or replaces the document inside the 250 ms quiet interval. Conversely, a pending edit can be destroyed by New/Open while the close path would have prompted. A later commit can also make a just-saved document dirty again.
- **Recommendation:** Define one document-boundary state machine: on save/close/New/Open/export, either commit the pending command first or cancel it and redraw, then perform the dirty/confirmation check. Surface failed save and failed pending validation. Add tests for Frame Close, canvas crop close/save, and New/Open after a dirty edit, including Cancel, Discard, successful Save, and failed Save.

### PIX-003 — High — Enforce the 200 MP budget at the actual output and bitmap boundaries

- **Confidence:** High
- **Locations:** `crates/pixlay-cli/src/args.rs:699-710, 761-771`; `crates/pixlay-cli/src/cli.rs:574-602, 959-960, 1070-1087`; `crates/pixlay-core/src/canvas.rs:31-64`; `crates/pixlay-core/src/crop.rs:296-307`; `crates/pixlay-imaging/src/layout.rs:161-203`; `crates/pixlay-imaging/src/resample.rs:145-173`; `crates/pixlay-imaging/src/error.rs:64-70`.
- **Evidence:** `PixelSize::for_long_edge` checks the base canvas, but `render --preview-px` derives an output grid by scaling that base without applying `MAX_CANVAS_PIXELS` to the scaled grid. A square template at `--preview-px 20000` therefore passes argument validation and requests a 20,000×20,000 output. `gesture --grid` builds its grid separately and has no budget check. Separately, legal source aspects up to 20,000:1 and legal zoom up to 1000 can make `displayed_h` enormous; `slot_bitmap` checks only positive texels, and `resample` allocates RGBA16, a row strip, the flattened buffer, and an ARgb bitmap without a preflight. `ImagingError::BitmapTooLarge` is declared but has no production constructor.
- **Impact:** Valid commands can request multi-hundred-megapixel outputs or multi-gigabyte bitmap buffers, causing OOM, allocator abort, or severe thrashing instead of the contract's typed refusal. This occurs on the ordinary preview path at a documented maximum, not only on malicious input.
- **Recommendation:** Validate the rounded actual preview/gesture/output grid with `MAX_CANVAS_PIXELS` before decoding. Add checked preflight budgets for displayed width/height, texel count, destination bytes, strip bytes, and the full conversion peak. Use fallible reservations where possible and return a typed imaging error naming the slot and limit. Add a lightweight synthetic boundary test that must refuse without constructing the large allocation.

### PIX-004 — High — Include frame geometry and reduction quality in preview bitmap identity

- **Confidence:** High
- **Locations:** `crates/pixlay-imaging/src/preview.rs:288-323, 413-493`; `crates/pixlay/src/window.rs:1559-1562, 1812-1845`; `crates/pixlay-render/src/draw.rs:120-140`; `crates/pixlay-imaging/src/layout.rs:165-185`.
- **Evidence:** `Preview::Grid::reuses` compares template geometry, the cell, source path, and modification time, but neither `doc.frame` nor the requested source-reduction edge. A valid frame change calls `request_decode`, yet the same grid/cache slot can reuse the old bitmap. `draw` then recomputes the crop fit using the new frame while the reused bitmap still carries the old displayed-photo dimensions and region origin. Separately, `build_at_source_edge` selects a grid cache solely by target grid, so a build first reduced from a low-quality edge and then requested at a higher edge reuses bitmaps made from the wrong source grade.
- **Impact:** The editor can show a picture whose pixels, framing, and region metadata do not correspond to the current document after a gap change, or a supposedly higher-quality build can silently retain lower-quality source pixels. The stale bitmap is stored in the new grid entry, so either error persists until another invalidating change.
- **Recommendation:** Make bitmap identity depend on the geometry-bearing fitted result and normalized source-reduction edge per slot, or at minimum on `gap_rel`/`radius_rel` plus the source edge while excluding color-only changes. Add preview tests that change only the frame and that request two source edges for the same grid, requiring newly built geometry, pixels, and reduction metadata.

### PIX-005 — Medium — Keep GUI in-memory source paths synchronized with Save As rebasing

- **Confidence:** High
- **Locations:** `crates/pixlay-core/src/doc.rs:375-392`; `crates/pixlay/src/state.rs:190-250`.
- **Evidence:** `Project::save_as` rebases a clone for the file on disk and explicitly does not change the source project. GUI `Editor::save` then changes only `self.path` and `self.dir`; the history document retains the old relative spellings. Subsequent source resolution joins those old spellings to the new directory.
- **Impact:** Save As from `/old/project.pixlay` with `photos/a.jpg` to `/new/copy.pixlay` writes a correct disk document but leaves the in-memory editor resolving `/new/photos/a.jpg`. Preview/export can report the photo missing, and a later ordinary save can rebase a different spelling again.
- **Recommendation:** Have Save As return and adopt the effective rebased document, replacing the current history document and saved baseline atomically. Add a GUI test for written bytes, in-memory resolved identity, preview/export resolution, and second-save idempotence.

### PIX-006 — Medium — Normalize lexical `..` components before Save As rebasing

- **Confidence:** High
- **Locations:** `crates/pixlay-core/src/doc.rs:421-478`; `crates/pixlay-core/tests/project.rs:189-284`; CLI/GUI consumers `crates/pixlay-cli/src/cli.rs:135-180, 431-450` and `crates/pixlay/src/state.rs:221-249`.
- **Evidence:** `rebase_sources` and `relative_to` compare `Path::components` literally, including `..`. `std::path::absolute` intentionally preserves POSIX `..` components, so an absolute project or copy path containing `..` is not a normalized base. Existing tests use normalized project/copy directories and therefore miss this input. For example, moving a project semantically from `/a/b` to `/a` while spelling the copy as `/a/b/../copy.pixlay` can produce a relative source with one extra `..` and resolve to the wrong file.
- **Impact:** A successfully written Save As/save copy can point at the wrong or missing source. The original project remains intact, so recovery requires reconstructing the path manually.
- **Recommendation:** Lexically normalize `.`/`..` components for both absolute bases and targets before finding a common prefix, without filesystem canonicalization or symlink resolution. Add round-trip tests with `..` in project, source, and copy paths.

### PIX-007 — Medium — Validate global topology of embedded project templates

- **Confidence:** High
- **Locations:** `crates/pixlay-core/src/template.rs:98-150`; `crates/pixlay-core/src/geometry.rs:129-150`; `crates/pixlay-core/src/doc.rs:79-122`; `crates/pixlay-render/src/draw.rs:117-141`; `crates/pixlay-core/src/template.rs:167-175`; `crates/pixlay-core/tests/templates.rs:112-210`.
- **Evidence:** Production validation checks each outline independently and cross-checks each declared area, but does not check pairwise overlap, interior holes, simple outlines, or global area sums. Those checks run only over `templates::all()`, the shipped library, not arbitrary embedded project geometry. A current-version document with two equal overlapping rectangles can pass loading. Rendering paints cells in ascending index order, while `slot_at` returns the first containing slot, so the visible owner and selected owner differ.
- **Impact:** A hand-authored or externally generated `.pixlay` can violate the declared template invariants and route GUI actions/CLI hit results to a cell that is not the one painted.
- **Recommendation:** Enforce global topology at document load, with an explicit representation for the shipped gutter layouts, or restrict embedded geometry to a validated library fingerprint. Add malformed current-version JSON cases for overlap, holes, non-cut area sums, and self-intersection, including render/hit agreement.

### PIX-008 — Medium — Make hit testing frame-aware

- **Confidence:** High
- **Locations:** `crates/pixlay-core/src/frame.rs:160-205`; `crates/pixlay-core/src/template.rs:167-175`; `crates/pixlay-render/src/draw.rs:198-218`; `crates/pixlay-cli/src/cli.rs:84-113`; `crates/pixlay/src/canvas.rs:135-139`.
- **Evidence:** Rendering clips each cell to `outline ∩ rounded_rect(inset)`, but `Template::slot_at` tests only the original outline. GUI canvas selection and CLI `hit` both call that outline-only method. Existing hit tests use the default frame and discard backdrop pixels, so they cannot observe a gap or corner disagreement.
- **Impact:** A press in visible frame backdrop can select, drag, clear, replace, or swap the hidden cell. The GUI and CLI report ownership differently from what the user sees.
- **Recommendation:** Add a document-aware hit function using the same visible region as the renderer and route GUI and CLI through it. Keep geometry-only hit testing only if its semantics are explicitly separate. Add pixel-backed frame-gap and rounded-corner hit tests.

### PIX-009 — Medium — Fit a combined CLI edit against the final frame

- **Confidence:** High
- **Locations:** `crates/pixlay-cli/src/cli.rs:263-307`; `crates/pixlay-imaging/src/layout.rs:167-177`; `crates/pixlay-render/src/draw.rs:120-140`; `crates/pixlay-cli/tests/cli.rs:2046-2110`.
- **Evidence:** `edit` fits a crop against a document clone before applying the requested frame flags. The frame command is applied later. The image/render paths then refit against the final frame, so the stored crop is not the fit the document will draw. The idempotence test exercises framing and frame-only edits separately and does not combine them.
- **Impact:** A command such as `edit --slot i --zoom z --gap g` writes a crop fitted for the old frame. Re-running the same edit can change the file bytes, violating the stored-fit and byte-idempotence contracts.
- **Recommendation:** Apply the frame before fitting the crop, then run a combined gap plus zoom/rotation/offset test twice and compare bytes.

### PIX-010 — Medium — Match export paths, format suffixes, and keyboard defaults to the contract

- **Confidence:** High
- **Locations:** `crates/pixlay/src/dialogs.rs:42-47, 355-379, 396-483, 552-557`; `crates/pixlay/src/export.rs:103-141`; `crates/pixlay-imaging/src/encode.rs:56-79, 101-125`; `docs/HIG-REVIEW.md`.
- **Evidence:** The GUI stores selected `Format` and the editable filename independently. On first use, `dir` is unset and the path is only a bare suggested name, so Export resolves it against the process working directory rather than requiring a user-selected destination. The extension rewrite table contains only `jpg` and `png`, while the chooser accepts `.jpg` and `.jpeg`; switching is case-sensitive and only rewrites the exact old extension. Manually typed or chooser-returned names are not validated. The exporter encodes according to `Settings.format`, not the final extension, and the `AdwDialog` never installs an Export default widget even though the HIG review says Enter activates Export.
- **Impact:** The first export can land in an unintended process-relative location; selecting PNG while retaining `photo.jpeg` can write PNG bytes under a misleading filename; and a keyboard user's documented default action does nothing.
- **Recommendation:** Require a chooser-selected directory on first export, make the final path authoritative or validate it before export, support both JPEG suffixes case-insensitively, reject empty/extensionless names, and repeat path validation in `start_export`/`export::run` so direct callers cannot bypass it. Install and test the Export dialog's default widget. Add first-use, manual-name, chooser-name, mixed-case suffix, and Enter-key GUI cases.

### PIX-011 — Medium — Confirm GUI replacement and write image exports atomically

- **Confidence:** High
- **Locations:** `crates/pixlay/src/dialogs.rs:428-483`; `crates/pixlay-imaging/src/encode.rs:87-124`; `crates/pixlay/src/export.rs:128-142`; `crates/pixlay-cli/src/cli.rs:597-614, 840-861`; `crates/pixlay-imaging/tests/encode.rs:386-429`.
- **Evidence:** Once a destination has been chosen, pressing Export calls `start_export` directly without asking the platform chooser again, so re-exporting the remembered name silently replaces the prior image. Independently, `encode::write` opens the final destination with `File::create` before PNG/JPEG encoding and returns errors without restoring the previous file. Tests cover malformed input before opening, but not an existing target plus write/flush failure.
- **Impact:** A normal re-export destroys a valid prior export without confirmation. Even when replacement is intended, ENOSPC, quota, I/O, or encoder failure can destroy that prior file and leave a partial result. The same direct writer is shared by GUI and CLI.
- **Recommendation:** Require a GUI overwrite confirmation immediately before starting an export to an existing destination. Encode into a uniquely created same-directory temporary file, flush/sync it, atomically rename it over the confirmed target, and remove it on every failure. Add GUI confirmation/cancel cases and an induced post-create encoder failure test that asserts the old target remains valid.

### PIX-012 — Medium — Invalidate picker caches when a listed file changes in place

- **Confidence:** High
- **Locations:** `crates/pixlay/src/picker.rs:254-282, 357-420, 1260-1285, 1445-1473`; `crates/pixlay/tests/picker.rs:436-449`.
- **Evidence:** Tile and preview caches are keyed by `(index, requested_px)`. Request paths check that key before decoding; no path, size, or modification time is part of the key. A folder replacement in place therefore leaves old cached pictures and old status dimensions in place. Folder-generation invalidation only runs when the folder itself changes.
- **Impact:** The picker can show a stale tile/preview after a user edits or replaces a photo at the same path, contrary to the preview cache's file-identity intent.
- **Recommendation:** Add path plus modification time/size or a generation bump to picker cache identity, and add a same-folder replacement test covering tile, preview, and status facts.

### PIX-013 — Medium — Keep 8-bit preview reductions in 16-bit intermediates

- **Confidence:** High
- **Locations:** `AGENTS.md:253-256`; `docs/CONTRACT.md:245-249`; `crates/pixlay-imaging/src/reduce.rs:141-152, 190-259`; `crates/pixlay-imaging/src/lib.rs:14-28`; `crates/pixlay-imaging/tests/reduce.rs:201-219`.
- **Evidence:** The hard contract says everything after decode is 16-bit and quantization happens at the final write. `reduce` preserves the source depth, and for an 8-bit source stores the reduced sRGB values back as 8-bit before the later Lanczos resample. The tests verify that a 16-bit source stays 16-bit, but do not require an 8-bit reduction to be widened.
- **Impact:** Common 8-bit JPEGs undergo an avoidable intermediate quantization before final quantization, introducing preview drift and violating the stated precision invariant.
- **Recommendation:** Produce reduced 8-bit inputs as exact 16-bit samples (`byte * 257`) and account for the larger cache bytes. Add an 8-bit reduction test that inspects the returned depth and representative exact values.

### PIX-014 — Medium — Recover from worker start/send failures instead of staying pending

- **Confidence:** High
- **Locations:** `crates/pixlay/src/decode.rs:126-129, 150-159`; `crates/pixlay/src/thumbs.rs:114-118, 171-182`; `crates/pixlay/src/export.rs:176-185`; `crates/pixlay/src/window.rs:1812-1845`.
- **Evidence:** All three worker spawns use `expect`; a resource-exhaustion failure can panic during window construction or export. Channel send failures are discarded. Canvas requests still advance the generation and mark a requested grid even when no job was queued, while thumbnail in-flight keys remain held when send fails. The waiting loops then have no completion event.
- **Impact:** A dead or unavailable worker can leave a cell/gallery/export permanently marked in flight, with no retry or user-facing error; startup can panic instead of returning a typed failure.
- **Recommendation:** Make worker startup and request submission fallible, clear pending state on failure, show an actionable error, and either restart or disable the affected operation. Add injectable failure tests for each worker boundary.

### PIX-015 — Medium — Make `init`'s no-overwrite guarantee atomic

- **Confidence:** High
- **Locations:** `crates/pixlay-cli/src/cli.rs:431-442`; `docs/CONTRACT.md:366-369`; `crates/pixlay-cli/tests/cli.rs:1337-1350`.
- **Evidence:** `init` performs `args.out.exists()` and then calls ordinary `std::fs::write` in a separate operation. Only a sequential second invocation is tested.
- **Impact:** Concurrent `init` processes can both observe an absent target and then truncate/overwrite one another's project, while both report success. A dangling output symlink can also pass the existence check and be followed by the write.
- **Recommendation:** Open the destination with `OpenOptions::new().write(true).create_new(true)`, map `AlreadyExists` to the documented refusal, and write the serialized bytes through that handle. Add a synchronized two-process regression test.

### PIX-016 — Medium — Preserve restrictive project-file permissions during atomic replacement

- **Confidence:** High
- **Locations:** `crates/pixlay-core/src/doc.rs:279-312`; `crates/pixlay/src/state.rs:218-250`; `crates/pixlay-core/tests/project.rs:63-166`.
- **Evidence:** `write_atomic` creates a fresh temporary file with `File::create` and renames it over the target. It does not copy the existing target's mode. A project saved first under umask 077 and later under umask 022 can change from mode 0600 to 0644. Tests cover bytes and litter, not permissions.
- **Impact:** Saving a private project can broaden access to other local users. This is a privacy regression even though the document bytes remain correct.
- **Recommendation:** Read and apply the existing target's permissions to the temporary file before rename, while choosing an explicit safe mode for new files. Add a permission-mode test that does not depend on root-sensitive chmod behavior.

### PIX-017 — Medium — Make the canvas keyboard path select and identify cells

- **Confidence:** High
- **Locations:** `crates/pixlay/src/canvas.rs:350-470, 517-608`; `crates/pixlay/src/window.rs:740-751, 971-975`; `crates/pixlay/src/a11y.rs:16-18`; `crates/pixlay/tests/hig.rs:317-332`.
- **Evidence:** Only pointer click/drag hit testing calls `select`. Opening a document starts with no selection, and the canvas key handler returns immediately when no slot is selected. The canvas's accessible name is always “Collage canvas”; selecting a different slot does not update an accessible value, description, selected state, or relation. Existing HIG tests check that controls have some name, not that the selected cell is announced.
- **Impact:** A keyboard-only user can focus the canvas but cannot choose a cell, so framing, Delete, Enter, and swap keys remain inert until a pointer has selected one. Screen-reader users cannot tell which cell subsequent actions affect.
- **Recommendation:** Add explicit keyboard cell selection (focusable per-cell proxies or a navigation model with a clear focus/selection state), and expose slot/cell identity and selected state through the accessibility tree. Test a project opened directly followed by keyboard-only selection and editing, without calling `select()` from the test.

### PIX-018 — Medium — Preserve arbitrary-byte paths in the default `scan` report

- **Confidence:** High
- **Locations:** `crates/pixlay-cli/src/args.rs:654-675`; `crates/pixlay-cli/src/cli.rs:756-762`; `crates/pixlay-cli/src/report.rs:56-63, 90-111`.
- **Evidence:** CLI paths are accepted as arbitrary `OsString` bytes, but `scan` serializes them with `Path::display().to_string()`. The default line format appends one raw newline per field and does not escape control characters. A filename containing a newline can inject extra key/value lines; a non-UTF-8 filename is lossily converted, and distinct byte paths can become indistinguishable.
- **Impact:** The documented one-record-per-line machine output can become unparsable for valid Linux filenames, and JSON cannot losslessly identify a non-UTF-8 path.
- **Recommendation:** Define a lossless byte-path field (for example base64 or an explicitly escaped representation), retain a display field if useful, and escape default line values. Test newline, control-character, and non-UTF-8 filenames.

### PIX-019 — Medium — Report the actual long edge for preview renders

- **Confidence:** High
- **Locations:** `crates/pixlay-cli/src/args.rs:82-89`; `crates/pixlay-cli/src/cli.rs:574-578, 621-636`; `crates/pixlay-cli/tests/cli.rs:260-278`; `docs/CONTRACT.md:340-342`.
- **Evidence:** `render --preview-px 800` derives an 800-pixel output but emits the base `long_edge` (the default 4000, or the explicitly supplied export grid) at `cli.rs:621-622`; it separately emits `preview_px`. The preview test checks `preview_px` and `out_w/out_h`, not `long_edge`.
- **Impact:** A machine consumer following the documented `long_edge` field reads a value that is not the edge at which the output was rendered.
- **Recommendation:** Make `long_edge` equal the actual rounded output long edge for both full and preview renders, or introduce a separately named export-base field. Add a report assertion for `--preview-px 800`.

### PIX-020 — Medium — Report rejected Frame-dialog values instead of silently retaining pending state

- **Confidence:** High
- **Locations:** `crates/pixlay/src/dialogs.rs:81-99, 145-180, 530-542`; `crates/pixlay/src/window.rs:910-918, 1559-1562`; `crates/pixlay-core/src/doc.rs:119-137`; `crates/pixlay-core/src/frame.rs:207-242`.
- **Evidence:** The dialog exposes the full 0–100% range, but a gap can still empty a slot and be rejected by core. `Editor::begin` validates before replacing `pending`; `set_frame` ignores the error and still schedules a commit. The row can display an invalid value while the document/pending command remains a previous value, with no user-facing error.
- **Impact:** A normal-looking dialog value silently fails to apply, and the delayed commit can later write a value the row no longer shows. This contradicts the contract that frame edits are validated and errors name the offending slot.
- **Recommendation:** Propagate the `CoreError` from `set_frame`, do not schedule a commit on refusal, surface the error in the row/toast, and add a narrow-layout invalid-gap GUI test.

### PIX-020A — Medium — Stabilize heavy GTK layout tests instead of extending blind waits

- **Confidence:** High
- **Locations:** `crates/pixlay/tests/compose.rs:59-73`; repository verification entry in `AGENTS.md`.
- **Evidence:** The companion review's clean full `cargo test` run failed after 191.95 seconds because the selected-cell strip was never laid out as a row and remained 0x0. The same test passed in isolation after 117.50 seconds, already close to its 180-second ceiling. This is consistent with parallel GTK/Xvfb integration-test contention rather than a deterministic layout assertion.
- **Impact:** The required repository verification entry can be randomly red even when production behavior is correct, and the near-ceiling isolated runtime makes regressions difficult to distinguish from environmental delay.
- **Recommendation:** Wait for the actual GTK layout/allocation condition that makes the strip nonzero, with a bounded diagnostic timeout, rather than relying on one very long settling loop. If contention remains after that correction, serialize only the heavy GUI integration-test binaries or isolate their display resources. Reproduce with repeated full and isolated runs before accepting the fix.

### PIX-021 — Low — Include the failing project path in JSON diagnostics

- **Confidence:** High
- **Locations:** `crates/pixlay-core/src/error.rs:8-21`; `crates/pixlay-core/src/doc.rs:245-253`; `crates/pixlay-cli/src/cli.rs:88-92, 131-135, 173-180`; `crates/pixlay-cli/tests/cli.rs:839-855`; `docs/CONTRACT.md:330-332`.
- **Evidence:** `CoreError::Json` stores only serde's message. File-backed `CollageDoc::load` converts parsing errors directly, and CLI loaders stringify them without adding the path. The malformed-project test checks only “project JSON”, not `broken.pixlay`.
- **Impact:** A syntax/type/unknown-field failure exits correctly but may not identify which of several project paths failed, contrary to the stderr path contract.
- **Recommendation:** Wrap file-backed parse/validation failures with the project path while retaining serde detail, and strengthen the test to assert the exact path.

### PIX-022 — Low — Reject no-op commands centrally and derive dirty state from a saved baseline

- **Confidence:** High
- **Locations:** `crates/pixlay-core/src/history.rs:327-342`; `crates/pixlay/src/state.rs:94-115, 121-155`; `crates/pixlay/src/window.rs:1055-1075, 1148-1166`.
- **Evidence:** `History::apply` pushes an undo snapshot after any valid command, even if the document is equal. `Editor::apply`, `undo`, and `redo` set dirty unconditionally. The pending gesture path has a no-op check, but direct Reset/Clear/SetSource paths do not. The editor stores only a boolean, not the last saved document identity.
- **Impact:** Resetting an already identity crop or clearing an already default cell creates an empty undo step, discards redo, and can produce a false unsaved-work prompt. A save→edit→undo sequence can return to the saved document while still reporting dirty.
- **Recommendation:** Reject no-op commands in `History::apply` and track the saved document/revision so dirty is `current != saved`. Add same-value command and save→edit→undo tests.

### PIX-023 — Low — Follow the standard EXIF SubIFD for DateTimeOriginal

- **Confidence:** High
- **Locations:** `crates/pixlay-imaging/src/exif.rs:49-105`; `crates/pixlay-cli/tests/fixtures/generate.py:98-106`; `crates/pixlay-imaging/tests/decode.rs:284-303`; `docs/CONTRACT.md:344-345`.
- **Evidence:** The parser scans only TIFF IFD0 for tag `0x9003` and never follows IFD0's Exif-IFD pointer `0x8769`. The committed fixture is generated with Pillow placing the tag in the generated EXIF block, and the only date assertion uses that fixture. EXIF tag references identify `0x9003` in the ExifIFD and `0x8769` as the IFD0 Exif pointer.
- **Impact:** Normal camera JPEGs/HEICs that keep DateTimeOriginal in the standard Exif SubIFD produce an empty `date` field in `image` and `scan`, while the synthetic fixture can pass.
- **Recommendation:** Follow the bounds-checked `0x8769` pointer and scan the ExifIFD, retaining the IFD0 fallback if desired. Add hand-built little- and big-endian SubIFD fixtures plus malformed offsets/counts.

### PIX-024 — Low — Reject JPEG dimensions above the encoder's `u16` boundary

- **Confidence:** High
- **Locations:** `crates/pixlay-imaging/src/encode.rs:87-99, 151-160`; `crates/pixlay-imaging/src/error.rs:39-64`.
- **Evidence:** Public `encode::write` accepts any positive `i32` dimensions after checking buffer length, then casts JPEG width/height to `u16` for `jpeg_encoder`. A valid 70,000×1 buffer wraps to 4,464×1 instead of being refused. Current CLI/GUI grids cap the product path below this, so this is a library-boundary defect.
- **Impact:** Direct callers can receive a successful file whose recorded dimensions do not match the supplied buffer.
- **Recommendation:** Reject JPEG dimensions above 65,535 before opening the file, use checked arithmetic for the expected byte count, and add 65,535/65,536/70,000 boundary tests.

### PIX-025 — Low — Emit a conformant ICC creation date

- **Confidence:** High
- **Locations:** `crates/pixlay-imaging/src/icc.rs:104-123`; embedded profile bytes tested in `crates/pixlay-imaging/tests/encode.rs:237-313`; ICC.1:2010 §4.2 and §7.2.8.
- **Evidence:** The ICC v4.3 profile deliberately leaves header bytes 24–35, the creation date, all zero. The ICC specification defines `dateTimeNumber` with month 1–12 and day 1–31 and requires bytes 24–35 to contain the profile creation time. Zero is a documented sentinel for profile ID, not for the creation date. Existing tests validate colorants and curves but not this header field.
- **Impact:** Strict color-management validators may reject or misinterpret the embedded profile, even though lenient readers currently use it successfully.
- **Recommendation:** Write a fixed valid UTC date constant to preserve deterministic bytes, update the comment, and add header-date validation to the ICC tests.

### PIX-026 — Low — Cover dynamic and multiline gettext entries in catalog freshness checks

- **Confidence:** High
- **Locations:** `crates/pixlay/src/app.rs:52-67, 195-200`; `crates/pixlay/tests/i18n.rs:69-81, 102-121`; `po/pixlay.pot:70-125, 230-365`.
- **Evidence:** Shortcut section labels are stored as variables and passed through `gettext(*section)`, so `xgettext --language=Rust` cannot extract them as literal msgids. The POT freshness helper reads only the first line after `msgid`/`msgid_plural` and skips multiline entries, so changed or removed long strings can pass unnoticed. The current POT source references are also behind current call sites.
- **Impact:** Future language packs can leave shortcut headings English and ship stale/missing long descriptions while the freshness test remains green.
- **Recommendation:** Use literal calls in the shortcut section table or an explicit extraction-friendly mapping, parse concatenated PO msgids (or invoke a PO-aware tool), and compare source references or regenerate the POT in the test policy.

### PIX-027 — Low — Synchronize authoritative records with the current implementation

- **Confidence:** High
- **Locations:** `AGENTS.md:45-46, 28`; `docs/CONTRACT.md:371, 378, 390, 505, 585, 611-619`; `docs/archive/2026-09-22-STEPS.md:239-263, 268-302`; `crates/pixlay-imaging/src/lib.rs:1-55`; `crates/pixlay-core/src/templates/mod.rs:24-40`; `crates/pixlay-render/tests/history.rs:1-25`; `crates/pixlay-cli/tests/history.rs:1-20`.
- **Evidence:** The authoritative progress block contains two conflicting “Next action” statements, says the narrow strip overlays the neighbor although the current code/test selects a vertical column, and calls S15 built/green while its own gate remains pending. `AGENTS.md` calls `verify.pixlay` docVersion 2 although the fixture starts at 3. `CONTRACT.md` still names removed `Removed::restore`, describes a fit-and-zoom preview although the current implementation and later contract say static `Contain`, says four output formats although only three extensions are accepted, and carries pre-S12c template/string/test counts. Imaging and render module docs still list removed grading, filtering, TIFF, and resolution stages; history test comments still describe removed commands.
- **Impact:** Future contributors and release reviewers are directed toward APIs, counts, and capabilities that no longer exist, making contract drift and incorrect remediation likely.
- **Recommendation:** Synchronize only current/authoritative sections with the source while preserving historical step records as history. Give the status block exactly one next action, regenerate source references, and add a lightweight consistency check for the current constants/paths.

### PIX-027A — Low — Reject invalid canvas aspects before deriving an output grid

- **Confidence:** High
- **Locations:** `crates/pixlay-core/src/canvas.rs:22-65`; callers in `crates/pixlay-cli/src/args.rs` and `crates/pixlay-cli/src/cli.rs`.
- **Evidence:** `PixelSize::for_long_edge` validates `long_edge_px` but not `aspect`. `NaN`, zero, negative, and infinite aspects bypass the intended shape and can produce a one-pixel-by-`N` grid after conversion and clamping rather than a typed validation error.
- **Impact:** A malformed template aspect can yield a grid unrelated to the template, invalidates later geometry assumptions, and may turn an invalid document into an apparently usable output request.
- **Recommendation:** Require a positive finite aspect before any multiplication or branch, return `CoreError::OutOfRange` with the accepted domain, and add zero, negative, `NaN`, and infinite boundary tests.

### PIX-027B — Low — Honor the documented untouched boundary for zero-area covering polygons

- **Confidence:** High
- **Locations:** `crates/pixlay-core/src/crop.rs:206-238, 377-409`; polygon degeneracy definitions in `crates/pixlay-core/src/geometry.rs`.
- **Evidence:** `CropTransform::fit` documents that a degenerate `covering` polygon returns the request untouched. `Frame::new` checks only that the polygon has at least three vertices, so a valid three-point collinear polygon reaches the covering calculation instead of taking the documented boundary.
- **Impact:** A zero-area cover can produce a different transform even though the API promises no framing fit, making behavior depend on which degenerate representation reached the function.
- **Recommendation:** Use the polygon's existing degeneracy predicate before constructing `Frame`, return `CropFit { transform: *self }` for zero area, and add collinear and repeated-point three-vertex cases.

### PIX-027C — Low — Report the maximum rebuilt source size across preview slots

- **Confidence:** High
- **Locations:** `crates/pixlay-imaging/src/preview.rs:326-344, 438-475`.
- **Evidence:** `Built.source_px` is documented as the largest preview-grade source any cell rebuilt, but each successful source reduction assigns the field rather than taking a maximum. With different source aspect ratios, the returned value is the last processed source instead of the documented maximum.
- **Impact:** `gesture` can print a `src_w`/`src_h` that understates the work actually performed, weakening the machine-readable performance probe and its review thresholds.
- **Recommendation:** Accumulate the per-slot maximum explicitly and add a multi-slot test whose processed source sizes increase, remain equal, and decrease across the loop.

### PIX-028 — Note — Resolve the picker zoom/pan contract ambiguity before the human gate

- **Confidence:** High
- **Locations:** `docs/archive/2026-09-22-STEPS.md:34-48, 1180-1192, 1285-1292, 1350-1370`; `crates/pixlay/src/picker.rs:39-45, 564-595`; `docs/CONTRACT.md:1155-1158`.
- **Evidence:** The early UX ruling says the picker is “grid + a large preview that can zoom and pan.” The implementation is a static `GtkPicture` with `ContentFit::Contain`, and a later S13 result explicitly reinterprets magnification as editor-only while leaving the open question recorded. The current contract documents the static fit.
- **Impact:** This is not a confirmed pixel defect, but the authoritative product contract is ambiguous about a user-visible picker capability. A human walk cannot unambiguously accept or reject the stage.
- **Recommendation:** Record one authoritative ruling: implement a keyboard/pointer zoom-and-pan preview with a machine-visible test surface, or explicitly supersede the earlier ruling. Do not treat this as an implementation bug until that decision is recorded.

### PIX-028A — Note — Verify decoded color state for CICP-only and malformed-ICC sources

- **Confidence:** Medium
- **Locations:** `crates/pixlay-imaging/src/decode.rs:263-309`; glycin loader color-conversion contract; existing decode fixtures under `crates/pixlay-cli/tests/fixtures/`.
- **Evidence:** The loader requests ICC-to-sRGB conversion, but the returned frame's color state is not independently checked after decoding. Static inspection cannot establish whether glycin rejects a malformed ICC or whether a CICP-only wide-gamut source bypasses the requested ICC conversion and reaches the linear-light pipeline with an incorrect assumption.
- **Impact:** A real BT.2020/PQ AVIF or HEIC, or a malformed embedded profile, could be rejected inconsistently or processed as if it were sRGB. The current fixtures do not settle the behavior, so this remains an API-dependent risk rather than a confirmed defect.
- **Recommendation:** Add a real CICP-only wide-gamut AVIF/HEIC and a malformed-ICC fixture, assert the returned frame's color state before resampling, and only then assign severity and choose rejection or conversion behavior.

## Positive observations

1. **Crate boundaries are clean.** `pixlay-core` has no Cairo/GTK dependency; imaging depends only on core plus decoder/encoder libraries; render depends on core and Cairo; CLI and GUI share the lower layers without reversing the graph. No prohibited GUI dependency or direct GUI pixel pipeline was found.
2. **Unsafe code policy is effective.** The workspace denies `unsafe_code`; no application-authored unsafe was found. Production thread boundaries are plain-data based, and GTK objects stay on the main thread.
3. **The document boundary is deliberate.** Version is probed before strict deserialization, unknown fields are denied, rotation normalization is centralized, and all public limit failures have typed errors. The frozen template generator/fingerprint tests protect shipped geometry rather than merely comparing a function with itself.
4. **Framing coverage evidence is strong.** The core sweep writes an independent placement/coverage model and exercises rotations, pan, zoom, photo aspects, frames, and the concave library slot. Renderer tests then measure actual painted interiors, seams, gaps, rounded corners, and spill.
5. **The single renderer invariant holds across consumers.** GUI canvas, layout gallery, CLI render/probe, and GUI export all call `pixlay_render::draw`; the GUI clips the sheet before handing Cairo the document. No second production compositor was found.
6. **Imaging correctness has meaningful independent oracles.** Decode tests use real JPEG, PNG, HEIC, 16-bit, grayscale, alpha, ICC, and EXIF-oriented fixtures; resampling is compared with ImageMagick and an analytic zone plate; output tests inspect PNG chunks and JPEG markers rather than trusting the report.
7. **Project persistence is thoughtfully implemented.** Save uses a same-directory temporary, `sync_all`, rename, cleanup, and non-UTF-8-preserving `OsString` construction. The main remaining issues are permissions, concurrent same-process temporary names, and GUI state adoption.
8. **CLI stream contracts are unusually well covered.** Tests run the built binary with closed stdin, no TTY, a fixed environment, locale variants, and actual exit codes. Reports are sorted and deterministic, and GUI/CLI structural edits share `pixlay_core::Command`.
9. **Async routing has sound generation/epoch gates.** Picker replies carry folder epochs; canvas and gallery replies carry generations; stale answers are rejected. The missing coverage is failure recovery, not ordinary stale-result routing.
10. **GUI tests exercise production surfaces.** The suite uses real `EditorWindow`, workers, GTK snapshots, and the actual renderer/CLI rather than mocks. The main limitation is that several event paths are invoked through public helpers instead of synthesized controllers.

## Verdict

- **overall:** `Incorrect` — the current tree contains multiple confirmed data-loss, memory-budget, and state-integrity defects on valid user paths; do not treat it as release-ready.
- **release_readiness:** `Not ready` — resolve all High findings and rerun the repository verification entry before S15 acceptance or S16 packaging. Packaging/CI assets are also intentionally absent until S16, so installed-release readiness is not claimed.
- **contract_conformance:** `Partial` — crate boundaries, document versioning, shipped-template invariants, single-renderer convergence, output metadata, and much of the CLI contract conform; source read-only, pending-edit durability, export replacement, global document topology, actual pixel-budget enforcement, frame-aware hit behavior, and some current documentation claims do not.
- **test_assessment:** `Strong on geometry, resampling, metadata, CLI streams, and GUI happy paths; incomplete on destructive transitions, failure recovery, memory boundaries, event-controller operation, malformed embedded geometry, real-world metadata/layout edge cases, and deterministic heavy-GUI execution. The companion full-suite run failed in compose; this review itself did not run the suite.`
- **risk_posture:** `High` until source aliasing, pending edits, stale preview identity, export replacement, and allocation budgets are fixed. The next tier is loaded-template/hit/fit correctness and export filesystem safety. Accessibility, keyboard operability, GUI test determinism, and catalog/docs drift remain release-quality risks even where they do not immediately corrupt pixels.

## Recommended remediation order

1. **Stop irreversible data loss:** enforce source/output identity refusal; centralize pending-edit commit/cancel and New/Open/save/close confirmation; require confirmation before replacing an existing export.
2. **Make large work fail safely:** enforce the actual output-grid and per-slot memory budgets before allocation; add fallible/checked resource accounting; reject invalid canvas aspects.
3. **Fix current-document correctness:** invalidate frame- and source-edge-dependent preview bitmaps; make GUI Save As adopt the rebased document; normalize lexical `..` bases.
4. **Align geometry consumers:** validate embedded template topology and make hit testing use the same visible region as rendering; move combined CLI frame application before crop fitting.
5. **Harden filesystem behavior:** atomic image export, atomic `init` creation, unique project-save temporaries, and permission preservation.
6. **Close GUI and metadata gaps:** frame validation feedback, export path/default-action behavior, picker file identity, 8-bit reduction precision, keyboard cell selection/AT identity, EXIF SubIFD, ICC date, JPEG dimension checks, and preview maximum-source reporting.
7. **Stabilize machine/documentation contracts:** correct preview `long_edge`, arbitrary-byte `scan` paths, gettext extraction/freshness, stale status/module/contract text, the picker zoom/pan decision, and decoded color-state coverage.
8. **Expand tests in the same order:** failure injection, alias/collision tests, real GTK event traversal, malformed document fixtures, boundary memory cases, deterministic heavy-GUI execution, and complete end-to-end decoder combinations. Keep the existing independent geometry, resampling, metadata, and GUI snapshot tests as regression anchors.
