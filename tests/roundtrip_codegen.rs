/// Two-way roundtrip test for CozoScript codegen.
///
/// Verifies that the DB→.cozo→DB pipeline is lossless:
///   1. seed → DB₁ → generated init+seed → DB₂ → generated init+seed → compare text (must match)
///   2. DB₁ rows == DB₂ rows (every manifest-phase row survives the roundtrip)
///
/// This is the correctness proof that .cozo files are faithful projections of the DB.

use samskara_codegen::SchemaGenerator;

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
                .unwrap_or_else(|e| panic!("load failed: {e}\nStatement: {trimmed}"));
        }
    }
}

/// Query all manifest-phase rows from a relation (or all rows if no phase column),
/// sorted by first column. Returns canonical JSON string.
fn query_sorted(db: &criome_cozo::CriomeDb, rel: &str) -> String {
    let cols_result = db.run_script(&format!("::columns {rel}")).unwrap();
    let cols: Vec<String> = cols_result["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|row| {
            row.as_array()?.first()
                .and_then(|v| v.get("Str").and_then(|s| s.as_str()).or(v.as_str()))
                .map(String::from)
        })
        .collect();

    let has_phase = cols.iter().any(|c| c == "phase");
    let col_list = cols.join(", ");
    let order_col = &cols[0];

    // Only compare manifest-phase rows — becoming-phase data is intentionally
    // excluded from seed codegen (it hasn't been committed yet).
    let query = if has_phase {
        format!("?[{col_list}] := *{rel}{{{col_list}}}, phase == \"manifest\" :order {order_col}")
    } else {
        format!("?[{col_list}] := *{rel}{{{col_list}}} :order {order_col}")
    };
    let result = db.run_script(&query).unwrap();
    serde_json::to_string(&result["rows"]).unwrap()
}

/// Get all non-infrastructure relation names from a DB.
fn relation_names(db: &criome_cozo::CriomeDb) -> Vec<String> {
    let result = db.run_script("::relations").unwrap();
    let mut names: Vec<String> = result["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|row| {
            row.as_array()?.first()
                .and_then(|v| v.get("Str").and_then(|s| s.as_str()).or(v.as_str()))
                .map(String::from)
        })
        .collect();
    names.sort();
    names
}

#[test]
fn cozo_roundtrip_is_lossless() {
    // ── DB₁: load from original .cozo files ──────────────────────────

    let db1 = criome_cozo::CriomeDb::open_memory().expect("open db1");
    load_script(&db1, include_str!("../schema/samskara-world-init.cozo"));
    load_script(&db1, include_str!("../schema/samskara-world-seed.cozo"));

    let schema1 = SchemaGenerator::from_db(&db1).expect("schema from db1");

    // Generate init + seed from DB₁
    let init_text_1 = schema1.to_cozo_init_text(&db1).expect("init from db1");
    let seed_text_1 = schema1.to_cozo_seed_text(&db1).expect("seed from db1");

    eprintln!("--- Generated init (from DB₁) ---");
    eprintln!("{init_text_1}");
    eprintln!("--- Generated seed (from DB₁) ---");
    eprintln!("{seed_text_1}");

    // ── DB₂: load from generated .cozo text ──────────────────────────

    let db2 = criome_cozo::CriomeDb::open_memory().expect("open db2");
    load_script(&db2, &init_text_1);
    load_script(&db2, &seed_text_1);

    let schema2 = SchemaGenerator::from_db(&db2).expect("schema from db2");

    // Generate init + seed from DB₂
    let init_text_2 = schema2.to_cozo_init_text(&db2).expect("init from db2");
    let seed_text_2 = schema2.to_cozo_seed_text(&db2).expect("seed from db2");

    // ── Assert text roundtrip: generated₁ == generated₂ ─────────────

    assert_eq!(
        init_text_1, init_text_2,
        "init text must survive DB₁ → .cozo → DB₂ → .cozo roundtrip"
    );
    assert_eq!(
        seed_text_1, seed_text_2,
        "seed text must survive DB₁ → .cozo → DB₂ → .cozo roundtrip"
    );

    // ── Assert data roundtrip: every relation's rows match ───────────

    let names1 = relation_names(&db1);
    let names2 = relation_names(&db2);
    assert_eq!(names1, names2, "both DBs must have the same relations");

    for name in &names1 {
        let rows1 = query_sorted(&db1, name);
        let rows2 = query_sorted(&db2, name);
        assert_eq!(
            rows1, rows2,
            "rows in '{name}' must match between DB₁ and DB₂"
        );
    }

    // ── Assert schema hash stability ─────────────────────────────────

    let hash1 = schema1.schema_hash().expect("hash1");
    let hash2 = schema2.schema_hash().expect("hash2");
    assert_eq!(hash1, hash2, "schema hash must be identical across roundtrip");

    eprintln!("Roundtrip test passed!");
    eprintln!("  Relations: {}", names1.len());
    eprintln!("  Schema hash: {hash1}");
    eprintln!("  Init text: {} bytes", init_text_1.len());
    eprintln!("  Seed text: {} bytes", seed_text_1.len());
}
