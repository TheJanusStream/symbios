/// An expression node in a rule's parameter or condition.
///
/// Constructed by the parser and consumed by [`crate::vm::Compiler`] to emit
/// bytecode. The [`std::fmt::Display`] impl emits source text with proper
/// parenthesisation for round-tripping.
#[derive(Debug, PartialEq, Clone)]
pub enum Expr {
    /// A numeric literal.
    Number(f64),
    /// A bound name — a rule parameter, the reserved `age`, or a `#define`d
    /// constant. Resolution happens at compile time.
    Variable(String),
    /// A built-in math function call. The function name must match one of
    /// the [`crate::vm::MathOp`] entries (case-sensitive).
    Call(String, Vec<Expr>),
    /// Logical negation (`!x`).
    Not(Box<Expr>),
    /// Arithmetic negation (`-x`).
    Neg(Box<Expr>),
    /// Power (`a ^ b`, right-associative).
    Pow(Box<Expr>, Box<Expr>),
    /// Addition.
    Add(Box<Expr>, Box<Expr>),
    /// Subtraction.
    Sub(Box<Expr>, Box<Expr>),
    /// Multiplication.
    Mul(Box<Expr>, Box<Expr>),
    /// Division. Division by zero produces a VM math error at eval time.
    Div(Box<Expr>, Box<Expr>),
    /// Greater-than. Epsilon-aware at eval time.
    Gt(Box<Expr>, Box<Expr>),
    /// Less-than. Epsilon-aware at eval time.
    Lt(Box<Expr>, Box<Expr>),
    /// Greater-or-equal. Epsilon-aware at eval time.
    Ge(Box<Expr>, Box<Expr>),
    /// Less-or-equal. Epsilon-aware at eval time.
    Le(Box<Expr>, Box<Expr>),
    /// Equality. Epsilon-aware at eval time.
    Eq(Box<Expr>, Box<Expr>),
    /// Inequality. Epsilon-aware at eval time.
    Ne(Box<Expr>, Box<Expr>),
    /// Logical AND (any non-zero is true).
    And(Box<Expr>, Box<Expr>),
    /// Logical OR (any non-zero is true).
    Or(Box<Expr>, Box<Expr>),
}

/// Iterative Drop to prevent stack overflow on deeply nested ASTs
/// (e.g., left-associative chains like `1+1+1+...+1` with 10,000+ terms).
impl Drop for Expr {
    fn drop(&mut self) {
        let mut stack = Vec::new();
        self.take_children(&mut stack);
        while let Some(mut expr) = stack.pop() {
            expr.take_children(&mut stack);
        }
    }
}

impl Expr {
    /// Extracts owned children, replacing them with inert `Number(0.0)` leaves.
    /// The caller is responsible for dropping the returned children.
    fn take_children(&mut self, stack: &mut Vec<Expr>) {
        let dummy = || Box::new(Expr::Number(0.0));
        match self {
            Expr::Not(inner) | Expr::Neg(inner) => {
                stack.push(*std::mem::replace(inner, dummy()));
            }
            Expr::Pow(l, r)
            | Expr::Add(l, r)
            | Expr::Sub(l, r)
            | Expr::Mul(l, r)
            | Expr::Div(l, r)
            | Expr::Gt(l, r)
            | Expr::Lt(l, r)
            | Expr::Ge(l, r)
            | Expr::Le(l, r)
            | Expr::Eq(l, r)
            | Expr::Ne(l, r)
            | Expr::And(l, r)
            | Expr::Or(l, r) => {
                stack.push(*std::mem::replace(l, dummy()));
                stack.push(*std::mem::replace(r, dummy()));
            }
            Expr::Call(_, args) => {
                for arg in args.drain(..) {
                    stack.push(arg);
                }
            }
            Expr::Number(_) | Expr::Variable(_) => {}
        }
    }
}

/// A parsed module reference (axiom element, predecessor, context entry,
/// or successor): a symbol name plus its parameter expressions.
#[derive(Debug, PartialEq, Clone)]
pub struct ModuleSym {
    /// The module's symbol name (e.g. `"A"`, `"+"`, `"F"`).
    pub symbol: String,
    /// Parameter expressions in source order. Empty for parameter-less modules.
    pub params: Vec<Expr>,
}

/// A parsed production rule.
///
/// `Rule` carries everything the parser captured for one rule. Compile it via
/// [`crate::System::add_rule`] (or the lower-level [`crate::vm::Compiler`]) to
/// produce a [`crate::system::RuntimeRule`].
#[derive(Debug, PartialEq, Clone)]
pub struct Rule {
    /// Optional labeled identifier (`label:` prefix). Not used at runtime —
    /// preserved for tooling.
    pub label: Option<String>,
    /// Stochastic weight. Defaults to `1.0` when neither a probability
    /// prefix (`0.5 : A -> B`) nor a numeric condition sugar (`A : 0.5 -> B`)
    /// is present.
    pub probability: f64,
    /// The module being replaced.
    pub predecessor: ModuleSym,
    /// Left context modules in left-to-right order (rightmost is adjacent
    /// to the predecessor).
    pub left_context: Vec<ModuleSym>,
    /// Right context modules in left-to-right order (leftmost is adjacent
    /// to the predecessor).
    pub right_context: Vec<ModuleSym>,
    /// Optional guard expression. The rule fires only when this evaluates
    /// to a non-zero value. A bare numeric condition is interpreted as a
    /// probability when no explicit probability prefix was given.
    pub condition: Option<Expr>,
    /// The replacement sequence.
    pub successors: Vec<ModuleSym>,
    /// Per-rule ignore-list override (issue #95). `None` means the rule
    /// inherits the system-global `#ignore` list at match time. `Some(list)`
    /// fully replaces the global list for this rule, including the empty-list
    /// case (`{ ignore: }`) which suppresses all ignoring.
    pub ignored_symbols: Option<Vec<String>>,
}

/// A parsed top-level directive (`#define` or `#ignore`).
#[derive(Debug, PartialEq, Clone)]
pub enum Directive {
    /// `#ignore: <syms>` — symbols to skip during context matching.
    Ignore(Vec<String>),
    /// `#define <name> <expr>` — a named constant. The expression is
    /// evaluated at directive time against constants already in scope.
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
        // Determine if probability needs explicit output.
        // Condition-as-probability sugar (e.g., `A : 0.5 -> B`) means
        // the numeric condition already encodes the probability.
        let is_sugar = if let Some(Expr::Number(n)) = &self.condition {
            (n - self.probability).abs() < f64::EPSILON
        } else {
            false
        };
        let needs_probability = (self.probability - 1.0).abs() > f64::EPSILON && !is_sugar;

        // Probability prefix (written before label/predecessor so the parser
        // can consume it with the `float :` prefix grammar).
        if needs_probability {
            write!(f, "{} : ", self.probability)?;
        }

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

        // Per-rule ignore postfix (issue #95)
        if let Some(syms) = &self.ignored_symbols {
            write!(f, " {{ ignore:")?;
            for sym in syms {
                write!(f, " {}", sym)?;
            }
            write!(f, " }}")?;
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
            ignored_symbols: None,
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
            ignored_symbols: None,
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
            ignored_symbols: None,
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
            ignored_symbols: None,
        };
        assert_eq!(rule.to_string(), "0.5 : A -> B");
    }

    #[test]
    fn test_format_directive() {
        let ignore = Directive::Ignore(vec!["F".into(), "f".into()]);
        assert_eq!(ignore.to_string(), "#ignore F f");

        let define = Directive::Define("PI".into(), Expr::Number(3.14159));
        assert_eq!(define.to_string(), "#define PI 3.14159");
    }
}
