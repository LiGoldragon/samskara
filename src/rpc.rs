//! capnp RPC server for samskara — typed binary interface alongside MCP.
//! Listens on a Unix domain socket and exposes the Samskara interface.

use std::sync::Arc;

use capnp::capability::Promise;
use capnp_rpc::pry;

use crate::samskara_rpc_capnp::samskara;

pub struct SamskaraRpc {
    db: Arc<criome_cozo::CriomeDb>,
}

impl SamskaraRpc {
    pub fn new(db: Arc<criome_cozo::CriomeDb>) -> Self {
        Self { db }
    }
}

fn bytes_to_str(b: &[u8]) -> &str {
    std::str::from_utf8(b).unwrap_or("")
}

fn escape_cozo(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

fn run_query(db: &criome_cozo::CriomeDb, script: &str) -> Vec<u8> {
    match db.run_script(script) {
        Ok(result) => result.to_string().into_bytes(),
        Err(e) => format!("error: {e}").into_bytes(),
    }
}

impl samskara::Server for SamskaraRpc {
    fn query(
        &mut self,
        params: samskara::QueryParams,
        mut results: samskara::QueryResults,
    ) -> Promise<(), capnp::Error> {
        let script = bytes_to_str(pry!(pry!(params.get()).get_script()));
        let output = run_query(&self.db, script);
        results.get().set_result(&output);
        Promise::ok(())
    }

    fn list_relations(
        &mut self,
        _params: samskara::ListRelationsParams,
        mut results: samskara::ListRelationsResults,
    ) -> Promise<(), capnp::Error> {
        let output = run_query(&self.db, "::relations");
        results.get().set_result(&output);
        Promise::ok(())
    }

    fn describe_relation(
        &mut self,
        params: samskara::DescribeRelationParams,
        mut results: samskara::DescribeRelationResults,
    ) -> Promise<(), capnp::Error> {
        let name = bytes_to_str(pry!(pry!(params.get()).get_name()));
        let output = run_query(&self.db, &format!("::columns {name}"));
        results.get().set_result(&output);
        Promise::ok(())
    }

    fn commit_world(
        &mut self,
        params: samskara::CommitWorldParams,
        mut results: samskara::CommitWorldResults,
    ) -> Promise<(), capnp::Error> {
        let p = pry!(params.get());
        let _message = bytes_to_str(pry!(p.get_message()));
        let _agent_id = bytes_to_str(pry!(p.get_agent_id()));
        // MVP: commit_world requires the async MCP handler path.
        // For now, return a placeholder. Full integration comes when
        // samskara-core exposes commit as a sync function.
        results.get().set_commit_hash(b"not-yet-implemented");
        Promise::ok(())
    }

    fn restore_world(
        &mut self,
        _params: samskara::RestoreWorldParams,
        mut results: samskara::RestoreWorldResults,
    ) -> Promise<(), capnp::Error> {
        results.get().set_result(b"not-yet-implemented");
        Promise::ok(())
    }

    fn assert_thought(
        &mut self,
        params: samskara::AssertThoughtParams,
        mut results: samskara::AssertThoughtResults,
    ) -> Promise<(), capnp::Error> {
        let p = pry!(params.get());
        let kind = bytes_to_str(pry!(p.get_kind()));
        let scope = bytes_to_str(pry!(p.get_scope()));
        let status = bytes_to_str(pry!(p.get_status()));
        let title = bytes_to_str(pry!(p.get_title()));
        let body = bytes_to_str(pry!(p.get_body()));

        let title_hash = blake3::hash(title.as_bytes()).to_hex().to_string();
        let ek = escape_cozo(kind);
        let es = escape_cozo(scope);
        let est = escape_cozo(status);
        let et = escape_cozo(title);
        let eb = escape_cozo(body);

        let script = format!(
            "?[kind, scope, title_hash, status, title, body, created_ts, updated_ts, phase, dignity] <- \
             [['{ek}', '{es}', '{title_hash}', '{est}', '{et}', '{eb}', '', '', 'becoming', 'seen']] \
             :put thought {{ kind, scope, title_hash => status, title, body, created_ts, updated_ts, phase, dignity }}"
        );

        let _ = run_query(&self.db, &script);
        results.get().set_title_hash(title_hash.as_bytes());
        Promise::ok(())
    }

    fn query_thoughts(
        &mut self,
        params: samskara::QueryThoughtsParams,
        mut results: samskara::QueryThoughtsResults,
    ) -> Promise<(), capnp::Error> {
        let p = pry!(params.get());
        let kind = bytes_to_str(pry!(p.get_kind()));
        let scope = bytes_to_str(pry!(p.get_scope()));
        let tag = bytes_to_str(pry!(p.get_tag()));
        let phase = bytes_to_str(pry!(p.get_phase()));

        let mut conditions = vec!["phase != 'retired'".to_string()];
        if !kind.is_empty() { conditions.push(format!("kind == '{}'", escape_cozo(kind))); }
        if !scope.is_empty() { conditions.push(format!("scope == '{}'", escape_cozo(scope))); }
        if !phase.is_empty() { conditions.push(format!("phase == '{}'", escape_cozo(phase))); }

        let filter = conditions.join(", ");
        let query = if tag.is_empty() {
            format!("?[kind, scope, title, body, phase, dignity] := *thought{{kind, scope, title_hash, status, title, body, created_ts, updated_ts, phase, dignity}}, {filter}")
        } else {
            format!("?[kind, scope, title, body, phase, dignity] := *thought{{kind, scope, title_hash, status, title, body, created_ts, updated_ts, phase, dignity}}, *thought_tag{{kind, scope, title_hash, tag}}, tag == '{}', {filter}", escape_cozo(tag))
        };

        let output = run_query(&self.db, &query);
        results.get().set_result(&output);
        Promise::ok(())
    }
}

/// Start the capnp RPC server on a Unix domain socket.
pub async fn serve_rpc(
    db: Arc<criome_cozo::CriomeDb>,
    socket_path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    use futures::AsyncReadExt;
    use tokio::net::UnixListener;

    let _ = std::fs::remove_file(socket_path);
    let listener = UnixListener::bind(socket_path)?;
    tracing::info!("samskara capnp RPC listening on {}", socket_path.display());

    let local = tokio::task::LocalSet::new();
    local.run_until(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let db = db.clone();

            tokio::task::spawn_local(async move {
                let stream = tokio_util::compat::TokioAsyncReadCompatExt::compat(stream);
                let (reader, writer) = stream.split();
                let rpc_server = SamskaraRpc::new(db);
                let client: samskara::Client = capnp_rpc::new_client(rpc_server);

                let network = capnp_rpc::twoparty::VatNetwork::new(
                    reader,
                    writer,
                    capnp_rpc::rpc_twoparty_capnp::Side::Server,
                    Default::default(),
                );

                let rpc_system = capnp_rpc::RpcSystem::new(
                    Box::new(network),
                    Some(client.clone().client),
                );
                if let Err(e) = rpc_system.await {
                    tracing::error!("capnp RPC error: {e}");
                }
            });
        }
    }).await;

    Ok(())
}
