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
//! *   **Sovereign Architecture**: Zero heavy dependencies.
//! *   **Structure-of-Arrays (SoA)**: Optimized memory layout for cache locality.
//! *   **Parametric**: Full support for arithmetic expressions in rules.
//! *   **Context-Sensitive**: `(k,l)-systems` with variable binding.
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

pub mod core;
pub mod parser;
pub mod system;
pub mod vm;

pub use crate::core::{NUM_TOPOLOGY_KINDS, PairKind, SymbiosState, interner::SymbolTable};
pub use crate::system::System;
pub use crate::system::export::{ExportConfig, export_rule, export_rule_to_string};
pub use crate::system::source::SourceGenotype;
