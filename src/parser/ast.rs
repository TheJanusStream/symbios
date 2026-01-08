#[derive(Debug, PartialEq, Clone)]
pub enum Expr {
    Number(f64),
    Variable(String),
    Call(String, Vec<Expr>),
    // Unary
    Not(Box<Expr>),
    Neg(Box<Expr>),
    // Binary
    Pow(Box<Expr>, Box<Expr>),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
    // Relations
    Gt(Box<Expr>, Box<Expr>),
    Lt(Box<Expr>, Box<Expr>),
    Ge(Box<Expr>, Box<Expr>),
    Le(Box<Expr>, Box<Expr>),
    Eq(Box<Expr>, Box<Expr>),
    Ne(Box<Expr>, Box<Expr>),
    // Logical
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
}

#[derive(Debug, PartialEq, Clone)]
pub struct ModuleSym {
    pub symbol: char,
    pub params: Vec<Expr>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct Rule {
    pub label: Option<String>,
    pub probability: f64,
    pub predecessor: ModuleSym,
    pub left_context: Vec<ModuleSym>,
    pub right_context: Vec<ModuleSym>,
    pub condition: Option<Expr>,
    pub successors: Vec<ModuleSym>,
}

#[derive(Debug, PartialEq, Clone)]
pub enum Directive {
    Ignore(Vec<char>),
    Define(String, Expr),
}
