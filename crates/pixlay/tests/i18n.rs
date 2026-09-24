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

use std::collections::{BTreeMap, BTreeSet};
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

/// One message of a `.pot`: its id, and the source files its references name.
#[derive(Default, PartialEq, Eq, Debug)]
struct Message {
    /// The msgid as a `String` — a multiline entry's continuation lines
    /// concatenated, which is what makes a changed or removed long description
    /// visible to the freshness check (PIX-026, S15i). `msgid_plural` is a second
    /// message, because it is a second string a translator has to write.
    id: String,
    /// The files the entry's `#:` references name, without their line numbers:
    /// the line a string sits on changes with every edit above it, and a check
    /// that compared those would fail for a reason that says nothing about the
    /// catalog. Which *file* a message lives in is a fact that does not churn.
    files: BTreeSet<String>,
}

/// The messages of a `.pot`, in file order.
///
/// A parser rather than a line filter: `xgettext` writes a long string as `msgid
/// ""` followed by one quoted continuation per line, takes the id of a
/// `System.String`-built string from nothing at all, and emits a `#:` reference
/// block before each entry — so reading only the first line after `msgid` sees
/// neither the multiline entries nor where they come from.
fn messages(pot: &str) -> Vec<Message> {
    let mut lines = pot.lines().peekable();
    let mut files: BTreeSet<String> = BTreeSet::new();
    let mut found = Vec::new();
    while let Some(line) = lines.next() {
        if let Some(references) = line.strip_prefix("#:") {
            for reference in references.split_whitespace() {
                // `path:line`; the path may hold a colon of its own, so the split
                // is on the last one.
                let file = match reference.rsplit_once(':') {
                    Some((file, line)) if line.chars().all(|digit| digit.is_ascii_digit()) => file,
                    _ => reference,
                };
                files.insert(file.to_string());
            }
            continue;
        }
        let Some(id) = line
            .strip_prefix("msgid ")
            .or_else(|| line.strip_prefix("msgid_plural "))
        else {
            // An entry ends at the blank line between it and the next reference
            // block, which is where the references stop belonging to it.
            if line.is_empty() {
                files.clear();
            }
            continue;
        };
        let mut id = po_string(id);
        // The continuation lines: quoted literals up to the `msgstr` line.
        while let Some(next) = lines.peek() {
            if !next.starts_with('"') {
                break;
            }
            id.push_str(&po_string(next));
            lines.next();
        }
        // The header entry is `msgid ""`, and it is not a string a translator
        // translates.
        if !id.is_empty() {
            found.push(Message {
                id,
                files: files.clone(),
            });
        }
    }
    found
}

/// The value of one quoted PO string literal: the surrounding quotes off, the C
/// escapes `xgettext` writes (`\n`, `\t`, `\"`, `\\`) undone.
fn po_string(literal: &str) -> String {
    let literal = literal.trim();
    let literal = literal
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(literal);
    let mut value = String::with_capacity(literal.len());
    let mut chars = literal.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            value.push(character);
            continue;
        }
        match chars.next() {
            Some('n') => value.push('\n'),
            Some('t') => value.push('\t'),
            Some('r') => value.push('\r'),
            Some(escape) => value.push(escape),
            None => break,
        }
    }
    value
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

    // The committed `.pot` carries every string a fresh extraction finds, and the
    // entry for each one names the files it really comes from. The comparison is on
    // the messages, not on bytes: the file's header holds a creation date, and a
    // byte comparison would fail for a reason that says nothing about the strings.
    let extracted = run_xgettext();
    let committed =
        std::fs::read_to_string(root().join("po/pixlay.pot")).expect("po/pixlay.pot is committed");
    let fresh = messages(&extracted);
    let saved = messages(&committed);
    assert!(
        !fresh.is_empty(),
        "xgettext found no strings at all, which means it was not looking at the sources"
    );
    let ids = |messages: &[Message]| -> BTreeSet<String> {
        messages.iter().map(|message| message.id.clone()).collect()
    };
    let (fresh_ids, saved_ids) = (ids(&fresh), ids(&saved));
    assert_eq!(
        saved_ids,
        fresh_ids,
        "po/pixlay.pot is out of date:\n  missing: {:?}\n  stale: {:?}\n\
         regenerate it with:\n  xgettext --language=Rust --from-code=UTF-8 --package-name=pixlay \
         -f po/POTFILES -o po/pixlay.pot",
        fresh_ids.difference(&saved_ids).collect::<Vec<_>>(),
        saved_ids.difference(&fresh_ids).collect::<Vec<_>>(),
    );
    // Where each string comes from, as files rather than lines (see [`Message`]):
    // a message that moved to another source file leaves references that mislead,
    // and that is the half of the freshness question `xgettext` answers for free.
    let sources = |messages: &[Message]| -> BTreeMap<String, BTreeSet<String>> {
        messages
            .iter()
            .map(|message| (message.id.clone(), message.files.clone()))
            .collect()
    };
    let (fresh_sources, saved_sources) = (sources(&fresh), sources(&saved));
    let moved: Vec<String> = fresh_sources
        .iter()
        .filter_map(|(id, files)| {
            let saved = saved_sources.get(id)?;
            (saved != files).then(|| format!("{id:?}: {saved:?} against {files:?}"))
        })
        .collect();
    assert!(
        moved.is_empty(),
        "po/pixlay.pot's references are behind the sources (the .pot first):\n  {}",
        moved.join("\n  ")
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
