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

#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    // Data
    Push(f64),
    LoadParam(u16),
    LoadAge,

    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Neg,

    // Relational
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,

    // Logical
    And,
    Or,
    Not,

    // Functions
    Math(MathOp),
}
