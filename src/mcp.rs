use std::sync::Arc;

use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
};

use criome_cozo::CriomeDb;

// ── Parameter types ────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct QueryParams {
    /// CozoScript to execute against the world database
    pub script: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct AssertThoughtParams {
    /// Unique thought identifier
    pub id: String,
    /// Kind: user, feedback, project, reference, observation
    pub kind: String,
    /// Scope: repo name or "global"
    pub scope: String,
    /// Status: draft, proposed, approved
    pub status: String,
    /// Short title
    pub title: String,
    /// Full body text
    pub body: String,
    /// Liveness: doctrine, trusted_fact, observation, rumor, web_gossip
    #[schemars(default = "default_liveness")]
    #[serde(default = "default_liveness")]
    pub liveness: String,
    /// Optional tags for indexing
    #[serde(default)]
    pub tags: Vec<String>,
}

fn default_liveness() -> String {
    "observation".to_string()
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct QueryThoughtsParams {
    /// Filter by kind (user, feedback, project, reference, observation)
    #[serde(default)]
    pub kind: Option<String>,
    /// Filter by scope (repo name or "global")
    #[serde(default)]
    pub scope: Option<String>,
    /// Filter by tag
    #[serde(default)]
    pub tag: Option<String>,
    /// Minimum liveness level (default: include all live)
    #[serde(default)]
    pub min_liveness: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct DescribeRelationParams {
    /// Name of the relation to describe
    pub name: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CommitWorldParams {
    /// Commit message describing what changed
    pub message: String,
    /// Agent ID recording the commit
    pub agent_id: String,
    /// Optional session ID
    #[serde(default)]
    pub session_id: Option<String>,
}

// ── Versioned relations (those participating in the world hash) ────

const VERSIONED_RELATIONS: &[&str] = &[
    "agent",
    "agent_session",
    "liveness_vocab",
    "principle",
    "repo",
    "repo_state",
    "thought",
    "thought_link",
    "thought_tag",
    "trust_review",
];

// ── Server struct ──────────────────────────────────────────────────

#[derive(Clone)]
pub struct SamskaraMcp {
    db: Arc<CriomeDb>,
    tool_router: ToolRouter<Self>,
}

impl SamskaraMcp {
    pub fn new(db: Arc<CriomeDb>) -> Self {
        Self {
            db,
            tool_router: Self::tool_router(),
        }
    }

    fn run_script_blocking(
        db: &Arc<CriomeDb>,
        script: &str,
    ) -> Result<serde_json::Value, String> {
        db.run_script(script).map_err(|e| e.to_string())
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for SamskaraMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Samskara — pure datalog agent. Query and mutate the world model \
                 through CozoDB relations. The world state is version-controlled \
                 via content-addressed commits."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

#[tool_router]
impl SamskaraMcp {
    #[tool(description = "Execute arbitrary CozoScript against the world database. Returns JSON results.")]
    async fn query(&self, Parameters(params): Parameters<QueryParams>) -> String {
        let db = self.db.clone();
        let script = params.script;
        let result = tokio::task::spawn_blocking(move || {
            Self::run_script_blocking(&db, &script)
        })
        .await;

        match result {
            Ok(Ok(json)) => serde_json::to_string_pretty(&json).unwrap_or_else(|e| {
                format!("{{\"error\": \"serialization failed: {e}\"}}")
            }),
            Ok(Err(e)) => format!("{{\"error\": {}}}", serde_json::json!(e)),
            Err(e) => format!("{{\"error\": \"task join failed: {e}\"}}"),
        }
    }

    #[tool(description = "Assert a new thought into the world model with kind, scope, status, liveness, and optional tags.")]
    async fn assert_thought(
        &self,
        Parameters(params): Parameters<AssertThoughtParams>,
    ) -> String {
        let db = self.db.clone();
        let now = chrono::Utc::now().to_rfc3339();
        let result = tokio::task::spawn_blocking(move || {
            // Escape strings for CozoScript
            let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");

            let thought_script = format!(
                r#"?[id, kind, scope, status, title, body, created_ts, updated_ts, liveness] <- [[
                    "{}", "{}", "{}", "{}", "{}", "{}", "{}", "{}", "{}"
                ]]
                :put thought {{ id => kind, scope, status, title, body, created_ts, updated_ts, liveness }}"#,
                esc(&params.id),
                esc(&params.kind),
                esc(&params.scope),
                esc(&params.status),
                esc(&params.title),
                esc(&params.body),
                esc(&now),
                esc(&now),
                esc(&params.liveness),
            );
            Self::run_script_blocking(&db, &thought_script)?;

            // Insert tags
            for tag in &params.tags {
                let tag_script = format!(
                    r#"?[thought_id, tag] <- [["{}","{}"]]
                    :put thought_tag {{ thought_id, tag }}"#,
                    esc(&params.id),
                    esc(tag),
                );
                Self::run_script_blocking(&db, &tag_script)?;
            }

            Ok::<String, String>(format!(
                "Thought '{}' asserted with liveness '{}'",
                params.id, params.liveness
            ))
        })
        .await;

        match result {
            Ok(Ok(msg)) => msg,
            Ok(Err(e)) => format!("{{\"error\": {}}}", serde_json::json!(e)),
            Err(e) => format!("{{\"error\": \"task join failed: {e}\"}}"),
        }
    }

    #[tool(description = "List all stored relations in the database.")]
    async fn list_relations(&self) -> String {
        let db = self.db.clone();
        let result = tokio::task::spawn_blocking(move || {
            Self::run_script_blocking(&db, "::relations")
        })
        .await;

        match result {
            Ok(Ok(json)) => serde_json::to_string_pretty(&json).unwrap_or_else(|e| {
                format!("{{\"error\": \"serialization failed: {e}\"}}")
            }),
            Ok(Err(e)) => format!("{{\"error\": {}}}", serde_json::json!(e)),
            Err(e) => format!("{{\"error\": \"task join failed: {e}\"}}"),
        }
    }

    #[tool(description = "Show the schema (columns and types) of a specific relation.")]
    async fn describe_relation(
        &self,
        Parameters(params): Parameters<DescribeRelationParams>,
    ) -> String {
        let db = self.db.clone();
        let name = params.name;
        let result = tokio::task::spawn_blocking(move || {
            let script = format!("::columns {name}");
            Self::run_script_blocking(&db, &script)
        })
        .await;

        match result {
            Ok(Ok(json)) => serde_json::to_string_pretty(&json).unwrap_or_else(|e| {
                format!("{{\"error\": \"serialization failed: {e}\"}}")
            }),
            Ok(Err(e)) => format!("{{\"error\": {}}}", serde_json::json!(e)),
            Err(e) => format!("{{\"error\": \"task join failed: {e}\"}}"),
        }
    }

    #[tool(description = "Query thoughts with optional filters. Excludes dead (superseded/disproven) by default.")]
    async fn query_thoughts(
        &self,
        Parameters(params): Parameters<QueryThoughtsParams>,
    ) -> String {
        let db = self.db.clone();
        let result = tokio::task::spawn_blocking(move || {
            let mut conditions = vec![
                "liveness != \"superseded\"".to_string(),
                "liveness != \"disproven\"".to_string(),
            ];

            if let Some(ref kind) = params.kind {
                conditions.push(format!("kind = \"{}\"", kind.replace('"', "\\\"")));
            }
            if let Some(ref scope) = params.scope {
                conditions.push(format!("scope = \"{}\"", scope.replace('"', "\\\"")));
            }

            let base = if let Some(ref tag) = params.tag {
                format!(
                    "?[id, kind, scope, status, title, body, liveness] := \
                     *thought{{id, kind, scope, status, title, body, liveness}}, \
                     *thought_tag{{thought_id: id, tag: \"{}\"}}, \
                     {}",
                    tag.replace('"', "\\\""),
                    conditions.join(", ")
                )
            } else {
                format!(
                    "?[id, kind, scope, status, title, body, liveness] := \
                     *thought{{id, kind, scope, status, title, body, liveness}}, \
                     {}",
                    conditions.join(", ")
                )
            };

            Self::run_script_blocking(&db, &base)
        })
        .await;

        match result {
            Ok(Ok(json)) => serde_json::to_string_pretty(&json).unwrap_or_else(|e| {
                format!("{{\"error\": \"serialization failed: {e}\"}}")
            }),
            Ok(Err(e)) => format!("{{\"error\": {}}}", serde_json::json!(e)),
            Err(e) => format!("{{\"error\": \"task join failed: {e}\"}}"),
        }
    }

    #[tool(description = "Commit the current world state. Computes blake3 content hash over all live relations, records world_commit and world_manifest.")]
    async fn commit_world(
        &self,
        Parameters(params): Parameters<CommitWorldParams>,
    ) -> String {
        let db = self.db.clone();
        let now = chrono::Utc::now().to_rfc3339();
        let result = tokio::task::spawn_blocking(move || {
            let mut manifest_entries: Vec<(String, usize, String)> = Vec::new();
            let mut world_hasher = blake3::Hasher::new();

            // Hash each versioned relation's live rows
            for &rel_name in VERSIONED_RELATIONS {
                // First get column names for this relation
                let cols_result = Self::run_script_blocking(
                    &db,
                    &format!("::columns {rel_name}"),
                ).map_err(|e| format!("failed to get columns for {rel_name}: {e}"))?;

                let col_names: Vec<String> = cols_result
                    .get("rows")
                    .and_then(|r| r.as_array())
                    .map(|rows| {
                        rows.iter()
                            .filter_map(|row| {
                                row.as_array()
                                    .and_then(|r| r.first())
                                    .and_then(|v| v.get("Str"))
                                    .and_then(|s| s.as_str())
                                    .map(|s| s.to_string())
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                let col_list = col_names.join(", ");

                // Build query for all rows, filtering dead ones for liveness-aware relations
                let query = if has_liveness_column(rel_name) {
                    format!(
                        "?[{col_list}] := *{rel_name}{{{col_list}}}, \
                         liveness != \"superseded\", liveness != \"disproven\"",
                    )
                } else {
                    format!("?[{col_list}] := *{rel_name}{{{col_list}}}")
                };

                let rows = match Self::run_script_blocking(&db, &query) {
                    Ok(v) => v,
                    Err(e) => return Err(format!("failed to query {rel_name}: {e}")),
                };

                let rows_str = serde_json::to_string(&rows).unwrap_or_default();
                let row_count = rows
                    .get("rows")
                    .and_then(|r| r.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);

                let rel_hash = blake3::hash(rows_str.as_bytes());
                let rel_hash_hex = rel_hash.to_hex().to_string();

                world_hasher.update(rel_name.as_bytes());
                world_hasher.update(rel_hash.as_bytes());

                manifest_entries.push((rel_name.to_string(), row_count, rel_hash_hex));
            }

            let world_hash = world_hasher.finalize().to_hex().to_string();

            // Find parent commit (latest existing)
            let parent_result = Self::run_script_blocking(
                &db,
                "?[id, ts] := *world_commit{id, ts} :order -ts :limit 1",
            );
            let parent_id = parent_result
                .ok()
                .and_then(|v| {
                    v.get("rows")
                        .and_then(|r| r.as_array())
                        .and_then(|a| a.first())
                        .and_then(|row| row.as_array())
                        .and_then(|r| r.first())
                        .and_then(|id| id.as_str().map(|s| s.to_string()))
                })
                .unwrap_or_default();

            let session_id = params.session_id.unwrap_or_default();
            let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");

            // Record world_commit
            let commit_script = format!(
                r#"?[id, parent_id, agent_id, session_id, message, ts, manifest_hash] <- [[
                    "{}", "{}", "{}", "{}", "{}", "{}", "{}"
                ]]
                :put world_commit {{ id => parent_id, agent_id, session_id, message, ts, manifest_hash }}"#,
                esc(&world_hash),
                esc(&parent_id),
                esc(&params.agent_id),
                esc(&session_id),
                esc(&params.message),
                esc(&now),
                esc(&world_hash),
            );
            Self::run_script_blocking(&db, &commit_script)?;

            // Record world_manifest entries
            for (rel_name, row_count, content_hash) in &manifest_entries {
                let manifest_script = format!(
                    r#"?[commit_id, relation_name, row_count, content_hash] <- [[
                        "{}", "{}", {}, "{}"
                    ]]
                    :put world_manifest {{ commit_id, relation_name => row_count, content_hash }}"#,
                    esc(&world_hash),
                    rel_name,
                    row_count,
                    content_hash,
                );
                Self::run_script_blocking(&db, &manifest_script)?;
            }

            // Build summary
            let summary: Vec<String> = manifest_entries
                .iter()
                .map(|(name, count, hash)| {
                    format!("  {name}: {count} rows ({hash:.12}…)")
                })
                .collect();

            Ok(format!(
                "World committed: {world_hash}\nParent: {}\nManifest:\n{}",
                if parent_id.is_empty() {
                    "(genesis)"
                } else {
                    &parent_id
                },
                summary.join("\n")
            ))
        })
        .await;

        match result {
            Ok(Ok(msg)) => msg,
            Ok(Err(e)) => format!("{{\"error\": {}}}", serde_json::json!(e)),
            Err(e) => format!("{{\"error\": \"task join failed: {e}\"}}"),
        }
    }
}

/// Relations that have a `liveness` column (and should filter out dead rows).
fn has_liveness_column(rel: &str) -> bool {
    matches!(
        rel,
        "thought" | "agent" | "agent_session" | "repo" | "repo_state" | "principle"
    )
}
