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

    // 5. Finalize: create meta sentinel (must be last)
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

    // Start MCP server on stdio
    let db = Arc::new(db);
    let server = samskara::mcp::SamskaraMcp::new(db);

    tracing::info!("samskara MCP server starting on stdio");
    let service = rmcp::ServiceExt::serve(server, rmcp::transport::stdio()).await?;
    service.waiting().await?;

    Ok(())
}
