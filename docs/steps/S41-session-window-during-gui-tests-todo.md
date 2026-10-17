<!--
SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>

SPDX-License-Identifier: GPL-3.0-or-later
-->

# S41 · The GUI suite put a dialog on the session's display

**Progress**: recorded (2026-10-17) from S40's runs at the human's word — 「把这个事儿，记录下。回头再弄。」
— and **not started**. **→ next: name the process that maps the window, then fix the harness.**

**Goal**: no window the GUI suite creates may ever appear on the developer's own desktop. The suite is
*headless by requirement*: it runs inside a private `mutter --wayland --headless --no-x11 --virtual-monitor
1920x1200@60.0` (`crates/pixlay/tests/support/mod.rs:114-136`), and a dialog drawn by the session's own
compositor is both a broken requirement and a hazard — a window that steals focus and clicks while someone is
working, and, in a run on a CI box, a window nothing can see.

## What was observed (2026-10-17)

The human, watching their desktop while the S40 GUI suite ran: **the app's Export dialog appeared** —
「出来了。导出拼贴对话框」. That string is `crates/pixlay/src/window.rs:2525-2526`:
`gtk::FileDialog::builder().title(gettext("Export the collage"))`, whose `zh_CN` catalog entry is 导出拼贴, so the
window is this product's own chooser and not some other application's.

## What was measured, and what it rules out

| Item | Measurement |
|---|---|
| the harness's compositor | `mutter --wayland --headless --no-x11 --virtual-monitor 1920x1200@60.0 -- <test binary>` — read from the code, and seen live in `ps` (pid 1290467 of the `layout` binary's run) |
| the test process's display | `/proc/<pid>/environ` of a `layout` test child: `WAYLAND_DISPLAY=wayland-1` (the private socket) while the invoking shell has `wayland-0` — so the re-execution *did* land the binary inside the private compositor |
| the escape hatch | `PIXLAY_TEST_CHILD` was **not** set (`env`), so `support::start()` took the private-mutter path, not the "use the display I have" path |
| the private compositor's renderer | its log says `Added device '/dev/dri/renderD128' (amdgpu) ... using no mode setting`, i.e. the render node only, never DRM master — which is *not* the configuration `AGENTS.md` records (`/dev/dri` hidden → `Created surfaceless renderer without GPU`). A renderer difference, and it cannot take the display |
| the portal | `dbus-monitor --session` during `cargo test -p pixlay --test export` **and** `--test canvas`: **0** `org.freedesktop.portal.FileChooser` calls (only `Settings` / `Documents` traffic). `GTK_USE_PORTAL` is unset, there is no `use-portal` key in `org.gtk.Settings.FileChooser` on this machine, and GTK 4.24's chooser is `gtkfilechoosernative.c` / `gtkfilechoosernativeportal.c` with the native path taken for an unsandboxed process. **So the portal is not the mechanism** |
| the suite itself | all **17** GUI test binaries green under mutter 51.1 (the one `export` failure in the first full run was a mutter `SIGSEGV` *at teardown*, after the test had passed — a flake, and it did not recur) |

## What is still unknown

**Which process maps that window, and on which connection.** The measurements above rule out the two
mechanisms I first suspected (the escape hatch and the portal) without yet naming the third. The next
measurements, in order:

1. `ss -xp | grep wayland-` sampled *while the dialog is up* (`cargo test -p pixlay --test export` is 22 s and
   opens it), and mapped to `cmdline`s: every process holding the **session's** `wayland-0` socket that is part
   of the suite's tree, and every one holding the private `wayland-1`, says which display each window went to.
2. `GDK_BACKEND` and `WAYLAND_DISPLAY` in the *dialog's own* process: a GTK process with `GDK_BACKEND=x11`, or
   with `WAYLAND_DISPLAY` unset while `DISPLAY=:0` is present, draws on the session's Xwayland instead of the
   private Wayland — which would explain a dialog on the desktop and *not* the main window only if the dialog
   is created by a second process.
3. Whether the window belongs to the test process's own GTK (`gtk::Window::list_toplevels` from inside the
   test, and the same list *of the session* is what the human sees) or to a session service — the portal is
   ruled out, so a session service would mean a second, unnamed portal/dialog path.
4. Whether an *outer* shell of the run had `PIXLAY_TEST_CHILD=1` exported in a way the harness's child still
   inherited (the child's own `/proc/<pid>/environ` is the authority, and it read `PIXLAY_TEST_CHILD=1` — set
   by the harness itself for its re-executed child, which is expected; what matters is whether some *other*
   process of the tree got it without the private display).

## Machine-checkable exit

- A probe that runs the whole suite (`cargo test -p pixlay`) while sampling the session's compositor — the
  `wayland-0` socket's owner list, and `gtk::Window::list_toplevels` for the session's own clients if that can
  be reached — reports **no process of the suite connected to the session's display**, and no window of ours
  in the session's window list.
- The human watches their own desktop through one full `cargo test -p pixlay` and sees **no window**: 「没有
  弹出」.
- The fix is one of (each cheap, and the *cause* decides which): `GDK_BACKEND=wayland` beside the existing
  `GTK_IM_MODULE` / `NO_AT_BRIDGE` in `support/mod.rs`'s child environment; clearing `DISPLAY` for the child so
  only the private Wayland display exists; or `GTK_USE_PORTAL=0` if measurement 3 finds a portal path after all.
  Whatever it is, it goes in as one line with its reason, and this file records which measurement chose it.

## Result

Not yet — this file is the record the human asked for; the measurements above are what a later session starts
from.
