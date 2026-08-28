use std::fs;
use std::path::Path;

pub const MIB: u64 = 1024 * 1024;
const BLOCK: u64 = 4096;
pub const MAX_ACCOUNTING_ENTRIES: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Partition {
    pub bytes: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageBudget {
    pub total: u64,
    pub headroom: Partition,
    pub state: Partition,
    pub team: Partition,
    pub projection: Partition,
    pub diagnostic: Partition,
}

impl StorageBudget {
    pub fn calculate(total: u64, team_enabled: bool) -> Result<Self, StorageError> {
        if !(256 * MIB..=20 * 1024 * MIB).contains(&total) {
            return Err(StorageError::BudgetOutOfBounds);
        }
        let head = (32 * MIB).max(total / 8);
        let remainder = total - head;
        let mut state = remainder.saturating_mul(40) / 100;
        let mut team = remainder.saturating_mul(50) / 100;
        let mut projection = remainder.saturating_mul(8) / 100;
        let mut diagnostic = remainder.saturating_mul(2) / 100;
        state = state / MIB * MIB;
        team = team / MIB * MIB;
        projection = projection / MIB * MIB;
        diagnostic = diagnostic / MIB * MIB;
        if state < 80 * MIB || team < 96 * MIB || projection < 16 * MIB || diagnostic < 4 * MIB {
            return Err(StorageError::MinimumPartition);
        }
        // The disabled profile lends the team partition to state, but never the
        // separately reserved headroom. Keep the total accounting identity exact.
        let used = head + state + team + projection + diagnostic;
        let extra = total - used;
        let team_allocation = if team_enabled { team } else { 0 };
        Ok(Self {
            total,
            headroom: Partition {
                bytes: head + extra,
            },
            state: Partition {
                bytes: state + if !team_enabled { team } else { 0 },
            },
            team: Partition {
                bytes: team_allocation,
            },
            projection: Partition { bytes: projection },
            diagnostic: Partition { bytes: diagnostic },
        })
    }
    #[allow(clippy::needless_return)]
    pub fn allocated_blocks(path: &Path) -> std::io::Result<u64> {
        let metadata = fs::metadata(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            return Ok(metadata.blocks().saturating_mul(512).div_ceil(BLOCK));
        }
        #[cfg(not(unix))]
        {
            Ok(metadata.len().div_ceil(BLOCK))
        }
    }
    pub fn allocated_bytes(path: &Path) -> std::io::Result<u64> {
        Self::allocated_blocks(path).map(|blocks| blocks * BLOCK)
    }
    pub fn allocated_tree_bytes(path: &Path) -> Result<u64, StorageAccountingError> {
        let mut pending = vec![(path.to_path_buf(), true)];
        let mut entries = 1_usize;
        let mut bytes = 0_u64;
        while let Some((current, is_root)) = pending.pop() {
            let metadata = match fs::symlink_metadata(&current) {
                Ok(metadata) => metadata,
                Err(error) if !is_root && error.kind() == std::io::ErrorKind::NotFound => {
                    continue;
                }
                Err(error) => return Err(StorageAccountingError::Io(error)),
            };
            if metadata.file_type().is_symlink() {
                return Err(StorageAccountingError::Symlink);
            }
            bytes = bytes
                .checked_add(allocated_metadata_bytes(&metadata))
                .ok_or(StorageAccountingError::Overflow)?;
            if metadata.is_dir() {
                let directory = match fs::read_dir(&current) {
                    Ok(directory) => directory,
                    Err(error) if !is_root && error.kind() == std::io::ErrorKind::NotFound => {
                        continue;
                    }
                    Err(error) => return Err(StorageAccountingError::Io(error)),
                };
                for entry in directory {
                    entries = entries.saturating_add(1);
                    if entries > MAX_ACCOUNTING_ENTRIES {
                        return Err(StorageAccountingError::EntryLimit);
                    }
                    match entry {
                        Ok(entry) => pending.push((entry.path(), false)),
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(error) => return Err(StorageAccountingError::Io(error)),
                    }
                }
            } else if !metadata.is_file() {
                return Err(StorageAccountingError::UnsupportedFileType);
            }
        }
        Ok(bytes)
    }
    pub fn admit(&self, current_allocated: u64, worst_case_write: u64) -> Admission {
        if current_allocated.saturating_add(worst_case_write) > self.writable_limit() {
            Admission::Denied
        } else {
            Admission::Allowed {
                reserved: worst_case_write,
            }
        }
    }

    #[must_use]
    pub fn writable_limit(&self) -> u64 {
        self.total.saturating_sub(self.headroom.bytes)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admission {
    Allowed { reserved: u64 },
    Denied,
}
#[derive(Debug, PartialEq, Eq)]
pub enum StorageError {
    BudgetOutOfBounds,
    MinimumPartition,
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::BudgetOutOfBounds => "local storage budget is outside 256 MiB..=20 GiB",
            Self::MinimumPartition => "local storage budget cannot satisfy minimum partitions",
        })
    }
}

impl std::error::Error for StorageError {}

#[derive(Debug)]
pub enum StorageAccountingError {
    Io(std::io::Error),
    EntryLimit,
    Symlink,
    UnsupportedFileType,
    Overflow,
}

impl std::fmt::Display for StorageAccountingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Io(_) => "storage accounting I/O failure",
            Self::EntryLimit => "storage accounting entry limit exceeded",
            Self::Symlink => "storage accounting refuses symbolic links",
            Self::UnsupportedFileType => "storage accounting refuses unsupported file types",
            Self::Overflow => "storage accounting overflow",
        })
    }
}

#[allow(clippy::needless_return)]
fn allocated_metadata_bytes(metadata: &fs::Metadata) -> u64 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        return metadata.blocks().saturating_mul(512).div_ceil(BLOCK) * BLOCK;
    }
    #[cfg(not(unix))]
    {
        metadata.len().div_ceil(BLOCK) * BLOCK
    }
}

impl std::error::Error for StorageAccountingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partitions_preserve_hard_cap_and_disabled_team_is_lendable() {
        let a = StorageBudget::calculate(256 * MIB, true).unwrap();
        let b = StorageBudget::calculate(256 * MIB, false).unwrap();
        assert_eq!(
            a.total,
            a.headroom.bytes
                + a.state.bytes
                + a.team.bytes
                + a.projection.bytes
                + a.diagnostic.bytes
        );
        assert_eq!(
            b.total,
            b.headroom.bytes
                + b.state.bytes
                + b.team.bytes
                + b.projection.bytes
                + b.diagnostic.bytes
        );
        assert!(b.state.bytes > a.state.bytes);
        assert_eq!(a.headroom, b.headroom);
    }
    #[test]
    fn admission_is_worst_case() {
        let b = StorageBudget::calculate(256 * MIB, true).unwrap();
        assert_eq!(b.admit(b.writable_limit() - 1, 2), Admission::Denied);
        assert_eq!(b.admit(0, 10), Admission::Allowed { reserved: 10 });
    }

    #[test]
    fn allocated_tree_is_bounded_and_counts_files() {
        let root = std::env::temp_dir().join(format!("runtime-accounting-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("state")).unwrap();
        fs::write(root.join("state/data"), vec![0_u8; 5000]).unwrap();
        assert!(StorageBudget::allocated_tree_bytes(&root).unwrap() >= 8192);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn allocated_tree_still_rejects_a_missing_root() {
        let root =
            std::env::temp_dir().join(format!("runtime-accounting-missing-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        assert!(matches!(
            StorageBudget::allocated_tree_bytes(&root),
            Err(StorageAccountingError::Io(error))
                if error.kind() == std::io::ErrorKind::NotFound
        ));
    }

    #[cfg(unix)]
    #[test]
    fn allocated_tree_rejects_symlinks() {
        use std::os::unix::fs::symlink;

        let root =
            std::env::temp_dir().join(format!("runtime-accounting-link-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("target"), b"x").unwrap();
        symlink(root.join("target"), root.join("link")).unwrap();
        assert!(matches!(
            StorageBudget::allocated_tree_bytes(&root),
            Err(StorageAccountingError::Symlink)
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn allocated_tree_fails_closed_above_entry_limit() {
        let root =
            std::env::temp_dir().join(format!("runtime-accounting-limit-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        for index in 0..MAX_ACCOUNTING_ENTRIES {
            fs::write(root.join(index.to_string()), []).unwrap();
        }
        assert!(matches!(
            StorageBudget::allocated_tree_bytes(&root),
            Err(StorageAccountingError::EntryLimit)
        ));
        let _ = fs::remove_dir_all(root);
    }
}
