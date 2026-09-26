//! Packaging (S16): the identity the package installs is the identity the code
//! and the data files carry.
//!
//! Three things can drift apart silently and none of them is visible in a build
//! that succeeds: the **app-id** (`pixlay::APP_ID`) against the desktop entry,
//! the icon and the metainfo — a package whose desktop file says a name its
//! icon does not have shows a blank icon, and whose metainfo id differs is not
//! the component the desktop file launches; the **version**, which lives in
//! `Cargo.toml`, in the PKGBUILD's `pkgver` and in the metainfo's `<release>`;
//! and the **file names**, which the desktop file, the metainfo and the MIME
//! registration all spell out and the PKGBUILD installs.
//!
//! All three are text, so all three are checked here — offline, without
//! `makepkg` and without the validators, which are the PKGBUILD's `check()`
//! (`desktop-file-validate`, `appstreamcli validate --no-net`, `msgfmt`). What
//! this file reads is the repository as committed: the templates the package
//! generates its installed files from (`AGENTS.md`'s AUR discipline, "the
//! PKGBUILD uses `--frozen --offline`").

use std::collections::BTreeMap;
use std::path::PathBuf;

/// The repository root, from this crate's manifest directory.
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(path: &str) -> String {
    let path = root().join(path);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} is committed: {error}", path.display()))
}

/// The file without its XML comments.
///
/// A comment is not data, and the comments here quote the very tags the checks
/// below look for (`<id>`) — so a search that read them would answer with a
/// sentence from a comment and pass for the wrong reason.
fn without_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        match rest[start..].find("-->") {
            Some(end) => rest = &rest[start + end + 3..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
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
    let app = pixlay::APP_ID;
    let desktop = read(&format!("data/{app}.desktop.in"));
    let metainfo = without_comments(&read(&format!("data/{app}.metainfo.xml.in")));
    let mime = without_comments(&read(&format!("data/{app}.mime.xml")));
    let pkgbuild = read("PKGBUILD");

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
        pixlay::i18n::DOMAIN,
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
        env!("CARGO_PKG_NAME"),
        "the metainfo's <provides><binary> is the installed binary's name",
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
    assert!(
        scalable.contains("width=\"128\"") && scalable.contains("height=\"128\""),
        "the app icon is drawn on the HIG's 128x128 canvas",
    );
    let symbolic = without_comments(&read(&format!(
        "data/icons/hicolor/symbolic/apps/{app}-symbolic.svg"
    )));
    assert!(
        symbolic.contains("width=\"16\"") && symbolic.contains("height=\"16\""),
        "the symbolic icon is drawn on the 16x16 grid",
    );

    // --- the package -------------------------------------------------------
    assert_eq!(
        assignment(&pkgbuild, "pkgname").as_deref(),
        Some(env!("CARGO_PKG_NAME"))
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
    // And what the package installs is what the other files name: the two
    // binaries, the app-id's desktop file, metainfo and icons, the mime
    // registration, and the locale tree the domain is bound to.
    for installed in [
        "usr/bin/pixlay",
        "usr/bin/pixlay-render",
        "usr/share/applications/org.yangtse.Pixlay.desktop",
        "usr/share/metainfo/org.yangtse.Pixlay.metainfo.xml",
        "usr/share/icons/hicolor/scalable/apps/org.yangtse.Pixlay.svg",
        "usr/share/icons/hicolor/symbolic/apps/org.yangtse.Pixlay-symbolic.svg",
        "usr/share/mime/packages/org.yangtse.Pixlay.xml",
        "usr/share/locale",
    ] {
        assert!(
            pkgbuild.contains(installed),
            "the PKGBUILD installs {installed}",
        );
    }
    assert!(
        read("crates/pixlay-cli/Cargo.toml").contains("name = \"pixlay-render\""),
        "the second binary the package installs is the CLI's own name",
    );

    // --- what `check()` needs to work where it runs ------------------------
    // S16's own requirement, and the reason it is asserted: `makepkg` runs
    // check() in a chroot with no display, so the GUI tests have to be given one —
    // and it is the compositor they are written for, not an Xvfb (`AGENTS.md`'s
    // entry: mutter when mutter is available). mutter needs no GPU node (measured
    // 2026-09-26 with `/dev/dri` hidden: `Created surfaceless renderer without
    // GPU`) but it does need a session bus, which `dbus-run-session` is; a bare
    // Xvfb is not the same session at all — with no window manager GTK frames the
    // window inside its own surface there, and every window geometry the suite
    // reads comes out 10 px smaller in each direction (1090x584 against
    // 1100x594).
    assert!(
        pkgbuild.contains("dbus-run-session"),
        "check() starts the compositor with a session bus of its own",
    );
    for dependency in ["mutter", "dbus", "mesa"] {
        assert!(
            lists(&pkgbuild, "makedepends", dependency),
            "{dependency} is a makedepend of the package whose check() starts that compositor",
        );
    }
}
