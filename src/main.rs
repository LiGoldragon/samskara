use std::sync::Arc;

use clap::Parser;
use std::path::PathBuf;

/// Returns true if the statement is only comments (no executable CozoScript).
/// Handles `#` comments and `//` section separators (COZO_PATTERNS convention).
fn is_comment_only(stmt: &str) -> bool {
    stmt.lines()
        .all(|line| {
            let trimmed = line.trim();
            trimmed.is_empty() || trimmed.starts_with('#') || trimmed == "//"
        })
}

/// Load a CozoScript file into CozoDB, skipping comment-only blocks.
fn load_cozo_script(
    db: &criome_cozo::CriomeDb,
    script: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    for stmt in criome_cozo::Script::from_str(script) {
        let trimmed = stmt.trim();
        if !trimmed.is_empty() && !is_comment_only(trimmed) {
            db.run_script(trimmed)?;
        }
    }
    Ok(())
}

/// Samskara — the pure datalog agent.
/// Runs as an MCP server over stdio. Its entire world is CozoDB relations.
#[derive(Parser)]
#[command(name = "samskara", about = "Pure datalog agent — MCP server mode")]
struct Args {
    /// Path to the sqlite-backed CozoDB database.
    /// Defaults to "world.db" in the current directory.
    #[arg(long, value_name = "DB_PATH")]
    db_path: Option<PathBuf>,

    /// Use an in-memory database instead of sqlite.
    #[arg(long)]
    memory: bool,
}

/// Check if the database has already been initialized by looking for the meta relation.
fn is_initialized(db: &criome_cozo::CriomeDb) -> bool {
    db.run_script("::columns meta").is_ok()
}

/// Ontological bedrock — these relations are eternal dignity.
const ETERNAL_RELATIONS: &[&str] = &[
    "Aspect", "Dignity", "Element", "Enum", "Measure", "Modality",
    "Phase", "Planet", "Sign",
];

/// Reconstruct a `:create` statement from `::columns` output for a relation.
fn create_script_for(
    db: &criome_cozo::CriomeDb,
    rel: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let result = db.run_script(&format!("::columns {rel}"))?;
    let rows = result["rows"]
        .as_array()
        .ok_or("no columns rows")?;

    let mut keys = Vec::new();
    let mut vals = Vec::new();

    for row in rows {
        let arr = row.as_array().ok_or("bad column row")?;
        let name = arr[0]
            .get("Str").and_then(|s| s.as_str())
            .or_else(|| arr[0].as_str())
            .ok_or("no column name")?;
        let is_key = arr[1]
            .get("Bool").and_then(|b| b.as_bool())
            .or_else(|| arr[1].as_bool())
            .unwrap_or(false);
        let col_type = arr[3]
            .get("Str").and_then(|s| s.as_str())
            .or_else(|| arr[3].as_str())
            .ok_or("no column type")?;

        let col_def = format!("{name}: {col_type}");
        if is_key {
            keys.push(col_def);
        } else {
            vals.push(col_def);
        }
    }

    let body = if vals.is_empty() {
        keys.join(", ")
    } else {
        format!("{} => {}", keys.join(", "), vals.join(", "))
    };

    Ok(format!(":create {rel} {{ {body} }}"))
}

/// Populate world_schema by introspecting all relations in the database.
fn populate_world_schema(db: &criome_cozo::CriomeDb) -> Result<(), Box<dyn std::error::Error>> {
    let relations = db.run_script("::relations")?;
    let rows = relations["rows"]
        .as_array()
        .ok_or("no relations rows")?;

    for row in rows {
        let name = row.as_array()
            .and_then(|a| a[0].get("Str").and_then(|s| s.as_str()).or_else(|| a[0].as_str()))
            .ok_or("no relation name")?;

        // Skip world_schema itself — it's the one writing
        if name == "world_schema" {
            continue;
        }

        let script = create_script_for(db, name)?;
        let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");

        let dignity = if ETERNAL_RELATIONS.contains(&name) {
            "eternal"
        } else {
            "proven"
        };

        let origin = if ["transpiler_version", "eval_request", "eval_result"]
            .contains(&name) {
            "contract"
        } else {
            "genesis"
        };

        let put = format!(
            r#"?[relation_name, create_script, origin, phase, dignity] <- [[
                "{}", "{}", "{}", "manifest", "{}"
            ]]
            :put world_schema {{ relation_name => create_script, origin, phase, dignity }}"#,
            esc(name), esc(&script), origin, dignity,
        );
        db.run_script(&put)?;
    }

    tracing::info!("world_schema populated with all relation definitions");
    Ok(())
}

/// Run the full genesis sequence: create all relations and load seed data.
/// Only runs on a fresh (uninitialized) database.
fn genesis(db: &criome_cozo::CriomeDb) -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!("fresh database — running genesis");

    // 1. Contract relations (Samskara <-> Lojix interface)
    samskara_lojix_contract::init(db)?;
    tracing::info!("contract relations loaded");

    // 2. Internal relations
    load_cozo_script(db, include_str!("../AI-init.cozo"))?;
    tracing::info!("internal relations loaded");

    // 3. World schema
    load_cozo_script(db, include_str!("../schema/samskara-world-init.cozo"))?;
    tracing::info!("samskara-world relations created");

    // 4. Seed data
    load_cozo_script(db, include_str!("../schema/samskara-world-seed.cozo"))?;
    tracing::info!("samskara-world seed loaded");

    // 5. Populate world_schema by introspecting all relations
    populate_world_schema(db)?;

    // 6. Finalize: create meta sentinel (must be last)
    db.run_script(":create meta { key: String => value: String }")?;
    db.run_script(r#"?[key, value] <- [["schema_version", "1"]] :put meta { key => value }"#)?;
    tracing::info!("genesis complete — meta sentinel written");

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Tracing to stderr — stdout is reserved for MCP JSON-RPC
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let args = Args::parse();

    // Open CriomeDb — default to world.db in current directory
    let db = if args.memory {
        tracing::info!("opening in-memory db");
        criome_cozo::CriomeDb::open_memory()?
    } else {
        let path = args.db_path.unwrap_or_else(|| PathBuf::from("world.db"));
        tracing::info!("opening sqlite db at {}", path.display());
        criome_cozo::CriomeDb::open_sqlite(&path)?
    };

    // Idempotent boot: only run genesis on a fresh database
    if is_initialized(&db) {
        tracing::info!("database already initialized — skipping genesis");
    } else {
        genesis(&db)?;
    }

    // List all relations
    let relations = db.run_script("::relations")?;
    tracing::info!("active relations: {relations}");

    // Open content-addressed store at ~/.criome/store
    let store_path = dirs::home_dir()
        .expect("no home directory")
        .join(".criome")
        .join("store");
    let store = criome_store::Store::open(&store_path)
        .expect("failed to open criome store");
    tracing::info!("content store at {}", store_path.display());

    // Start MCP server on stdio
    let db = Arc::new(db);
    let store = Arc::new(store);
    let server = samskara::mcp::SamskaraMcp::new(db, store);

    tracing::info!("samskara MCP server starting on stdio");
    let service = rmcp::ServiceExt::serve(server, rmcp::transport::stdio()).await?;
    service.waiting().await?;

    Ok(())
}
