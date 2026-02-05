//! Search module for Gomoku AI
//!
//! Contains:
//! - Zobrist hashing for position identification
//! - Transposition table for caching search results
//! - Alpha-Beta search with iterative deepening (upcoming)
//! - VCF/VCT threat search (upcoming)

pub mod tt;
pub mod zobrist;

pub use tt::{EntryType, TTEntry, TTStats, TranspositionTable};
pub use zobrist::ZobristTable;
