# SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
#
# SPDX-License-Identifier: GPL-3.0-or-later

# The data the desktop reads: the entry and the metainfo are their templates with the
# catalogs merged in (`msgfmt --desktop` and `msgfmt --xml`, which is what
# `i18n.merge_file` runs), the two icons, and the private `.pixlay` type. What this file
# installs is what `crates/pixlay-core/tests/packaging.rs` holds to the names beside it.
app_id = 'org.yangtse.Pixlay'

desktop_file = i18n.merge_file(
  input: 'org.yangtse.Pixlay.desktop.in',
  output: app_id + '.desktop',
  type: 'desktop',
  po_dir: '../po',
  install: true,
  install_dir: get_option('datadir') / 'applications',
)

# Both files are what only these two tools can judge, so they are what they run on —
# `meson test`, wherever the tools are installed. A package build does not run them:
# its standard is that it builds and packages.
desktop_utils = find_program('desktop-file-validate', required: false)
if desktop_utils.found()
  test('desktop file', desktop_utils, args: [desktop_file])
endif

appstream_file = i18n.merge_file(
  input: 'org.yangtse.Pixlay.metainfo.xml.in',
  output: app_id + '.metainfo.xml',
  po_dir: '../po',
  install: true,
  install_dir: get_option('datadir') / 'metainfo',
)

appstreamcli = find_program('appstreamcli', required: false)
if appstreamcli.found()
  test('metainfo', appstreamcli, args: ['validate', '--no-net', appstream_file])
endif

install_data(
  'icons/hicolor/scalable/apps/' + app_id + '.svg',
  install_dir: get_option('datadir') / 'icons' / 'hicolor' / 'scalable' / 'apps',
)
install_data(
  'icons/hicolor/symbolic/apps/' + app_id + '-symbolic.svg',
  install_dir: get_option('datadir') / 'icons' / 'hicolor' / 'symbolic' / 'apps',
)

# `update-mime-database` (shared-mime-info's pacman hook) turns this file into the
# `.pixlay` type, and the desktop entry's `MimeType=` line is what makes this
# application its handler. Installed under the app-id, which is what the file itself
# registers.
install_data(
  'org.yangtse.Pixlay.mime.xml',
  rename: app_id + '.xml',
  install_dir: get_option('datadir') / 'mime' / 'packages',
)
