//! Search module for Gomoku AI
//!
//! Contains:
//! - Zobrist hashing for position identification
//! - Transposition table for caching search results (upcoming)
//! - Alpha-Beta search with iterative deepening (upcoming)
//! - VCF/VCT threat search (upcoming)

pub mod zobrist;

pub use zobrist::ZobristTable;
