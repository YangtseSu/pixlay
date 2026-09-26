# The project's own install definition (S31): `meson setup build && meson compile -C
# build && meson install -C build` is how this application is built and installed on a
# Linux system, and the PKGBUILD only wraps that (`AGENTS.md`, "AUR discipline").
# GNOME's applications are built this way, and this is one.
#
# `'rust'` is the language meson has to know about — the compiler itself is cargo's
# business — and the dependencies below are declared so that `meson setup` fails naming
# the version a person has to install, instead of failing deep inside a cargo build.
project(
  'pixlay',
  'rust',
  version: '0.1.0',
  license: 'GPL-3.0-or-later',
  meson_version: '>= 1.0.0',
)

# The shell, at the level the bindings need (`AGENTS.md`, the dependency registry:
# `v4_12` is what `GtkCssProvider::load_from_string` and `GdkSurface::layout` need, and
# libadwaita's `v1_8` is what `AdwDialog`, `AdwToastOverlay` and `AdwShortcutsDialog`
# need).
dependency('gtk4', version: '>= 4.12.0')
dependency('libadwaita-1', version: '>= 1.8.0')
# The decoding backend (S4): glycin's loader, and the seccomp it sandboxes itself with.
# The version floor is the crate's own (`pixlay-imaging`'s `glycin` dependency), which
# checks it where it is used.
dependency('glycin-1')
dependency('libseccomp')
# `pixlay-imaging` drives a glib main context and gio files directly.
dependency('glib-2.0')
dependency('gio-2.0')

# `data/meson.build` merges the catalogs into the desktop entry and the metainfo with
# it; `po/meson.build` does *not* use it (`i18n.gettext` would bring a second pot
# definition — that file says why).
i18n = import('i18n')

subdir('crates')
subdir('data')
subdir('po')
