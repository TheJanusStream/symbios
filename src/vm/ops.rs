#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    // Data
    Push(f64),
    /// Loads a parameter from the combined context buffer.
    /// Index 0..N = Predecessor parameters.
    /// Index N..M = Left Context parameters.
    /// Index M..K = Right Context parameters.
    LoadParam(u16),
    /// Loads the age (current_time - birth_time) of the Predecessor.
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
}
