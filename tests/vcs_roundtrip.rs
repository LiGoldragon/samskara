/// Full VCS round-trip integration test.
///
/// 1. Load schema + seed into in-memory CozoDB
/// 2. Genesis commit → verify snapshot taken, HEAD set
/// 3. Assert a new thought (luna phase)
/// 4. Second commit → verify luna→sol promotion, deltas computed
/// 5. Restore to genesis → verify state matches original
/// 6. Third commit → verify hash matches genesis (same state = same hash)

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

fn query_str(db: &criome_cozo::CriomeDb, script: &str) -> serde_json::Value {
    db.run_script(script).unwrap_or_else(|e| panic!("query failed: {e}\nScript: {script}"))
}

fn row_count(result: &serde_json::Value) -> usize {
    result
        .get("rows")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0)
}

#[test]
fn full_vcs_roundtrip() {
    let db = criome_cozo::CriomeDb::open_memory().expect("open memory db");

    // Load schema
    load_script(&db, include_str!("../schema/samskara-world-init.cozo"));

    // Load seed
    load_script(&db, include_str!("../schema/samskara-world-seed.cozo"));

    // Load internal relations (intent, policy, evidence, etc.)
    load_script(&db, include_str!("../AI-init.cozo"));

    let vcs = samskara::vcs::WorldVcs::new(&db);

    // Verify seed loaded: 5 sol thoughts (t-project-2 is luna/staged)
    let thoughts = query_str(&db, "?[id] := *thought{id, phase}, phase == \"sol\"");
    assert_eq!(row_count(&thoughts), 5, "should have 5 sol-phase seeded thoughts");

    // ── Genesis commit ──────────────────────────────────────────────

    let genesis = vcs.commit(samskara::vcs::commit::CommitInput {
        message: "genesis", agent_id: "claude-code-1",
        session_id: "test-session", now: "2026-03-18T00:00:00Z",
    })
    .expect("genesis commit");

    assert!(genesis.snapshot_taken, "genesis must take a snapshot");
    assert!(genesis.parent_id.is_empty(), "genesis has no parent");
    assert!(!genesis.world_hash.is_empty(), "genesis hash must not be empty");

    let genesis_hash = genesis.world_hash.clone();

    // Verify world_commit exists
    let commits = query_str(&db, "?[id] := *world_commit{id}");
    assert_eq!(row_count(&commits), 1, "should have 1 commit");

    // Verify world_snapshot_index
    let snap_idx = query_str(&db, &format!(
        "?[snapshot_exists, delta_depth] := \
         *world_snapshot_index{{commit_id: \"{genesis_hash}\", snapshot_exists, delta_depth}}"
    ));
    assert_eq!(row_count(&snap_idx), 1, "snapshot index should exist");

    // Verify HEAD ref
    let head = query_str(&db, "?[ref_value] := *world_commit_ref{commit_id, ref_type: \"HEAD\", ref_value}");
    assert_eq!(row_count(&head), 1, "HEAD ref should exist");

    // Verify snapshots were stored
    let snaps = query_str(&db, &format!(
        "?[relation_name] := *world_snapshot{{commit_id: \"{genesis_hash}\", relation_name}}"
    ));
    assert!(row_count(&snaps) >= 10, "should have snapshots for all versioned relations");

    // ── Mutate: add a luna-phase thought ────────────────────────────

    db.run_script(
        r#"?[id, kind, scope, status, title, body, created_ts, updated_ts, phase, dignity] <- [[
            "t-new-1", "observation", "global", "draft", "New thought",
            "This is a test thought added after genesis.",
            "2026-03-18", "2026-03-18", "luna", "seen"
        ]]
        :put thought { id => kind, scope, status, title, body, created_ts, updated_ts, phase, dignity }"#,
    )
    .expect("assert new thought");

    // Verify luna thought exists
    let luna_thoughts = query_str(&db, "?[id] := *thought{id, phase}, phase == \"luna\"");
    assert_eq!(row_count(&luna_thoughts), 1, "should have 1 luna thought");

    // ── Second commit ───────────────────────────────────────────────

    let second = vcs.commit(samskara::vcs::commit::CommitInput {
        message: "add test thought", agent_id: "claude-code-1",
        session_id: "test-session", now: "2026-03-18T00:01:00Z",
    })
    .expect("second commit");

    assert!(!second.snapshot_taken, "second commit should NOT snapshot (delta_depth < 10)");
    assert_eq!(second.parent_id, genesis_hash, "parent should be genesis");
    assert_ne!(second.world_hash, genesis_hash, "hash should differ (new thought added)");

    let second_hash = second.world_hash.clone();

    // Verify luna→sol promotion happened
    let luna_after = query_str(&db, "?[id] := *thought{id, phase}, phase == \"luna\"");
    assert_eq!(row_count(&luna_after), 0, "no luna thoughts after commit (promoted to sol)");

    let sol_thoughts = query_str(&db, "?[id] := *thought{id, phase}, phase == \"sol\"");
    assert_eq!(row_count(&sol_thoughts), 7, "should have 7 sol thoughts (6 from genesis + 1 new)");

    // Verify deltas were recorded
    let deltas = query_str(&db, &format!(
        "?[seq] := *world_delta{{commit_id: \"{second_hash}\", seq}}"
    ));
    assert!(row_count(&deltas) > 0, "should have deltas for the changed relation");

    // ── Restore to genesis ──────────────────────────────────────────

    let restore = vcs.restore(&genesis_hash)
        .expect("restore to genesis");

    assert_eq!(restore.commit_id, genesis_hash);
    assert!(restore.relations_restored > 0);

    // Verify state matches genesis: 6 thoughts (the new one should be gone)
    let restored_thoughts = query_str(&db, "?[id] := *thought{id, phase}, phase == \"sol\"");
    assert_eq!(
        row_count(&restored_thoughts), 6,
        "restored state should have 6 thoughts (genesis state)"
    );

    // The new thought should not exist
    let new_thought = query_str(&db, "?[id] := *thought{id}, id == \"t-new-1\"");
    assert_eq!(row_count(&new_thought), 0, "new thought should not exist after restore");

    // ── Third commit after restore ──────────────────────────────────

    let third = vcs.commit(samskara::vcs::commit::CommitInput {
        message: "post-restore commit", agent_id: "claude-code-1",
        session_id: "test-session", now: "2026-03-18T00:02:00Z",
    })
    .expect("third commit");

    // Same state as genesis → same world hash
    assert_eq!(
        third.world_hash, genesis_hash,
        "same state should produce same hash (deterministic)"
    );

    eprintln!("VCS round-trip test passed!");
    eprintln!("  Genesis hash: {genesis_hash}");
    eprintln!("  Second hash:  {second_hash}");
    eprintln!("  Third hash:   {} (matches genesis)", third.world_hash);
}
