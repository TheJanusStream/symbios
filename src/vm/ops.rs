/* src/vm/ops.rs */
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
    // Randomness could be added here later if we want it inside expressions
    // but usually L-systems handle stochasticity at the rule selection level.
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
    // (Op, Arity) - Arity stored for fast stack check
    Math(MathOp, u8), 
}