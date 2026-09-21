//! i18n: the strings are extractable, the catalog is committed, and the wiring
//! really translates.
//!
//! The step's criteria are machine-checkable here, and this is where they live:
//! "the set of files listed in `po/POTFILES` == `crates/pixlay/src/**/*.rs`", "the
//! `.pot` is committed with the repository", and "the interface is English when
//! the locale is missing, `C` or unknown". The last one is the child half of this
//! test: the locale is a process property, so a process that wants another one has
//! to be a different process.
//!
//! What is *not* machine-checkable, and is in `docs/HIG-REVIEW.md` instead, is
//! whether any copy missed its wrapping: the extractor cannot see a string nobody
//! called `gettext` on.

mod support;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use pixlay::i18n;

/// The repository root, resolved once so that paths can be made relative to it.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository root")
}

/// The source files of this crate, as `po/POTFILES` spells them.
fn source_files() -> BTreeSet<String> {
    let root = root();
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .expect("this crate's directory");
    let mut found = BTreeSet::new();
    let mut stack = vec![crate_dir.join("src")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("src is readable") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                found.insert(
                    path.strip_prefix(&root)
                        .expect("the path is inside the repository")
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
    }
    found
}

fn potfiles() -> BTreeSet<String> {
    std::fs::read_to_string(root().join("po/POTFILES"))
        .expect("po/POTFILES is committed")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

/// The msgids of a `.pot`, which is what has to stay in step as the code changes.
fn msgids(pot: &str) -> BTreeSet<String> {
    pot.lines()
        .filter_map(|line| {
            let rest = line
                .strip_prefix("msgid ")
                .or_else(|| line.strip_prefix("msgid_plural "))?;
            if rest == "\"\"" {
                return None;
            }
            Some(rest.trim_matches('"').to_string())
        })
        .collect()
}

#[test]
fn the_strings_are_extractable_and_the_fallback_is_english() {
    support::start();
    // The child half: a process whose locale is not `C`, to prove that a catalog
    // installed in a locale directory is really used.
    if std::env::var_os("PIXLAY_I18N_CHILD").is_some() {
        check_translation();
        return;
    }
    // `po/POTFILES` lists every source file of this crate, and nothing else.
    let sources = source_files();
    let listed = potfiles();
    assert_eq!(
        listed,
        sources,
        "po/POTFILES and crates/pixlay/src disagree:\n  only in POTFILES: {:?}\n  only in the tree: {:?}",
        listed.difference(&sources).collect::<Vec<_>>(),
        sources.difference(&listed).collect::<Vec<_>>(),
    );

    // The committed `.pot` carries every string a fresh extraction finds. The
    // comparison is on the msgids: the file's header holds a creation date, and a
    // byte comparison would fail for a reason that says nothing about the strings.
    let extracted = run_xgettext();
    let committed =
        std::fs::read_to_string(root().join("po/pixlay.pot")).expect("po/pixlay.pot is committed");
    let fresh = msgids(&extracted);
    let saved = msgids(&committed);
    assert!(
        !fresh.is_empty(),
        "xgettext found no strings at all, which means it was not looking at the sources"
    );
    assert_eq!(
        saved,
        fresh,
        "po/pixlay.pot is out of date:\n  missing: {:?}\n  stale: {:?}\n\
         regenerate it with:\n  xgettext --language=Rust -f po/POTFILES -o po/pixlay.pot",
        fresh.difference(&saved).collect::<Vec<_>>(),
        saved.difference(&fresh).collect::<Vec<_>>(),
    );

    // The other half of the wiring: a catalog in a locale directory is found and
    // used. Where the environment has no language at all there is nothing to
    // translate into, and that is stated rather than silently skipped.
    match test_language() {
        Some(language) => {
            let dir = support::out_dir().join("locale");
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join(&language).join("LC_MESSAGES"))
                .expect("the locale tree can be created");
            write_mo(
                &dir.join(&language).join("LC_MESSAGES/pixlay.mo"),
                &[("Export the collage", "EXPORT")],
            );
            let output = Command::new(std::env::current_exe().expect("the test binary path"))
                .arg("--exact")
                .arg("the_strings_are_extractable_and_the_fallback_is_english")
                .arg("--nocapture")
                .env("PIXLAY_I18N_CHILD", "1")
                .env("PIXLAY_LOCALE_DIR", &dir)
                .env("LANG", format!("{language}.UTF-8"))
                .env("LC_ALL", format!("{language}.UTF-8"))
                .output()
                .expect("the child test runs");
            assert!(
                output.status.success(),
                "a catalog in {language} was not used:\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        None => eprintln!(
            "no language in this environment: the translation half of this check was not run"
        ),
    }

    // And with no catalog at all, the source strings are what the user sees. That
    // is gettext's own fallback, which is why this step ships no `.po`.
    assert_eq!(
        i18n::gettext("Export the collage"),
        "Export the collage",
        "an unbound domain must return the msgid (the English source string)"
    );
}

/// The language this environment can translate into, if it has one.
fn test_language() -> Option<String> {
    // `C` and `POSIX` are not languages; anything else names a locale the C
    // library has, since the session is running under it.
    let locale = std::env::var("LANG").ok()?;
    // `gettext` looks in `<dir>/<locale>/LC_MESSAGES`, where `<locale>` is the
    // locale name without its codeset and modifier (`zh_CN.UTF-8` → `zh_CN`) —
    // keeping the territory matters, since the language-only name is a second,
    // implementation-specific fallback.
    let language = locale.split(['.', '@']).next()?.to_string();
    if language.is_empty() || language == "C" || language == "POSIX" {
        return None;
    }
    Some(language)
}

/// The child half: a locale directory with a catalog in it is really consulted.
fn check_translation() {
    let dir = std::env::var("PIXLAY_LOCALE_DIR").expect("the child is given a locale directory");
    i18n::init();
    assert!(
        i18n::bind(&dir),
        "the domain can be bound to the test catalog"
    );
    assert_eq!(
        i18n::gettext("Export the collage"),
        "EXPORT",
        "the installed catalog has to be used"
    );
    assert_eq!(
        i18n::gettext("Untranslated string"),
        "Untranslated string",
        "and a string the catalog does not carry falls back to English"
    );
}

fn run_xgettext() -> String {
    let output = Command::new("xgettext")
        .current_dir(root())
        .args([
            "--language=Rust",
            "--from-code=UTF-8",
            "--package-name=pixlay",
            "-o",
            "-",
            "-f",
            "po/POTFILES",
        ])
        .output()
        .expect("xgettext (gettext-tools) is needed to check the catalog");
    assert!(
        output.status.success(),
        "xgettext failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("xgettext writes UTF-8")
}

/// A minimal `.mo` writer: the format is small and writing it here keeps the test
/// free of a build-time dependency on `msgfmt`.
fn write_mo(path: &Path, entries: &[(&str, &str)]) {
    let count = entries.len() as u32;
    let header = 28u32 + 16 * count;
    let mut originals = Vec::new();
    let mut translations = Vec::new();
    let mut offset = header;
    for (original, _) in entries {
        originals.push((original.len() as u32, offset));
        offset += original.len() as u32 + 1;
    }
    for (_, translated) in entries {
        translations.push((translated.len() as u32, offset));
        offset += translated.len() as u32 + 1;
    }
    let mut bytes = Vec::new();
    for value in [
        0x9504_12deu32,
        0,
        count,
        28,
        28 + 8 * count,
        0,
        28 + 16 * count,
    ] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    for (length, start) in originals.iter().chain(translations.iter()) {
        bytes.extend_from_slice(&length.to_le_bytes());
        bytes.extend_from_slice(&start.to_le_bytes());
    }
    for (original, _) in entries {
        bytes.extend_from_slice(original.as_bytes());
        bytes.push(0);
    }
    for (_, translated) in entries {
        bytes.extend_from_slice(translated.as_bytes());
        bytes.push(0);
    }
    std::fs::write(path, bytes).expect("the catalog can be written");
}
