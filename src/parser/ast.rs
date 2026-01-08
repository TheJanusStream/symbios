/// A mathematical expression parsed from the string.
/// This will be compiled to RPN Bytecode in Phase 2.
#[derive(Debug, PartialEq, Clone)]
pub enum Expr {
    Number(f32),
    Variable(String), // e.g., "x"
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
    // Logical operators for guards
    Gt(Box<Expr>, Box<Expr>),
    Lt(Box<Expr>, Box<Expr>),
    Eq(Box<Expr>, Box<Expr>),
}

/// A module in a rule definition (e.g., "A(x, 1)")
#[derive(Debug, PartialEq, Clone)]
pub struct ModuleSym {
    pub symbol: char, // We restrict symbols to single chars for standard L-System notation
    pub params: Vec<Expr>, // Parameters can be expressions "x+1"
}

/// A complete production rule.
/// Format: LC < P(params) > RC : Condition -> Successor
#[derive(Debug, PartialEq, Clone)]
pub struct Rule {
    pub probability: f32,
    pub predecessor: ModuleSym,
    pub left_context: Option<ModuleSym>,
    pub right_context: Option<ModuleSym>,
    pub condition: Option<Expr>,
    pub successors: Vec<ModuleSym>,
}
