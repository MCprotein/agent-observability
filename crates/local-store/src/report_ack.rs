//! Fixed-width report acknowledgement, isolated from variable-size metadata.

use super::{StoreError, metadata_generation, normalize_schema_sql, required_schema_text};
use rusqlite::{Connection, Transaction, TransactionBehavior, params};

pub(super) const TABLE_SQL: &str = "CREATE TABLE report_acknowledgement (id INTEGER PRIMARY KEY CHECK(id=1), generation BLOB NOT NULL CHECK(typeof(generation)='blob' AND length(generation)=8))";

// SQLite 3.53.2 DELETE journal, cache_spill=OFF: at most the singleton leaf and
// database-header page are dirtied, including their co-sector neighbours. With
// S <= 65536 and legal P=512..65536, S + 2*ceil(S/P)*(P+8) <= 198656.
// Journal rounds up to 200704 bytes. Co-sector writes may materialize 131072
// previously sparse authority bytes; add one 4096-byte directory allocation unit.
/// Maximum incremental allocated bytes for a validated fixed-width acknowledgement.
pub const MAX_REPORT_ACKNOWLEDGEMENT_BYTES: u64 = MAX_ACK_JOURNAL_BYTES + 131_072 + 4_096;
const MAX_ACK_JOURNAL_BYTES: u64 = 200_704;

pub(super) fn read(db: &Connection) -> Result<u64, StoreError> {
    let legacy: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM metadata WHERE key=?1)",
        [super::REPORT_ACKNOWLEDGED_GENERATION_KEY],
        |row| row.get(0),
    )?;
    if legacy {
        return Err(StoreError::SchemaMismatch);
    }
    let mut query = db.prepare("SELECT id, generation FROM report_acknowledgement LIMIT 2")?;
    let mut rows = query.query([])?;
    let row = rows.next()?.ok_or(StoreError::SchemaMismatch)?;
    let id: i64 = row.get(0)?;
    let value = row
        .get_ref(1)?
        .as_blob()
        .map_err(|_| StoreError::SchemaMismatch)?;
    let bytes: [u8; 8] = value.try_into().map_err(|_| StoreError::SchemaMismatch)?;
    if id != 1 || rows.next()?.is_some() {
        return Err(StoreError::SchemaMismatch);
    }
    Ok(u64::from_be_bytes(bytes))
}

pub(super) fn insert(db: &Connection, generation: u64) -> Result<(), StoreError> {
    db.execute(
        "INSERT INTO report_acknowledgement(id,generation) VALUES(1,?1)",
        [generation.to_be_bytes().as_slice()],
    )?;
    Ok(())
}

fn validate_layout(db: &Connection) -> Result<(), StoreError> {
    let sql = required_schema_text(db.query_row(
        "SELECT sql FROM main.sqlite_schema WHERE type='table' AND name='report_acknowledgement'",
        [],
        |row| row.get(0),
    ))?;
    let related: i64 = db.query_row(
        "SELECT count(*) FROM main.sqlite_schema WHERE tbl_name='report_acknowledgement'",
        [],
        |row| row.get(0),
    )?;
    let temporary: i64 = db.query_row("SELECT count(*) FROM temp.sqlite_schema", [], |row| {
        row.get(0)
    })?;
    let attached: i64 = db.query_row(
        "SELECT count(*) FROM pragma_database_list WHERE name NOT IN ('main','temp')",
        [],
        |row| row.get(0),
    )?;
    if normalize_schema_sql(&sql) != normalize_schema_sql(TABLE_SQL)
        || related != 1
        || temporary != 0
        || attached != 0
    {
        return Err(StoreError::SchemaMismatch);
    }
    let mut query = db.prepare("SELECT pagetype, ncell, pgsize FROM dbstat('main') WHERE name='report_acknowledgement' LIMIT 2")?;
    let mut rows = query.query([])?;
    let row = rows.next()?.ok_or(StoreError::SchemaMismatch)?;
    let kind: String = row.get(0)?;
    let cells: i64 = row.get(1)?;
    let page_size: u32 = row.get(2)?;
    if kind != "leaf"
        || cells != 1
        || !(512..=65536).contains(&page_size)
        || !page_size.is_power_of_two()
        || rows.next()?.is_some()
    {
        return Err(StoreError::SchemaMismatch);
    }
    Ok(())
}

pub(super) fn acknowledge(db: &Connection, generation: u64) -> Result<bool, StoreError> {
    // Do not silently change the durable journal mode or destroy caller-owned temp tables.
    let journal: String = db.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
    let locking: String = db.pragma_query_value(None, "locking_mode", |row| row.get(0))?;
    let synchronous: i64 = db.pragma_query_value(None, "synchronous", |row| row.get(0))?;
    if journal != "delete" || locking != "normal" || synchronous != 2 || !db.is_autocommit() {
        return Err(StoreError::SchemaMismatch);
    }
    let spill: i64 = db.pragma_query_value(None, "cache_spill", |row| row.get(0))?;
    db.pragma_update(None, "cache_spill", false)?;
    let result = acknowledge_inner(db, generation);
    db.pragma_update(None, "cache_spill", spill)?;
    result
}

fn acknowledge_inner(db: &Connection, generation: u64) -> Result<bool, StoreError> {
    acknowledge_observing(db, generation, || Ok(()))
}

fn acknowledge_observing(
    db: &Connection,
    generation: u64,
    before_commit: impl FnOnce() -> Result<(), StoreError>,
) -> Result<bool, StoreError> {
    let tx = Transaction::new_unchecked(db, TransactionBehavior::Immediate)?;
    // Durable modes may be changed by another connection before BEGIN acquires its lock.
    let journal: String = tx.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
    let auto_vacuum: i64 = tx.pragma_query_value(None, "auto_vacuum", |row| row.get(0))?;
    if journal != "delete" || auto_vacuum != 2 {
        return Err(StoreError::SchemaMismatch);
    }
    validate_layout(&tx)?;
    let current = metadata_generation(&tx, super::REPORT_GENERATION_KEY)?;
    let previous = read(&tx)?;
    if previous > current {
        return Err(StoreError::SchemaMismatch);
    }
    if current != generation {
        tx.commit()?;
        return Ok(false);
    }
    if previous != generation {
        let changed = tx.execute(
            "UPDATE report_acknowledgement SET generation=?1 WHERE id=1",
            params![generation.to_be_bytes().as_slice()],
        )?;
        if changed != 1 {
            return Err(StoreError::SchemaMismatch);
        }
    }
    before_commit()?;
    tx.commit()?;
    Ok(true)
}

pub(super) fn migrate(db: &Connection) -> Result<(), StoreError> {
    let spill: i64 = db.pragma_query_value(None, "cache_spill", |row| row.get(0))?;
    let maximum: u32 = db.pragma_query_value(None, "max_page_count", |row| row.get(0))?;
    let pages: u32 = db.pragma_query_value(None, "page_count", |row| row.get(0))?;
    db.pragma_update(
        None,
        "max_page_count",
        pages.saturating_add(16).min(maximum),
    )?;
    db.pragma_update(None, "cache_spill", false)?;
    let result = migrate_inner(db, || Ok(()));
    db.pragma_update(None, "cache_spill", spill)?;
    db.pragma_update(None, "max_page_count", maximum)?;
    result
}

fn migrate_inner(
    db: &Connection,
    before_commit: impl FnOnce() -> Result<(), StoreError>,
) -> Result<(), StoreError> {
    let tx = Transaction::new_unchecked(db, TransactionBehavior::Immediate)?;
    migrate_body(&tx)?;
    before_commit()?;
    tx.commit()?;
    Ok(())
}

pub(super) fn migrate_body(tx: &Transaction<'_>) -> Result<(), StoreError> {
    if super::required_schema_version(tx)? != super::VISIBILITY_LOCAL_STORE_SCHEMA_VERSION {
        return Err(StoreError::SchemaMismatch);
    }
    super::validate_schema_version(tx, false)?;
    let generation = metadata_generation(tx, super::REPORT_GENERATION_KEY)?;
    let acknowledged = metadata_generation(tx, super::REPORT_ACKNOWLEDGED_GENERATION_KEY)?;
    if acknowledged > generation {
        return Err(StoreError::SchemaMismatch);
    }
    tx.execute_batch(TABLE_SQL)?;
    insert(tx, acknowledged)?;
    tx.execute(
        "DELETE FROM metadata WHERE key=?1",
        [super::REPORT_ACKNOWLEDGED_GENERATION_KEY],
    )?;
    tx.execute(
        "UPDATE metadata SET value=?1 WHERE key='schema_version'",
        [super::LOCAL_STORE_SCHEMA_VERSION],
    )?;
    validate_layout(tx)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        LocalStore, REPORT_ACKNOWLEDGED_GENERATION_KEY, REPORT_GENERATION_KEY,
        VISIBILITY_LOCAL_STORE_SCHEMA_VERSION,
    };
    use std::{fs, path::PathBuf, process::Command};

    fn fixture(label: &str, page_size: u32) -> LocalStore {
        let dir = std::env::temp_dir().join(format!("agentobs-ack-{label}-{}", std::process::id()));
        crate::private_dir(&dir).unwrap();
        let path = dir.join(crate::DB_NAME);
        crate::private_create_new(&path).unwrap();
        let db = Connection::open(&path).unwrap();
        db.pragma_update(None, "page_size", page_size).unwrap();
        drop(db);
        LocalStore::open(dir).unwrap()
    }

    fn legacy(store: &LocalStore, acknowledged: &str) {
        store
            .db
            .execute(
                "INSERT INTO metadata(key,value) VALUES(?1,?2)",
                params![REPORT_ACKNOWLEDGED_GENERATION_KEY, acknowledged],
            )
            .unwrap();
        store
            .db
            .execute_batch("DROP TABLE report_acknowledgement")
            .unwrap();
        store
            .db
            .execute(
                "UPDATE metadata SET value=?1 WHERE key='schema_version'",
                [VISIBILITY_LOCAL_STORE_SCHEMA_VERSION],
            )
            .unwrap();
    }

    fn discard(store: LocalStore) {
        let dir = store.dir.clone();
        drop(store);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn acknowledgement_bound_tracks_reviewed_sqlite_and_page_sizes() {
        assert_eq!(
            rusqlite::version_number(),
            3_053_002,
            "SQLite upgrade requires acknowledgement write-bound review"
        );
        for page in [512_u64, 1024, 2048, 4096, 8192, 16384, 32768, 65536] {
            let sectors = 65536_u64.div_ceil(page);
            let journal = 65536 + 2 * sectors * (page + 8);
            assert!(journal.div_ceil(4096) * 4096 <= MAX_ACK_JOURNAL_BYTES);
            assert!(2 * sectors * page <= 131_072);
        }
        assert_eq!(MAX_REPORT_ACKNOWLEDGEMENT_BYTES, 335_872);
    }

    #[test]
    fn fixed_ack_preserves_full_u64_and_bounded_journal_at_all_page_sizes() {
        for page_size in [512, 1024, 2048, 4096, 8192, 16384, 32768, 65536] {
            let store = fixture(&format!("width-{page_size}"), page_size);
            let db = &store.db;
            db.pragma_update(None, "cache_spill", false).unwrap();
            let page_count: i64 = db
                .pragma_query_value(None, "page_count", |row| row.get(0))
                .unwrap();
            for generation in [9, 10, 99, 100, i64::MAX as u64, u64::MAX] {
                db.execute(
                    "UPDATE metadata SET value=?1 WHERE key=?2",
                    params![generation.to_string(), REPORT_GENERATION_KEY],
                )
                .unwrap();
                assert!(
                    acknowledge_observing(db, generation, || {
                        let bytes =
                            fs::metadata(store.database_path().with_extension("sqlite3-journal"))
                                .unwrap()
                                .len();
                        assert!(
                            bytes <= MAX_ACK_JOURNAL_BYTES,
                            "P={page_size}, bytes={bytes}"
                        );
                        Ok(())
                    })
                    .unwrap()
                );
                assert_eq!(read(db).unwrap(), generation);
                assert!(store.acknowledge_report_generation(generation).unwrap());
                assert!(!store.acknowledge_report_generation(generation - 1).unwrap());
                assert_eq!(read(db).unwrap(), generation);
            }
            assert_eq!(
                db.pragma_query_value(None, "page_count", |row| row.get::<_, i64>(0))
                    .unwrap(),
                page_count
            );
            discard(store);
        }
    }

    #[test]
    fn fixed_ack_rejects_widening_schema_and_connection_modes() {
        for (label, sql) in [
            (
                "index",
                "CREATE INDEX forbidden_ack ON report_acknowledgement(generation)",
            ),
            (
                "trigger",
                "CREATE TRIGGER forbidden_ack AFTER UPDATE ON report_acknowledgement BEGIN UPDATE metadata SET value='99' WHERE key='report_generation'; END",
            ),
            ("temp", "CREATE TEMP TABLE forbidden_ack(x)"),
            ("attached", "ATTACH ':memory:' AS forbidden"),
            ("journal", "PRAGMA journal_mode=WAL"),
            ("sync", "PRAGMA synchronous=OFF"),
            ("locking", "PRAGMA locking_mode=EXCLUSIVE"),
            ("vacuum", "PRAGMA auto_vacuum=FULL"),
            ("missing", "DELETE FROM report_acknowledgement"),
            (
                "wrong_width",
                "PRAGMA ignore_check_constraints=ON; UPDATE report_acknowledgement SET generation=x'01'",
            ),
            (
                "wrong_type",
                "PRAGMA ignore_check_constraints=ON; UPDATE report_acknowledgement SET generation='00000000'",
            ),
            (
                "duplicate_authority",
                "INSERT INTO metadata VALUES('report_acknowledged_generation','0')",
            ),
        ] {
            let store = fixture(label, 4096);
            store.invalidate_report().unwrap();
            store.db.execute_batch(sql).unwrap();
            assert!(store.acknowledge_report_generation(1).is_err(), "{label}");
            assert_eq!(
                metadata_generation(&store.db, REPORT_GENERATION_KEY).unwrap(),
                1
            );
            discard(store);
        }
    }

    #[test]
    fn v6_migration_rejects_invalid_ack_without_advancing_schema() {
        for (label, value) in [
            ("overflow", "18446744073709551616"),
            ("invalid", "no"),
            ("ahead", "1"),
        ] {
            let store = fixture(label, 4096);
            legacy(&store, value);
            assert!(migrate(&store.db).is_err());
            assert_eq!(
                crate::required_schema_version(&store.db).unwrap(),
                VISIBILITY_LOCAL_STORE_SCHEMA_VERSION
            );
            assert_eq!(
                store
                    .db
                    .query_row(
                        "SELECT value FROM metadata WHERE key=?1",
                        [REPORT_ACKNOWLEDGED_GENERATION_KEY],
                        |row| row.get::<_, String>(0)
                    )
                    .unwrap(),
                value
            );
            discard(store);
        }
    }

    #[test]
    fn v6_migration_precommit_failure_rolls_back_and_retries() {
        let store = fixture("rollback", 4096);
        legacy(&store, "0");
        assert!(migrate_inner(&store.db, || Err(StoreError::SchemaMismatch)).is_err());
        assert_eq!(
            crate::required_schema_version(&store.db).unwrap(),
            VISIBILITY_LOCAL_STORE_SCHEMA_VERSION
        );
        migrate(&store.db).unwrap();
        assert_eq!(read(&store.db).unwrap(), 0);
        discard(store);
    }

    #[test]
    fn migration_process_probe() {
        let Ok(path) = std::env::var("AGENTOBS_ACK_CRASH_FIXTURE") else {
            return;
        };
        let phase = std::env::var("AGENTOBS_ACK_CRASH_PHASE").unwrap();
        let db = Connection::open(PathBuf::from(path).join(crate::DB_NAME)).unwrap();
        db.pragma_update(None, "synchronous", "FULL").unwrap();
        db.pragma_update(None, "cache_spill", false).unwrap();
        if phase.starts_with("ack-") {
            acknowledge_observing(&db, 1, || {
                if phase == "ack-before" {
                    std::process::exit(91);
                }
                Ok(())
            })
            .unwrap();
            std::process::exit(92);
        }
        migrate_inner(&db, || {
            if phase == "before" {
                std::process::exit(91);
            }
            Ok(())
        })
        .unwrap();
        std::process::exit(92);
    }

    #[test]
    fn v6_migration_recovers_process_exit_before_and_after_commit() {
        for phase in ["before", "after"] {
            let store = fixture(phase, 4096);
            legacy(&store, "0");
            let dir = store.dir.clone();
            drop(store);
            let status = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "report_ack::tests::migration_process_probe",
                    "--nocapture",
                ])
                .env("AGENTOBS_ACK_CRASH_FIXTURE", &dir)
                .env("AGENTOBS_ACK_CRASH_PHASE", phase)
                .status()
                .unwrap();
            assert_eq!(status.code(), Some(if phase == "before" { 91 } else { 92 }));
            let db = Connection::open(dir.join(crate::DB_NAME)).unwrap();
            assert_eq!(
                crate::required_schema_version(&db).unwrap(),
                if phase == "before" {
                    VISIBILITY_LOCAL_STORE_SCHEMA_VERSION
                } else {
                    crate::LOCAL_STORE_SCHEMA_VERSION
                }
            );
            drop(db);
            let reopened = LocalStore::open_with_migration_headroom(&dir, u64::MAX).unwrap();
            assert_eq!(read(&reopened.db).unwrap(), 0);
            discard(reopened);
        }
    }

    #[test]
    fn fixed_ack_recovers_process_exit_without_false_completion() {
        for phase in ["ack-before", "ack-after"] {
            let store = fixture(phase, 4096);
            store.invalidate_report().unwrap();
            let dir = store.dir.clone();
            drop(store);
            let status = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "report_ack::tests::migration_process_probe",
                    "--nocapture",
                ])
                .env("AGENTOBS_ACK_CRASH_FIXTURE", &dir)
                .env("AGENTOBS_ACK_CRASH_PHASE", phase)
                .status()
                .unwrap();
            assert_eq!(
                status.code(),
                Some(if phase == "ack-before" { 91 } else { 92 })
            );
            let store = LocalStore::open_current(dir).unwrap();
            assert_eq!(
                store.report_status().unwrap().pending(),
                phase == "ack-before"
            );
            discard(store);
        }
    }

    #[test]
    fn v7_is_incompatible_with_the_exact_v6_schema_contract() {
        let store = fixture("downgrade-denied", 4096);
        assert_ne!(
            crate::required_schema_version(&store.db).unwrap(),
            VISIBILITY_LOCAL_STORE_SCHEMA_VERSION
        );
        assert!(matches!(
            crate::validate_schema_version(&store.db, false),
            Err(StoreError::SchemaMismatch)
        ));
        assert_eq!(read(&store.db).unwrap(), 0);
        discard(store);
    }
}
