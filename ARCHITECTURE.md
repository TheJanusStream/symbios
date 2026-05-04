# Symbios Architecture

This document explains the design decisions and architectural patterns used in Symbios.

## Table of Contents

1. [System Overview](#system-overview)
2. [Structure-of-Arrays (SoA) Layout](#structure-of-arrays-soa-layout)
3. [Virtual Machine Design](#virtual-machine-design)
4. [Symbol Interning](#symbol-interning)
5. [Topology Skip-Links](#topology-skip-links)
6. [Compilation Pipeline](#compilation-pipeline)
7. [Derivation Engine](#derivation-engine)
8. [Genetic Algorithm Architecture](#genetic-algorithm-architecture)
9. [Source Round-Tripping Pipeline](#source-round-tripping-pipeline)
10. [Memory Bounds and Safety](#memory-bounds-and-safety)

---

## System Overview

Symbios is organized into four core modules, with the `system` module further split into focused submodules:

```text
src/
├── core/              # State management and data structures
│   ├── mod.rs         # SymbiosState with SoA layout
│   └── interner.rs    # Symbol table for string interning
├── parser/            # Rule and expression parsing
│   ├── mod.rs         # Parser combinators (nom-based)
│   └── ast.rs         # Abstract syntax tree definitions
├── vm/                # Expression evaluation engine
│   ├── mod.rs         # Stack-based virtual machine
│   ├── ops.rs         # Bytecode instruction set
│   ├── compiler.rs    # AST-to-bytecode compiler
│   └── decompiler.rs  # Bytecode-to-AST decompiler
└── system/            # Main orchestrator
    ├── mod.rs         # System struct, RuntimeRule, config types
    ├── derivation.rs  # Derive loop with stochastic rule selection
    ├── matching.rs    # Context-sensitive pattern matching
    ├── mutate.rs      # Mutation operators (9 types)
    ├── crossover.rs   # Crossover operators (4 strategies)
    ├── export.rs      # Rule decompilation and to_source()
    └── source.rs      # SourceGenotype wrapper, from_source()
```

### Data Flow

```text
Rule String → Parser → AST → Compiler → Bytecode → VM → f64 result
    ↓
Axiom String → Parser → Interned Symbols + Parameters → State
    ↓
Derivation Loop:
  1. Match predecessor + context (matching.rs)
  2. Select stochastic rule (derivation.rs)
  3. Evaluate guard condition (VM)
  4. Compile successor expressions (VM)
  5. Build new state with results
    ↓
Export Pipeline (reverse):
  Bytecode → Decompiler → AST → Formatter → Source Text
```

---

## Structure-of-Arrays (SoA) Layout

### Why SoA?

Traditional object-oriented approaches store module data as:

```rust
// Array-of-Structs (AoS) - NOT used
struct Module {
    symbol: u16,
    birth_time: f64,
    parameters: Vec<f64>,
    topology_link: Option<usize>,
}
let modules: Vec<Module>;
```

Symbios instead uses **Structure-of-Arrays**:

```rust
// SoA - What Symbios actually uses
struct ModuleData {
    symbol: u16,
    birth_time: f64,
    param_start: u32,
    param_len: u16,
    topology_link: u32,
}

pub struct SymbiosState {
    modules: Vec<ModuleData>,  // Flat array of metadata
    params: Vec<f64>,          // Separate flat array of all parameters
    current_time: f64,
    max_capacity: usize,
}
```

### Benefits

1. **Cache Locality**: During derivation, we iterate over all modules checking symbols and conditions. With SoA, all `symbol` fields are packed contiguously in memory, leading to fewer cache misses.

2. **Memory Efficiency**:
   - No per-module `Vec` overhead (24 bytes each on 64-bit systems)
   - Parameters stored in a single flat `Vec<f64>` with O(1) indexing via `param_start`

3. **WASM Compatibility**: Flat memory layout translates cleanly to WASM linear memory, avoiding pointer indirection.

4. **Serialization**: The entire state can be serialized/deserialized efficiently with serde.

### Trade-offs

- **Complexity**: Accessing a module requires indexing into multiple arrays
- **Solution**: Provide a `ModuleView<'a>` helper that presents a clean interface:

```rust
pub struct ModuleView<'a> {
    pub sym: u16,
    pub age: f64,
    pub params: &'a [f64],
    pub skip_idx: Option<usize>,
}
```

---

## Virtual Machine Design

### Stack-Based Bytecode VM

Symbios compiles arithmetic expressions (from conditions and successor parameters) into bytecode instructions, then executes them on a stack-based virtual machine.

#### Instruction Set

```rust
pub enum Op {
    // Data loading
    Push(f64),          // Push constant onto stack
    LoadParam(u16),     // Load predecessor parameter by index
    LoadAge,            // Load module age (current_time - birth_time)

    // Arithmetic
    Add, Sub, Mul, Div, Pow, Neg,

    // Relational (epsilon-based comparison)
    Eq, Ne, Gt, Lt, Ge, Le,

    // Logical
    And, Or, Not,

    // Math functions
    Math(MathOp),
}

pub enum MathOp {
    Sin, Cos, Tan,        // Trigonometric (unary)
    Sqrt, Abs,            // Unary
    Floor, Ceil, Round,   // Rounding (unary)
    Min, Max,             // Binary (arity=2)
}
```

#### Example Compilation

```text
Expression:  x + 2 * sin(y)
Bytecode:    LoadParam(0)  // x
             Push(2.0)
             LoadParam(1)  // y
             Math(Sin)
             Mul
             Add
Stack trace: [x] → [x, 2.0] → [x, 2.0, y] → [x, 2.0, sin(y)] → [x, 2.0*sin(y)] → [x + 2.0*sin(y)]
```

### Why a VM?

#### Alternative 1: Interpret AST directly

- Problem: Tree traversal overhead on every derivation step
- Overhead: Pointer chasing, dynamic dispatch, repeated allocations

#### Alternative 2: Code generation

- Problem: Requires JIT compilation or unsafe FFI
- Not viable for WASM or embedded targets

#### VM Approach

- Compile once, execute many times
- Bytecode is compact (`Vec<Op>`, enum-based)
- Stack operations are cache-friendly
- Deterministic execution (no JIT non-determinism)

### Safety Guarantees

- Stack overflow protection (max 256 items)
- Stack underflow checks before each pop
- NaN/Infinity rejection (returns `VMError::MathError`)
- Division by zero produces NaN, caught by finite check
- Epsilon-based float comparison for relational operators

---

## Symbol Interning

### Problem

L-System strings can grow exponentially. Storing symbol names as `String` for each module would be prohibitively expensive.

### Solution

A **symbol table** (interner) maps strings to u16 IDs:

```rust
pub struct SymbolTable {
    to_id: HashMap<Arc<str>, u16>,
    to_str: Vec<Arc<str>>,
    current_bytes: usize,
    max_bytes: usize,  // Default: 10 MB
}
```

**Usage:**

```rust
let id = interner.get_or_intern("A")?;  // Returns u16
let name = interner.resolve(id)?;       // Returns &str
```

### Outcome

1. **Memory**: Each module stores only 2 bytes (u16) instead of 24+ bytes (String)
2. **Comparison**: Symbol equality is integer comparison (O(1))
3. **Bounds**: Maximum 65,535 unique symbols (u16::MAX)

### Safety

- Heap overflow protection: Rejects new symbols if `current_bytes > max_bytes`
- Returns error instead of OOM panics
- Uses `Arc<str>` for shared ownership

---

## Topology Skip-Links

### Context-Sensitive Matching

L-Systems support context-sensitive rules like:

```text
B < A(x) > C : condition -> successor
```

This means "match A only when preceded by B and followed by C".

### Branch Problem

In branching structures, context can span branches:

```text
String: X [ Y A ] Z
Rule:   Y < A > Z
```

The predecessor `A` is inside a branch `[]`, but its right context `Z` is outside.

### Skip-Link Solution

Symbios pre-calculates **topology links** that map:

- Open bracket `[` → matching close bracket `]`
- Close bracket `]` → matching open bracket `[`

**Algorithm:**

1. Scan the string once
2. Use a stack to match bracket pairs
3. Store forward/backward indices in `topology_links: [u32; NUM_TOPOLOGY_KINDS]`

**Per-kind isolation.** Each module reserves one link slot per `PairKind`
variant (currently `Branch` for `[ ]` and `Polygon` for `{ }`). Calling
`calculate_topology_kind(open, close, PairKind::Polygon)` writes into the
polygon slot only, so it does not erase prior branch links — and vice versa.
The 2-arg `calculate_topology(open, close)` is a `PairKind::Branch` shorthand
preserved for back-compat.

**Result:** O(1) navigation to skip entire branches during context matching.

**Example:**

```text
String:     X [ Y A ] Z
Indices:    0 1 2 3 4 5
Links:      - 1→4 - - 4→1 -
```

### Interaction with `#ignore`

Topology skip-links take precedence over `#ignore` for bracket symbols. Even if `[` and `]` appear in an `#ignore` directive, they are still used for branch navigation during context matching. The `#ignore` only suppresses them as context match targets.

### Per-rule `#ignore` override (issue #95)

Individual rules can shadow the global `#ignore` directive with a postfix
modifier:

```text
A < B > C -> X { ignore: + - }   // per-rule list replaces global
A < B > C -> Y { ignore: }       // empty list = nothing ignored for this rule
A < B > C -> Z                   // no postfix = inherit the global list
```

The per-rule list fully replaces the global list for matching purposes — it
is not additive. Symbols are interned via the same path as the global
directive. The postfix is round-tripped by `export_rule_to_string`, so a
parse → export cycle preserves the per-rule list verbatim.

---

## Compilation Pipeline

### Phase 1: Parsing and Interning

```rust
// Input
sys.add_rule("A(x) : x < 10 -> B(x + 1)")?;

// Parse (nom combinators)
let rule = parse_rule(input)?;  // Returns Rule AST

// Intern symbols
let pred_sym = interner.get_or_intern("A")?;
let succ_sym = interner.get_or_intern("B")?;
```

### Phase 2: Bytecode Compilation

```rust
// Compile guard condition
let guard_code = compiler.compile(&rule.condition)?;  // Vec<Op>

// Compile successor parameter expressions
for expr in &rule.successor.parameters {
    let param_code = compiler.compile(expr)?;
    // Store in RuntimeModule
}
```

### Phase 3: Execution (Derivation)

```rust
for each module in state {
    for each rule matching module.symbol {
        // Evaluate guard
        let guard_result = vm.eval(&rule.guard_code, module.params, module.age)?;
        if guard_result != 0.0 {
            // Collect matching rules, select stochastically
            // Evaluate successor parameters
            // Add to new state
        }
    }
}
```

### Phase 4: Decompilation (Reverse)

The decompiler reconstructs ASTs from bytecode, and the exporter formats them back to source text:

```text
Vec<Op> → Decompiler → Expr AST → format_expr() → String
RuntimeRule → export_rule() → Rule AST → format_rule() → Source text
```

This supports lossless round-tripping with preserved parameter names.

---

## Derivation Engine

### Stochastic Rule Selection

When multiple rules match a module, Symbios uses weighted random selection:

1. Collect all rules whose predecessor symbol matches and whose guard condition evaluates to nonzero
2. Sum their `probability` weights to get `total_weight`
3. Generate a random value in `[0, total_weight)` using the system RNG
4. Select the rule whose cumulative weight range contains the random value
5. A single matching rule always fires regardless of its probability value

### Zero-Allocation Matching

The `MatchScratch` struct provides reusable buffers to avoid per-module allocation:

```rust
pub struct MatchScratch {
    pub context_frame: Vec<f64>,    // Reusable parameter buffer
    pub left_indices: Vec<usize>,   // Left context match indices
    pub right_indices: Vec<usize>,  // Right context match indices
}
```

### Context Parameter Binding

When a context-sensitive rule matches, parameters from context modules are loaded into the VM evaluation frame. The derivation engine builds a combined parameter vector: predecessor params, then left context params, then right context params.

---

## Genetic Algorithm Architecture

### Mutation Operators

Symbios provides 9 mutation operators, organized by granularity:

| Operator                 | Config                      | Targets                       |
|--------------------------|-----------------------------|-------------------------------|
| **Probability mutation** | `MutationConfig`            | Rule probability weights      |
| **Constant mutation**    | `MutationConfig`            | `#define` constant values     |
| **Gaussian jitter**      | `MutationConfig`            | `Push(f64)` bytecode literals |
| **Structural mutation**  | `StructuralMutationConfig`  | Successor module sequence     |
| **Operator flip**        | `OperatorFlipConfig`        | Arithmetic/relational ops     |
| **Rule duplication**     | `RuleDuplicationConfig`     | Clone + specialize rules      |
| **Topological mutation** | `TopologicalMutationConfig` | Turtle command pairs          |
| **Literal promotion**    | `LiteralPromotionConfig`    | Promote literals to constants |
| **Sub-expression graft** | `graft_rate: f64`           | Branch blocks between systems |

### Crossover Strategies

| Strategy                    | Config                    | Behavior                                       |
|-----------------------------|---------------------------|------------------------------------------------|
| **Uniform**                 | `CrossoverConfig`         | Select entire rule sets per predecessor        |
| **Homologous**              | `AdvancedCrossoverConfig` | Select individual rules per shared predecessor |
| **BLX-alpha**               | `AdvancedCrossoverConfig` | Interpolate bytecode literals between parents  |
| **Sub-expression grafting** | `graft_rate: f64`         | Swap balanced `[...]` branch blocks            |

### Arity Safety

The system tracks `symbol_arities: HashMap<u16, usize>` to ensure structural mutations insert modules with the correct number of parameters. When inserting a new module, the mutation engine samples from known symbols and respects their observed arity.

### All Operators Have `_with_rng` Variants

Every mutation and crossover method delegates to a `_with_rng` variant that accepts an external `&mut impl Rng`. The non-`_with_rng` methods use the system's internal `Pcg64` RNG. This separation supports both convenience and deterministic testing.

---

## Source Round-Tripping Pipeline

### `from_source(source: &str) -> Result<System>`

Parses a multi-line L-system definition:

1. Lines starting with `#define` → constants
2. Lines starting with `#ignore` → ignored symbols
3. Lines starting with `omega:` → axiom
4. Other non-empty lines → preamble (comments) or rules
5. Rules are parsed with `parse_rule`, compiled, and stored

### `to_source() -> String`

Reconstructs complete source text from a compiled system:

1. Emit preamble lines (comments, `#ignore` directives)
2. Emit `#define` entries from stored constants (sorted alphabetically)
3. Emit `omega: <axiom>` from `axiom_source`
4. For each rule, decompile bytecode to AST, format with preserved parameter names

### `SourceGenotype`

A thin wrapper around source text that provides the Parse → Mutate → Reconstruct cycle:

```rust
pub struct SourceGenotype {
    source: String,
}
```

Each mutation/crossover call:

1. Parses `self.source` into a `System`
2. Applies the genetic operator
3. Calls `to_source()` to reconstruct source text
4. Stores the result back in `self.source`

This enables evolutionary loops that operate entirely in source space while leveraging the full compiled system for operator application.

---

## Memory Bounds and Safety

### DoS Attack Protection

Symbios is designed to reject adversarial input that could cause resource exhaustion.

| Limit                        | Value                  | Error                              |
|------------------------------|------------------------|------------------------------------|
| Max recursion depth (parser) | 64                     | `ParseError`                       |
| Max identifier length        | 64 chars               | `ParseError`                       |
| Max function arguments       | 32                     | `ParseError`                       |
| Max successor count          | 128                    | `CompileError::TooManySuccessors`  |
| Max nesting depth (topology) | 4,096                  | `SymbiosError::MaxNestingExceeded` |
| VM stack size                | 256                    | `VMError::StackOverflow`           |
| State capacity               | 1,000,000 modules      | `SymbiosError::CapacityOverflow`   |
| Interner heap                | 10 MB                  | Interner error                     |

### Numeric Safety

- All floats validated with `.is_finite()` during parsing
- NaN and Infinity rejected at parse time via `InvalidNumericValue`
- VM rejects NaN/Inf results with `VMError::MathError`
- Float equality uses epsilon comparison
- Time overflow in `advance_time` is hardened against wrapping

### Index Safety

- All array accesses bounds-checked
- u16/u32 overflow protection in `SymbiosState::push`
- Parameter indices validated before access
- Topology links validated during construction

### Zero Unsafe Code

Symbios contains **zero `unsafe` blocks**. All safety is achieved through:

- Rust's type system
- Explicit bounds checks
- Result-based error handling
- Comprehensive validation

---

## Error Hierarchy

```text
SystemError (top-level)
├── ParseError(String)              # Syntax errors from nom parser
├── CompileError(String)            # Variable resolution, successor limits
├── InvalidPredecessorParam         # Context symbols with parameters
├── InternerError(String)           # Symbol table overflow
├── VMError(String)                 # Stack overflow, NaN/Inf, underflow
├── State(SymbiosError)             # Core state errors
│   ├── ParameterOverflow           # > u16::MAX parameters per module
│   ├── UnmatchedBracket(usize)     # Topology link failure
│   ├── AmbiguousTopology           # Same symbol for open and close
│   ├── MaxNestingExceeded          # > 4096 bracket levels
│   ├── CapacityOverflow            # > max_capacity modules
│   ├── InvalidIndex(usize)         # Out-of-bounds module access
│   └── InvalidNumericValue         # NaN/Inf in input
└── StateCorruption(usize)          # Internal invariant violation
```

---

## Performance Characteristics

| Operation            | Complexity       | Notes                               |
|----------------------|------------------|-------------------------------------|
| Rule matching        | O(N × R)         | N = modules, R = rules per symbol   |
| Context matching     | O(1) per context | Via skip-links                      |
| Parameter access     | O(1)             | Direct indexing into flat array     |
| Symbol comparison    | O(1)             | Integer equality                    |
| VM execution         | O(I)             | I = instruction count               |
| Topology calculation | O(N)             | Single scan with stack              |
| Stochastic selection | O(R)             | Linear scan of matching rules       |
| Mutation (all types) | O(R × S)         | R = rules, S = successors per rule  |
| Crossover            | O(R)             | R = total rules across both parents |

---

## Design Philosophy

1. **Determinism**: Same input always produces same output (given same RNG seed)
2. **No Surprises**: Reject bad input with errors, never panic
3. **Performance**: Optimize for common case (large strings, many derivations)
4. **Embeddability**: No heavy dependencies, portable to WASM/embedded
5. **Correctness**: Strictly adhere to ABOP specification
6. **Evolvability**: First-class support for genetic operations and source preservation

---

## References

- [Prusinkiewicz, P., & Lindenmayer, A. (1990). *The Algorithmic Beauty of Plants*. Springer-Verlag.](https://algorithmicbotany.org/papers/abop/abop.pdf)
- [L-Systems Wikipedia](https://en.wikipedia.org/wiki/L-system)
- [nom Parser Combinator Library](https://github.com/rust-bakery/nom)
