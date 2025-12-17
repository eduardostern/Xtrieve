//! Transaction operations: Begin, End, Abort

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::RwLock;

use crate::error::{BtrieveError, BtrieveResult, StatusCode};
use crate::file_manager::locking::SessionId;

use super::dispatcher::{Engine, OperationRequest, OperationResponse};

/// Transaction ID counter
static TRANSACTION_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Transaction state
#[derive(Debug, Clone)]
pub struct Transaction {
    pub id: u64,
    pub session: SessionId,
    pub files: Vec<PathBuf>,
    pub mode: TransactionMode,
}

/// Transaction mode (from lock bias)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionMode {
    /// Exclusive transaction (all or nothing)
    Exclusive,
    /// Concurrent transaction (allows other readers)
    Concurrent,
}

impl TransactionMode {
    pub fn from_lock_bias(bias: i32) -> Self {
        if bias >= 200 {
            TransactionMode::Exclusive
        } else {
            TransactionMode::Concurrent
        }
    }
}

/// Global transaction table
/// In a full implementation, this would be part of the Engine
lazy_static::lazy_static! {
    static ref TRANSACTIONS: RwLock<HashMap<SessionId, Transaction>> = RwLock::new(HashMap::new());
}

/// Operation 19: Begin Transaction
pub fn begin_transaction(
    engine: &Engine,
    session: SessionId,
    req: &OperationRequest,
) -> BtrieveResult<OperationResponse> {
    // Check if session already has active transaction
    {
        let transactions = TRANSACTIONS.read();
        if transactions.contains_key(&session) {
            return Err(BtrieveError::Status(StatusCode::TransactionActive));
        }
    }

    let mode = TransactionMode::from_lock_bias(req.lock_bias);

    // Create new transaction
    let transaction = Transaction {
        id: TRANSACTION_COUNTER.fetch_add(1, Ordering::SeqCst),
        session,
        files: Vec::new(),
        mode,
    };

    // Register transaction
    {
        let mut transactions = TRANSACTIONS.write();
        transactions.insert(session, transaction);
    }

    Ok(OperationResponse::success())
}

/// Operation 20: End Transaction (Commit)
pub fn end_transaction(
    engine: &Engine,
    session: SessionId,
    _req: &OperationRequest,
) -> BtrieveResult<OperationResponse> {
    // Get and remove transaction
    let transaction = {
        let mut transactions = TRANSACTIONS.write();
        transactions.remove(&session)
            .ok_or(BtrieveError::Status(StatusCode::TransactionError))?
    };

    // Commit transaction on all files (applies WAL to main file)
    for file_path in &transaction.files {
        if let Some(file) = engine.files.get(file_path) {
            let f = file.read();
            f.commit_transaction(session)?;
        }
    }

    // Invalidate cache for transaction files to ensure fresh reads
    for file_path in &transaction.files {
        engine.cache.invalidate_file(&file_path.to_string_lossy());
    }

    // Release all locks held by session
    engine.locks.release_session(session);

    Ok(OperationResponse::success())
}

/// Operation 21: Abort Transaction (Rollback)
pub fn abort_transaction(
    engine: &Engine,
    session: SessionId,
    _req: &OperationRequest,
) -> BtrieveResult<OperationResponse> {
    // Get and remove transaction
    let transaction = {
        let mut transactions = TRANSACTIONS.write();
        transactions.remove(&session)
            .ok_or(BtrieveError::Status(StatusCode::TransactionError))?
    };

    // Abort all files - just delete WAL (main file was never modified)
    for file_path in &transaction.files {
        if let Some(file) = engine.files.get(file_path) {
            let f = file.read();
            f.abort_transaction(session)?;
        }

        // Invalidate cache for this file to ensure fresh reads after rollback
        engine.cache.invalidate_file(&file_path.to_string_lossy());
    }

    // Release all locks held by session
    engine.locks.release_session(session);

    Ok(OperationResponse::success())
}

/// Helper: Add file to current transaction and create per-session WAL
pub fn add_file_to_transaction(engine: &Engine, session: SessionId, file_path: PathBuf) {
    let mut transactions = TRANSACTIONS.write();
    if let Some(transaction) = transactions.get_mut(&session) {
        if !transaction.files.contains(&file_path) {
            transaction.files.push(file_path.clone());

            // Create per-session WAL for this file
            if let Some(file) = engine.files.get(&file_path) {
                let f = file.read();
                let _ = f.begin_transaction(session);
            }
        }
    }
}

/// Helper: Check if session has active transaction
pub fn has_transaction(session: SessionId) -> bool {
    let transactions = TRANSACTIONS.read();
    transactions.contains_key(&session)
}

/// Helper: Get transaction mode for session
pub fn get_transaction_mode(session: SessionId) -> Option<TransactionMode> {
    let transactions = TRANSACTIONS.read();
    transactions.get(&session).map(|t| t.mode)
}

/// Check if a file is locked by another session's transaction (for ACID isolation)
pub fn is_file_in_transaction(file_path: &PathBuf, requesting_session: SessionId) -> bool {
    let transactions = TRANSACTIONS.read();
    for (session, transaction) in transactions.iter() {
        if *session != requesting_session && transaction.files.contains(file_path) {
            return true;
        }
    }
    false
}

/// Get the session that has a transaction lock on a file
pub fn get_transaction_owner(file_path: &PathBuf) -> Option<SessionId> {
    let transactions = TRANSACTIONS.read();
    for (session, transaction) in transactions.iter() {
        if transaction.files.contains(file_path) {
            return Some(*session);
        }
    }
    None
}
