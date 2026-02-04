use crate::parser::ast::Expr;
use crate::vm::ops::{MathOp, Op};

/// Decompiles bytecode back into an AST expression.
///
/// The decompiler simulates stack-based execution, but instead of computing
/// f64 values, it builds expression trees. This allows round-tripping from
/// source to bytecode and back.
pub struct Decompiler<'a> {
    param_names: &'a [String],
}

impl<'a> Decompiler<'a> {
    pub fn new(param_names: &'a [String]) -> Self {
        Self { param_names }
    }

    /// Decompiles a bytecode sequence into an expression AST.
    ///
    /// Returns an error if the bytecode is malformed (stack underflow, etc).
    pub fn decompile(&self, ops: &[Op]) -> Result<Expr, String> {
        if ops.is_empty() {
            return Err("Empty bytecode".into());
        }

        let mut stack: Vec<Expr> = Vec::new();

        for op in ops {
            match op {
                Op::Push(val) => {
                    stack.push(Expr::Number(*val));
                }
                Op::LoadParam(idx) => {
                    let name = self
                        .param_names
                        .get(*idx as usize)
                        .cloned()
                        .unwrap_or_else(|| format!("p{}", idx));
                    stack.push(Expr::Variable(name));
                }
                Op::LoadAge => {
                    stack.push(Expr::Variable("age".into()));
                }
                Op::Add => {
                    let (lhs, rhs) = self.pop_binary(&mut stack)?;
                    stack.push(Expr::Add(Box::new(lhs), Box::new(rhs)));
                }
                Op::Sub => {
                    let (lhs, rhs) = self.pop_binary(&mut stack)?;
                    stack.push(Expr::Sub(Box::new(lhs), Box::new(rhs)));
                }
                Op::Mul => {
                    let (lhs, rhs) = self.pop_binary(&mut stack)?;
                    stack.push(Expr::Mul(Box::new(lhs), Box::new(rhs)));
                }
                Op::Div => {
                    let (lhs, rhs) = self.pop_binary(&mut stack)?;
                    stack.push(Expr::Div(Box::new(lhs), Box::new(rhs)));
                }
                Op::Pow => {
                    let (lhs, rhs) = self.pop_binary(&mut stack)?;
                    stack.push(Expr::Pow(Box::new(lhs), Box::new(rhs)));
                }
                Op::Neg => {
                    let val = self.pop_unary(&mut stack)?;
                    stack.push(Expr::Neg(Box::new(val)));
                }
                Op::Eq => {
                    let (lhs, rhs) = self.pop_binary(&mut stack)?;
                    stack.push(Expr::Eq(Box::new(lhs), Box::new(rhs)));
                }
                Op::Ne => {
                    let (lhs, rhs) = self.pop_binary(&mut stack)?;
                    stack.push(Expr::Ne(Box::new(lhs), Box::new(rhs)));
                }
                Op::Gt => {
                    let (lhs, rhs) = self.pop_binary(&mut stack)?;
                    stack.push(Expr::Gt(Box::new(lhs), Box::new(rhs)));
                }
                Op::Lt => {
                    let (lhs, rhs) = self.pop_binary(&mut stack)?;
                    stack.push(Expr::Lt(Box::new(lhs), Box::new(rhs)));
                }
                Op::Ge => {
                    let (lhs, rhs) = self.pop_binary(&mut stack)?;
                    stack.push(Expr::Ge(Box::new(lhs), Box::new(rhs)));
                }
                Op::Le => {
                    let (lhs, rhs) = self.pop_binary(&mut stack)?;
                    stack.push(Expr::Le(Box::new(lhs), Box::new(rhs)));
                }
                Op::And => {
                    let (lhs, rhs) = self.pop_binary(&mut stack)?;
                    stack.push(Expr::And(Box::new(lhs), Box::new(rhs)));
                }
                Op::Or => {
                    let (lhs, rhs) = self.pop_binary(&mut stack)?;
                    stack.push(Expr::Or(Box::new(lhs), Box::new(rhs)));
                }
                Op::Not => {
                    let val = self.pop_unary(&mut stack)?;
                    stack.push(Expr::Not(Box::new(val)));
                }
                Op::Math(math_op) => {
                    let name = match math_op {
                        MathOp::Sin => "sin",
                        MathOp::Cos => "cos",
                        MathOp::Tan => "tan",
                        MathOp::Sqrt => "sqrt",
                        MathOp::Abs => "abs",
                        MathOp::Floor => "floor",
                        MathOp::Ceil => "ceil",
                        MathOp::Round => "round",
                        MathOp::Min => "min",
                        MathOp::Max => "max",
                    };

                    let arity = math_op.arity();
                    let mut args = Vec::with_capacity(arity as usize);
                    for _ in 0..arity {
                        args.push(
                            stack
                                .pop()
                                .ok_or_else(|| format!("Stack underflow in {}", name))?,
                        );
                    }
                    args.reverse(); // Args were pushed in order, popped in reverse
                    stack.push(Expr::Call(name.into(), args));
                }
            }
        }

        if stack.len() != 1 {
            return Err(format!(
                "Invalid bytecode: expected 1 value on stack, got {}",
                stack.len()
            ));
        }

        Ok(stack.pop().unwrap())
    }

    fn pop_binary(&self, stack: &mut Vec<Expr>) -> Result<(Expr, Expr), String> {
        let rhs = stack.pop().ok_or("Stack underflow (binary rhs)")?;
        let lhs = stack.pop().ok_or("Stack underflow (binary lhs)")?;
        Ok((lhs, rhs))
    }

    fn pop_unary(&self, stack: &mut Vec<Expr>) -> Result<Expr, String> {
        stack.pop().ok_or_else(|| "Stack underflow (unary)".into())
    }
}

/// Convenience function to decompile bytecode without parameter names.
/// Uses synthetic names like p0, p1, etc.
pub fn decompile(ops: &[Op]) -> Result<Expr, String> {
    Decompiler::new(&[]).decompile(ops)
}

/// Convenience function to decompile bytecode with parameter names.
pub fn decompile_with_params(ops: &[Op], param_names: &[String]) -> Result<Expr, String> {
    Decompiler::new(param_names).decompile(ops)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decompile_number() {
        let ops = vec![Op::Push(42.0)];
        let expr = decompile(&ops).unwrap();
        assert_eq!(expr, Expr::Number(42.0));
    }

    #[test]
    fn test_decompile_param() {
        let ops = vec![Op::LoadParam(0)];
        let params = vec!["x".into()];
        let expr = decompile_with_params(&ops, &params).unwrap();
        assert_eq!(expr, Expr::Variable("x".into()));
    }

    #[test]
    fn test_decompile_age() {
        let ops = vec![Op::LoadAge];
        let expr = decompile(&ops).unwrap();
        assert_eq!(expr, Expr::Variable("age".into()));
    }

    #[test]
    fn test_decompile_binary_add() {
        // x + 10
        let ops = vec![Op::LoadParam(0), Op::Push(10.0), Op::Add];
        let params = vec!["x".into()];
        let expr = decompile_with_params(&ops, &params).unwrap();
        assert_eq!(
            expr,
            Expr::Add(
                Box::new(Expr::Variable("x".into())),
                Box::new(Expr::Number(10.0))
            )
        );
    }

    #[test]
    fn test_decompile_nested() {
        // (x + y) * 2
        let ops = vec![
            Op::LoadParam(0),
            Op::LoadParam(1),
            Op::Add,
            Op::Push(2.0),
            Op::Mul,
        ];
        let params = vec!["x".into(), "y".into()];
        let expr = decompile_with_params(&ops, &params).unwrap();
        assert_eq!(
            expr,
            Expr::Mul(
                Box::new(Expr::Add(
                    Box::new(Expr::Variable("x".into())),
                    Box::new(Expr::Variable("y".into()))
                )),
                Box::new(Expr::Number(2.0))
            )
        );
    }

    #[test]
    fn test_decompile_comparison() {
        // x > 10
        let ops = vec![Op::LoadParam(0), Op::Push(10.0), Op::Gt];
        let params = vec!["x".into()];
        let expr = decompile_with_params(&ops, &params).unwrap();
        assert_eq!(
            expr,
            Expr::Gt(
                Box::new(Expr::Variable("x".into())),
                Box::new(Expr::Number(10.0))
            )
        );
    }

    #[test]
    fn test_decompile_logical() {
        // x > 0 && y < 10
        let ops = vec![
            Op::LoadParam(0),
            Op::Push(0.0),
            Op::Gt,
            Op::LoadParam(1),
            Op::Push(10.0),
            Op::Lt,
            Op::And,
        ];
        let params = vec!["x".into(), "y".into()];
        let expr = decompile_with_params(&ops, &params).unwrap();
        assert_eq!(
            expr,
            Expr::And(
                Box::new(Expr::Gt(
                    Box::new(Expr::Variable("x".into())),
                    Box::new(Expr::Number(0.0))
                )),
                Box::new(Expr::Lt(
                    Box::new(Expr::Variable("y".into())),
                    Box::new(Expr::Number(10.0))
                ))
            )
        );
    }

    #[test]
    fn test_decompile_math_unary() {
        // sin(x)
        let ops = vec![Op::LoadParam(0), Op::Math(MathOp::Sin)];
        let params = vec!["x".into()];
        let expr = decompile_with_params(&ops, &params).unwrap();
        assert_eq!(
            expr,
            Expr::Call("sin".into(), vec![Expr::Variable("x".into())])
        );
    }

    #[test]
    fn test_decompile_math_binary() {
        // max(x, y)
        let ops = vec![Op::LoadParam(0), Op::LoadParam(1), Op::Math(MathOp::Max)];
        let params = vec!["x".into(), "y".into()];
        let expr = decompile_with_params(&ops, &params).unwrap();
        assert_eq!(
            expr,
            Expr::Call(
                "max".into(),
                vec![Expr::Variable("x".into()), Expr::Variable("y".into())]
            )
        );
    }

    #[test]
    fn test_decompile_negation() {
        // -x
        let ops = vec![Op::LoadParam(0), Op::Neg];
        let params = vec!["x".into()];
        let expr = decompile_with_params(&ops, &params).unwrap();
        assert_eq!(expr, Expr::Neg(Box::new(Expr::Variable("x".into()))));
    }

    #[test]
    fn test_decompile_empty() {
        let ops = vec![];
        assert!(decompile(&ops).is_err());
    }

    #[test]
    fn test_decompile_stack_underflow() {
        let ops = vec![Op::Add];
        assert!(decompile(&ops).is_err());
    }
}
