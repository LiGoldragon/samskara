use std::sync::Arc;

use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
};

use criome_cozo::CriomeDb;
use criome_store::Store;

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

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct StorePutParams {
    /// Base64-encoded bytes to store
    pub data_b64: String,
    /// MIME type of the content
    pub media_type: String,
    /// Origin system (e.g. "annas-archive", "local")
    pub origin: String,
    /// Reference in origin system (e.g. anna's archive MD5)
    #[serde(default)]
    pub origin_ref: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct StoreGetParams {
    /// Blake3 content hash (hex)
    pub hash: String,
}

// ── Server struct ──────────────────────────────────────────────────

#[derive(Clone)]
pub struct SamskaraMcp {
    db: Arc<CriomeDb>,
    store: Arc<Store>,
    tool_router: ToolRouter<Self>,
}

impl SamskaraMcp {
    pub fn new(db: Arc<CriomeDb>, store: Arc<Store>) -> Self {
        Self {
            db,
            store,
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
    #[tool(description = "Execute CozoScript against the world database. Returns CozoScript tuples.")]
    async fn query(&self, Parameters(params): Parameters<QueryParams>) -> String {
        let db = self.db.clone();
        let script = params.script;
        let result = tokio::task::spawn_blocking(move || {
            db.run_script_cozo(&script)
                .map_err(|e| e.to_string())
        })
        .await;

        match result {
            Ok(Ok(text)) => text,
            Ok(Err(e)) => format!("error: {e}"),
            Err(e) => format!("error: task join failed: {e}"),
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
            db.run_script_raw(&thought_script).map_err(|e| e.to_string())?;

            // Insert tags
            for tag in &params.tags {
                let tag_script = format!(
                    r#"?[thought_id, tag] <- [["{}","{}"]]
                    :put thought_tag {{ thought_id, tag }}"#,
                    esc(&params.id),
                    esc(tag),
                );
                db.run_script_raw(&tag_script).map_err(|e| e.to_string())?;
            }

            Ok::<String, String>(format!(
                "Thought '{}' asserted with phase '{}', dignity '{}'",
                params.id, params.phase, params.dignity
            ))
        })
        .await;

        match result {
            Ok(Ok(msg)) => msg,
            Ok(Err(e)) => format!("error: {e}"),
            Err(e) => format!("{{\"error\": \"task join failed: {e}\"}}"),
        }
    }

    #[tool(description = "List all stored relations in the database.")]
    async fn list_relations(&self) -> String {
        let db = self.db.clone();
        let result = tokio::task::spawn_blocking(move || {
            db.run_script_cozo("::relations")
                .map_err(|e| e.to_string())
        })
        .await;

        match result {
            Ok(Ok(text)) => text,
            Ok(Err(e)) => format!("error: {e}"),
            Err(e) => format!("error: task join failed: {e}"),
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
            db.run_script_cozo(&script)
                .map_err(|e| e.to_string())
        })
        .await;

        match result {
            Ok(Ok(text)) => text,
            Ok(Err(e)) => format!("error: {e}"),
            Err(e) => format!("error: task join failed: {e}"),
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
            Ok(Err(e)) => format!("error: {e}"),
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
            Ok(Err(e)) => format!("error: {e}"),
            Err(e) => format!("{{\"error\": \"task join failed: {e}\"}}"),
        }
    }

    #[tool(description = "Store a blob in the content-addressed store. Returns blake3 hash. Also writes a blob row to the DB.")]
    async fn store_put(
        &self,
        Parameters(params): Parameters<StorePutParams>,
    ) -> String {
        let store = self.store.clone();
        let db = self.db.clone();
        let result = tokio::task::spawn_blocking(move || {
            use base64::Engine;
            let data = base64::engine::general_purpose::STANDARD
                .decode(&params.data_b64)
                .map_err(|e| format!("base64 decode failed: {e}"))?;

            let hash = store.put(&data).map_err(|e| e.to_string())?;
            let meta = store.meta(&hash).map_err(|e| e.to_string())?
                .ok_or("blob stored but meta missing")?;

            let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
            let origin_ref = params.origin_ref.as_deref().unwrap_or("");
            let script = format!(
                r#"?[hash, size, compressed_size, media_type, origin, origin_ref, phase, dignity] <- [[
                    "{}", {}, {}, "{}", "{}", "{}", "manifest", "seen"
                ]]
                :put blob {{ hash => size, compressed_size, media_type, origin, origin_ref, phase, dignity }}"#,
                hash.to_hex(),
                meta.size,
                meta.compressed_size,
                esc(&params.media_type),
                esc(&params.origin),
                esc(origin_ref),
            );
            db.run_script_raw(&script).map_err(|e| e.to_string())?;

            Ok::<String, String>(format!(
                "[hash,size,compressed_size]\n[\"{}\",{},{}]",
                hash.to_hex(), meta.size, meta.compressed_size,
            ))
        })
        .await;

        match result {
            Ok(Ok(msg)) => msg,
            Ok(Err(e)) => format!("error: {e}"),
            Err(e) => format!("error: task join failed: {e}"),
        }
    }

    #[tool(description = "Retrieve a blob from the content-addressed store by blake3 hash. Returns base64-encoded bytes.")]
    async fn store_get(
        &self,
        Parameters(params): Parameters<StoreGetParams>,
    ) -> String {
        let store = self.store.clone();
        let result = tokio::task::spawn_blocking(move || {
            let hash = criome_store::ContentHash::from_hex(&params.hash)
                .ok_or_else(|| format!("invalid hash: {}", params.hash))?;
            let data = store.get(&hash).map_err(|e| e.to_string())?;

            use base64::Engine;
            Ok::<String, String>(base64::engine::general_purpose::STANDARD.encode(&data))
        })
        .await;

        match result {
            Ok(Ok(b64)) => b64,
            Ok(Err(e)) => format!("error: {e}"),
            Err(e) => format!("error: task join failed: {e}"),
        }
    }
}
