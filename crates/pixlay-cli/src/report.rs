//! Result formatting.
//!
//! Two shapes of the same data: `key = value` lines (the default) and one JSON
//! object (`--json`). Both are sorted by key and contain no timestamps, no
//! durations and no paths, so the same input produces byte-identical output —
//! with `--stats`, which reports measurements, the measured fields are the
//! documented exception.
//!
//! Diagnostics never appear here: stderr carries them.

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
            Self::Text(value) => value.clone(),
            Self::Int(value) => value.to_string(),
            Self::Float(value) => format!("{value:.6}"),
            Self::Bool(value) => if *value { "true" } else { "false" }.to_string(),
        }
    }

    fn render_json(&self) -> String {
        match self {
            Self::Text(value) => format!("\"{}\"", escape(value)),
            Self::Int(value) => value.to_string(),
            Self::Float(value) => format!("{value:.6}"),
            Self::Bool(value) => value.to_string(),
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
