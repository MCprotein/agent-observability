use super::*;

const PREVIOUS_PRUNE_SQL: &str = "DELETE FROM adapter_dispositions WHERE rowid NOT IN (SELECT rowid FROM adapter_dispositions ORDER BY rowid DESC LIMIT ?1)";

fn ledger(rowids: &[i64]) -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    // Use the complete current schema, including the disposition code index.
    for (_, _, ddl) in SCHEMA_OBJECTS {
        db.execute_batch(ddl).unwrap();
    }
    let tx = db.transaction().unwrap();
    {
        let mut insert = tx.prepare(
            "INSERT INTO adapter_dispositions(rowid,source,generation,cursor,disposition,code,payload_hash) VALUES (?1,'codex','generation',?2,'diagnostic','unsupported_event','hash')",
        ).unwrap();
        for rowid in rowids.iter().rev() {
            insert.execute(params![rowid, rowid.to_string()]).unwrap();
        }
    }
    tx.commit().unwrap();
    db
}

fn rowids(db: &Connection) -> Vec<i64> {
    db.prepare("SELECT rowid FROM adapter_dispositions ORDER BY rowid")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[test]
fn scalar_prune_matches_previous_statement_at_sparse_signed_boundaries() {
    let sparse = [i64::MIN, i64::MIN + 5, -9, -1, 0, 17, i64::MAX];
    for ids in [&[][..], &sparse[..1], &sparse[..]] {
        for keep in [0_i64, 1, 3, 7, 8, 100_000] {
            let previous = ledger(ids);
            let candidate = ledger(ids);
            previous.execute(PREVIOUS_PRUNE_SQL, [keep]).unwrap();
            candidate
                .execute(PRUNE_ADAPTER_DISPOSITIONS_SQL, [keep])
                .unwrap();
            assert_eq!(rowids(&candidate), rowids(&previous), "keep={keep}");
        }
    }
}

#[test]
fn production_prune_preserves_latest_bound_and_rolls_back_atomically() {
    let mut ids: Vec<i64> = (0..MAX_ADAPTER_DISPOSITIONS + 5)
        .map(|index| i64::MIN + i64::try_from(index).unwrap() * 3)
        .collect();
    ids.push(i64::MAX);
    let mut db = ledger(&ids);
    let keep = usize::try_from(MAX_ADAPTER_DISPOSITIONS).unwrap();
    let expected = &ids[ids.len() - keep..];
    {
        let tx = db.transaction().unwrap();
        prune_adapter_dispositions(&tx).unwrap();
        assert_eq!(rowids(&tx), expected);
        prune_adapter_dispositions(&tx).unwrap();
        assert_eq!(rowids(&tx), expected);
        tx.rollback().unwrap();
    }
    assert_eq!(rowids(&db), ids);
    let tx = db.transaction().unwrap();
    prune_adapter_dispositions(&tx).unwrap();
    tx.commit().unwrap();
    assert_eq!(rowids(&db), expected);
}

#[test]
fn prune_program_does_not_materialize_the_retained_rowid_set() {
    let db = ledger(&[]);
    let opcodes: Vec<String> = db
        .prepare(&format!("EXPLAIN {PRUNE_ADAPTER_DISPOSITIONS_SQL}"))
        .unwrap()
        .query_map([i64::try_from(MAX_ADAPTER_DISPOSITIONS).unwrap()], |row| {
            row.get(1)
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    // This guards the pinned SQLite program shape, not all transaction memory or disk usage.
    for forbidden in ["OpenEphemeral", "SorterOpen", "IdxInsert"] {
        assert!(
            !opcodes.iter().any(|opcode| opcode == forbidden),
            "retained-set materialization opcode: {forbidden}"
        );
    }
}

#[test]
fn scalar_prune_reduces_vm_work_for_one_over_capacity_ledger() {
    let ids: Vec<i64> = (0..=MAX_ADAPTER_DISPOSITIONS)
        .map(|index| i64::try_from(index).unwrap())
        .collect();
    let mut db = ledger(&ids);
    let mut steps = Vec::new();
    for sql in [PREVIOUS_PRUNE_SQL, PRUNE_ADAPTER_DISPOSITIONS_SQL] {
        let tx = db.transaction().unwrap();
        {
            let mut statement = tx.prepare(sql).unwrap();
            assert_eq!(
                statement
                    .execute([i64::try_from(MAX_ADAPTER_DISPOSITIONS).unwrap()])
                    .unwrap(),
                1
            );
            steps.push(statement.get_status(rusqlite::StatementStatus::VmStep));
        }
        assert_eq!(rowids(&tx), &ids[1..]);
        tx.rollback().unwrap();
    }
    // A fixed synthetic instruction comparison, not a latency/RSS or write-reservation proof.
    assert!(steps[1] > 0 && steps[1] < steps[0]);
    println!(
        "disposition_prune_previous_vm_steps={} scalar_vm_steps={}",
        steps[0], steps[1]
    );
}
