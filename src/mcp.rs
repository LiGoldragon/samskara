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
    /// Phase: becoming (staged), manifest (current), retired (archived)
    #[schemars(default = "default_phase")]
    #[serde(default = "default_phase")]
    pub phase: String,
    /// Dignity: eternal, proven, seen, uncertain, delusion
    #[schemars(default = "default_dignity")]
    #[serde(default = "default_dignity")]
    pub dignity: String,
    /// Optional tags for indexing
    #[serde(default)]
    pub tags: Vec<String>,
}

fn default_phase() -> String {
    "becoming".to_string()
}

fn default_dignity() -> String {
    "seen".to_string()
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
    /// Filter by phase (becoming, manifest, retired). Default: exclude retired.
    #[serde(default)]
    pub phase: Option<String>,
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

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RestoreWorldParams {
    /// Commit ID to restore the world state to
    pub commit_id: String,
}

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
            Ok(Err(e)) => format!("{{\"error\": \"{}\"}}", e.replace('"', "\\\"")),
            Err(e) => format!("{{\"error\": \"task join failed: {e}\"}}"),
        }
    }

    #[tool(description = "Assert a new thought into the world model with kind, scope, status, phase, dignity, and optional tags.")]
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
                r#"?[id, kind, scope, status, title, body, created_ts, updated_ts, phase, dignity] <- [[
                    "{}", "{}", "{}", "{}", "{}", "{}", "{}", "{}", "{}", "{}"
                ]]
                :put thought {{ id => kind, scope, status, title, body, created_ts, updated_ts, phase, dignity }}"#,
                esc(&params.id),
                esc(&params.kind),
                esc(&params.scope),
                esc(&params.status),
                esc(&params.title),
                esc(&params.body),
                esc(&now),
                esc(&now),
                esc(&params.phase),
                esc(&params.dignity),
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
                "Thought '{}' asserted with phase '{}', dignity '{}'",
                params.id, params.phase, params.dignity
            ))
        })
        .await;

        match result {
            Ok(Ok(msg)) => msg,
            Ok(Err(e)) => format!("{{\"error\": \"{}\"}}", e.replace('"', "\\\"")),
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
            Ok(Err(e)) => format!("{{\"error\": \"{}\"}}", e.replace('"', "\\\"")),
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
            Ok(Err(e)) => format!("{{\"error\": \"{}\"}}", e.replace('"', "\\\"")),
            Err(e) => format!("{{\"error\": \"task join failed: {e}\"}}"),
        }
    }

    #[tool(description = "Query thoughts with optional filters. Excludes retired-phase by default.")]
    async fn query_thoughts(
        &self,
        Parameters(params): Parameters<QueryThoughtsParams>,
    ) -> String {
        let db = self.db.clone();
        let result = tokio::task::spawn_blocking(move || {
            let mut conditions = vec![
                "phase != \"retired\"".to_string(),
            ];

            if let Some(ref kind) = params.kind {
                conditions.push(format!("kind = \"{}\"", kind.replace('"', "\\\"")));
            }
            if let Some(ref scope) = params.scope {
                conditions.push(format!("scope = \"{}\"", scope.replace('"', "\\\"")));
            }

            let base = if let Some(ref tag) = params.tag {
                format!(
                    "?[id, kind, scope, status, title, body, phase, dignity] := \
                     *thought{{id, kind, scope, status, title, body, phase, dignity}}, \
                     *thought_tag{{thought_id: id, tag: \"{}\"}}, \
                     {}",
                    tag.replace('"', "\\\""),
                    conditions.join(", ")
                )
            } else {
                format!(
                    "?[id, kind, scope, status, title, body, phase, dignity] := \
                     *thought{{id, kind, scope, status, title, body, phase, dignity}}, \
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
            Ok(Err(e)) => format!("{{\"error\": \"{}\"}}", e.replace('"', "\\\"")),
            Err(e) => format!("{{\"error\": \"task join failed: {e}\"}}"),
        }
    }

    #[tool(description = "Commit the current world state. Promotes becoming→manifest, computes blake3 content hash, records world_commit + manifest + snapshot/deltas.")]
    async fn commit_world(
        &self,
        Parameters(params): Parameters<CommitWorldParams>,
    ) -> String {
        let db = self.db.clone();
        let now = chrono::Utc::now().to_rfc3339();
        let result = tokio::task::spawn_blocking(move || {
            let session_id = params.session_id.unwrap_or_default();
            let vcs = crate::vcs::WorldVcs::new(&db);
            let commit_result = vcs.commit(crate::vcs::commit::CommitInput {
                message: &params.message,
                agent_id: &params.agent_id,
                session_id: &session_id,
                now: &now,
            }).map_err(|e| e.to_string())?;

            let summary: Vec<String> = commit_result
                .manifest
                .iter()
                .map(|(name, count, hash)| {
                    format!("  {name}: {count} rows ({hash:.12}…)")
                })
                .collect();

            let snap_info = if commit_result.snapshot_taken {
                " [snapshot taken]"
            } else {
                ""
            };

            Ok::<String, String>(format!(
                "World committed: {}{snap_info}\nParent: {}\nDeltas: {}\nManifest:\n{}",
                commit_result.world_hash,
                if commit_result.parent_id.is_empty() {
                    "(genesis)"
                } else {
                    &commit_result.parent_id
                },
                commit_result.delta_count,
                summary.join("\n")
            ))
        })
        .await;

        match result {
            Ok(Ok(msg)) => msg,
            Ok(Err(e)) => format!("{{\"error\": \"{}\"}}", e.replace('"', "\\\"")),
            Err(e) => format!("{{\"error\": \"task join failed: {e}\"}}"),
        }
    }

    #[tool(description = "Restore the world state to a specific commit. Loads from snapshots + deltas.")]
    async fn restore_world(
        &self,
        Parameters(params): Parameters<RestoreWorldParams>,
    ) -> String {
        let db = self.db.clone();
        let result = tokio::task::spawn_blocking(move || {
            let vcs = crate::vcs::WorldVcs::new(&db);
            let restore_result = vcs.restore(&params.commit_id)
                .map_err(|e| e.to_string())?;

            Ok::<String, String>(format!(
                "Restored to commit: {}\nRelations restored: {}",
                restore_result.commit_id, restore_result.relations_restored,
            ))
        })
        .await;

        match result {
            Ok(Ok(msg)) => msg,
            Ok(Err(e)) => format!("{{\"error\": \"{}\"}}", e.replace('"', "\\\"")),
            Err(e) => format!("{{\"error\": \"task join failed: {e}\"}}"),
        }
    }
}
