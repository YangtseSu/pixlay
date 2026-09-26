# The catalogs: `LINGUAS` lists the languages, and one `msgfmt --check` per language
# compiles `po/<language>.po` into the catalog the shell binds its domain from.
#
# **Not `i18n.gettext()`**: the pot target that module brings builds one from `POTFILES`
# with a single xgettext pass and a hardcoded `_` keyword, while this project's pot comes
# from `po/extract-pot` — three passes (the shell's Rust strings, the desktop template,
# and the metainfo through gettext's AppStream ITS rules) — which
# `crates/pixlay/tests/i18n.rs` runs as well and holds to the committed `po/pixlay.pot`.
# A second extraction would quietly produce a pot missing two of the three.
#
# The `.mo` files are built here and installed by `install-catalogs.sh`, because every
# language's catalog is the same domain name (`pixlay.mo`) in a different locale
# directory: a `custom_target` is installed under its own output name, which cannot
# differ per language in one build directory.
fs = import('fs')
msgfmt = find_program('msgfmt')

catalogs = []
foreach line : fs.read('LINGUAS').split('\n')
  language = line.strip()
  if language == '' or language.startswith('#')
    continue
  endif
  catalogs += custom_target(
    language + '-catalog',
    input: language + '.po',
    output: language + '.mo',
    command: [msgfmt, '--check', '-o', '@OUTPUT@', '@INPUT@'],
    build_by_default: true,
  )
endforeach

if catalogs.length() > 0
  meson.add_install_script('install-catalogs.sh', get_option('localedir'), catalogs)
endif
