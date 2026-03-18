use std::io::Write;
use std::path::PathBuf;

/// Returns true if the statement is only comments (no executable CozoScript).
fn is_comment_only(stmt: &str) -> bool {
    stmt.lines().all(|line| {
        let trimmed = line.trim();
        trimmed.is_empty() || trimmed.starts_with('#') || trimmed == "//"
    })
}

fn load_script(db: &criome_cozo::CriomeDb, script: &str) {
    for stmt in criome_cozo::Script::from_str(script) {
        let trimmed = stmt.trim();
        if !trimmed.is_empty() && !is_comment_only(trimmed) {
            db.run_script(trimmed)
                .unwrap_or_else(|e| panic!("script load failed: {e}\nStatement: {trimmed}"));
        }
    }
}

fn main() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    // Open in-memory CriomeDb, load schema + seed from their authoritative files
    let db = criome_cozo::CriomeDb::open_memory().expect("open memory db for codegen");
    load_script(&db, include_str!("schema/samskara-world-init.cozo"));
    load_script(&db, include_str!("schema/samskara-world-seed.cozo"));

    // Generate capnp schema from the fully populated database
    let schema =
        samskara_codegen::SchemaGenerator::from_db(&db).expect("codegen schema generation");

    let capnp_text = schema.to_capnp_text().expect("capnp text generation");
    let capnp_path = out_dir.join("samskara_world.capnp");
    std::fs::write(&capnp_path, &capnp_text).expect("write .capnp file");

    capnpc::CompilerCommand::new()
        .src_prefix(&out_dir)
        .file(&capnp_path)
        .run()
        .expect("capnp schema compilation failed");

    let hash = schema.schema_hash().expect("schema hash");
    let hash_path = out_dir.join("schema_hash.txt");
    let mut f = std::fs::File::create(&hash_path).expect("create schema_hash.txt");
    write!(f, "{hash}").expect("write schema hash");

    println!("cargo:rerun-if-changed=schema/samskara-world-init.cozo");
    println!("cargo:rerun-if-changed=schema/samskara-world-seed.cozo");
}
