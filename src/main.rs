// mod agent; // future module
mod jj_mirror;

use clap::Parser;
use std::path::PathBuf;

/// Samskara — the pure datalog agent.
/// Its entire world is CozoDB relations. It never sees files, code, or OS state.
#[derive(Parser)]
#[command(name = "samskara", about = "Pure datalog agent — sees only relations")]
struct Args {
    /// Path to the sqlite-backed CozoDB database.
    #[arg(value_name = "DB_PATH")]
    db_path: Option<PathBuf>,

    /// Use an in-memory database instead of sqlite.
    #[arg(long)]
    memory: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Open CriomeDb
    let db = if args.memory || args.db_path.is_none() {
        eprintln!("samskara: opening in-memory db");
        criome_cozo::CriomeDb::open_memory()?
    } else {
        let path = args.db_path.as_ref().unwrap();
        eprintln!("samskara: opening sqlite db at {}", path.display());
        criome_cozo::CriomeDb::open_sqlite(path)?
    };

    // Load contract relations (Samskara <-> Lojix interface)
    samskara_lojix_contract::init(&db)?;
    eprintln!("samskara: contract relations loaded");

    // Load internal relations
    let init_script = include_str!("../AI-init.cozo");
    for stmt in criome_cozo::split_cozo_statements(init_script) {
        if !stmt.trim().is_empty() {
            db.run_script(stmt)?;
        }
    }
    eprintln!("samskara: internal relations initialized");

    // Seed commit type vocabulary
    jj_mirror::seed_commit_types(&db)?;
    eprintln!("samskara: commit type vocabulary seeded");

    // List all relations
    let relations = db.run_script("::relations")?;
    eprintln!("samskara: active relations: {relations}");

    eprintln!("samskara: agent loop not yet implemented — exiting");
    Ok(())
}
