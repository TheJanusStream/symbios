/// Built-in math functions callable from rule expressions.
///
/// These are emitted by the [`crate::vm::Compiler`] in response to `Expr::Call`
/// nodes whose function name matches one of the variants below
/// (case-sensitive: `sin`, `cos`, …). The VM dispatches each variant via
/// [`Op::Math`]. See [`MathOp::arity`] for how many arguments each consumes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MathOp {
    Sin,
    Cos,
    Tan,
    Sqrt,
    Abs,
    Floor,
    Ceil,
    Round,
    Min,
    Max,
}

impl MathOp {
    /// Returns the number of arguments this math operation consumes from the stack.
    #[inline]
    pub const fn arity(self) -> u8 {
        match self {
            MathOp::Sin
            | MathOp::Cos
            | MathOp::Tan
            | MathOp::Sqrt
            | MathOp::Abs
            | MathOp::Floor
            | MathOp::Ceil
            | MathOp::Round => 1,
            MathOp::Min | MathOp::Max => 2,
        }
    }
}

/// Stack-machine instruction emitted by the [`crate::vm::Compiler`] and consumed by
/// [`crate::vm::VirtualMachine`].
///
/// Each variant pushes, pops, or transforms `f64` values on the VM's evaluation
/// stack. Comparison and logical ops yield `1.0` for true and `0.0` for false.
/// Relational comparisons (`Eq`, `Ne`, `Gt`, `Lt`, `Ge`, `Le`) are
/// epsilon-aware — see [`crate::vm::DEFAULT_RELATIVE_EPSILON`].
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    /// Pushes a literal `f64` onto the stack.
    Push(f64),
    /// Pushes the parameter at the given index from the context frame.
    LoadParam(u16),
    /// Pushes the predecessor's `age` (current time minus birth time).
    LoadAge,

    /// Pops `b`, pops `a`, pushes `a + b`.
    Add,
    /// Pops `b`, pops `a`, pushes `a - b`.
    Sub,
    /// Pops `b`, pops `a`, pushes `a * b`.
    Mul,
    /// Pops `b`, pops `a`, pushes `a / b`. Division by zero yields a math error.
    Div,
    /// Pops `b`, pops `a`, pushes `a.powf(b)`.
    Pow,
    /// Pops `a`, pushes `-a`.
    Neg,

    /// Epsilon-aware equality.
    Eq,
    /// Epsilon-aware inequality.
    Ne,
    /// Epsilon-aware greater-than (strict).
    Gt,
    /// Epsilon-aware less-than (strict).
    Lt,
    /// Epsilon-aware greater-or-equal.
    Ge,
    /// Epsilon-aware less-or-equal.
    Le,

    /// Logical AND on truthiness (any non-zero is true).
    And,
    /// Logical OR on truthiness (any non-zero is true).
    Or,
    /// Logical NOT on truthiness.
    Not,

    /// Invokes a built-in math function. See [`MathOp`].
    Math(MathOp),
}
