use anyhow::Result;
use tokio::net::UnixStream;
use capnp_rpc::{rpc_twoparty_capnp, twoparty, RpcSystem};
use futures::{FutureExt, AsyncReadExt};
use tokio_util::compat::TokioAsyncReadCompatExt;
use std::path::PathBuf;

pub mod samskara_world_capnp {
    include!(concat!(env!("OUT_DIR"), "/samskara_world_capnp.rs"));
}

#[tokio::main]
async fn main() -> Result<()> {
    let socket_path = PathBuf::from(".samskara/samskara.sock");
    let stream = UnixStream::connect(&socket_path).await?;
    let (reader, writer) = stream.compat().split();
    let network = twoparty::VatNetwork::new(reader, writer, rpc_twoparty_capnp::Side::Client, Default::default());
    let mut rpc_system = RpcSystem::new(Box::new(network), None);
    let world_client: samskara_world_capnp::samskara_world::Client = rpc_system.bootstrap(rpc_twoparty_capnp::Side::Server);
    
    let local = tokio::task::LocalSet::new();
    local.run_until(async move {
        tokio::task::spawn_local(rpc_system.map(|_| ()));

        println!("--- Initializing Sandbox Schema ---");
        let schema_scripts = [
            ":create sandbox { id: String => path: String, agent_id: String, bookmark: String, status: String, intent: String }",
            ":create sandbox_event { sandbox_id: String, timestamp: Int => type: String, message: String }"
        ];

        for script in schema_scripts {
            let mut put_request = world_client.put_request();
            put_request.get().set_script(script);
            let _ = put_request.send().promise.await.unwrap();
            println!("Schema updated: {}", script);
        }

        println!("\n--- Registering Test Sandbox ---");
        let reg_script = "?[id, path, agent_id, bookmark, status, intent] <- [[\"sb-001\", \"Sandboxes/sb-001\", \"agent-alpha\", \"feature-x\", \"active\", \"Implement AST-based editing\"]]; :put sandbox { id => path, agent_id, bookmark, status, intent }";
        let mut put_request = world_client.put_request();
        put_request.get().set_script(reg_script);
        let _ = put_request.send().promise.await.unwrap();
        println!("Sandbox registered.");

        println!("\n--- Logging Test Event ---");
        let event_script = "?[sandbox_id, timestamp, type, message] <- [[\"sb-001\", 1710000000, \"commit_created\", \"Initial AST logic draft\"]]; :put sandbox_event { sandbox_id, timestamp => type, message }";
        let mut put_request = world_client.put_request();
        put_request.get().set_script(event_script);
        let _ = put_request.send().promise.await.unwrap();
        println!("Event logged.");

        println!("\n--- Querying Active Sandboxes ---");
        let mut query_request = world_client.query_request();
        query_request.get().set_script("?[id, agent_id, intent] := *sandbox{id, agent_id, intent, status}");
        let query_response = query_request.send().promise.await.unwrap();
        let result_json = query_response.get().unwrap().get_result().unwrap();
        println!("All Sandboxes: {}", result_json.to_str().unwrap());

        println!("\n--- Querying Events for sb-001 ---");
        let mut query_request = world_client.query_request();
        query_request.get().set_script("?[type, message] := *sandbox_event{sandbox_id, type, message}");
        let query_response = query_request.send().promise.await.unwrap();
        let result_json = query_response.get().unwrap().get_result().unwrap();
        println!("Events: {}", result_json.to_str().unwrap());
    }).await;

    Ok(())
}
