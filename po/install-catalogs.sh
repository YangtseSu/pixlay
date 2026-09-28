#!/bin/sh

# SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
#
# SPDX-License-Identifier: GPL-3.0-or-later

# Installs the catalogs `po/meson.build` built: `$1` is the `localedir` option (relative
# to the prefix, which is what `MESON_INSTALL_DESTDIR_PREFIX` already carries) and the
# rest are the `.mo` files, each named after its language. Every one of them is the same
# domain — `pixlay`, what `crates/pixlay/src/i18n.rs` binds — in that language's own
# locale directory, which is why this is a script and not a `custom_target`'s
# `install_dir`: the built file's name is the language, the installed one is the domain.
set -eu

localedir=$1
shift

for catalog in "$@"; do
  language=$(basename "$catalog" .mo)
  install -Dm644 "$catalog" \
    "${MESON_INSTALL_DESTDIR_PREFIX:?}/$localedir/$language/LC_MESSAGES/pixlay.mo"
done
