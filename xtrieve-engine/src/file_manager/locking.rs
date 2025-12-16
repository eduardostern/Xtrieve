//! Locking mechanisms for concurrent access to Btrieve files
//!
//! Supports file-level and record-level locking with Btrieve's lock modes.

use parking_lot::{Mutex, RwLock};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::error::{BtrieveError, BtrieveResult, StatusCode};
use crate::storage::record::RecordAddress;

/// Lock types matching Btrieve's lock modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockType {
    /// No lock
    None,
    /// Single-record lock with wait
    SingleWait,
    /// Single-record lock without wait
    SingleNoWait,
    /// Multiple-record lock with wait
    MultiWait,
    /// Multiple-record lock without wait
    MultiNoWait,
}

impl LockType {
    /// Create from Btrieve lock bias value
    pub fn from_bias(bias: i32) -> Self {
        match bias {
            100..=199 => LockType::SingleWait,
            200..=299 => LockType::SingleNoWait,
            300..=399 => LockType::MultiWait,
            400..=499 => LockType::MultiNoWait,
            _ => LockType::None,
        }
    }

    /// Convert to Btrieve lock bias value
    pub fn to_bias(&self) -> i32 {
        match self {
            LockType::None => 0,
            LockType::SingleWait => 100,
            LockType::SingleNoWait => 200,
            LockType::MultiWait => 300,
            LockType::MultiNoWait => 400,
        }
    }

    /// Check if this lock waits for conflicts
    pub fn waits(&self) -> bool {
        matches!(self, LockType::SingleWait | LockType::MultiWait)
    }

    /// Check if this is a multi-record lock
    pub fn is_multi(&self) -> bool {
        matches!(self, LockType::MultiWait | LockType::MultiNoWait)
    }
}

/// Session identifier (client connection)
pub type SessionId = u64;

/// Record lock information
#[derive(Debug, Clone)]
struct RecordLock {
    session: SessionId,
    lock_type: LockType,
    acquired_at: Instant,
}

/// File lock state
#[derive(Debug)]
struct FileLockState {
    /// Exclusive file lock holder (if any)
    exclusive_holder: Option<SessionId>,
    /// Sessions with shared access
    shared_holders: HashSet<SessionId>,
    /// Record-level locks: address -> lock info
    record_locks: HashMap<RecordAddress, RecordLock>,
}

impl Default for FileLockState {
    fn default() -> Self {
        FileLockState {
            exclusive_holder: None,
            shared_holders: HashSet::new(),
            record_locks: HashMap::new(),
        }
    }
}

/// Lock manager for Btrieve files
pub struct LockManager {
    /// Lock state per file
    files: RwLock<HashMap<String, Arc<Mutex<FileLockState>>>>,
    /// Lock timeout for waiting locks
    timeout: Duration,
}

impl LockManager {
    /// Create a new lock manager
    pub fn new(timeout: Duration) -> Self {
        LockManager {
            files: RwLock::new(HashMap::new()),
            timeout,
        }
    }

    /// Get or create lock state for a file
    fn get_file_state(&self, file_path: &str) -> Arc<Mutex<FileLockState>> {
        let files = self.files.read();
        if let Some(state) = files.get(file_path) {
            return state.clone();
        }
        drop(files);

        let mut files = self.files.write();
        files
            .entry(file_path.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(FileLockState::default())))
            .clone()
    }

    /// Acquire a file-level lock for a session
    pub fn lock_file(
        &self,
        file_path: &str,
        session: SessionId,
        exclusive: bool,
    ) -> BtrieveResult<()> {
        let state = self.get_file_state(file_path);
        let mut lock_state = state.lock();

        if exclusive {
            // Check for conflicts
            if lock_state.exclusive_holder.is_some() {
                return Err(StatusCode::FileInUse.into());
            }
            if !lock_state.shared_holders.is_empty()
                && !lock_state.shared_holders.contains(&session)
            {
                return Err(StatusCode::FileInUse.into());
            }

            lock_state.exclusive_holder = Some(session);
            lock_state.shared_holders.clear();
        } else {
            // Shared lock
            if let Some(holder) = lock_state.exclusive_holder {
                if holder != session {
                    return Err(StatusCode::FileInUse.into());
                }
            }
            lock_state.shared_holders.insert(session);
        }

        Ok(())
    }

    /// Release a file-level lock
    pub fn unlock_file(&self, file_path: &str, session: SessionId) {
        let state = self.get_file_state(file_path);
        let mut lock_state = state.lock();

        if lock_state.exclusive_holder == Some(session) {
            lock_state.exclusive_holder = None;
        }
        lock_state.shared_holders.remove(&session);
    }

    /// Acquire a record lock
    pub fn lock_record(
        &self,
        file_path: &str,
        address: RecordAddress,
        session: SessionId,
        lock_type: LockType,
    ) -> BtrieveResult<()> {
        if lock_type == LockType::None {
            return Ok(());
        }

        let state = self.get_file_state(file_path);
        let deadline = Instant::now() + self.timeout;

        loop {
            let mut lock_state = state.lock();

            // Check for existing lock
            if let Some(existing) = lock_state.record_locks.get(&address) {
                if existing.session != session {
                    // Conflict with another session
                    if !lock_type.waits() {
                        return Err(StatusCode::RecordInUse.into());
                    }

                    // Check timeout
                    if Instant::now() >= deadline {
                        return Err(StatusCode::WaitLockError.into());
                    }

                    // Drop lock and wait
                    drop(lock_state);
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                } else if !lock_type.is_multi() {
                    // Same session, single lock - replace
                }
            }

            // Acquire lock
            lock_state.record_locks.insert(
                address,
                RecordLock {
                    session,
                    lock_type,
                    acquired_at: Instant::now(),
                },
            );

            return Ok(());
        }
    }

    /// Release a record lock
    pub fn unlock_record(
        &self,
        file_path: &str,
        address: RecordAddress,
        session: SessionId,
    ) {
        let state = self.get_file_state(file_path);
        let mut lock_state = state.lock();

        if let Some(lock) = lock_state.record_locks.get(&address) {
            if lock.session == session {
                lock_state.record_locks.remove(&address);
            }
        }
    }

    /// Release all record locks for a session
    pub fn unlock_all_records(&self, file_path: &str, session: SessionId) {
        let state = self.get_file_state(file_path);
        let mut lock_state = state.lock();

        lock_state
            .record_locks
            .retain(|_, lock| lock.session != session);
    }

    /// Release all locks for a session (file and record)
    pub fn release_session(&self, session: SessionId) {
        let files = self.files.read();
        for (_, state) in files.iter() {
            let mut lock_state = state.lock();

            if lock_state.exclusive_holder == Some(session) {
                lock_state.exclusive_holder = None;
            }
            lock_state.shared_holders.remove(&session);
            lock_state
                .record_locks
                .retain(|_, lock| lock.session != session);
        }
    }

    /// Check if a record is locked by another session
    pub fn is_record_locked(
        &self,
        file_path: &str,
        address: RecordAddress,
        session: SessionId,
    ) -> bool {
        let state = self.get_file_state(file_path);
        let lock_state = state.lock();

        if let Some(lock) = lock_state.record_locks.get(&address) {
            return lock.session != session;
        }
        false
    }

    /// Clean up lock state for a closed file
    pub fn cleanup_file(&self, file_path: &str) {
        let mut files = self.files.write();
        files.remove(file_path);
    }
}

impl Default for LockManager {
    fn default() -> Self {
        Self::new(Duration::from_secs(30))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_lock() {
        let manager = LockManager::default();

        // Session 1 gets shared lock
        manager.lock_file("test.dat", 1, false).unwrap();

        // Session 2 can also get shared lock
        manager.lock_file("test.dat", 2, false).unwrap();

        // Session 3 cannot get exclusive lock
        let result = manager.lock_file("test.dat", 3, true);
        assert!(result.is_err());

        // Release shared locks
        manager.unlock_file("test.dat", 1);
        manager.unlock_file("test.dat", 2);

        // Now session 3 can get exclusive
        manager.lock_file("test.dat", 3, true).unwrap();
    }

    #[test]
    fn test_record_lock() {
        let manager = LockManager::default();
        let addr = RecordAddress::new(1, 0);

        // Session 1 locks record
        manager
            .lock_record("test.dat", addr, 1, LockType::SingleNoWait)
            .unwrap();

        // Session 2 cannot lock (no wait)
        let result = manager.lock_record("test.dat", addr, 2, LockType::SingleNoWait);
        assert!(result.is_err());

        // Session 1 can relock
        manager
            .lock_record("test.dat", addr, 1, LockType::SingleNoWait)
            .unwrap();

        // Unlock
        manager.unlock_record("test.dat", addr, 1);

        // Now session 2 can lock
        manager
            .lock_record("test.dat", addr, 2, LockType::SingleNoWait)
            .unwrap();
    }
}
