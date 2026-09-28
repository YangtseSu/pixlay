// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! Result formatting.
//!
//! Two shapes of the same data: `key = value` lines (the default) and one JSON
//! object (`--json`). Both are sorted by key and contain no timestamps, no
//! durations and no paths, so the same input produces byte-identical output —
//! with `--stats`, which reports measurements, the measured fields are the
//! documented exception.
//!
//! Diagnostics never appear here: stderr carries them.
//!
//! Every value a line carries is escaped as it is written. A value is text the
//! caller does not control — a decoder's reason, a filename off the disk — and the
//! one-record-per-line shape has to hold for all of them: a newline in a value
//! would otherwise turn into a line of its own and a consumer reading `key = value`
//! would read a field that was never set (S15h, PIX-018). The rule only escapes
//! what it must (see [`escape_bytes`]), so a value without a control byte is
//! printed exactly as before.

use std::collections::BTreeMap;
use std::fmt::Write as _;

/// An ordered set of machine-readable fields.
#[derive(Debug, Default)]
pub struct Report {
    fields: BTreeMap<String, Field>,
}

#[derive(Debug)]
pub enum Field {
    Text(String),
    /// Bytes known only as bytes: a path from the filesystem, which is not
    /// required to be UTF-8 and has to survive as the bytes it is (S15h, PIX-018).
    Bytes(Vec<u8>),
    Int(i64),
    Float(f64),
    Bool(bool),
}

impl Report {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn text(&mut self, key: &str, value: impl Into<String>) -> &mut Self {
        self.fields
            .insert(key.to_string(), Field::Text(value.into()));
        self
    }

    /// A field whose value is bytes rather than text: a filesystem path.
    ///
    /// The bytes are the path's own `as_bytes()`, so a name that is not valid
    /// UTF-8 is reported as the name it is instead of the replacement characters
    /// `Path::display` would print, and two distinct paths cannot become the same
    /// line (S15h, PIX-018).
    pub fn bytes(&mut self, key: &str, value: impl Into<Vec<u8>>) -> &mut Self {
        self.fields
            .insert(key.to_string(), Field::Bytes(value.into()));
        self
    }

    pub fn int(&mut self, key: &str, value: impl Into<i64>) -> &mut Self {
        self.fields
            .insert(key.to_string(), Field::Int(value.into()));
        self
    }

    pub fn float(&mut self, key: &str, value: f64) -> &mut Self {
        self.fields.insert(key.to_string(), Field::Float(value));
        self
    }

    pub fn bool(&mut self, key: &str, value: bool) -> &mut Self {
        self.fields.insert(key.to_string(), Field::Bool(value));
        self
    }

    /// Appends `.index` to `prefix`, so repeated rows stay greppable:
    /// `slot.0.actual = 200,30,40`.
    pub fn row(&mut self, prefix: &str, index: usize) -> String {
        format!("{prefix}.{index}")
    }

    /// `key = value` lines, one per field, sorted.
    pub fn lines(&self) -> String {
        let mut out = String::new();
        for (key, field) in &self.fields {
            let _ = writeln!(out, "{key} = {}", field.render());
        }
        out
    }

    /// One JSON object. Floats keep six decimals so the output does not depend
    /// on how the platform formats a double.
    pub fn json(&self) -> String {
        let mut out = String::from("{\n");
        for (index, (key, field)) in self.fields.iter().enumerate() {
            if index > 0 {
                out.push_str(",\n");
            }
            let _ = write!(out, "  \"{key}\": {}", field.render_json());
        }
        out.push_str("\n}\n");
        out
    }
}

impl Field {
    fn render(&self) -> String {
        match self {
            Self::Text(value) => escape_bytes(value.as_bytes()),
            Self::Bytes(value) => escape_bytes(value),
            Self::Int(value) => value.to_string(),
            Self::Float(value) => format!("{value:.6}"),
            Self::Bool(value) => if *value { "true" } else { "false" }.to_string(),
        }
    }

    fn render_json(&self) -> String {
        match self {
            Self::Text(value) => format!("\"{}\"", escape(value)),
            // The escaped form is the JSON *value*, not just the text between the
            // quotes: `escape` doubles the backslashes the byte rule inserted, so a
            // JSON parser hands the caller the same string the line format prints
            // and one rule reads a path out of either shape. Emitting the escaped
            // form verbatim is not an option — `\xNN` is not a JSON escape and
            // `\n` would come back as a newline, so the document would either not
            // parse or not say what it looks like it says (S15h, PIX-018).
            Self::Bytes(value) => format!("\"{}\"", escape(&escape_bytes(value))),
            Self::Int(value) => value.to_string(),
            Self::Float(value) => format!("{value:.6}"),
            Self::Bool(value) => value.to_string(),
        }
    }
}

/// Renders `bytes` as ASCII, leaving valid UTF-8 text byte-identical.
///
/// The rule, and why it is this one (S15h, PIX-018):
///
/// * `\` → `\\`, 0x0A → `\n`, 0x0D → `\r`, 0x09 → `\t`;
/// * any other byte below 0x20 or the byte 0x7F → `\xNN`, lowercase hex;
/// * a byte that is not part of a valid UTF-8 sequence → `\xNN` byte by byte.
///
/// Bytes of valid UTF-8 pass through unchanged, so an ordinary path or an ordinary
/// message prints exactly as it did before any of this existed, and only a value
/// that carries a control byte or is not UTF-8 changes. Escaping `\n` and `\r` is
/// what keeps a value from adding a field line; escaping `\` is what makes the rule
/// reversible, so a name that really contains the four characters `\x0a` prints as
/// `\\x0a` and reads back as those characters rather than as a newline.
fn escape_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    let mut rest = bytes;
    loop {
        match std::str::from_utf8(rest) {
            Ok(text) => {
                escape_text(text, &mut out);
                return out;
            }
            Err(error) => {
                let (valid, after) = rest.split_at(error.valid_up_to());
                // `valid_up_to` is a UTF-8 boundary by definition.
                escape_text(std::str::from_utf8(valid).expect("valid UTF-8"), &mut out);
                // `None` means the tail was truncated mid-sequence: every
                // remaining byte is invalid, so it is escaped as itself.
                let invalid = error.error_len().unwrap_or(after.len());
                for byte in &after[..invalid] {
                    let _ = write!(out, "\\x{byte:02x}");
                }
                rest = &after[invalid..];
            }
        }
    }
}

/// Escapes the text a valid UTF-8 run carries: the escapes that apply to text.
fn escape_text(text: &str, out: &mut String) {
    for character in text.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c as u32 == 0x7F => {
                let _ = write!(out, "\\x{:02x}", c as u32);
            }
            c => out.push(c),
        }
    }
}

fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}
