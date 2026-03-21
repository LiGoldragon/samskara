pub mod commit;
pub mod delta;
pub mod error;
pub mod restore;
pub mod snapshot;

pub use error::Error;

use criome_cozo::CriomeDb;

/// Relations that participate in the world hash (versioned state).
pub const VERSIONED_RELATIONS: &[&str] = &[
    "Aspect",
    "Dignity",
    "Element",
    "Enum",
    "Measure",
    "Modality",
    "Phase",
    "Planet",
    "Sign",
    "agent",
    "agent_session",
    "latina",
    "principle",
    "repo",
    "repo_state",
    "samskrta",
    "thought",
    "thought_link",
    "thought_tag",
    "trust_review",
];

/// Number of commits between full snapshots.
pub const SNAPSHOT_INTERVAL: u32 = 10;

/// Relations that have a `phase` column (and should filter to manifest-phase only).
pub fn has_phase_column(rel: &str) -> bool {
    matches!(
        rel,
        "thought" | "agent" | "agent_session" | "repo" | "repo_state" | "principle"
        | "Sign" | "Planet" | "Measure"
    )
}

/// The world version control system. Owns a reference to the CozoDB instance
/// and provides commit (saṅkalpa) and restore (pratiṣṭhā) operations.
pub struct WorldVcs<'a> {
    db: &'a CriomeDb,
}

impl<'a> WorldVcs<'a> {
    pub fn new(db: &'a CriomeDb) -> Self {
        Self { db }
    }

    /// Escape a string for embedding in CozoScript.
    pub(crate) fn esc(s: &str) -> String {
        s.replace('\\', "\\\\").replace('"', "\\\"")
    }

    /// Get column names and key count for a relation via `::columns`.
    pub(crate) fn columns(&self, rel_name: &str) -> Result<(Vec<String>, usize), Error> {
        let result = self.db.run_script(&format!("::columns {rel_name}"))?;
        let rows = result
            .get("rows")
            .and_then(|v| v.as_array())
            .ok_or_else(|| Error::Db { detail: format!("no columns for {rel_name}") })?;

        let mut names = Vec::new();
        let mut key_count = 0;

        for row in rows {
            let arr = row.as_array().ok_or_else(|| Error::Db { detail: "bad column row".into() })?;
            let name = arr
                .first()
                .and_then(|v| v.get("Str").and_then(|s| s.as_str()).or(v.as_str()))
                .ok_or_else(|| Error::Db { detail: "missing column name".into() })?;
            let is_key = arr
                .get(1)
                .and_then(|v| v.get("Bool").and_then(|b| b.as_bool()).or(v.as_bool()))
                .unwrap_or(false);

            names.push(name.to_string());
            if is_key {
                key_count += 1;
            }
        }

        Ok((names, key_count))
    }

    /// Build the `:put`/`:rm` clause with key=>value separation.
    pub(crate) fn kv_clause(col_names: &[String], key_count: usize) -> String {
        let key_part = col_names[..key_count].join(", ");
        let val_part = col_names[key_count..].join(", ");
        if val_part.is_empty() {
            format!("{{{key_part}}}")
        } else {
            format!("{{{key_part} => {val_part}}}")
        }
    }
}

/// Convert a CozoDB DataValue JSON to a CozoScript literal.
pub fn datavalue_to_cozo_literal(v: &serde_json::Value) -> String {
    if let Some(s) = v.get("Str").and_then(|s| s.as_str()) {
        let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
        return format!("\"{escaped}\"");
    }
    if let Some(b) = v.get("Bool").and_then(|b| b.as_bool()) {
        return if b { "true".into() } else { "false".into() };
    }
    if let Some(num) = v.get("Num") {
        if let Some(i) = num.get("Int").and_then(|i| i.as_i64()) {
            return i.to_string();
        }
        if let Some(f) = num.get("Float").and_then(|f| f.as_f64()) {
            return f.to_string();
        }
    }
    if let Some(s) = v.as_str() {
        let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
        return format!("\"{escaped}\"");
    }
    if let Some(b) = v.as_bool() {
        return if b { "true".into() } else { "false".into() };
    }
    if let Some(i) = v.as_i64() {
        return i.to_string();
    }
    if let Some(f) = v.as_f64() {
        return f.to_string();
    }
    "null".into()
}
