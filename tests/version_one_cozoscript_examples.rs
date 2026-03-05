use cozo::{DbInstance, ScriptMutability};
use std::time::{SystemTime, UNIX_EPOCH};

fn from_temp_db() -> DbInstance {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time must be monotonic")
        .as_nanos();
    let db_path = std::env::temp_dir().join(format!("samskara-version-one-{timestamp}.db"));
    DbInstance::new("sqlite", db_path.to_str().expect("utf8 path"), Default::default())
        .expect("create sqlite db")
}

fn apply_world_schema(db: &DbInstance) {
    let schemas = [
        ":create lane_policy { lane: String, rule: String => value: String }",
        ":create agent_role { agent: String, role: String => scope: String }",
        ":create tx_log { tx: Int => ts: Int, issuer: String, reason: String }",
        ":create statement { id: String => subj: String, pred: String, obj: String, tx: Int, source: String, confidence: Float, plane: String, state: String }",
    ];

    for schema in schemas {
        db.run_script(schema, Default::default(), ScriptMutability::Mutable)
            .expect("create schema");
    }
}

fn apply_seed(db: &DbInstance) {
    let seed_directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data");
    let mut seed_paths: Vec<std::path::PathBuf> = std::fs::read_dir(&seed_directory)
        .expect("read seed directory")
        .filter_map(|entry| entry.ok().map(|item| item.path()))
        .filter(|path| {
            path.is_file()
                && path.extension().and_then(|ext| ext.to_str()) == Some("cozo")
                && path.file_name().and_then(|name| name.to_str()).map(|name| name.contains("seed")).unwrap_or(false)
        })
        .collect();
    seed_paths.sort();

    for seed_path in seed_paths {
        let seed_script = std::fs::read_to_string(&seed_path)
            .expect("read seed script");
        db.run_script(&seed_script, Default::default(), ScriptMutability::Mutable)
            .expect("seed script must apply");
    }
}

#[test]
fn cozoscript_agent_examples_are_executable() {
    let db = from_temp_db();
    apply_world_schema(&db);
    apply_seed(&db);

    let queries = [
        "?[lane, rule, value] := *lane_policy{lane, rule, value} :order lane, rule",
        "?[component, status] := *statement{subj: component, pred: \"status\", obj: status, plane: \"versionone\", state: \"accepted\"}, status = \"selected\"",
        "?[id, subj, pred, obj, confidence] := *statement{id, subj, pred, obj, source: \"llm-inference\", state: \"proposed\", confidence} :order -confidence",
        "?[role] := *agent_role{agent: \"dev\", role}",
        "?[tx, reason, issuer, ts] := *tx_log{tx, reason, issuer, ts}, *statement{tx, pred: \"decision\", obj: \"deny\"} :order -ts",
        "?[pred, obj, state, source] := *statement{subj: \"lane-governor\", pred, obj, state, source} :order pred",
    ];

    for query in queries {
        let result = db
            .run_script(query, Default::default(), ScriptMutability::Immutable)
            .unwrap_or_else(|error| panic!("query should execute: {query}\nerror: {error:?}"));
        assert!(
            !result.rows.is_empty(),
            "query must return at least one row: {query}"
        );
    }
}
