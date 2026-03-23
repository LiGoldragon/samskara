use std::sync::Arc;

use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
};

use criome_cozo::CriomeDb;

// Re-export core param types for rmcp macros
pub use samskara_core::mcp::{
    QueryParams, DescribeRelationParams, CommitWorldParams, RestoreWorldParams,
};

// ── Samskara-specific param types ────────────────────────────────

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct AssertThoughtParams {
    /// Kind (Thought domain variant): user, feedback, project, reference, observation
    pub kind: String,
    /// Scope: repo name or "global"
    pub scope: String,
    /// Status: draft, proposed, approved
    pub status: String,
    /// Short title — blake3 hash of this becomes part of the composite key
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

// ── Server struct ────────────────────────────────────────────────

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
    // ── Generic tools (delegated to samskara-core) ───────────────

    #[tool(description = "Execute CozoScript against the world database. Returns CozoScript tuples.")]
    async fn query(&self, Parameters(params): Parameters<QueryParams>) -> String {
        samskara_core::mcp::query(self.db.clone(), params.script).await
    }

    #[tool(description = "List all stored relations in the database.")]
    async fn list_relations(&self) -> String {
        samskara_core::mcp::list_relations(self.db.clone()).await
    }

    #[tool(description = "Show the schema (columns and types) of a specific relation.")]
    async fn describe_relation(
        &self,
        Parameters(params): Parameters<DescribeRelationParams>,
    ) -> String {
        samskara_core::mcp::describe_relation(self.db.clone(), params.name).await
    }

    #[tool(description = "Commit the current world state. Promotes becoming→manifest, computes blake3 content hash, records world_commit + manifest + snapshot/deltas.")]
    async fn commit_world(
        &self,
        Parameters(params): Parameters<CommitWorldParams>,
    ) -> String {
        samskara_core::mcp::commit_world(self.db.clone(), params).await
    }

    #[tool(description = "Restore the world state to a specific commit. Loads from snapshots + deltas.")]
    async fn restore_world(
        &self,
        Parameters(params): Parameters<RestoreWorldParams>,
    ) -> String {
        samskara_core::mcp::restore_world(self.db.clone(), params.commit_id).await
    }

    // ── Samskara-specific tools ──────────────────────────────────

    #[tool(description = "Assert a new thought into the world model with kind, scope, status, phase, dignity, and optional tags.")]
    async fn assert_thought(
        &self,
        Parameters(params): Parameters<AssertThoughtParams>,
    ) -> String {
        let db = self.db.clone();
        let now = chrono::Utc::now().to_rfc3339();
        let result = tokio::task::spawn_blocking(move || {
            let esc = |s: &str| s.replace('"', "\\\"");
            let title_hash = &blake3::hash(params.title.as_bytes()).to_hex()[..16];

            let thought_script = format!(
                r#"?[kind, scope, title_hash, status, title, body, created_ts, updated_ts, phase, dignity] <- [[
                    "{}", "{}", "{}", "{}", "{}", "{}", "{}", "{}", "{}", "{}"
                ]]
                :put thought {{ kind, scope, title_hash => status, title, body, created_ts, updated_ts, phase, dignity }}"#,
                esc(&params.kind),
                esc(&params.scope),
                title_hash,
                esc(&params.status),
                esc(&params.title),
                esc(&params.body),
                esc(&now),
                esc(&now),
                esc(&params.phase),
                esc(&params.dignity),
            );
            db.run_script_raw(&thought_script).map_err(|e| e.to_string())?;

            for tag in &params.tags {
                let tag_script = format!(
                    r#"?[kind, scope, title_hash, tag] <- [["{}","{}","{}","{}"]]
                    :put thought_tag {{ kind, scope, title_hash, tag }}"#,
                    esc(&params.kind),
                    esc(&params.scope),
                    title_hash,
                    esc(tag),
                );
                db.run_script_raw(&tag_script).map_err(|e| e.to_string())?;
            }

            Ok::<String, String>(format!(
                "Thought '{}' asserted with phase '{}', dignity '{}'",
                params.title, params.phase, params.dignity
            ))
        })
        .await;

        match result {
            Ok(Ok(msg)) => msg,
            Ok(Err(e)) => format!("error: {e}"),
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
                    "?[kind, scope, status, title, body, phase, dignity] := \
                     *thought{{kind, scope, title_hash, status, title, body, phase, dignity}}, \
                     *thought_tag{{kind, scope, title_hash, tag: \"{}\"}}, \
                     {}",
                    tag.replace('"', "\\\""),
                    conditions.join(", ")
                )
            } else {
                format!(
                    "?[kind, scope, status, title, body, phase, dignity] := \
                     *thought{{kind, scope, title_hash: _, status, title, body, phase, dignity}}, \
                     {}",
                    conditions.join(", ")
                )
            };

            db.run_script_cozo(&base)
                .map_err(|e| e.to_string())
        })
        .await;

        match result {
            Ok(Ok(text)) => text,
            Ok(Err(e)) => format!("error: {e}"),
            Err(e) => format!("error: task join failed: {e}"),
        }
    }
}
