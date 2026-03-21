use std::sync::Arc;

use clap::Parser;
use std::path::PathBuf;

use samskara_core::boot;

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

/// Ontological bedrock — these relations are eternal dignity.
const ETERNAL_RELATIONS: &[&str] = &[
    "Aspect", "Dignity", "Element", "Enum", "Measure", "Modality",
    "Phase", "Planet", "Sign",
];

/// Relations from contracts (samskara-lojix).
const CONTRACT_RELATIONS: &[&str] = &[
    "transpiler_version", "eval_request", "eval_result",
];

/// Run the full genesis sequence: create all relations and load seed data.
fn genesis(db: &criome_cozo::CriomeDb) -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!("fresh database — running genesis");

    // 1. Core infrastructure (Phase, Dignity, world_schema, VCS)
    boot::core_genesis(db)?;

    // 2. Contract relations (Samskara <-> Lojix interface)
    samskara_lojix_contract::init(db)?;
    tracing::info!("contract relations loaded");

    // 3. Internal governance relations
    boot::load_cozo_script(db, include_str!("../AI-init.cozo"))?;
    tracing::info!("internal relations loaded");

    // 4. World schema + domain relations
    boot::load_cozo_script(db, include_str!("../schema/samskara-world-init.cozo"))?;
    tracing::info!("samskara-world relations created");

    // 5. Seed data
    boot::load_cozo_script(db, include_str!("../schema/samskara-world-seed.cozo"))?;
    tracing::info!("samskara-world seed loaded");

    // 6. Finalize: populate world_schema + create meta sentinel
    boot::finalize_genesis(db, ETERNAL_RELATIONS, CONTRACT_RELATIONS)?;

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
    if boot::is_initialized(&db) {
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
