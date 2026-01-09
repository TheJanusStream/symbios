#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    // Data
    Push(f64),      // Push literal
    LoadParam(u16), // Push parameter from context (by index)

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
}
