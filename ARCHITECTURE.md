# Symbios Architecture

This document explains the design decisions and architectural patterns used in Symbios.

## Table of Contents

1. [System Overview](#system-overview)
2. [Structure-of-Arrays (SoA) Layout](#structure-of-arrays-soa-layout)
3. [Virtual Machine Design](#virtual-machine-design)
4. [Symbol Interning](#symbol-interning)
5. [Topology Skip-Links](#topology-skip-links)
6. [Compilation Pipeline](#compilation-pipeline)
7. [Memory Bounds and Safety](#memory-bounds-and-safety)

---

## System Overview

Symbios is organized into four core modules:

```
src/
├── core/          # State management and data structures
│   ├── mod.rs     # SymbiosState with SoA layout
│   └── interner.rs # Symbol table for string interning
├── parser/        # Rule and expression parsing
│   ├── mod.rs     # Parser combinators (nom-based)
│   └── ast.rs     # Abstract syntax tree definitions
├── vm/            # Expression evaluation engine
│   ├── mod.rs     # Stack-based virtual machine
│   ├── ops.rs     # Bytecode instruction set
│   └── compiler.rs # AST-to-bytecode compiler
└── system.rs      # Main orchestrator (System struct)
```

### Data Flow

```
Rule String → Parser → AST → Compiler → Bytecode → VM → f64 result
    ↓
Axiom String → Parser → Interned Symbols + Parameters → State
    ↓
Derivation Loop:
  1. Match predecessor + context
  2. Evaluate guard condition (VM)
  3. Compile successor expressions (VM)
  4. Build new state with results
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

#### Example Compilation

```
Expression:  x + 2 * sin(y)
Bytecode:    LoadParam(0)  // x
             Push(2.0)
             LoadParam(1)  // y
             Math(Sin, 1)
             Mul
             Add
Stack trace: [x] → [x, 2.0] → [x, 2.0, y] → [x, 2.0, sin(y)] → [x, 2.0*sin(y)] → [x + 2.0*sin(y)]
```

### Why a VM?

**Alternative 1: Interpret AST directly**
- Problem: Tree traversal overhead on every derivation step
- Overhead: Pointer chasing, dynamic dispatch, repeated allocations

**Alternative 2: Code generation**
- Problem: Requires JIT compilation or unsafe FFI
- Not viable for WASM or embedded targets

**VM Approach:**
- Compile once, execute many times
- Bytecode is compact (`Vec<Op>`, enum-based)
- Stack operations are cache-friendly
- Deterministic execution (no JIT non-determinism)

### Safety Guarantees

- Stack overflow protection (max 256 items)
- Stack underflow checks before each pop
- NaN/Infinity rejection (returns `VMError::MathError`)
- Division by zero returns NaN (not panic)

---

## Symbol Interning

### Problem

L-System strings can grow exponentially. Storing symbol names as `String` for each module would be prohibitively expensive.

### Solution

A **symbol table** (interner) maps strings ↔ u16 IDs:

```rust
pub struct SymbolTable {
    map: HashMap<String, u16>,
    strings: Vec<String>,
    heap_size: usize,
    max_heap: usize,  // Default: 10 MB
}
```

**Usage:**
```rust
let id = interner.get_or_intern("A")?;  // Returns u16
let name = interner.resolve(id)?;       // Returns &str
```

### Benefits

1. **Memory**: Each module stores only 2 bytes (u16) instead of 24+ bytes (String)
2. **Comparison**: Symbol equality is integer comparison (O(1))
3. **Bounds**: Maximum 65,535 unique symbols (u16::MAX)

### Safety

- Heap overflow protection: Rejects new symbols if `heap_size > max_heap`
- Returns `InternerError::HeapOverflow` instead of OOM panics

---

## Topology Skip-Links

### Context-Sensitive Matching

L-Systems support context-sensitive rules like:

```
B < A(x) > C : condition -> successor
```

This means "match A only when preceded by B and followed by C".

### Branch Problem

In branching structures, context can span branches:

```
String: X [ Y A ] Z
Rule:   Y < A > Z
```

The predecessor `A` is inside a branch `[]`, but its right context `Z` is outside.

### Skip-Link Solution

Symbios pre-calculates **topology links** that map:
- Open bracket `[` → matching close bracket `]`
- Close bracket `]` → matching open bracket `[`

```rust
pub fn calculate_topology(&mut self, open_sym: u16, close_sym: u16) -> Result<(), SymbiosError>
```

**Algorithm:**
1. Scan the string once
2. Use a stack to match bracket pairs
3. Store forward/backward indices in `topology_link: u32`

**Result:** O(1) navigation to skip entire branches during context matching.

**Example:**
```
String:     X [ Y A ] Z
Indices:    0 1 2 3 4 5
Links:      - 1→4 - - 4→1 -
```

When matching at index 3 (`A`), the right context matcher can:
1. Check index 4 (close bracket)
2. Follow `topology_link` to index 5
3. Find `Z` in constant time

---

## Compilation Pipeline

### Phase 1: Parsing and Interning

```rust
// Input
sys.add_rule("A(x) : x < 10 -> B(x + 1)")?;

// Parse
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
    // Store in compiled rule
}
```

### Phase 3: Execution (Derivation)

```rust
for each module in state {
    for each rule {
        if module.symbol == rule.predecessor_symbol {
            // Evaluate guard
            let guard_result = vm.eval(&rule.guard_code, module.params, module.age)?;
            if guard_result != 0.0 {
                // Evaluate successor parameters
                let new_params = rule.successor_codes
                    .iter()
                    .map(|code| vm.eval(code, module.params, module.age))
                    .collect()?;

                // Add to new state
                new_state.push(rule.successor_symbol, 0.0, &new_params)?;
            }
        }
    }
}
```

---

## Memory Bounds and Safety

### DoS Attack Protection

Symbios is designed to reject adversarial input that could cause resource exhaustion.

| Limit | Value | Error |
|-------|-------|-------|
| Max recursion depth (parser) | 64 | `ParseError::MaxRecursionDepth` |
| Max identifier length | 64 chars | `ParseError::IdentifierTooLong` |
| Max function arguments | 32 | `ParseError::TooManyArgs` |
| Max successor count | 128 | `CompileError::TooManySuccessors` |
| Max nesting depth (topology) | 4,096 | `SymbiosError::MaxNestingExceeded` |
| VM stack size | 256 | `VMError::StackOverflow` |
| State capacity | 1,000,000 modules | `SymbiosError::CapacityOverflow` |
| Interner heap | 10 MB | `InternerError::HeapOverflow` |

### Numeric Safety

- All floats validated with `.is_finite()` during parsing
- NaN and Infinity rejected in input
- Division by zero returns NaN (safe, not panic)
- Float equality uses epsilon comparison (`float_eq`)

### Index Safety

- All array accesses bounds-checked
- u16/u32 overflow protection
- Parameter indices validated before access
- Topology links validated during construction

### Zero Unsafe Code

Symbios contains **zero `unsafe` blocks**. All safety is achieved through:
- Rust's type system
- Explicit bounds checks
- Result-based error handling
- Comprehensive validation

---

## Performance Characteristics

| Operation | Complexity | Notes |
|-----------|-----------|-------|
| Rule matching | O(N × R) | N = modules, R = rules |
| Context matching | O(1) per context | Via skip-links |
| Parameter access | O(1) | Direct indexing |
| Symbol comparison | O(1) | Integer equality |
| VM execution | O(I) | I = instruction count |
| Topology calculation | O(N) | Single scan with stack |

---

## Design Philosophy

1. **Determinism**: Same input always produces same output (given same RNG seed)
2. **No Surprises**: Reject bad input with errors, never panic
3. **Performance**: Optimize for common case (large strings, many derivations)
4. **Embeddability**: No heavy dependencies, portable to WASM/embedded
5. **Correctness**: Strictly adhere to ABOP specification

---

## Future Considerations

### Potential Optimizations

1. **SIMD Parameter Evaluation**: Batch evaluate expressions across multiple modules
2. **Rule Compilation Cache**: Memoize frequently-matched rule patterns
3. **Parallel Derivation**: Partition state for multi-threaded execution

### Non-Goals

- **GPU Acceleration**: Adds complexity, limits portability
- **JIT Compilation**: Incompatible with WASM, adds non-determinism
- **Dynamic Rule Modification**: Complicates caching, rarely needed

---

## References

- [Prusinkiewicz, P., & Lindenmayer, A. (1990). *The Algorithmic Beauty of Plants*. Springer-Verlag.](https://algorithmicbotany.org/papers/abop/abop.pdf)
- [L-Systems Wikipedia](https://en.wikipedia.org/wiki/L-system)
- [nom Parser Combinator Library](https://github.com/rust-bakery/nom)
