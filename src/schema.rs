pub mod samskara_world_capnp {
    include!(concat!(env!("OUT_DIR"), "/samskara_world_capnp.rs"));
}

pub const SCHEMA_HASH: &str = include_str!(concat!(env!("OUT_DIR"), "/schema_hash.txt"));
