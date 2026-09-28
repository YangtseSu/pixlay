// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! i18n: the strings are extractable, every catalog that ships is complete, and the
//! wiring really translates.
//!
//! The step's criteria are machine-checkable here, and this is where they live:
//! "the set of files listed in `po/POTFILES` == `crates/pixlay/src/**/*.rs`", "the
//! set in `po/POTFILES.data` == the translatable data files" (S16: the desktop
//! entry and the AppStream metainfo), "the `.pot` is committed with the
//! repository" — as `po/extract-pot` extracts it, which is the one implementation
//! of the three xgettext passes that build it — and "the interface is English when
//! the locale is missing, `C` or unknown". The last one is the child half of this
//! test: the locale is a process property, so a process that wants another one has
//! to be a different process.
//!
//! S16 left `po/LINGUAS` empty and shipped no catalog; the language pack added on
//! 2026-09-27 (`po/zh_CN.po`) is what makes the other half of the same criteria
//! checkable, and it is checked per language: a listed catalog carries the
//! template's whole message set, nothing `fuzzy`, the `{}` placeholders of every
//! entry, and `msgfmt --check` compiles it — and the compiled catalog is what
//! `gettext` then answers with, in a child whose locale selects it.
//!
//! What is *not* machine-checkable, and is in `docs/HIG-REVIEW.md` instead, is
//! whether any copy missed its wrapping: the extractor cannot see a string nobody
//! called `gettext` on — and whether a translation reads well.

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

/// One of the two lists `po/extract-pot` extracts from, as it spells its files.
fn listed(list: &str) -> BTreeSet<String> {
    let path = root().join("po").join(list);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} is committed: {error}", path.display()))
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

/// The data files that carry translatable strings: the templates `msgfmt`
/// generates the installed desktop entry and metainfo from. Their extension is
/// what `po/extract-pot` reads them by (`--language=Desktop` for the first, the
/// AppStream ITS rules for the second), so it is also what this walk looks for —
/// a template with a name neither pass would pick up is a template whose strings
/// no translator sees, and the set comparison below is what catches it.
fn data_files() -> BTreeSet<String> {
    fn translatable(path: &Path) -> bool {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        name.ends_with(".desktop.in") || name.ends_with(".metainfo.xml.in")
    }
    let root = root();
    let mut found = BTreeSet::new();
    let mut stack = vec![root.join("data")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("data is readable") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if translatable(&path) {
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
fn the_strings_are_extractable_the_catalogs_are_complete_and_the_fallback_is_english() {
    support::start();
    // The two child halves: a process whose locale is not `C`, because a catalog
    // installed in a locale directory is selection by locale — a property of the
    // process, not of this call.
    if std::env::var_os("PIXLAY_I18N_CHILD").is_some() {
        check_translation();
        return;
    }
    if std::env::var_os("PIXLAY_I18N_CATALOG_CHILD").is_some() {
        check_catalog_translation();
        return;
    }
    // `po/POTFILES` lists every source file of this crate, and nothing else.
    let sources = source_files();
    let listed_sources = listed("POTFILES");
    assert_eq!(
        listed_sources,
        sources,
        "po/POTFILES and crates/pixlay/src disagree:\n  only in POTFILES: {:?}\n  only in the tree: {:?}",
        listed_sources.difference(&sources).collect::<Vec<_>>(),
        sources.difference(&listed_sources).collect::<Vec<_>>(),
    );
    // And `po/POTFILES.data` is the same statement about the translated data
    // files: the desktop entry and the metainfo (S16).
    let data = data_files();
    let listed_data = listed("POTFILES.data");
    assert_eq!(
        listed_data,
        data,
        "po/POTFILES.data and data/ disagree:\n  only in POTFILES.data: {:?}\n  only in the tree: {:?}",
        listed_data.difference(&data).collect::<Vec<_>>(),
        data.difference(&listed_data).collect::<Vec<_>>(),
    );

    // The committed `.pot` carries every string a fresh extraction finds, and the
    // entry for each one names the files it really comes from. The comparison is on
    // the messages, not on bytes: the file's header holds a creation date, and a
    // byte comparison would fail for a reason that says nothing about the strings.
    let extracted = run_extract_pot(&support::out_dir().join("pixlay.pot"));
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
         regenerate it with:\n  po/extract-pot",
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
                &[(PROBE, "EXPORT")],
            );
            let output = Command::new(std::env::current_exe().expect("the test binary path"))
                .arg("--exact")
                .arg(TEST)
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

    // The catalogs the repository ships (`po/LINGUAS`): each one is the template's
    // message set translated, and each one compiles with the command the install
    // runs. A `.po` that has fallen behind the template is the failure no build can
    // see — `msgfmt` compiles a stale catalog happily.
    for language in listed("LINGUAS") {
        check_catalog(&language);
    }

    // And with no catalog bound, the source strings are what the user sees: that is
    // gettext's own fallback, and it is asserted against a directory that holds no
    // catalog rather than against a machine that has none — an installed package
    // puts `zh_CN`'s exactly there.
    let empty = support::out_dir().join("no-catalog");
    let _ = std::fs::remove_dir_all(&empty);
    std::fs::create_dir_all(&empty).expect("the empty locale tree can be created");
    assert!(
        i18n::bind(&empty),
        "the domain can be bound to an empty directory"
    );
    assert_eq!(
        i18n::gettext("Export the collage"),
        "Export the collage",
        "an unbound domain must return the msgid (the English source string)"
    );
}

/// The test's own name, so that a child can be asked to run exactly it.
const TEST: &str =
    "the_strings_are_extractable_the_catalogs_are_complete_and_the_fallback_is_english";

/// The one string every catalog the shell ships carries: the check reads a
/// language's own words back through it, so it needs no word of its own.
const PROBE: &str = "Export the collage";

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

/// The child half of [`check_translation`]: a locale directory with a catalog in it
/// is really consulted.
fn check_translation() {
    let dir = std::env::var("PIXLAY_LOCALE_DIR").expect("the child is given a locale directory");
    i18n::init();
    assert!(
        i18n::bind(&dir),
        "the domain can be bound to the test catalog"
    );
    assert_eq!(
        i18n::gettext(PROBE),
        "EXPORT",
        "the installed catalog has to be used"
    );
    assert_eq!(
        i18n::gettext("Untranslated string"),
        "Untranslated string",
        "and a string the catalog does not carry falls back to English"
    );
}

/// One entry of a `.po`: its ids (a plural message has two), its translation, and
/// whether it is flagged `fuzzy`.
struct CatalogEntry {
    ids: Vec<String>,
    translation: Option<String>,
    fuzzy: bool,
}

/// The entries of a `.po`, in file order.
fn catalog_entries(po: &str) -> Vec<CatalogEntry> {
    let mut entries: Vec<CatalogEntry> = Vec::new();
    let mut fuzzy = false;
    let mut lines = po.lines().peekable();
    while let Some(line) = lines.next() {
        if let Some(flags) = line.strip_prefix("#,") {
            // `#, fuzzy` is the flag that makes `msgfmt` drop the entry from the
            // catalog it builds: a translated string nobody would ever see.
            fuzzy = flags.split(',').any(|flag| flag.trim() == "fuzzy");
            continue;
        }
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let (keyword, literal) = line.split_once(' ').unwrap_or((line, ""));
        let mut value = po_string(literal);
        while let Some(next) = lines.peek() {
            if !next.starts_with('"') {
                break;
            }
            value.push_str(&po_string(next));
            lines.next();
        }
        match keyword {
            "msgid" => {
                entries.push(CatalogEntry {
                    ids: vec![value],
                    translation: None,
                    fuzzy,
                });
                fuzzy = false;
            }
            "msgid_plural" => {
                if let Some(entry) = entries.last_mut() {
                    entry.ids.push(value);
                }
            }
            keyword if keyword.starts_with("msgstr") => {
                if let Some(entry) = entries.last_mut() {
                    entry.translation = Some(value);
                }
            }
            _ => {}
        }
    }
    // The header is `msgid ""`, and it is not a string a translator writes.
    entries.retain(|entry| entry.ids.first().is_some_and(|id| !id.is_empty()));
    entries
}

/// One language's catalog: the template's message set translated, no `fuzzy`, the
/// `{}` placeholders kept, compiled by the tool the install uses — and then that
/// compiled catalog really answering.
fn check_catalog(language: &str) {
    let root = root();
    let path = root.join("po").join(format!("{language}.po"));
    let po = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} is committed: {error}", path.display()));
    let template =
        std::fs::read_to_string(root.join("po/pixlay.pot")).expect("po/pixlay.pot is committed");
    let template_ids: BTreeSet<String> = messages(&template)
        .into_iter()
        .map(|message| message.id)
        .collect();
    let entries = catalog_entries(&po);
    let ids: BTreeSet<String> = entries
        .iter()
        .flat_map(|entry| entry.ids.iter().cloned())
        .collect();
    assert_eq!(
        ids,
        template_ids,
        "{} is out of date with the template:\n  missing: {:?}\n  stale: {:?}\n\
         bring it up to date with `msgmerge -U {} po/pixlay.pot`, then translate what it marks",
        path.display(),
        template_ids.difference(&ids).collect::<Vec<_>>(),
        ids.difference(&template_ids).collect::<Vec<_>>(),
        path.display(),
    );
    for entry in &entries {
        let id = &entry.ids[0];
        assert!(
            !entry.fuzzy,
            "{}: {id:?} is fuzzy, so a build would drop it",
            path.display()
        );
        let translated = entry
            .translation
            .as_deref()
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| panic!("{}: {id:?} is not translated", path.display()));
        for form in &entry.ids {
            assert_eq!(
                form.matches("{}").count(),
                translated.matches("{}").count(),
                "{}: {id:?} has to keep every placeholder of its msgid — the values are \
                 substituted into the translation, left to right",
                path.display()
            );
        }
    }
    // The install's own command (`po/meson.build`), on the file that ships.
    let dir = support::out_dir().join("catalogs");
    std::fs::create_dir_all(&dir).expect("the catalog directory can be created");
    let mo = dir.join(format!("{language}.mo"));
    let output = Command::new("msgfmt")
        .args(["--check", "-o"])
        .arg(&mo)
        .arg(&path)
        .output()
        .expect("msgfmt runs (the install needs it too: `po/meson.build`)");
    assert!(
        output.status.success(),
        "msgfmt --check refused {}:\n{}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    // And the compiled catalog is the one gettext reads. Selection is by the
    // process's locale, so the file is installed under *this environment's*
    // language and the child runs in it: the directory name is a lookup path here,
    // and the words that come back are the catalog's own.
    let Some(environment) = test_language() else {
        eprintln!(
            "no language in this environment: {language}'s catalog was compiled, not read back"
        );
        return;
    };
    let lookup = dir.join("locale");
    let _ = std::fs::remove_dir_all(&lookup);
    std::fs::create_dir_all(lookup.join(&environment).join("LC_MESSAGES"))
        .expect("the locale tree can be created");
    std::fs::copy(&mo, lookup.join(&environment).join("LC_MESSAGES/pixlay.mo"))
        .expect("the compiled catalog can be installed");
    let expected = entries
        .iter()
        .find(|entry| entry.ids[0] == PROBE)
        .and_then(|entry| entry.translation.clone())
        .expect("every catalog carries the probe");
    let output = Command::new(std::env::current_exe().expect("the test binary path"))
        .arg("--exact")
        .arg(TEST)
        .arg("--nocapture")
        .env("PIXLAY_I18N_CATALOG_CHILD", "1")
        .env("PIXLAY_LOCALE_DIR", &lookup)
        .env("PIXLAY_CATALOG_EXPECT", &expected)
        .env("LANG", format!("{environment}.UTF-8"))
        .env("LC_ALL", format!("{environment}.UTF-8"))
        .output()
        .expect("the child test runs");
    assert!(
        output.status.success(),
        "{language}'s catalog was not used:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The child half of [`check_catalog`]: the catalog that ships, compiled and
/// installed in a locale directory, is what [`i18n::gettext`] answers with.
fn check_catalog_translation() {
    let dir = std::env::var("PIXLAY_LOCALE_DIR").expect("the child is given a locale directory");
    let expected =
        std::env::var("PIXLAY_CATALOG_EXPECT").expect("the child is given the catalog's words");
    i18n::init();
    assert!(
        i18n::bind(&dir),
        "the domain can be bound to the compiled catalog"
    );
    assert_eq!(
        i18n::gettext(PROBE),
        expected,
        "the shipped catalog has to be the one gettext reads"
    );
    assert_eq!(
        i18n::gettext("Untranslated string"),
        "Untranslated string",
        "and a string the catalog does not carry falls back to English"
    );
}

/// A fresh extraction of the template, by the one implementation of it the
/// repository has: `po/extract-pot` writes `out` and this reads it back.
///
/// The script rather than three `xgettext` calls here, so the check cannot drift
/// from what a translator's `msgmerge` is handed: the passes, their languages and
/// their `--join-existing` are the script's, and this only compares its output
/// with the committed file.
fn run_extract_pot(out: &Path) -> String {
    let root = root();
    let script = root.join("po/extract-pot");
    let output = Command::new("sh")
        .current_dir(&root)
        .arg(&script)
        .arg(out)
        .output()
        .unwrap_or_else(|error| panic!("{} runs: {error}", script.display()));
    assert!(
        output.status.success(),
        "{} failed:\n{}",
        script.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::read_to_string(out).expect("the extraction wrote its template")
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
