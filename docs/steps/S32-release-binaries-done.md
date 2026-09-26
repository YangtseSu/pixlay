# S32 · A release publishes the binaries

**Progress**: done (2026-09-27), committed in `2bb05ec`. One thing is left and it is the human's — the
`Human` line below.

**Goal**: a pushed tag turns into a downloadable artifact, and the package built on this machine has its
own place in the same release.

**Work**

- `.github/workflows/release.yml` (new): on a `v*` tag, the project's own build (`meson setup` /
  `meson compile`) inside `archlinux:latest`, the tag checked against `meson.build`'s
  `project(version:)`, and the two binaries packed as `pixlay-<version>-linux-amd64.tar.gz` beside their
  `.sha256`, attached to that tag's GitHub Release (`gh release create`/`upload --clobber`).
  **amd64 only, and the binaries only**: no GitHub runner has an aarch64 Arch userland, and the Arch
  package is built on a machine — which is where the `Human` line's work happens.
- `packaging/arch/PKGBUILD`: `arch=('x86_64' 'aarch64')` (the `aarch64` half is built on Arch Linux ARM),
  and `url=` is the repository, `https://github.com/YangtseSu/pixlay`.
- `AGENTS.md` "AUR discipline": the four steps of a release, and what CI does and does not build.
  `docs/CONTRACT.md` §10 carries the same shape.

**Verified** (2026-09-27, on this machine, the release job's own steps): `meson setup`/`meson compile`
produced `build/crates/pixlay` and `build/crates/pixlay-render` (1 m 25 s); the tag/version check passed;
the tar and `sha256sum` made a 78 MB asset holding both executables; every `run:` block of both workflows
passes `bash -n`; the publish step's heredoc writes the notes it should; `makepkg --printsrcinfo` reports
`arch = x86_64`, `arch = aarch64` and the new `url`; `cargo test -p pixlay --test packaging` (the test
that reads the PKGBUILD) and `cargo test -p pixlay-core --test contract` are green.

**Human**: the release itself — push `v0.1.0`, `updpkgsums` the checksum the PKGBUILD's `SKIP` stands for,
`makepkg` in `packaging/arch`, upload `x86_64.pkg.tar.zst` to the same release, delete the previous
version's files under `packaging/arch/`, and upload the PKGBUILD and `.SRCINFO` to the AUR. Pending.
