use crate::parser::ast::Expr;
use crate::vm::ops::Op;

pub struct Compiler {
    // We map variable names to parameter indices.
    // e.g. "x" -> 0, "y" -> 1
    param_map: Vec<String>,
}

impl Compiler {
    pub fn new(params: Vec<String>) -> Self {
        Self { param_map: params }
    }

    pub fn compile(&mut self, expr: &Expr) -> Result<Vec<Op>, String> {
        let mut ops = Vec::new();
        self.compile_expr(expr, &mut ops)?;
        Ok(ops)
    }

    fn compile_expr(&mut self, expr: &Expr, ops: &mut Vec<Op>) -> Result<(), String> {
        match expr {
            Expr::Number(val) => ops.push(Op::Push(*val)),
            Expr::Variable(name) => {
                if let Some(idx) = self.param_map.iter().position(|p| p == name) {
                    ops.push(Op::LoadParam(idx as u16));
                } else {
                    return Err(format!("Unknown parameter: {}", name));
                }
            }
            Expr::Add(lhs, rhs) => {
                self.compile_expr(lhs, ops)?;
                self.compile_expr(rhs, ops)?;
                ops.push(Op::Add);
            }
            Expr::Sub(lhs, rhs) => {
                self.compile_expr(lhs, ops)?;
                self.compile_expr(rhs, ops)?;
                ops.push(Op::Sub);
            }
            Expr::Mul(lhs, rhs) => {
                self.compile_expr(lhs, ops)?;
                self.compile_expr(rhs, ops)?;
                ops.push(Op::Mul);
            }
            Expr::Div(lhs, rhs) => {
                self.compile_expr(lhs, ops)?;
                self.compile_expr(rhs, ops)?;
                ops.push(Op::Div);
            }
            Expr::Pow(lhs, rhs) => {
                self.compile_expr(lhs, ops)?;
                self.compile_expr(rhs, ops)?;
                ops.push(Op::Pow);
            }
            Expr::Neg(val) => {
                self.compile_expr(val, ops)?;
                ops.push(Op::Neg);
            }

            Expr::Eq(lhs, rhs) => {
                self.compile_expr(lhs, ops)?;
                self.compile_expr(rhs, ops)?;
                ops.push(Op::Eq);
            }
            Expr::Ne(lhs, rhs) => {
                self.compile_expr(lhs, ops)?;
                self.compile_expr(rhs, ops)?;
                ops.push(Op::Ne);
            }
            Expr::Gt(lhs, rhs) => {
                self.compile_expr(lhs, ops)?;
                self.compile_expr(rhs, ops)?;
                ops.push(Op::Gt);
            }
            Expr::Lt(lhs, rhs) => {
                self.compile_expr(lhs, ops)?;
                self.compile_expr(rhs, ops)?;
                ops.push(Op::Lt);
            }
            Expr::Ge(lhs, rhs) => {
                self.compile_expr(lhs, ops)?;
                self.compile_expr(rhs, ops)?;
                ops.push(Op::Ge);
            }
            Expr::Le(lhs, rhs) => {
                self.compile_expr(lhs, ops)?;
                self.compile_expr(rhs, ops)?;
                ops.push(Op::Le);
            }

            Expr::And(lhs, rhs) => {
                self.compile_expr(lhs, ops)?;
                self.compile_expr(rhs, ops)?;
                ops.push(Op::And);
            }
            Expr::Or(lhs, rhs) => {
                self.compile_expr(lhs, ops)?;
                self.compile_expr(rhs, ops)?;
                ops.push(Op::Or);
            }
            Expr::Not(val) => {
                self.compile_expr(val, ops)?;
                ops.push(Op::Not);
            }

            Expr::Call(_, _) => return Err("Function calls not yet supported in VM".to_string()),
        }
        Ok(())
    }
}
