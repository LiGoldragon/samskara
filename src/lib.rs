pub mod mcp;
pub mod rpc;
pub mod schema;

#[allow(unused)]
pub mod samskara_rpc_capnp {
    include!(concat!(env!("OUT_DIR"), "/samskara_rpc_capnp.rs"));
}
