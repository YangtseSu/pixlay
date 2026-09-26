# S33 · CI runs no GUI suite

**Progress**: done (2026-09-27) — `ci.yml` runs the entry's commands minus the GUI suite, and the failures
that led there are recorded below and in the workflow's own header.

**Goal**: a CI that answers something true. The GUI suite needed a display, a compositor and the target's
libraries at once; a GitHub runner could not give it all three, and every attempt to make one of them
optional cost the others.

**Work**

- `.github/workflows/ci.yml`: `runs-on: ubuntu-26.04`, no container, `cargo test --workspace --exclude
  pixlay` (the four windowless crates), plus `cargo fmt --check`, `cargo clippy --workspace --all-targets
  -- -D warnings` — which still compiles the shell and its tests, so a shell that does not build still
  fails — and the entry's own render.
- `AGENTS.md`: the verification entry says the GUI suite is the machine's, and the CI bullet carries the
  reason; `docs/CONTRACT.md` is untouched (no shape moved).

**The errors, recorded** (all 2026-09-27, in the order they were found; the runs are in the repository's
Actions history under the tag `v0.1.0` and the commits of that day)

| Attempt | Failure |
|---|---|
| `archlinux:latest` container on `ubuntu-latest`, the harness's own headless mutter | `the_main_path_can_be_walked`: `the widget never produced a render node: a PixlayEditorWindow of 1100x760, visible true, mapped true, 1 child(ren); waited 180.0s, 0 frames arrived, 1084 frames, active false, mapped true, last canvas draw refusal: None` — three runs in a row; the window is mapped and the compositor never presents a frame |
| `archlinux:latest` container on `ubuntu-26.04` | the same failure (the runner's OS is not what the container's display needs) |
| `ubuntu-26.04`, no container, mutter | the suite runs: `the_compose_stage_edits_the_selected_cell_and_the_document` fails with `the export landed at "/var/tmp/pixlay-s7/compose.png" … the window's last toast was Some("1 photo could not be read")` — the document is `verify.pixlay`, which carries a HEIC, and Ubuntu splits libheif's decoders into plugin packages that `--no-install-recommends` did not bring |
| the same, with `libheif-plugin-libde265` and `-dav1d` installed | that test passes; `Mutter terminated with a failure: The command exited with a nonzero status: 139` — mutter itself dies of its GL setup on a runner with no GPU node |
| the same, with `LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe MESA_LOADER_DRIVER_OVERRIDE=llvmpipe` | the same `139` |
| `ubuntu-26.04`, no container, a headless Weston (`pyvista/setup-headless-display-action`, `wm: weston`, llvmpipe) with `PIXLAY_TEST_CHILD=1` | the suite runs and reaches the HIG walk, which then reports **47** widgets with no accessible name — `the_interface_meets_the_machine_checkable_hig` fails. The list includes GTK's own chrome (`GtkButton inside GtkWindowControls`, the increment/decrement `GtkButton`s inside `GtkSpinButton`, `AdwSheetControls`), so the runner's GTK 4.21 exposes unnamed internals that the target's 4.24 does not — a difference the walk reads through `gtk4::test_accessible_has_property` |
| locally, `GTK_A11Y=none` (an attempt to silence the runner's missing accessibility bus) | the harness's mutter dies as well (`Mutter terminated with a failure: … 25856`), so that variable is not a fix and is not in any workflow |

**What would re-enable it**: a runner that can give the suite a compositor *and* the libraries the product
targets. Two candidates, neither tried: an Arch container whose display comes from the host (a mount of
`/tmp/.X11-unix` with `PIXLAY_TEST_CHILD=1 xvfb-run -a`, `xorg-server-xvfb` inside the container), or a
runner with a GPU node so mutter's own GL path survives. Until then the suite is the entry's, on a machine
— which is where its numbers have always come from.
