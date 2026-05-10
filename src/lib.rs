//! # Symbios
//!
//! **A Sovereign Derivation Engine for Parametric L-Systems.**
//!
//! Symbios is a pure-Rust engine for generating Lindenmayer Systems, strictly adhering to the
//! syntax and semantics defined in *The Algorithmic Beauty of Plants* (Prusinkiewicz & Lindenmayer, 1990).
//!
//! It is designed for high-performance, embedded, and WebAssembly environments where
//! reliability and determinism are paramount.
//!
//! ## Key Features
//!
//! *   **Sovereign Architecture**: Pure Rust, only `nom`, `rand`, `thiserror`, `serde`.
//! *   **Structure-of-Arrays (SoA)**: Optimized memory layout for cache locality.
//! *   **Parametric**: Full support for arithmetic expressions in rules.
//! *   **Context-Sensitive**: `(k,l)-systems` with variable binding.
//! *   **Evolvable**: Mutation, crossover, and source-text round-tripping for
//!     genetic-algorithm workflows. See [`SourceGenotype`] and the
//!     [`system::mutate`] / [`system::crossover`] modules.
//! *   **Zero `unsafe`**: All safety via Rust's type system and explicit
//!     bounds checks.
//!
//! ## Example
//!
//! ```rust
//! use symbios::System;
//!
//! let mut sys = System::new();
//! // Define a rule: A module 'A' grows if parameter 'x' is small
//! sys.add_rule("A(x) : x < 10 -> A(x + 1) B(x)").unwrap();
//! sys.set_axiom("A(0)").unwrap();
//! sys.derive(5).unwrap();
//! ```
//!
//! ## Source round-tripping
//!
//! [`System::from_source`] parses a whole L-system file (directives, axiom,
//! rules, and comments). [`System::to_source`] reverses the process,
//! emitting a deterministic, lossless serialization that preserves
//! parameter names and the preamble. This pair is what makes
//! [`SourceGenotype`] possible.

pub mod core;
pub mod parser;
pub mod system;
pub mod vm;

pub use crate::core::{NUM_TOPOLOGY_KINDS, PairKind, SymbiosState, interner::SymbolTable};
pub use crate::system::System;
pub use crate::system::export::{ExportConfig, export_rule, export_rule_to_string};
pub use crate::system::source::SourceGenotype;
