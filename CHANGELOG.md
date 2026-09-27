# Changelog

All notable changes to Pixlay are documented in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); dates are ISO 8601, and versions are the
`vX.Y.Z` release tags.

## [Unreleased]

### Added

- **Simplified Chinese**: every string the interface shows, the desktop entry and the AppStream
  metainfo are translated (`po/zh_CN.po`, `zh_CN` in `po/LINGUAS`). English stays the source language
  and the fallback for anything a catalog does not carry, and the CLI is not translated at all.

## [0.1.1] - 2026-09-27

### Added

- Linux **arm64** binaries: every release now carries `pixlay-<version>-linux-arm64.tar.gz` with both
  binaries beside the amd64 tarball, each with its `.sha256`, and both built from the tag by
  `release.yml` (`ubuntu-26.04` and `ubuntu-26.04-arm`).

## [0.1.0] - 2026-09-27

The first release.

### Added

- The editor (`pixlay`): one window over the collage, one to nine photos — from `Add photos…`, a
  drop from the file manager onto the cell it is aimed at, `Ctrl+C` / `Ctrl+X` / `Ctrl+V` on the
  selected cell, or the command line (`pixlay a.jpg b.jpg …`, in argument order). A tenth and beyond
  is trimmed to the first nine with one report.
- 27 layouts for one to nine cells over five sheet aspects (4:3, 16:9, 1:1, 3:2, 2:3), regular and
  irregular, drawn in the layout band as sketches of their own geometry.
- Framing per cell: pan, zoom and rotation by any angle, with the clamp recomputed for the exact
  angle so the visible cell stays covered; `Ctrl+0` resets it.
- Swapping any two cells whole — photo and framing — by a drag onto the other cell, `Shift`+click,
  or `Ctrl+Shift`+arrow; one undo step.
- Undo and redo across every document edit, atomic writes, and a check before unsaved work is
  discarded; `Ctrl+S` and `Ctrl+Shift+S`.
- A canvas frame on the document: the gap between two photos, the cell corner radius and the
  backdrop colour — white, gapless and square-cornered by default, so a project written before the
  field existed renders byte-identically.
- Export as PNG or JPEG at a long edge of 1 … 30000 pixels through the platform's own save dialog
  (`Ctrl+E`); the format, the long edge and the last folder are remembered in
  `~/.config/pixlay/settings.json`.
- One renderer for preview and export, resampling in linear light with 16-bit buffers, an opaque
  backdrop, and source photos that are never written to.
- Encoding in one pass: every export carries its ICC profile, and a JPEG is 4:4:4 in its own `SOF0`.
- Dark by default, keyboard-first with every action bound (`Ctrl+?` lists the shortcuts), an
  accessible name on every control, and gettext-based i18n with English as the source language.
- `pixlay-render`, the windowless half: `render`, `probe`, `image`, `scan`, `thumb`, `templates`,
  `init`, `edit`, `save`, `hit`, `gesture` and `switch`, with `--json` and `--stats`, fixed exit
  codes (0 success, 1 usage error, 2 failure) and output unaffected by locale.
- `.pixlay` projects: JSON that embeds its own layout geometry under a `templateVersion`, so a
  project keeps its pixels when the shipped library changes, with photo paths relative to the file.
- Packaging: a meson build that installs both binaries, the desktop entry, the AppStream metainfo,
  the app icon and its symbolic variant, the `application/x-pixlay` MIME registration and the
  catalogs; an Arch PKGBUILD that wraps it; and a GitHub release carrying the amd64 binaries and
  the `x86_64` package.

[Unreleased]: https://github.com/YangtseSu/pixlay/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/YangtseSu/pixlay/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/YangtseSu/pixlay/releases/tag/v0.1.0
