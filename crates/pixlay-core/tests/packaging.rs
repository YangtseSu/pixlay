//! Packaging (S16): the identity the package installs is the identity the code
//! and the data files carry.
//!
//! Three things can drift apart silently and none of them is visible in a build
//! that succeeds: the **app-id** (`pixlay_core::APP_ID`) against the desktop entry,
//! the icon and the metainfo — a package whose desktop file says a name its
//! icon does not have shows a blank icon, and whose metainfo id differs is not
//! the component the desktop file launches; the **version**, which lives in
//! `Cargo.toml`, in the PKGBUILD's `pkgver`, in `meson.build`'s `project()`, in
//! the metainfo's `<release>` and in the newest section of `CHANGELOG.md`; and the
//! **file names**, which the desktop file, the
//! metainfo and the MIME registration all spell out and the meson files install.
//!
//! All three are text, so all three are checked here — offline, without `makepkg`, and
//! without the validators, which `meson test` runs wherever they are installed
//! (`data/meson.build`). What this file reads is the repository as committed: the
//! templates the install generates its files from, the three `meson.build` files that
//! name every installed path, and the PKGBUILD that wraps them (`AGENTS.md`'s AUR
//! discipline: the install is the project's, and a package build runs no tests).
//!
//! It lives in `pixlay-core`, the crate with no GTK, because the identity it holds
//! together must be checkable without a display: measured 2026-09-27, the CI step that
//! ran it inside the shell's crate spent **199 s of the run** compiling the GTK stack
//! for the two strings it needs. `APP_ID` and `DOMAIN` moved here with it for the same
//! reason, and the shell's own name comes from the shell's manifest — `CARGO_PKG_NAME`
//! in this test would answer for *this* crate. The version and the license are
//! `env!`'s here as everywhere: every member inherits them from the workspace
//! (`version.workspace = true`), which is the one place they are declared.

use std::collections::BTreeMap;
use std::path::PathBuf;

use pixlay_core::{APP_ID, DOMAIN};

/// The repository root, from this crate's manifest directory.
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(path: &str) -> String {
    let path = root().join(path);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} is committed: {error}", path.display()))
}

/// A `key = "value"` line from one table of a member's manifest.
///
/// Read from that member's own manifest, and as text: this test runs in
/// `pixlay-core`, so cargo's `CARGO_PKG_NAME` answers for *this* crate, while the
/// name the package installs is the shell's — and the second binary's is the CLI's
/// `[[bin]]`, which is not the CLI's package name. `marker` is the table header
/// verbatim (`[package]`, `[[bin]]`).
fn manifest_value(path: &str, marker: &str, key: &str) -> String {
    let manifest = read(path);
    let table = manifest
        .split(marker)
        .nth(1)
        .unwrap_or_else(|| panic!("{path} declares {marker}"));
    let prefix = format!("{key} = ");
    table
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .unwrap_or_else(|| panic!("{path}'s {marker} declares {key}"))
        .trim()
        .trim_matches('"')
        .to_string()
}

/// The file without its XML comments, **every remaining byte at its own offset**.
///
/// A comment is not data, and the comments here quote the very tags the checks
/// below look for (`<id>`) — so a search that read them would answer with a
/// sentence from a comment and pass for the wrong reason. A comment's bytes
/// become spaces instead of going away, because one check below asks where an
/// element starts **in the file as it is committed**: an icon's first bytes are
/// what the desktop sniffs.
fn without_comments(text: &str) -> String {
    let mut bytes = text.as_bytes().to_vec();
    let mut from = 0;
    while let Some(start) = text[from..].find("<!--") {
        let start = from + start;
        let end = match text[start..].find("-->") {
            Some(end) => start + end + "-->".len(),
            None => text.len(),
        };
        for byte in &mut bytes[start..end] {
            if *byte != b'\n' {
                *byte = b' ';
            }
        }
        from = end;
    }
    String::from_utf8(bytes).expect("only ASCII bytes were replaced")
}

/// The `[Desktop Entry]` group's keys, as `key -> value`.
///
/// A parser and not a `grep`, because the value is what matters and the file has
/// a group header and a `Name[lang]` form per translation: the entry's own keys
/// are the ones with no bracket in the key.
fn desktop_keys(text: &str) -> BTreeMap<String, String> {
    let mut keys = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('[') || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            keys.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    keys
}

/// Where `<tag>` starts, as an element of its own: `<release` must not answer
/// with `<releases>`.
fn opening(text: &str, tag: &str) -> usize {
    let needle = format!("<{tag}");
    let mut from = 0;
    while let Some(at) = text[from..].find(&needle) {
        let at = from + at;
        match text[at + needle.len()..].chars().next() {
            Some(' ') | Some('\t') | Some('>') | Some('/') => return at,
            _ => from = at + needle.len(),
        }
    }
    panic!("the data files have a <{tag}> element")
}

/// The text of the first `<tag>...</tag>` (an opening tag with attributes is
/// read the same way: everything up to its `>` is the tag, not the text).
fn element(text: &str, tag: &str) -> String {
    let rest = &text[opening(text, tag)..];
    let rest = &rest[rest.find('>').expect("<tag> has an end") + 1..];
    let close = format!("</{tag}>");
    let end = rest
        .find(&close)
        .unwrap_or_else(|| panic!("<{tag}> is closed"));
    rest[..end].trim().to_string()
}

/// One attribute of the first `<tag ...>` element, e.g. `<launchable type="…">`.
fn attribute(text: &str, tag: &str, name: &str) -> String {
    let start = opening(text, tag);
    let rest = &text[start..];
    let element = &rest[..rest.find('>').expect("<tag> has an end")];
    let needle = format!("{name}=\"");
    let at = element
        .find(&needle)
        .unwrap_or_else(|| panic!("<{tag}> carries {name}"))
        + needle.len();
    element[at..]
        .split('"')
        .next()
        .expect("the attribute is closed")
        .to_string()
}

/// `CHANGELOG.md`'s newest *released* version and its date: the first `## [x.y.z]`
/// heading, skipping `## [Unreleased]` — Keep a Changelog's own shape, which the file
/// states it follows.
fn newest_release(text: &str) -> (String, Option<String>) {
    for line in text.lines() {
        let Some(heading) = line.strip_prefix("## [") else {
            continue;
        };
        let (version, rest) = heading
            .split_once(']')
            .unwrap_or_else(|| panic!("a changelog heading closes its bracket: {line:?}"));
        if version == "Unreleased" {
            continue;
        }
        let date = rest
            .strip_prefix(" - ")
            .map(|date| date.trim().to_string())
            .filter(|date| !date.is_empty());
        return (version.to_string(), date);
    }
    panic!("CHANGELOG.md has a released version's heading, `## [x.y.z] - <date>`");
}

/// A bash assignment at the start of a line: `name=value` or `name=(a b c)`.
///
/// The PKGBUILD's lists are written one line each, and this is a read of that
/// text rather than a shell: a list that grows a second line makes this return
/// `None`, and the assertions below then fail with their own message instead of
/// pretending the value was checked.
fn assignment(pkgbuild: &str, name: &str) -> Option<String> {
    for line in pkgbuild.lines() {
        let Some(rest) = line.strip_prefix(&format!("{name}=")) else {
            continue;
        };
        let value = rest
            .strip_prefix('(')
            .and_then(|rest| rest.strip_suffix(')'))
            .unwrap_or(rest);
        return Some(value.to_string());
    }
    None
}

/// Whether a bash list assignment carries `word` as one of its entries.
fn lists(pkgbuild: &str, name: &str, word: &str) -> bool {
    assignment(pkgbuild, name)
        .unwrap_or_else(|| panic!("the PKGBUILD assigns {name} on one line"))
        .split_whitespace()
        .any(|entry| entry.trim_matches(['\'', '"']) == word)
}

#[test]
fn the_package_ships_the_identity_the_code_declares() {
    let app = APP_ID;
    let shell = manifest_value("crates/pixlay/Cargo.toml", "[package]", "name");
    let desktop = read(&format!("data/{app}.desktop.in"));
    let metainfo = without_comments(&read(&format!("data/{app}.metainfo.xml.in")));
    let mime = without_comments(&read(&format!("data/{app}.mime.xml")));
    let pkgbuild = read("packaging/arch/PKGBUILD");

    // --- the desktop entry -------------------------------------------------
    // It is the template the package generates the installed file from, so
    // `msgfmt --desktop` reads this text: the keys are the plain ones, and the
    // `[lang]` forms appear only in the generated file.
    let keys = desktop_keys(&desktop);
    let key = |name: &str| -> String {
        keys.get(name)
            .unwrap_or_else(|| panic!("the desktop entry has {name}"))
            .clone()
    };
    assert_eq!(key("Type"), "Application");
    assert_eq!(key("Name"), "Pixlay");
    assert_eq!(
        key("Icon"),
        app,
        "Icon= has to name the installed icon exactly: the icon tree is \
         hicolor/{{scalable,symbolic}}/apps/<Icon>.svg",
    );
    assert_eq!(
        key("Exec"),
        "pixlay %F",
        "the application takes files (`HANDLES_OPEN`, and `pixlay a.jpg b.jpg …` opens them in \
         argument order), so the entry has to pass them on — one at a time or as a group",
    );
    assert_eq!(key("Terminal"), "false");
    for category in ["Graphics", "Photography"] {
        assert!(
            key("Categories").contains(&format!("{category};")),
            "a desktop entry's categories are semicolon-terminated; {category} is missing from {:?}",
            key("Categories"),
        );
    }

    // --- the metainfo ------------------------------------------------------
    // AppStream finds a component by its file name and checks the `<id>` in it,
    // so both are the app-id, and the `<launchable>` is the desktop file the
    // package installs beside it.
    assert_eq!(element(&metainfo, "id"), app);
    assert_eq!(element(&metainfo, "name"), "Pixlay");
    assert_eq!(element(&metainfo, "launchable"), format!("{app}.desktop"));
    assert_eq!(attribute(&metainfo, "launchable", "type"), "desktop-id");
    assert_eq!(
        element(&metainfo, "project_license"),
        env!("CARGO_PKG_LICENSE")
    );
    assert_eq!(
        element(&metainfo, "translation"),
        DOMAIN,
        "a <translation type=\"gettext\"> names the gettext domain, which is the one the shell binds",
    );
    // The version is the workspace's, and a release that forgets one of the two
    // is a package that installs a binary claiming another version.
    assert_eq!(
        attribute(&metainfo, "release", "version"),
        env!("CARGO_PKG_VERSION"),
    );
    // The same sentence in both files: AppStream shows the metainfo's summary
    // where the desktop file's Comment is not read, and it is the same sentence.
    assert_eq!(
        element(&metainfo, "summary"),
        key("Comment"),
        "the desktop entry's Comment and the metainfo's summary are one sentence",
    );
    assert_eq!(
        element(&metainfo, "binary"),
        shell,
        "the metainfo's <provides><binary> is the installed binary's name",
    );

    // --- the changelog -----------------------------------------------------
    // The release's own notes. Its newest section is the workspace's version, so a
    // version bump that forgets its section fails here rather than at the tag — and the
    // bump and the tag are the same day's work (`AGENTS.md`, "AUR discipline").
    let (version, date) = newest_release(&read("CHANGELOG.md"));
    assert_eq!(
        version,
        env!("CARGO_PKG_VERSION"),
        "CHANGELOG.md's newest section is the workspace's version",
    );
    let date = date.expect("the newest release's section carries its date");
    let bytes = date.as_bytes();
    assert!(
        bytes.len() == 10
            && bytes[4] == b'-'
            && bytes[7] == b'-'
            && bytes
                .iter()
                .enumerate()
                .all(|(at, byte)| at == 4 || at == 7 || byte.is_ascii_digit()),
        "a release's date is ISO 8601 (YYYY-MM-DD): {date:?}",
    );

    // --- the `.pixlay` type -----------------------------------------------
    // The registration and the desktop file have to agree, or a double click on
    // a project finds no handler.
    let media_type = attribute(&mime, "mime-type", "type");
    assert_eq!(
        media_type, "application/x-pixlay",
        "the project file is a private JSON type; its name is what the mime glob and the desktop \
         entry both spell out",
    );
    assert!(
        mime.contains("<glob pattern=\"*.pixlay\"/>"),
        "the type is recognised by its extension",
    );
    assert_eq!(
        attribute(&mime, "icon", "name"),
        app,
        "the icon a file manager shows for a project is the app's own",
    );
    assert!(
        key("MimeType").contains(&format!("{media_type};")),
        "the desktop entry claims to handle the type the mime registration defines",
    );

    // --- the icons ---------------------------------------------------------
    // The names are the app-id (the desktop entry's Icon= is the same string),
    // and the sizes are the GNOME HIG's: an app icon is drawn on a 128x128
    // canvas (https://developer.gnome.org/hig/guidelines/app-icons) and the
    // symbolic variant on the 16x16 grid
    // (https://developer.gnome.org/hig/guidelines/ui-icons).
    let scalable = without_comments(&read(&format!(
        "data/icons/hicolor/scalable/apps/{app}.svg"
    )));
    let symbolic = without_comments(&read(&format!(
        "data/icons/hicolor/symbolic/apps/{app}-symbolic.svg"
    )));

    // And `<svg` has to start inside the file's first **256 bytes**. The desktop
    // loads an icon file by sniffing its bytes — gnome-shell's `St` through
    // `gdk_pixbuf_new_from_stream_at_scale` and GTK4 (this application's own About
    // dialog) through `gdk_pixbuf_new_from_file_at_size` — and gdk-pixbuf recognises
    // an SVG only when `<svg` starts within that window: past it the loader answers
    // "Unrecognized image file format", which is a blank icon in the shell's app grid
    // and in About, with only `gnome-shell[…] Could not load a pixbuf from icon
    // theme.` to say so. Measured 2026-09-27 by loading this tree's icons at 128 px:
    // `<svg` at byte 255 loads and at byte 256 does not, and both files' explanatory
    // comments had pushed it to 735 and 547 — the files carry no comments any more, and
    // the drawing's own rationale is where it is read from: the two rows in
    // `docs/HIG-REVIEW.md` §1.
    for (name, icon) in [
        ("the app icon", &scalable),
        ("the symbolic icon", &symbolic),
    ] {
        let at = icon
            .find("<svg")
            .expect("an icon file has an <svg> element");
        assert!(
            at < 256,
            "{name}: its `<svg` starts at byte {at}, outside the 256-byte window in which \
             gdk-pixbuf recognises an SVG — an icon the desktop cannot load",
        );
    }

    assert!(
        scalable.contains("width=\"128\"") && scalable.contains("height=\"128\""),
        "the app icon is drawn on the HIG's 128x128 canvas",
    );
    assert!(
        symbolic.contains("width=\"16\"") && symbolic.contains("height=\"16\""),
        "the symbolic icon is drawn on the 16x16 grid",
    );

    // --- the package -------------------------------------------------------
    assert_eq!(
        assignment(&pkgbuild, "pkgname").as_deref(),
        Some(shell.as_str())
    );
    assert_eq!(
        assignment(&pkgbuild, "pkgver").as_deref(),
        Some(env!("CARGO_PKG_VERSION")),
        "the package's version is the workspace's, or the installed binary reports another one",
    );
    let source = assignment(&pkgbuild, "source").expect("the PKGBUILD names its source");
    assert!(
        source.contains("refs/tags/v$pkgver.tar.gz"),
        "the source is the release tag's tarball, built from pkgver: {source:?}",
    );
    assert!(
        lists(&pkgbuild, "license", env!("CARGO_PKG_LICENSE")),
        "the package's license is the manifest's SPDX ({})",
        env!("CARGO_PKG_LICENSE"),
    );
    assert!(
        !assignment(&pkgbuild, "arch")
            .expect("arch is set")
            .is_empty()
    );
    // The decoding backend S4 measured is a linked library, so it is a runtime
    // dependency and not only a build one.
    for dependency in ["gtk4", "libadwaita", "glycin"] {
        assert!(
            lists(&pkgbuild, "depends", dependency),
            "{dependency} is a runtime dependency",
        );
    }
    // And what the package installs is the project's own install definition (S31):
    // `meson.build` and the two files under it name every installed path, and the
    // PKGBUILD wraps them rather than repeating them. So the identity is checked where
    // it can drift silently: the meson files against the data files' own names.
    let meson = read("meson.build");
    let crates_meson = read("crates/meson.build");
    let data_meson = read("data/meson.build");
    let po_meson = read("po/meson.build");
    let install_script = read("po/install-catalogs.sh");

    assert!(
        meson.contains(&format!("version: '{}'", env!("CARGO_PKG_VERSION"))),
        "meson.build names the workspace's version (meson cannot read Cargo.toml, so it is \
         written twice and a release has to move both)",
    );
    assert!(
        meson.contains(&format!("license: '{}'", env!("CARGO_PKG_LICENSE"))),
        "and the manifest's SPDX license",
    );
    // Every runtime dependency the package names is one meson has to find by name,
    // with the version the bindings need (`AGENTS.md`, the dependency registry).
    for (package, pc) in [
        ("gtk4", "gtk4"),
        ("libadwaita", "libadwaita-1"),
        ("glycin", "glycin-2"),
    ] {
        assert!(
            meson.contains(&format!("dependency('{pc}'")),
            "meson.build declares {pc}, which is what depends=('{package}') links",
        );
    }
    // The two binaries: `crates/meson.build` copies what the workspace builds into the
    // build directory, under the names `bindir` gets.
    for binary in [shell.as_str(), "pixlay-render"] {
        assert!(
            crates_meson.contains(&format!("'{binary}'")),
            "crates/meson.build installs {binary} into the bindir",
        );
    }
    assert!(
        crates_meson.contains("get_option('bindir')"),
        "and installs them where the desktop entry's Exec= looks for them",
    );
    // The data files, each named from the app-id this crate declares — which is the
    // drift that matters: a file renamed under the desktop entry's `Icon=` or the
    // metainfo's `<id>` is a package that installs files nothing finds.
    for suffix in [".desktop", ".metainfo.xml", ".svg", "-symbolic.svg", ".xml"] {
        assert!(
            data_meson.contains(&format!("app_id + '{suffix}'")),
            "data/meson.build installs app_id + '{suffix}'",
        );
    }
    for directory in [
        "applications",
        "metainfo",
        "icons",
        "hicolor",
        "scalable",
        "symbolic",
        "mime",
        "packages",
    ] {
        assert!(
            data_meson.contains(&format!("'{directory}'")),
            "data/meson.build installs into {directory}/",
        );
    }
    // The catalogs: one per language `LINGUAS` lists, installed as the domain the
    // shell binds, under the prefix `localedir` names (which is also what
    // `crates/meson.build` passes as `PIXLAY_LOCALEDIR`, the value `i18n.rs` reads).
    assert!(
        po_meson.contains("'LINGUAS'") && po_meson.contains("msgfmt"),
        "po/meson.build compiles the catalogs `LINGUAS` lists",
    );
    assert!(
        install_script.contains(&format!("LC_MESSAGES/{DOMAIN}.mo")),
        "the catalogs are installed as the domain the shell binds",
    );
    assert!(
        crates_meson.contains("PIXLAY_LOCALEDIR"),
        "the build passes the prefix's localedir into the binary",
    );
    assert_eq!(
        manifest_value("crates/pixlay-cli/Cargo.toml", "[[bin]]", "name"),
        "pixlay-render",
        "the second binary the package installs is the CLI's own name",
    );

    // --- the PKGBUILD wraps that install, and tests nothing ----------------
    // What a package has to get right is building and packaging (`AGENTS.md`, "AUR
    // discipline"): the suite is the verification entry's, and the PKGBUILD names no
    // `check()` at all.
    assert!(
        pkgbuild.contains("cargo vendor") && pkgbuild.contains("CARGO_NET_OFFLINE=true"),
        "the build runs offline against the vendored registry",
    );
    for step in ["meson setup", "meson compile", "meson install"] {
        assert!(
            pkgbuild.contains(step),
            "the PKGBUILD runs the project's own build and install: `{step}`",
        );
    }
    assert!(
        pkgbuild.contains("--destdir \"$pkgdir\""),
        "and installs into the package root",
    );
    assert!(
        !pkgbuild.contains("check()"),
        "the PKGBUILD runs no tests: the suite is CI's and the entry's (ruled 2026-09-26)",
    );
    for dependency in ["cargo", "rust", "meson", "gettext"] {
        assert!(
            lists(&pkgbuild, "makedepends", dependency),
            "{dependency} is a makedepend of the build meson drives",
        );
    }
}
