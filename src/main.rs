use anyhow::{Result, Context};
use cozo::{DbInstance, ScriptMutability, DataValue, Num};
use std::fs;
use walkdir::WalkDir;
use std::path::{Path, PathBuf};
use ractor::{Actor, ActorRef, ActorProcessingErr};
use tokio::net::UnixListener;
use capnp_rpc::{rpc_twoparty_capnp, twoparty, RpcSystem, pry};
use futures::{FutureExt, AsyncReadExt};
use tokio::task::LocalSet;

#[allow(unused_parens, dead_code, unused_imports, non_snake_case, unused_qualifications)]
pub mod samskara_world_capnp {
    include!(concat!(env!("OUT_DIR"), "/samskara_world_capnp.rs"));
}

// --- Actor Implementation ---

struct WorldState {
    db: DbInstance,
    db_path: PathBuf,
}

enum WorldMsg {
    Query(String, ractor::RpcReplyPort<Result<String>>),
    Put(String, ractor::RpcReplyPort<Result<()>>),
    Rescan(ractor::RpcReplyPort<Result<()>>),
    GetStatus(ractor::RpcReplyPort<WorldStatus>),
}

struct WorldStatus {
    version: String,
    db_path: String,
    component_count: u32,
    file_count: u32,
}

struct WorldActor;

#[ractor::async_trait]
impl Actor for WorldActor {
    type Msg = WorldMsg;
    type State = WorldState;
    type Arguments = PathBuf;

    async fn pre_start(&self, _myself: ActorRef<Self::Msg>, db_path: Self::Arguments) -> Result<Self::State, ActorProcessingErr> {
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent).map_err(|e| ActorProcessingErr::from(anyhow::anyhow!(e)))?;
        }
        
        let db = DbInstance::new("sqlite", db_path.to_str().unwrap(), Default::default())
            .map_err(|e| ActorProcessingErr::from(anyhow::anyhow!("{:?}", e)))?;

        // Initialize schema
        let schemas = [
            ":create component { name: String => rating: Int, description: String }",
            ":create agent_skill { name: String => path: String, description: String, push_branch: String }",
            ":create dependency { dependent: String, dependency: String }",
            ":create config_file { path: String => component: String, ext: String }",
            ":create sandbox { id: String => path: String, agent_id: String, bookmark: String, status: String, intent: String }",
            ":create sandbox_event { sandbox_id: String, timestamp: Int => type: String, message: String }",
            ":create lane_policy { lane: String, rule: String => value: String }",
            ":create agent_role { agent: String, role: String => scope: String }",
            ":create tx_log { tx: Int => ts: Int, issuer: String, reason: String }",
            ":create statement { id: String => subj: String, pred: String, obj: String, tx: Int, source: String, confidence: Float, plane: String, state: String }"
        ];
        for s in schemas {
            let _ = db.run_script(s, Default::default(), ScriptMutability::Mutable);
        }

        let mut state = WorldState { db, db_path };
        apply_seed_scripts(&mut state)?;
        perform_rescan(&mut state).await?;

        Ok(state)
    }

    async fn handle(&self, _myself: ActorRef<Self::Msg>, msg: Self::Msg, state: &mut Self::State) -> Result<(), ActorProcessingErr> {
        match msg {
            WorldMsg::Query(script, reply) => {
                let res = state.db.run_script(&script, Default::default(), ScriptMutability::Immutable)
                    .map(|r| serde_json::to_string(&r).unwrap())
                    .map_err(|e| anyhow::anyhow!("{:?}", e));
                reply.send(res)?;
            }
            WorldMsg::Put(script, reply) => {
                let res = state.db.run_script(&script, Default::default(), ScriptMutability::Mutable)
                    .map(|_| ())
                    .map_err(|e| anyhow::anyhow!("{:?}", e));
                reply.send(res)?;
            }
            WorldMsg::Rescan(reply) => {
                let res = perform_rescan(state).await;
                reply.send(res)?;
            }
            WorldMsg::GetStatus(reply) => {
                let comp_count = match state.db.run_script("?[count(name)] := *component{name}", Default::default(), ScriptMutability::Immutable)
                    .ok().and_then(|r| r.rows.get(0)?.get(0).cloned()) {
                        Some(DataValue::Num(Num::Int(i))) => i as u32,
                        _ => 0,
                    };
                let file_count = match state.db.run_script("?[count(path)] := *config_file{path}", Default::default(), ScriptMutability::Immutable)
                    .ok().and_then(|r| r.rows.get(0)?.get(0).cloned()) {
                        Some(DataValue::Num(Num::Int(i))) => i as u32,
                        _ => 0,
                    };
                
                reply.send(WorldStatus {
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    db_path: state.db_path.to_string_lossy().into_owned(),
                    component_count: comp_count,
                    file_count,
                })?;
            }
        }
        Ok(())
    }
}

fn apply_seed_scripts(state: &mut WorldState) -> Result<()> {
    let seed_directory = Path::new("Components/samskara/data");
    if !seed_directory.exists() {
        return Ok(());
    }

    let mut seed_files: Vec<PathBuf> = fs::read_dir(seed_directory)?
        .filter_map(|entry| entry.ok().map(|item| item.path()))
        .filter(|path| {
            path.is_file()
                && path.extension().and_then(|ext| ext.to_str()) == Some("cozo")
                && path.file_name().and_then(|name| name.to_str()).map(|name| name.contains("seed")).unwrap_or(false)
        })
        .collect();
    seed_files.sort();

    for script_path in seed_files {
        let script = fs::read_to_string(&script_path)
            .with_context(|| format!("failed to read seed script {}", script_path.display()))?;
        state
            .db
            .run_script(&script, Default::default(), ScriptMutability::Mutable)
            .map_err(|error| anyhow::anyhow!("failed to run seed script {}: {:?}", script_path.display(), error))?;
    }

    Ok(())
}

async fn perform_rescan(state: &mut WorldState) -> Result<()> {
    let _ = state.db.run_script(":rm component { name }", Default::default(), ScriptMutability::Mutable);
    let _ = state.db.run_script(":rm config_file { path }", Default::default(), ScriptMutability::Mutable);

    let mut insert_script = String::from("?[name, rating, description] <- [\n");
    let mut count = 0;
    let components_dir = fs::read_dir("Components").context("Failed to read Components directory")?;
    for entry in components_dir {
        let entry = entry?;
        if entry.path().is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                insert_script.push_str(&format!("    [\"{}\", 1, \"Component {}\"],\n", name, name));
                count += 1;
            }
        }
    }
    insert_script.push_str("];\n:put component { name => rating, description }");
    if count > 0 {
        state.db.run_script(&insert_script, Default::default(), ScriptMutability::Mutable).map_err(|e| anyhow::anyhow!("{:?}", e))?;
    }

    let mut files_insert = String::from("?[path, component, ext] <- [\n");
    let mut files_count = 0;
    
    for entry in WalkDir::new("Components").into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        if p.is_file() {
            if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                if ext == "edn" || ext == "capnp" {
                    let path_str = p.to_string_lossy().replace("\\", "/");
                    let parts: Vec<&str> = path_str.split('/').collect();
                    let comp_name = if parts.len() > 1 { parts[1] } else { "unknown" };
                    files_insert.push_str(&format!("    [\"{}\", \"{}\", \"{}\"],\n", path_str, comp_name, ext));
                    files_count += 1;
                }
            }
        }
    }
    for entry in WalkDir::new("Core").into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        if p.is_file() {
            if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                if ext == "edn" {
                    let path_str = p.to_string_lossy().replace("\\", "/");
                    files_insert.push_str(&format!("    [\"{}\", \"Core\", \"{}\"],\n", path_str, ext));
                    files_count += 1;
                }
            }
        }
    }
    for entry in WalkDir::new("VersionOne").into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        if p.is_file() {
            if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                if ext == "edn" || ext == "md" {
                    let path_str = p.to_string_lossy().replace("\\", "/");
                    files_insert.push_str(&format!("    [\"{}\", \"VersionOne\", \"{}\"],\n", path_str, ext));
                    files_count += 1;
                }
            }
        }
    }
    files_insert.push_str("];\n:put config_file { path => component, ext }");
    if files_count > 0 {
        state.db.run_script(&files_insert, Default::default(), ScriptMutability::Mutable).map_err(|e| anyhow::anyhow!("{:?}", e))?;
    }

    Ok(())
}

// --- RPC Implementation ---

struct SamskaraWorldImpl {
    world_actor: ActorRef<WorldMsg>,
}

impl samskara_world_capnp::samskara_world::Server for SamskaraWorldImpl {
    fn query(
        &mut self,
        params: samskara_world_capnp::samskara_world::QueryParams,
        mut results: samskara_world_capnp::samskara_world::QueryResults,
    ) -> capnp::capability::Promise<(), capnp::Error> {
        let script = match pry!(pry!(params.get()).get_script()).to_string() {
            Ok(s) => s,
            Err(e) => return capnp::capability::Promise::err(capnp::Error::failed(format!("{:?}", e))),
        };
        let actor = self.world_actor.clone();
        
        capnp::capability::Promise::from_future(async move {
            let res = ractor::call!(actor, WorldMsg::Query, script).map_err(|e| capnp::Error::failed(format!("{:?}", e)))?;
            let mut results_builder = results.get();
            results_builder.set_result(&res.map_err(|e| capnp::Error::failed(format!("{:?}", e)))?);
            Ok(())
        })
    }

    fn put(
        &mut self,
        params: samskara_world_capnp::samskara_world::PutParams,
        _results: samskara_world_capnp::samskara_world::PutResults,
    ) -> capnp::capability::Promise<(), capnp::Error> {
        let script = match pry!(pry!(params.get()).get_script()).to_string() {
            Ok(s) => s,
            Err(e) => return capnp::capability::Promise::err(capnp::Error::failed(format!("{:?}", e))),
        };
        let actor = self.world_actor.clone();

        capnp::capability::Promise::from_future(async move {
            let _ = ractor::call!(actor, WorldMsg::Put, script).map_err(|e| capnp::Error::failed(format!("{:?}", e)))?;
            Ok(())
        })
    }

    fn rescan(
        &mut self,
        _params: samskara_world_capnp::samskara_world::RescanParams,
        _results: samskara_world_capnp::samskara_world::RescanResults,
    ) -> capnp::capability::Promise<(), capnp::Error> {
        let actor = self.world_actor.clone();
        capnp::capability::Promise::from_future(async move {
            let _ = ractor::call!(actor, WorldMsg::Rescan).map_err(|e| capnp::Error::failed(format!("{:?}", e)))?;
            Ok(())
        })
    }

    fn get_status(
        &mut self,
        _params: samskara_world_capnp::samskara_world::GetStatusParams,
        mut results: samskara_world_capnp::samskara_world::GetStatusResults,
    ) -> capnp::capability::Promise<(), capnp::Error> {
        let actor = self.world_actor.clone();
        capnp::capability::Promise::from_future(async move {
            let status = ractor::call!(actor, WorldMsg::GetStatus).map_err(|e| capnp::Error::failed(format!("{:?}", e)))?;
            let mut status_builder = results.get().init_status();
            status_builder.set_version(&status.version);
            status_builder.set_db_path(&status.db_path);
            status_builder.set_component_count(status.component_count);
            status_builder.set_file_count(status.file_count);
            Ok(())
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let db_path = PathBuf::from(".samskara/samskara.db");
    let socket_path = PathBuf::from(".samskara/samskara.sock");

    if socket_path.exists() {
        fs::remove_file(&socket_path)?;
    }

    // Start Actor
    let (actor, _handle) = Actor::spawn(None, WorldActor, db_path).await
        .map_err(|e| anyhow::anyhow!("Failed to spawn WorldActor: {:?}", e))?;

    println!("Saṃskāra World Daemon started.");
    println!("Socket: {:?}", socket_path);

    let listener = UnixListener::bind(&socket_path)?;
    let world_client: samskara_world_capnp::samskara_world::Client = capnp_rpc::new_client(SamskaraWorldImpl { world_actor: actor });

    let local = LocalSet::new();
    local.run_until(async move {
        loop {
            let (stream, _) = listener.accept().await?;
            let (reader, writer) = tokio_util::compat::TokioAsyncReadCompatExt::compat(stream).split();
            let network = twoparty::VatNetwork::new(reader, writer, rpc_twoparty_capnp::Side::Server, Default::default());
            let rpc_system = RpcSystem::new(Box::new(network), Some(world_client.clone().client));
            tokio::task::spawn_local(rpc_system.map(|_| ()));
        }
    }).await
}
