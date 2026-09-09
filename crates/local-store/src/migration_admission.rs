//! Admission for the non-rewriting v4/v5/v6 → v7 transaction.
use super::{StoreError, private_open_file};
use rusqlite::{Connection, Transaction, TransactionBehavior};
use std::{fs, path::Path};

const SLACK: u64 = 6 * 1024 * 1024;
const SECTOR: u64 = 65536;
const MAX_GROWTH_BYTES: u64 = 64 * 1024 * 1024;

fn overflow() -> StoreError {
    StoreError::MigrationAdmissionRequired
}

pub(super) fn initial_allowance(pages: u64, page: u64) -> Result<u64, StoreError> {
    pages
        .checked_mul(page.checked_add(8).ok_or_else(overflow)?)
        .and_then(|value| value.checked_add(SLACK))
        .ok_or_else(overflow)
}

fn peak(
    pages: u64,
    page: u64,
    free: u64,
    growth: u64,
    journal: u64,
) -> Result<(u64, u64), StoreError> {
    if !(512..=65536).contains(&page)
        || !page.is_power_of_two()
        || free > pages
        || growth > MAX_GROWTH_BYTES / page
    {
        return Err(StoreError::SchemaMismatch);
    }
    let record = page.checked_add(8).ok_or_else(overflow)?;
    let neighbours = SECTOR.div_ceil(page);
    let dirty = journal
        .div_ceil(record)
        .checked_add(neighbours)
        .ok_or_else(overflow)?
        .min(pages);
    let materialize = dirty
        .checked_add(free)
        .ok_or_else(overflow)?
        .min(pages)
        .checked_add(growth)
        .and_then(|value| value.checked_mul(page))
        .ok_or_else(overflow)?;
    let journal_peak = journal
        .checked_add(SECTOR)
        .and_then(|value| value.checked_add(neighbours.checked_mul(record)?))
        .ok_or_else(overflow)?;
    Ok((journal_peak, materialize))
}

fn pragma(db: &Connection, name: &str) -> Result<u64, StoreError> {
    Ok(u64::from(db.pragma_query_value(None, name, |row| {
        row.get::<_, u32>(0)
    })?))
}

pub(super) fn validate_modes(db: &Connection) -> Result<(), StoreError> {
    let journal: String = db.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
    let locking: String = db.pragma_query_value(None, "locking_mode", |row| row.get(0))?;
    let attached: u32 = db.query_row(
        "SELECT count(*) FROM pragma_database_list WHERE name NOT IN ('main','temp')",
        [],
        |row| row.get(0),
    )?;
    let temporary: u32 = db.query_row("SELECT count(*) FROM temp.sqlite_schema", [], |row| {
        row.get(0)
    })?;
    let atomic: bool = db.query_row("SELECT sqlite_compileoption_used('ENABLE_ATOMIC_WRITE') OR sqlite_compileoption_used('ENABLE_BATCH_ATOMIC_WRITE')", [], |row| row.get(0))?;
    if journal != "delete"
        || locking != "normal"
        || pragma(db, "synchronous")? != 2
        || pragma(db, "auto_vacuum")? != 2
        || attached != 0
        || temporary != 0
        || atomic
    {
        return Err(StoreError::SchemaMismatch);
    }
    Ok(())
}

pub(super) fn migrate(db: &Connection, path: &Path, admitted: u64) -> Result<(), StoreError> {
    migrate_observing(db, path, admitted, |_| Ok(()))
}

pub(super) fn open_private_journal(path: &Path) -> Result<Option<fs::File>, StoreError> {
    open_private_journal_observing(path, || {})
}

fn open_private_journal_observing(
    path: &Path,
    before_open: impl FnOnce(),
) -> Result<Option<fs::File>, StoreError> {
    let initial_metadata = match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
        Ok(metadata) if metadata.file_type().is_symlink() => return Err(StoreError::Symlink),
        Ok(metadata) if !metadata.is_file() => return Err(StoreError::InvalidPath),
        Ok(metadata) => metadata,
    };
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(super::no_follow_flag() | super::nonblocking_open_flag());
    }
    before_open();
    let file = options.open(path)?;
    private_open_file(&file)?;
    super::storage_ownership::validate_private_file_identity(path, &file)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let opened_metadata = file.metadata()?;
        if initial_metadata.dev() != opened_metadata.dev()
            || initial_metadata.ino() != opened_metadata.ino()
        {
            return Err(StoreError::InvalidPath);
        }
    }
    #[cfg(not(unix))]
    let _ = initial_metadata;
    Ok(Some(file))
}

fn validate_inactive_journal(path: &Path) -> Result<(), StoreError> {
    use std::io::Read;
    match open_private_journal(path)? {
        None => Ok(()),
        Some(mut file) => {
            if file.metadata()?.len() == 0 {
                return Ok(());
            }
            let mut header = [0_u8; 8];
            file.read_exact(&mut header)?;
            if header != [0; 8] {
                return Err(StoreError::SchemaMismatch);
            }
            Ok(())
        }
    }
}

fn migrate_observing(
    db: &Connection,
    path: &Path,
    admitted: u64,
    before_commit: impl FnOnce(&Transaction<'_>) -> Result<(), StoreError>,
) -> Result<(), StoreError> {
    validate_modes(db)?;
    if !db.is_autocommit() {
        return Err(StoreError::SchemaMismatch);
    }
    let journal_path = path.with_extension("sqlite3-journal");
    validate_inactive_journal(&journal_path)?;
    let spill = pragma(db, "cache_spill")?;
    let maximum = pragma(db, "max_page_count")?;
    let temporary = pragma(db, "temp_store")?;
    db.pragma_update(None, "temp_store", "MEMORY")?;
    db.pragma_update(None, "cache_spill", false)?;
    let result = (|| {
        let tx = Transaction::new_unchecked(db, TransactionBehavior::Immediate)?;
        validate_modes(&tx)?;
        // SQLite already performed any hot-journal recovery while acquiring its lock.
        // A zeroed, non-hot journal is reused by SQLite itself; never delete it manually.
        validate_inactive_journal(&journal_path)?;
        if pragma(&tx, "cache_spill")? != 0 || pragma(&tx, "temp_store")? != 2 {
            return Err(StoreError::SchemaMismatch);
        }
        let pages = pragma(&tx, "page_count")?;
        let page = pragma(&tx, "page_size")?;
        let free = pragma(&tx, "freelist_count")?;
        if admitted < initial_allowance(pages, page)? {
            return Err(overflow());
        }
        let ceiling = pages
            .checked_add(admitted.saturating_sub(SLACK).min(MAX_GROWTH_BYTES) / page)
            .ok_or_else(overflow)?
            .min(maximum);
        tx.pragma_update(
            None,
            "max_page_count",
            u32::try_from(ceiling).map_err(|_| overflow())?,
        )?;
        migrate_steps(&tx)?;
        before_commit(&tx)?;
        let growth = pragma(&tx, "page_count")?
            .checked_sub(pages)
            .ok_or_else(overflow)?;
        let journal_file =
            open_private_journal(&journal_path)?.ok_or(StoreError::SchemaMismatch)?;
        let metadata = journal_file.metadata()?;
        let journal = metadata.len();
        if journal < 28 {
            return Err(StoreError::SchemaMismatch);
        }
        let (journal_peak, materialize) = peak(pages, page, free, growth, journal)?;
        #[cfg(unix)]
        let allocated = {
            use std::os::unix::fs::MetadataExt;
            metadata.blocks().checked_mul(512).ok_or_else(overflow)?
        };
        #[cfg(not(unix))]
        let allocated = journal;
        let total = journal_peak
            .max(allocated)
            .checked_add(materialize)
            .and_then(|value| value.checked_add(SLACK))
            .ok_or_else(overflow)?;
        if total > admitted {
            return Err(overflow());
        }
        let remaining = journal_peak
            .saturating_sub(allocated)
            .checked_add(materialize)
            .and_then(|value| value.checked_add(SLACK))
            .ok_or_else(overflow)?;
        if fs2::available_space(path)? < remaining {
            return Err(overflow());
        }
        tx.commit()?;
        Ok(())
    })();
    db.pragma_update(
        None,
        "cache_spill",
        u32::try_from(spill).map_err(|_| overflow())?,
    )?;
    db.pragma_update(
        None,
        "max_page_count",
        u32::try_from(maximum).map_err(|_| overflow())?,
    )?;
    db.pragma_update(
        None,
        "temp_store",
        u32::try_from(temporary).map_err(|_| overflow())?,
    )?;
    result
}

fn migrate_steps(tx: &Transaction<'_>) -> Result<(), StoreError> {
    let schema = super::required_schema_version(tx)?;
    if !matches!(
        schema.as_str(),
        super::LIFECYCLE_LOCAL_STORE_SCHEMA_VERSION
            | super::PREVIOUS_LOCAL_STORE_SCHEMA_VERSION
            | super::VISIBILITY_LOCAL_STORE_SCHEMA_VERSION
    ) {
        return Err(StoreError::SchemaMismatch);
    }
    if schema == super::LIFECYCLE_LOCAL_STORE_SCHEMA_VERSION {
        super::migrate_v4_to_v5_body(tx)?;
    }
    if super::required_schema_version(tx)? == super::PREVIOUS_LOCAL_STORE_SCHEMA_VERSION {
        super::migrate_v5_to_v6_body(tx)?;
    }
    if schema != super::VISIBILITY_LOCAL_STORE_SCHEMA_VERSION {
        super::ensure_report_metadata_body(tx)?;
    }
    super::report_ack::migrate_body(tx)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(label: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("agentobs-staged-{label}-{}", std::process::id()));
        super::super::private_dir(&dir).unwrap();
        let path = dir.join(super::super::DB_NAME);
        let mut file = super::super::private_create_new(&path).unwrap();
        std::io::Write::write_all(
            &mut file,
            include_bytes!("../tests/fixtures/local_state_v6.sqlite3"),
        )
        .unwrap();
        (dir, path)
    }

    #[test]
    fn preflight_rejects_persistent_mode_changes_without_rewriting_database() {
        for (name, value) in [("journal_mode", "WAL"), ("auto_vacuum", "FULL")] {
            let (dir, path) = fixture(name);
            let db = Connection::open(&path).unwrap();
            db.pragma_update(None, name, value).unwrap();
            let before = fs::read(&path).unwrap();
            assert!(matches!(
                super::super::LocalStore::open_with_migration_headroom(&dir, u64::MAX),
                Err(StoreError::SchemaMismatch)
            ));
            assert!(
                fs::read(&path).unwrap() == before,
                "refusal changed durable mode or schema"
            );
            assert_eq!(
                super::super::required_schema_version(&db).unwrap(),
                super::super::VISIBILITY_LOCAL_STORE_SCHEMA_VERSION
            );
            drop(db);
            fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    #[cfg(unix)]
    fn freelist_authority_with_sparse_copy_migrates_without_losing_records() {
        use std::io::{Seek, SeekFrom, Write};
        #[cfg(target_os = "linux")]
        use std::os::unix::fs::MetadataExt;
        let (dir, path) = fixture("sparse");
        let db = Connection::open(&path).unwrap();
        db.execute_batch("CREATE TABLE freed_payload(value BLOB); INSERT INTO freed_payload VALUES(zeroblob(4194304)); DROP TABLE freed_payload;").unwrap();
        assert!(pragma(&db, "freelist_count").unwrap() > 1000);
        drop(db);
        let mut bytes = fs::read(&path).unwrap();
        // Freelist leaf contents are unused. Preserve trunk linkage, but clear leaves
        // before sparse copying: freed overflow pages otherwise retain nonzero links.
        let mut trunk = u32::from_be_bytes(bytes[32..36].try_into().unwrap());
        let mut visited = 0;
        while trunk != 0 {
            visited += 1;
            assert!(visited < bytes.len() / 4096);
            let offset = (usize::try_from(trunk).unwrap() - 1) * 4096;
            let next = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap());
            let count = u32::from_be_bytes(bytes[offset + 4..offset + 8].try_into().unwrap());
            for index in 0..usize::try_from(count).unwrap() {
                let entry = offset + 8 + index * 4;
                let leaf = u32::from_be_bytes(bytes[entry..entry + 4].try_into().unwrap());
                let start = (usize::try_from(leaf).unwrap() - 1) * 4096;
                bytes[start..start + 4096].fill(0);
            }
            trunk = next;
        }
        let sparse_path = dir.join("sparse-copy.sqlite3");
        let mut file = super::super::private_create_new(&sparse_path).unwrap();
        for chunk in bytes.chunks(4096) {
            if chunk.iter().all(|byte| *byte == 0) {
                file.seek(SeekFrom::Current(i64::try_from(chunk.len()).unwrap()))
                    .unwrap();
            } else {
                file.write_all(chunk).unwrap();
            }
        }
        file.set_len(u64::try_from(bytes.len()).unwrap()).unwrap();
        file.sync_all().unwrap();
        // APFS on the local verification host materializes seek-created gaps.
        // Linux CI must prove physical holes; other hosts still exercise freelist reuse.
        #[cfg(target_os = "linux")]
        assert!(
            file.metadata().unwrap().blocks() * 512 < u64::try_from(bytes.len()).unwrap(),
            "sparse fixture allocated={} logical={}",
            file.metadata().unwrap().blocks() * 512,
            bytes.len()
        );
        drop(file);
        fs::rename(sparse_path, &path).unwrap();
        let store =
            super::super::LocalStore::open_with_migration_headroom(&dir, 64 * 1024 * 1024).unwrap();
        assert_eq!(store.record_count().unwrap(), 2);
        assert_eq!(store.report_status().unwrap().acknowledged_generation, 1);
        assert_eq!(store.report_visibility_epoch().unwrap(), 7);
        drop(store);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn staged_peak_accounts_for_freelist_growth_and_commit_sector() {
        for page in [512, 1024, 2048, 4096, 8192, 16384, 32768, 65536] {
            let (journal, materialize) = peak(1000, page, 100, 32, 0).unwrap();
            assert_eq!(journal, SECTOR + SECTOR.div_ceil(page) * (page + 8));
            assert_eq!(materialize, (SECTOR.div_ceil(page) + 100 + 32) * page);
            assert!(peak(1000, page, 100, MAX_GROWTH_BYTES / page + 1, 0).is_err());
        }
        assert!(initial_allowance(u64::MAX, 65536).is_err());
        assert!(peak(1, 513, 0, 0, 0).is_err());
    }

    #[test]
    fn smallest_page_v4_with_full_disposition_ledger_can_upgrade() {
        let (dir, path) = fixture("v4-512");
        let db = Connection::open(&path).unwrap();
        for (kind, name, _) in super::super::SCHEMA_OBJECTS {
            if [
                "expired_trace_states",
                "hot_trace_index",
                "warm_traces",
                "warm_records",
                "cold_traces",
                "lifecycle_trace_control",
                "lifecycle_span_control",
                "adapter_dispositions_code_idx",
                "hot_trace_lifecycle_idx",
                "warm_trace_lifecycle_idx",
                "warm_records_trace_idx",
                "cold_trace_lifecycle_idx",
                "lifecycle_span_control_trace_idx",
            ]
            .contains(name)
            {
                db.execute_batch(&format!("DROP {kind} IF EXISTS {name}"))
                    .unwrap();
            }
        }
        db.execute(
            "UPDATE metadata SET value='local_state.v4' WHERE key='schema_version'",
            [],
        )
        .unwrap();
        db.execute(
            "DELETE FROM metadata WHERE key IN (?1,?2)",
            [
                super::super::LIFECYCLE_SCAN_CURSOR_KEY,
                super::super::LIFECYCLE_BACKFILL_CURSOR_KEY,
            ],
        )
        .unwrap();
        db.pragma_update(None, "page_size", 512).unwrap();
        db.execute_batch("VACUUM; WITH RECURSIVE n(i) AS (VALUES(1) UNION ALL SELECT i+1 FROM n WHERE i<100000) INSERT INTO adapter_dispositions(source,generation,cursor,disposition,code,payload_hash) SELECT 'codex','fixture',CAST(i AS TEXT),'diagnostic','unknown_event',printf('%064d',0) FROM n;").unwrap();
        assert_eq!(pragma(&db, "page_size").unwrap(), 512);
        assert!(pragma(&db, "freelist_count").unwrap() < 16);
        drop(db);
        let store =
            super::super::LocalStore::open_with_migration_headroom(&dir, 64 * 1024 * 1024).unwrap();
        assert_eq!(store.disposition_count().unwrap(), 100_000);
        assert_eq!(store.record_count().unwrap(), 2);
        drop(store);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn unsafe_journal_is_rejected_before_sqlite_can_open_it() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        for linked in [false, true] {
            let (dir, path) = fixture(if linked {
                "journal-link"
            } else {
                "journal-mode"
            });
            let journal = path.with_extension("sqlite3-journal");
            let target = dir.join("operator-file");
            let mut file = super::super::private_create_new(&target).unwrap();
            std::io::Write::write_all(&mut file, b"preserve operator bytes").unwrap();
            drop(file);
            if linked {
                symlink(&target, &journal).unwrap();
            } else {
                fs::copy(&target, &journal).unwrap();
                fs::set_permissions(&journal, fs::Permissions::from_mode(0o644)).unwrap();
            }
            let before = fs::read(&path).unwrap();
            assert!(
                super::super::LocalStore::open_with_migration_headroom(&dir, u64::MAX).is_err()
            );
            assert!(
                fs::read(&path).unwrap() == before,
                "unsafe journal changed authority"
            );
            assert_eq!(fs::read(target).unwrap(), b"preserve operator bytes");
            fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    #[cfg(unix)]
    fn journal_open_rejects_replacement_and_hardlink_alias_without_modification() {
        for alias in [false, true] {
            let (dir, path) = fixture(if alias {
                "journal-open-hardlink"
            } else {
                "journal-open-replaced"
            });
            let journal = path.with_extension("sqlite3-journal");
            let retained = dir.join("retained-journal");
            let mut file = super::super::private_create_new(&journal).unwrap();
            std::io::Write::write_all(&mut file, b"original journal bytes").unwrap();
            drop(file);
            let result = open_private_journal_observing(&journal, || {
                if alias {
                    fs::hard_link(&journal, &retained).unwrap();
                } else {
                    fs::rename(&journal, &retained).unwrap();
                    let mut replacement = super::super::private_create_new(&journal).unwrap();
                    std::io::Write::write_all(&mut replacement, b"replacement journal bytes")
                        .unwrap();
                }
            });
            assert!(
                result.is_err(),
                "journal open accepted replacement or alias"
            );
            assert_eq!(fs::read(&retained).unwrap(), b"original journal bytes");
            assert_eq!(
                fs::read(&journal).unwrap(),
                if alias {
                    b"original journal bytes".as_slice()
                } else {
                    b"replacement journal bytes".as_slice()
                }
            );
            fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    #[cfg(unix)]
    fn journal_open_rejects_fifo_swap_without_waiting_or_modification() {
        use std::os::unix::fs::FileTypeExt;
        const PROBE: &str = "AGENTOBS_JOURNAL_OPEN_FIFO_PROBE";
        if std::env::var_os(PROBE).is_none() {
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "migration_admission::tests::journal_open_rejects_fifo_swap_without_waiting_or_modification",
                ])
                .env(PROBE, "1")
                .spawn()
                .unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                if let Some(status) = child.try_wait().unwrap() {
                    assert!(status.success());
                    return;
                }
                if std::time::Instant::now() >= deadline {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("journal open blocked on FIFO replacement");
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        let (dir, path) = fixture("journal-open-fifo");
        let journal = path.with_extension("sqlite3-journal");
        let retained = dir.join("retained-journal");
        let mut file = super::super::private_create_new(&journal).unwrap();
        std::io::Write::write_all(&mut file, b"original journal bytes").unwrap();
        drop(file);
        assert!(
            open_private_journal_observing(&journal, || {
                fs::rename(&journal, &retained).unwrap();
                assert!(
                    std::process::Command::new("mkfifo")
                        .args(["-m", "600"])
                        .arg(&journal)
                        .status()
                        .unwrap()
                        .success()
                );
            })
            .is_err()
        );
        assert_eq!(fs::read(retained).unwrap(), b"original journal bytes");
        assert!(
            fs::symlink_metadata(&journal)
                .unwrap()
                .file_type()
                .is_fifo()
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn final_refusal_rolls_back_authentic_v6_and_retry_preserves_state() {
        let dir = std::env::temp_dir().join(format!("agentobs-staged-v6-{}", std::process::id()));
        super::super::private_dir(&dir).unwrap();
        // This test invokes migration directly, so retain the same open lock that
        // LocalStore::open_internal establishes around production migration.
        let open_guard =
            super::super::acquire_private_lock(&dir, super::super::STORE_OPEN_LOCK_NAME).unwrap();
        let path = dir.join(super::super::DB_NAME);
        let mut file = super::super::private_create_new(&path).unwrap();
        std::io::Write::write_all(
            &mut file,
            include_bytes!("../tests/fixtures/local_state_v6.sqlite3"),
        )
        .unwrap();
        drop(file);
        let before = fs::read(&path).unwrap();
        let db = Connection::open(&path).unwrap();
        db.pragma_update(None, "synchronous", "FULL").unwrap();
        let admitted = initial_allowance(
            pragma(&db, "page_count").unwrap(),
            pragma(&db, "page_size").unwrap(),
        )
        .unwrap();
        let result = migrate_observing(&db, &path, admitted, |tx| {
            tx.execute(
                "UPDATE metadata SET value=?1 WHERE key='compatibility_fixture_padding'",
                ["y".repeat(524_288)],
            )?;
            Ok(())
        });
        assert!(matches!(
            result,
            Err(StoreError::MigrationAdmissionRequired)
        ));
        assert!(
            fs::read(&path).unwrap() == before,
            "rollback changed authority bytes"
        );
        migrate(&db, &path, 16 * 1024 * 1024).unwrap();
        drop(db);
        drop(open_guard);
        let store = super::super::LocalStore::open_current(&dir).unwrap();
        assert_eq!(store.record_count().unwrap(), 2);
        assert_eq!(store.report_status().unwrap().generation, 2);
        assert_eq!(store.report_status().unwrap().acknowledged_generation, 1);
        assert_eq!(store.report_visibility_epoch().unwrap(), 7);
        drop(store);
        fs::remove_dir_all(dir).unwrap();
    }
}
