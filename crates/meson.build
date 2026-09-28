# The workspace, built by cargo: the two binaries are the two `[[bin]]` targets the
# workspace produces, and they are what this application installs.
#
# The target directory is this build directory's own, so `meson compile` neither reads
# nor writes the tree's `target/` — that one belongs to a developer's `cargo build`.
# `--locked` is `Cargo.lock` (committed: the build is the lock file's)
# and `--profile release` is the profile every number this project records is measured
# in. Where cargo gets its registry from is the environment's business: a distribution
# build points `CARGO_HOME` at its sources or vendors them, with
# `CARGO_NET_OFFLINE=true` making that a rule rather than a habit.
cargo = find_program('cargo', required: true)

cargo_target_dir = meson.current_build_dir() / 'target'
cargo_options = [
  '--manifest-path', meson.project_source_root() / 'Cargo.toml',
  '--target-dir', cargo_target_dir,
  '--profile', 'release',
  '--locked',
]
cargo_env = {
  # `crates/pixlay/src/i18n.rs` reads this at compile time (`option_env!`), so the
  # installed binary looks for its catalogs under the prefix this build installs into
  # rather than the one gettext was compiled with.
  'PIXLAY_LOCALEDIR': get_option('prefix') / get_option('localedir'),
}

cargo_build = custom_target(
  'cargo-build',
  build_by_default: true,
  build_always_stale: true,
  output: 'target',
  console: true,
  env: cargo_env,
  command: [cargo, 'build', cargo_options],
)

# Each binary is copied into the build directory under its own name, which is what
# `meson install` then installs into `bindir`: a `custom_target` is installed under its
# own output name, and the workspace's output is `target/release/<name>`.
foreach binary : ['pixlay', 'pixlay-render']
  custom_target(
    binary,
    depends: cargo_build,
    build_by_default: true,
    build_always_stale: true,
    output: binary,
    install: true,
    install_dir: get_option('bindir'),
    command: ['cp', cargo_target_dir / 'release' / binary, '@OUTPUT@'],
  )
endforeach
