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

        println!("Querying status...");
        let request = world_client.get_status_request();
        let response = request.send().promise.await.unwrap();
        let status = response.get().unwrap().get_status().unwrap();
        
        println!("Status:");
        println!("  Version: {}", status.get_version().unwrap().to_str().unwrap());
        println!("  DB Path: {}", status.get_db_path().unwrap().to_str().unwrap());
        println!("  Component Count: {}", status.get_component_count());
        println!("  File Count: {}", status.get_file_count());

        println!("\nQuerying components...");
        let mut query_request = world_client.query_request();
        query_request.get().set_script("?[name, rating] := *component{name, rating}");
        let query_response = query_request.send().promise.await.unwrap();
        let result_json = query_response.get().unwrap().get_result().unwrap();
        println!("Components: {}", result_json.to_str().unwrap());
    }).await;

    Ok(())
}
