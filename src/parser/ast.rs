#[derive(Debug, PartialEq, Clone)]
pub enum Expr {
    Number(f64),
    Variable(String),
    Call(String, Vec<Expr>),
    Not(Box<Expr>),
    Neg(Box<Expr>),
    Pow(Box<Expr>, Box<Expr>),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
    Gt(Box<Expr>, Box<Expr>),
    Lt(Box<Expr>, Box<Expr>),
    Ge(Box<Expr>, Box<Expr>),
    Le(Box<Expr>, Box<Expr>),
    Eq(Box<Expr>, Box<Expr>),
    Ne(Box<Expr>, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
}

#[derive(Debug, PartialEq, Clone)]
pub struct ModuleSym {
    pub symbol: String,
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
    Ignore(Vec<String>),
    Define(String, Expr),
}

// ============================================================================
// Formatting (AST -> Source Text)
// ============================================================================

use std::fmt;

/// Operator precedence levels (higher = binds tighter)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Precedence {
    Or = 1,
    And = 2,
    Comparison = 3,
    Additive = 4,
    Multiplicative = 5,
    Power = 6,
    Unary = 7,
    Atom = 8,
}

impl Expr {
    fn precedence(&self) -> Precedence {
        match self {
            Expr::Number(_) | Expr::Variable(_) | Expr::Call(_, _) => Precedence::Atom,
            Expr::Not(_) | Expr::Neg(_) => Precedence::Unary,
            Expr::Pow(_, _) => Precedence::Power,
            Expr::Mul(_, _) | Expr::Div(_, _) => Precedence::Multiplicative,
            Expr::Add(_, _) | Expr::Sub(_, _) => Precedence::Additive,
            Expr::Gt(_, _)
            | Expr::Lt(_, _)
            | Expr::Ge(_, _)
            | Expr::Le(_, _)
            | Expr::Eq(_, _)
            | Expr::Ne(_, _) => Precedence::Comparison,
            Expr::And(_, _) => Precedence::And,
            Expr::Or(_, _) => Precedence::Or,
        }
    }

    /// Formats the expression, wrapping in parens if needed for precedence.
    fn fmt_with_precedence(
        &self,
        f: &mut fmt::Formatter<'_>,
        parent_prec: Precedence,
    ) -> fmt::Result {
        let needs_parens = self.precedence() < parent_prec;
        if needs_parens {
            write!(f, "(")?;
        }
        write!(f, "{}", self)?;
        if needs_parens {
            write!(f, ")")?;
        }
        Ok(())
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Number(val) => {
                if val.fract() == 0.0 && val.abs() < 1e15 {
                    write!(f, "{}", *val as i64)
                } else {
                    write!(f, "{}", val)
                }
            }
            Expr::Variable(name) => write!(f, "{}", name),
            Expr::Call(name, args) => {
                write!(f, "{}(", name)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg)?;
                }
                write!(f, ")")
            }
            Expr::Not(inner) => {
                write!(f, "!")?;
                inner.fmt_with_precedence(f, Precedence::Unary)
            }
            Expr::Neg(inner) => {
                write!(f, "-")?;
                inner.fmt_with_precedence(f, Precedence::Unary)
            }
            Expr::Pow(lhs, rhs) => {
                lhs.fmt_with_precedence(f, Precedence::Power)?;
                write!(f, " ^ ")?;
                rhs.fmt_with_precedence(f, Precedence::Power)
            }
            Expr::Add(lhs, rhs) => {
                lhs.fmt_with_precedence(f, Precedence::Additive)?;
                write!(f, " + ")?;
                rhs.fmt_with_precedence(f, Precedence::Additive)
            }
            Expr::Sub(lhs, rhs) => {
                lhs.fmt_with_precedence(f, Precedence::Additive)?;
                write!(f, " - ")?;
                // Right side needs higher precedence to handle a - b - c correctly
                rhs.fmt_with_precedence(f, Precedence::Multiplicative)
            }
            Expr::Mul(lhs, rhs) => {
                lhs.fmt_with_precedence(f, Precedence::Multiplicative)?;
                write!(f, " * ")?;
                rhs.fmt_with_precedence(f, Precedence::Multiplicative)
            }
            Expr::Div(lhs, rhs) => {
                lhs.fmt_with_precedence(f, Precedence::Multiplicative)?;
                write!(f, " / ")?;
                // Right side needs higher precedence to handle a / b / c correctly
                rhs.fmt_with_precedence(f, Precedence::Power)
            }
            Expr::Gt(lhs, rhs) => {
                lhs.fmt_with_precedence(f, Precedence::Comparison)?;
                write!(f, " > ")?;
                rhs.fmt_with_precedence(f, Precedence::Comparison)
            }
            Expr::Lt(lhs, rhs) => {
                lhs.fmt_with_precedence(f, Precedence::Comparison)?;
                write!(f, " < ")?;
                rhs.fmt_with_precedence(f, Precedence::Comparison)
            }
            Expr::Ge(lhs, rhs) => {
                lhs.fmt_with_precedence(f, Precedence::Comparison)?;
                write!(f, " >= ")?;
                rhs.fmt_with_precedence(f, Precedence::Comparison)
            }
            Expr::Le(lhs, rhs) => {
                lhs.fmt_with_precedence(f, Precedence::Comparison)?;
                write!(f, " <= ")?;
                rhs.fmt_with_precedence(f, Precedence::Comparison)
            }
            Expr::Eq(lhs, rhs) => {
                lhs.fmt_with_precedence(f, Precedence::Comparison)?;
                write!(f, " == ")?;
                rhs.fmt_with_precedence(f, Precedence::Comparison)
            }
            Expr::Ne(lhs, rhs) => {
                lhs.fmt_with_precedence(f, Precedence::Comparison)?;
                write!(f, " != ")?;
                rhs.fmt_with_precedence(f, Precedence::Comparison)
            }
            Expr::And(lhs, rhs) => {
                lhs.fmt_with_precedence(f, Precedence::And)?;
                write!(f, " && ")?;
                rhs.fmt_with_precedence(f, Precedence::And)
            }
            Expr::Or(lhs, rhs) => {
                lhs.fmt_with_precedence(f, Precedence::Or)?;
                write!(f, " || ")?;
                rhs.fmt_with_precedence(f, Precedence::Or)
            }
        }
    }
}

impl fmt::Display for ModuleSym {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.symbol)?;
        if !self.params.is_empty() {
            write!(f, "(")?;
            for (i, param) in self.params.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{}", param)?;
            }
            write!(f, ")")?;
        }
        Ok(())
    }
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Label (if present)
        if let Some(label) = &self.label {
            write!(f, "{}: ", label)?;
        }

        // Left context
        if !self.left_context.is_empty() {
            for ctx in &self.left_context {
                write!(f, "{} ", ctx)?;
            }
            write!(f, "< ")?;
        }

        // Predecessor
        write!(f, "{}", self.predecessor)?;

        // Right context
        if !self.right_context.is_empty() {
            write!(f, " > ")?;
            for (i, ctx) in self.right_context.iter().enumerate() {
                if i > 0 {
                    write!(f, " ")?;
                }
                write!(f, "{}", ctx)?;
            }
        }

        // Condition
        if let Some(cond) = &self.condition {
            write!(f, " : {}", cond)?;
        }

        // Arrow and successors
        write!(f, " ->")?;
        for succ in &self.successors {
            write!(f, " {}", succ)?;
        }

        // Probability (only if not 1.0 and not redundant with condition)
        if (self.probability - 1.0).abs() > f64::EPSILON {
            let is_redundant_sugar = if let Some(Expr::Number(n)) = &self.condition {
                (n - self.probability).abs() < f64::EPSILON
            } else {
                false
            };

            if !is_redundant_sugar {
                write!(f, " : {}", self.probability)?;
            }
        }

        Ok(())
    }
}

impl fmt::Display for Directive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Directive::Ignore(symbols) => {
                write!(f, "#ignore ")?;
                for (i, sym) in symbols.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }
                    write!(f, "{}", sym)?;
                }
                Ok(())
            }
            Directive::Define(name, expr) => {
                write!(f, "#define {} {}", name, expr)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_number() {
        assert_eq!(Expr::Number(42.0).to_string(), "42");
        assert_eq!(Expr::Number(3.14).to_string(), "3.14");
        assert_eq!(Expr::Number(-5.0).to_string(), "-5");
    }

    #[test]
    fn test_format_variable() {
        assert_eq!(Expr::Variable("x".into()).to_string(), "x");
        assert_eq!(Expr::Variable("age".into()).to_string(), "age");
    }

    #[test]
    fn test_format_binary_ops() {
        let x = Expr::Variable("x".into());
        let y = Expr::Variable("y".into());

        assert_eq!(
            Expr::Add(Box::new(x.clone()), Box::new(y.clone())).to_string(),
            "x + y"
        );
        assert_eq!(
            Expr::Sub(Box::new(x.clone()), Box::new(y.clone())).to_string(),
            "x - y"
        );
        assert_eq!(
            Expr::Mul(Box::new(x.clone()), Box::new(y.clone())).to_string(),
            "x * y"
        );
        assert_eq!(
            Expr::Div(Box::new(x.clone()), Box::new(y.clone())).to_string(),
            "x / y"
        );
    }

    #[test]
    fn test_format_precedence() {
        // x + y * z should be "x + y * z" (no parens needed)
        let x = Expr::Variable("x".into());
        let y = Expr::Variable("y".into());
        let z = Expr::Variable("z".into());

        let mul = Expr::Mul(Box::new(y.clone()), Box::new(z.clone()));
        let add = Expr::Add(Box::new(x.clone()), Box::new(mul));
        assert_eq!(add.to_string(), "x + y * z");

        // (x + y) * z should be "(x + y) * z"
        let add2 = Expr::Add(Box::new(x.clone()), Box::new(y.clone()));
        let mul2 = Expr::Mul(Box::new(add2), Box::new(z.clone()));
        assert_eq!(mul2.to_string(), "(x + y) * z");
    }

    #[test]
    fn test_format_comparison() {
        let x = Expr::Variable("x".into());
        let ten = Expr::Number(10.0);

        assert_eq!(
            Expr::Gt(Box::new(x.clone()), Box::new(ten.clone())).to_string(),
            "x > 10"
        );
        assert_eq!(
            Expr::Le(Box::new(x.clone()), Box::new(ten.clone())).to_string(),
            "x <= 10"
        );
    }

    #[test]
    fn test_format_logical() {
        let x = Expr::Variable("x".into());
        let y = Expr::Variable("y".into());
        let zero = Expr::Number(0.0);
        let ten = Expr::Number(10.0);

        let cond1 = Expr::Gt(Box::new(x.clone()), Box::new(zero.clone()));
        let cond2 = Expr::Lt(Box::new(y.clone()), Box::new(ten.clone()));
        let and = Expr::And(Box::new(cond1), Box::new(cond2));

        assert_eq!(and.to_string(), "x > 0 && y < 10");
    }

    #[test]
    fn test_format_call() {
        let x = Expr::Variable("x".into());
        let y = Expr::Variable("y".into());

        assert_eq!(
            Expr::Call("sin".into(), vec![x.clone()]).to_string(),
            "sin(x)"
        );
        assert_eq!(
            Expr::Call("max".into(), vec![x.clone(), y.clone()]).to_string(),
            "max(x, y)"
        );
    }

    #[test]
    fn test_format_unary() {
        let x = Expr::Variable("x".into());

        assert_eq!(Expr::Neg(Box::new(x.clone())).to_string(), "-x");
        assert_eq!(Expr::Not(Box::new(x.clone())).to_string(), "!x");
    }

    #[test]
    fn test_format_module_sym() {
        let m1 = ModuleSym {
            symbol: "A".into(),
            params: vec![],
        };
        assert_eq!(m1.to_string(), "A");

        let m2 = ModuleSym {
            symbol: "B".into(),
            params: vec![Expr::Variable("x".into()), Expr::Number(10.0)],
        };
        assert_eq!(m2.to_string(), "B(x, 10)");
    }

    #[test]
    fn test_format_rule_simple() {
        let rule = Rule {
            label: None,
            probability: 1.0,
            predecessor: ModuleSym {
                symbol: "A".into(),
                params: vec![],
            },
            left_context: vec![],
            right_context: vec![],
            condition: None,
            successors: vec![
                ModuleSym {
                    symbol: "A".into(),
                    params: vec![],
                },
                ModuleSym {
                    symbol: "B".into(),
                    params: vec![],
                },
            ],
        };
        assert_eq!(rule.to_string(), "A -> A B");
    }

    #[test]
    fn test_format_rule_with_params() {
        let rule = Rule {
            label: Some("p1".into()),
            probability: 1.0,
            predecessor: ModuleSym {
                symbol: "A".into(),
                params: vec![Expr::Variable("x".into())],
            },
            left_context: vec![],
            right_context: vec![],
            condition: Some(Expr::Gt(
                Box::new(Expr::Variable("x".into())),
                Box::new(Expr::Number(10.0)),
            )),
            successors: vec![ModuleSym {
                symbol: "B".into(),
                params: vec![Expr::Variable("x".into())],
            }],
        };
        assert_eq!(rule.to_string(), "p1: A(x) : x > 10 -> B(x)");
    }

    #[test]
    fn test_format_rule_with_context() {
        let rule = Rule {
            label: None,
            probability: 1.0,
            predecessor: ModuleSym {
                symbol: "B".into(),
                params: vec![],
            },
            left_context: vec![ModuleSym {
                symbol: "A".into(),
                params: vec![],
            }],
            right_context: vec![ModuleSym {
                symbol: "C".into(),
                params: vec![],
            }],
            condition: None,
            successors: vec![ModuleSym {
                symbol: "D".into(),
                params: vec![],
            }],
        };
        assert_eq!(rule.to_string(), "A < B > C -> D");
    }

    #[test]
    fn test_format_rule_stochastic() {
        let rule = Rule {
            label: None,
            probability: 0.5,
            predecessor: ModuleSym {
                symbol: "A".into(),
                params: vec![],
            },
            left_context: vec![],
            right_context: vec![],
            condition: None,
            successors: vec![ModuleSym {
                symbol: "B".into(),
                params: vec![],
            }],
        };
        assert_eq!(rule.to_string(), "A -> B : 0.5");
    }

    #[test]
    fn test_format_directive() {
        let ignore = Directive::Ignore(vec!["F".into(), "f".into()]);
        assert_eq!(ignore.to_string(), "#ignore F f");

        let define = Directive::Define("PI".into(), Expr::Number(3.14159));
        assert_eq!(define.to_string(), "#define PI 3.14159");
    }
}
