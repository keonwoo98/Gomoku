//! Search module for Gomoku AI
//!
//! Contains:
//! - Zobrist hashing for position identification
//! - Transposition table for caching search results
//! - Alpha-Beta search with iterative deepening
//! - VCF/VCT threat search for forced wins

pub mod alphabeta;
pub mod threat;
pub mod tt;
pub mod zobrist;

pub use alphabeta::{SearchResult, Searcher};
pub use threat::{ThreatResult, ThreatSearcher};
pub use tt::{AtomicTT, EntryType, TTEntry, TTStats, TranspositionTable};
pub use zobrist::ZobristTable;
