<!--
SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>

SPDX-License-Identifier: GPL-3.0-or-later
-->

# S41 · The GUI suite put a dialog on the session's display

**Progress**: **doing** (2026-10-17) — the two causes are named and fixed, and the machine-checkable half of the
exit is green: the export dialog was the **session's portal**, and the `export` binary's
SIGSEGV was the test process's **own decoding worker** being killed by `exit()` mid-draw —
neither of them the compositor the first record suspected. What is left is the human's watch of
their own desktop through a full `cargo test -p pixlay`. **→ next: the human's 「没有弹出」, then
this file is renamed `-done.md`.**

**Goal**: no window the GUI suite creates may ever appear on the developer's own desktop. The suite is
*headless by requirement*: it runs inside a private `mutter --wayland --headless --no-x11 --virtual-monitor
1920x1200@60.0` (`crates/pixlay/tests/support/mod.rs`), and a window drawn by the session's own
compositor is both a broken requirement and a hazard — a window that steals focus and clicks while someone is
working, and, in a run on a CI box, a window nothing can see.

## What was observed (2026-10-17)

The human, watching their desktop while the S40 GUI suite ran: **the app's Export dialog appeared** —
「出来了。导出拼贴对话框」. That string is `crates/pixlay/src/window.rs`:
`gtk::FileDialog::builder().title(gettext("Export the collage"))`, whose `zh_CN` catalog entry is 导出拼贴, so
the window is this product's own chooser and not some other application's.

A second symptom, from the same runs: a full `cargo test` had failed twice at the **`export` binary's
teardown** — its one test passes, then mutter reports `The command exited with a nonzero status: 139` and
`cargo test` reports the crate red, while the same binary alone passes.

## What the measurements found

**Two causes, and neither of them is the compositor.** The record this file replaces was right about the
symptom and wrong about the mechanism in both cases, so the first thing here is what overturned it.

### 1. The dialog is the session portal's window, not a window of ours

The earlier reading — 「`dbus-monitor` during `--test export` and `--test canvas`: **0**
`org.freedesktop.portal.FileChooser` calls」 — is void: **those two binaries never open a file dialog.**
`export.rs` calls `export_seed`/`export_to_chosen` and never `window.export()`; the dialog is opened by
`mainpath.rs`, `compose.rs` and `hig.rs` (through the `win.export` action) and by `canvas.rs`/`layout.rs`
(through the photo chooser). Watched on the right binary, the mechanism is plain:

| Item | Measurement |
|---|---|
| the call | `dbus-monitor --session "interface='org.freedesktop.portal.FileChooser'"` during `cargo test -p pixlay --test mainpath`: one **`SaveFile`** — `sender=:1.3251 -> destination=org.freedesktop.portal.Desktop`, title **`导出拼贴`** (`gettext("Export the collage")`, `window.rs`), seed `mainpath-framed.jpg`, parent handle `wayland:…`, filters `*.jpg`, `*.jpeg` |
| whose window that is | `xdg-desktop-portal-gnome` (pid 3721, a **session** service) draws it on the session's compositor. Nothing of the suite ever connected to that compositor; the portal did the drawing, which is why the window appeared with nothing of ours in the session's client list |
| GTK's own gate | GTK 4.24's `gtkfilechoosernativeportal.c`: `gdk_display_should_use_portal (display, "org.freedesktop.portal.FileChooser", 3)` — and on this machine the portal answers, so the portal is what a `GtkFileDialog` uses |
| the switch that does **not** work | `GTK_USE_PORTAL=0`: a `Gtk.FileDialog` probe under it **still** sent `SaveFile` |
| the switch that does | **`GDK_DEBUG=no-portals`** — the same probe under it sent nothing, and the dialog is then GTK's own `GtkFileChooserDialog`, a window of the test process inside the private mutter |

### 2. The SIGSEGV is the test process's own decoding worker, killed by `exit()`

Not mutter, not the amdgpu render node, not a nine-compositor sequence: `coredumpctl` holds four SIGSEGV
cores from this day alone (10:29:57, 10:40:43, 11:02:00, 13:07:12), and every one of them is an
`export-<hash>` **test binary**, its crashing thread named **`pixlay-decode`**:

| Item | Measurement |
|---|---|
| the stack | `decode::work` → `sketch_rgb8` (`pixlay-render/src/sketch.rs`) → `cairo_paint` → `pixman_fill`, and there the `call *%rax` at pixman's `0xf728` (objdump) jumped to a *heap* address holding text — the implementation struct's function pointer it read through had been reused. `cairo_paint` on a solid source is the sketch's paper fill |
| what the other thread was doing | the **main** thread was in `exit()` → `__run_exit_handlers` → `_dl_fini`, i.e. the process was tearing down its libraries *while* the worker drew |
| why the window is hit so often | the band's own batch — every layout with the document's cell count, drawn as a sketch on the *same* worker — is usually still in flight when a test's last statement runs. Measured with the fix below: the join at the end of `--test export` waited **2.43 s** for the job that was running |
| what is *not* the cause | `mutter`'s renderer: `Added device '/dev/dri/renderD128' (amdgpu) … using no mode setting` is the same line S40's runs logged, but the crash reproduces with no compositor in the picture at all; and the neighbouring binaries' `unable to lock lockfile …/wayland-0.lock` is mutter picking another socket name, not a failure |

## The two fixes

1. **`crates/pixlay/tests/support/mod.rs`**: the test child's environment gains
   `.env("GDK_DEBUG", "no-portals")` beside `GTK_IM_MODULE` / `NO_AT_BRIDGE`, with the portal call and
   the two switches above written into the comment. The suite's dialogs are then its own windows, in
   the private compositor.
2. **The worker's lifetime, in the product and in the harness**: `WorkerPlan::start` hands back the
   thread's `JoinHandle`; `decode.rs`'s `Worker` closes the queue and **joins** when the decoder is
   dropped; `EditorWindow::stop_decoding` drops that decoder; and the harness hands each test a
   `#[must_use]` `Harness` — bound as `let _harness = support::start();` in all 18 GUI tests — whose
   drop stops every window it built. The last statement of a test function is the last moment the
   worker can be waited for (libtest calls `exit` as it returns), so the wait happens *there*, by
   construction, and a test that forgets the binding fails `-D warnings` rather than flaking.
   The export worker keeps no handle, deliberately (`export.rs`'s comment): its progress and its toast
   are its own "it is done", and the decode worker is the one measured here.
   The `#[must_use]` half was checked by un-binding one test again: `cargo clippy -p pixlay
   --all-targets -- -D warnings` refuses it — “unused `support::Harness` that must be used,
   crates/pixlay/tests/kept.rs:63” — so the rule is a build failure and not a note.

## Machine-checkable exit

- **No process of the suite is connected to the session's display.**
  `strace -f -e trace=connect -o … cargo test -p pixlay` (all 18 GUI binaries, 21 targets
  green in **482.3 s** of test time): **0** `connect` calls to `/run/user/1000/wayland-0` and
  **63** to the private `/run/user/1000/wayland-1`. The same run's session bus, watched on
  `/org/freedesktop/portal/desktop`: **0** `FileChooser` calls (only `portal.Settings` reads —
  120 of them). As a cross-check, `ss -x -p -a` sampled **928** times through one full run:
  `gnome-shell` is the only process ever named on the session's `wayland-0` socket, and no
  process of the suite appears at all.
- **The session's own window list is not readable here**, so "no window of ours in it" is measured
  through what a window needs to exist rather than by reading the list:
  `org.gnome.Shell.Introspect.GetWindows` answers `AccessDenied` (「GetWindows is not allowed」) on this
  machine, while a client window requires a connection to the session's socket — and there is none.
- **The entry is green, and the crash is gone.** `cargo test -p pixlay` three times in a row,
  plain: **21 targets, 0 failures** each (443.0 s / 441.2 s / 440.2 s of test time), and
  **no core dump after 13:07:12** — the four that exist today were all taken before the fix,
  at a rate of about two per full run. `cargo fmt --check`, `cargo clippy --workspace
  --all-targets -- -D warnings`, `cargo test` (the workspace) and `reuse lint` — the entry's
  four commands — ran beside them.

## Human

The human watches their own desktop through one full `cargo test -p pixlay` and sees **no window**:
「没有弹出」. *(Waiting: the runs' own passes are recorded above; this is the one line a session cannot
write for itself.)*

## Result

Two environment facts, both measured, both fixed in the suite's own harness: the suite's dialogs used to
be the **session portal's** windows (they are now its own, `GDK_DEBUG=no-portals`), and its process used
to exit while its **own decoding worker** was drawing in cairo (the worker is now ended and joined at the
end of every test). The compositor, the render node and the mutter sequence the first record suspected
are all refuted by four core dumps.
