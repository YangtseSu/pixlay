//! gettext wiring: the one place a user-visible string becomes a translated one.
//!
//! The mechanism was decided before S7 (`docs/STEPS.md`, "S7 · review
//! additions"): gettext, domain `pixlay`, source language English, `.pot` +
//! `po/POTFILES` committed with the repository, and the dependency only in this
//! crate — `pixlay-core` / `-imaging` / `-render` / `-cli` keep English error
//! text that the GUI attaches as it is (`AGENTS.md`, "Language conventions").
//!
//! Two properties this module is responsible for:
//!
//! * **A missing or unknown locale falls back to English.** That is gettext's own
//!   behaviour (an unbound or empty catalog returns the msgid, and the msgid *is*
//!   the English source string), which is why this step ships no `.po` and still
//!   satisfies the criterion "with `LANG` unset, `C` or unknown, the interface is
//!   English and starts up".
//! * **The locale is set before the first string is asked for.** No `setlocale`
//!   call of ours is needed or possible (`unsafe`, and this workspace denies
//!   `unsafe_code`): measured 2026-09-21 on this machine, `g_gettext` returns the
//!   English msgid before `gtk::init()` and the translated string after it —
//!   GTK's own initialisation sets the locale, so the domain only has to be bound
//!   before the window is built.
//!
//! `po/POTFILES` lists exactly the source files of this crate, and a test in
//! `tests/i18n.rs` compares that list with the tree and re-extracts the strings
//! to check the committed `.pot`.

use std::path::PathBuf;

pub use gettextrs::{bind_textdomain_codeset, bindtextdomain, gettext, ngettext};

/// The gettext domain every string in this crate belongs to. The `.desktop` file
/// and the AppStream metainfo of S8 use the same one.
pub const DOMAIN: &str = "pixlay";

/// Where compiled catalogs (`<lang>/LC_MESSAGES/pixlay.mo`) are looked up when
/// `PIXLAY_LOCALEDIR` was not set at build time. It is the prefix an installed
/// package uses; a development build can point the domain anywhere with
/// `bindtextdomain` (the tests do exactly that).
pub const DEFAULT_LOCALE_DIR: &str = "/usr/share/locale";

/// The directory this build looks for translations in.
///
/// A compile-time override rather than a runtime one: the install prefix is fixed
/// when the binary is built, and reading it from the environment would make the
/// interface depend on how the app was launched.
pub fn locale_dir() -> PathBuf {
    PathBuf::from(option_env!("PIXLAY_LOCALEDIR").unwrap_or(DEFAULT_LOCALE_DIR))
}

/// Binds the domain, so that [`gettext`] finds `pixlay.mo` once GTK has set the
/// locale. Idempotent, and silent on a missing directory: with no catalog
/// installed every string stays English, which is the documented fallback rather
/// than an error.
pub fn init() {
    let _ = bindtextdomain(DOMAIN, locale_dir());
    let _ = textdomain(DOMAIN);
    // Without this, a non-UTF-8 locale would hand back bytes in that encoding;
    // every string in this crate is UTF-8.
    let _ = bind_textdomain_codeset(DOMAIN, "UTF-8");
}

/// A translated string with its `{}` placeholders filled in, left to right.
///
/// `format!` cannot do this job: its first argument has to be a string literal, and
/// the msgid has to keep the *literal* placeholders — that is what makes
/// `xgettext --language=Rust` tag the entry `rust-format` and what lets
/// `msgfmt --check-format` prove that a translation kept them (measured
/// 2026-09-21 on gettext-tools 1.0). So the template goes through `gettext` as it
/// is written and the values are substituted afterwards.
///
/// A template with fewer placeholders than values ignores the extras; one with
/// more leaves them visible, which is exactly the bug the format check exists to
/// catch, and a test in `tests/i18n.rs` covers both directions.
pub fn fill<T: std::fmt::Display>(template: String, values: &[T]) -> String {
    let mut out = template;
    for value in values {
        match out.find("{}") {
            Some(at) => out.replace_range(at..at + 2, &value.to_string()),
            None => break,
        }
    }
    out
}

/// Rebinds the domain to `dir`, which is what a language pack installation does
/// and what the i18n test uses to prove the wiring works at all.
pub fn bind(dir: impl Into<PathBuf>) -> bool {
    bindtextdomain(DOMAIN, dir.into()).is_ok() && textdomain(DOMAIN).is_ok()
}

pub use gettextrs::textdomain;
