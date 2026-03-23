use samskara_codegen::SchemaGenerator;

fn is_comment_only(stmt: &str) -> bool {
    stmt.lines()
        .all(|line| {
            let trimmed = line.trim();
            trimmed.is_empty() || trimmed.starts_with('#') || trimmed == "//"
        })
}

fn load_script(db: &criome_cozo::CriomeDb, script: &str) {
    for stmt in criome_cozo::Script::from_str(script) {
        let trimmed = stmt.trim();
        if !trimmed.is_empty() && !is_comment_only(trimmed) {
            db.run_script(trimmed)
                .unwrap_or_else(|e| panic!("load failed: {e}\nStatement: {trimmed}"));
        }
    }
}

/// Load the samskara-world schema + seed into an in-memory CozoDB, then
/// generate the .capnp output and verify it's deterministic and well-formed.
#[test]
fn full_pipeline_samskara_world() {
    let db = criome_cozo::CriomeDb::open_memory().expect("open memory db");

    // Load core schema first (Phase, Dignity, world_schema, VCS)
    load_script(&db, samskara_core::boot::CORE_WORLD_INIT);
    load_script(&db, samskara_core::boot::CORE_WORLD_SEED);

    // Load domain schema and seed
    load_script(&db, include_str!("../schema/samskara-world-init.cozo"));
    load_script(&db, include_str!("../schema/samskara-world-seed.cozo"));

    // Generate schema
    let schema = SchemaGenerator::from_db(&db).expect("from_db");

    // Should have detected Phase and Dignity as enums
    assert!(
        schema.enums.iter().any(|e| e.name == "Phase"),
        "should detect Phase enum"
    );
    assert!(
        schema.enums.iter().any(|e| e.name == "Dignity"),
        "should detect Dignity enum"
    );

    // Should have multiple relation structs
    assert!(
        schema.relations.len() >= 10,
        "expected at least 10 non-vocab relations, got {}",
        schema.relations.len()
    );

    // Generate .capnp text
    let capnp_text = schema.to_capnp_text().expect("to_capnp_text");

    // Basic structure checks
    assert!(capnp_text.contains("@0x"), "should have file ID");
    assert!(capnp_text.contains("struct Thought"), "should have Thought struct");
    assert!(capnp_text.contains("struct WorldCommit"), "should have WorldCommit struct");
    assert!(capnp_text.contains("enum Phase"), "should have Phase enum");
    assert!(capnp_text.contains("enum Dignity"), "should have Dignity enum");

    // Field naming conventions
    assert!(capnp_text.contains("createdTs"), "should camelCase created_ts");
    assert!(capnp_text.contains("parentId"), "should camelCase parent_id");

    // Determinism: generate twice, compare
    let capnp_text_2 = schema.to_capnp_text().expect("to_capnp_text second time");
    assert_eq!(capnp_text, capnp_text_2, "capnp output must be deterministic");

    // Hash determinism
    let hash_1 = schema.schema_hash().expect("hash 1");
    let hash_2 = schema.schema_hash().expect("hash 2");
    assert_eq!(hash_1, hash_2, "schema hash must be deterministic");

    eprintln!("--- Generated .capnp schema ---");
    eprintln!("{capnp_text}");
    eprintln!("--- Schema hash: {hash_1} ---");
}
