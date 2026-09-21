//! The `pixlay` binary: bind the text domain and run the window.
//!
//! The locale itself is not set here: `gtk::init()` does that as part of starting
//! the window (measured 2026-09-21 — `gettext` returns the English msgid before it
//! and the translated string after), which is why this crate needs no `unsafe`
//! `setlocale` call and why binding the domain is enough.

use gtk4::glib;

fn main() -> glib::ExitCode {
    pixlay::i18n::init();
    pixlay::app::run()
}
