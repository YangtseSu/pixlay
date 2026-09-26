# S32 · A release publishes the binaries

**Progress**: done (2026-09-27), committed in `2bb05ec` — and **released**: `v0.1.0` is published, with
the three assets below. The one thing left is the human's AUR upload (`Human`).

**Goal**: a pushed tag turns into a downloadable artifact, and the package built on this machine has its
own place in the same release.

**Work**

- `.github/workflows/release.yml` (new): on a `v*` tag, the project's own build (`meson setup` /
  `meson compile`) inside `archlinux:latest`, the tag checked against `meson.build`'s
  `project(version:)`, and the two binaries packed as `pixlay-<version>-linux-amd64.tar.gz` beside their
  `.sha256`, attached to that tag's GitHub Release. **amd64 only, and the binaries only**: no GitHub
  runner has an aarch64 Arch userland, and the Arch package is built on a machine — which is where the
  release's package comes from.
- `packaging/arch/PKGBUILD`: `arch=('x86_64' 'aarch64')` (the `aarch64` half is built on Arch Linux ARM),
  and `url=` is the repository, `https://github.com/YangtseSu/pixlay`.
- `AGENTS.md` "AUR discipline": the four steps of a release, and what CI does and does not build.
  `docs/CONTRACT.md` §10 carries the same shape.

**Released** (2026-09-27): tag `v0.1.0` → commit `a605bca`; the repository went public the same hour.
`https://github.com/YangtseSu/pixlay/releases/tag/v0.1.0` carries

| Asset | Size | Built by |
|---|---|---|
| `pixlay-0.1.0-linux-amd64.tar.gz` | 20,385,817 B | the release workflow, in `archlinux:latest` |
| `pixlay-0.1.0-linux-amd64.tar.gz.sha256` | 98 B | the same job |
| `pixlay-0.1.0-1-x86_64.pkg.tar.zst` | 4,130,531 B | `makepkg` here, from the tag's own tarball |

The published tarball was downloaded again, its checksum verified (`sha256sum -c` → OK), and both
executables run: `pixlay-render templates` lists 27 and `pixlay-render --version` says `0.1.0`. The
PKGBUILD's `sha256sums` is `9818d3b507d2cf49442b3e01fa8d03ea4b26a7120eb4e800ce473a49832dbbf4`, computed
with `updpkgsums` over a fresh download of the tag. `packaging/arch/` holds the current version's
PKGBUILD and package and nothing else.

**What the first release taught the workflow** (both fixed in the same step, the tag re-pointed twice
before it published): `meson.build` asked for `glycin-1`, which stock Arch does not ship — the 2.x name
is the one that links, and this machine only saw the 1.x compatibility file; and `gh` inside the
container needs `git` (absent from `base-devel`) *and* `--repo` (it cannot discover a checkout there).

**Human**: the AUR upload — `/var/tmp/pixlay-aur/{PKGBUILD,.SRCINFO}` is staged from this tree (the name
`pixlay` is free on the AUR), and it wants the AUR account's own SSH key. The release itself, which this
line used to carry, is done.
