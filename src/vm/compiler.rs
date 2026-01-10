use crate::parser::ast::Expr;
use crate::vm::ops::{MathOp, Op};
use std::collections::HashMap;

pub struct Compiler<'a> {
    param_map: Vec<String>,
    constants: &'a HashMap<String, f64>,
}

impl<'a> Compiler<'a> {
    pub fn new(params: Vec<String>, constants: &'a HashMap<String, f64>) -> Self {
        Self {
            param_map: params,
            constants,
        }
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
                if name == "age" {
                    ops.push(Op::LoadAge);
                } else if let Some(idx) = self.param_map.iter().position(|p| p == name) {
                    ops.push(Op::LoadParam(idx as u16));
                } else if let Some(val) = self.constants.get(name) {
                    // Inline the constant value directly
                    ops.push(Op::Push(*val));
                } else {
                    return Err(format!("Unknown parameter or constant: {}", name));
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
            Expr::Call(name, args) => {
                // Compile arguments first (pushed to stack)
                for arg in args {
                    self.compile_expr(arg, ops)?;
                }

                // Map name to Op
                match name.as_str() {
                    "sin" => {
                        if args.len() != 1 {
                            return Err("sin takes 1 argument".into());
                        }
                        ops.push(Op::Math(MathOp::Sin, 1));
                    }
                    "cos" => {
                        if args.len() != 1 {
                            return Err("cos takes 1 argument".into());
                        }
                        ops.push(Op::Math(MathOp::Cos, 1));
                    }
                    "tan" => {
                        if args.len() != 1 {
                            return Err("tan takes 1 argument".into());
                        }
                        ops.push(Op::Math(MathOp::Tan, 1));
                    }
                    "sqrt" => {
                        if args.len() != 1 {
                            return Err("sqrt takes 1 argument".into());
                        }
                        ops.push(Op::Math(MathOp::Sqrt, 1));
                    }
                    "abs" => {
                        if args.len() != 1 {
                            return Err("abs takes 1 argument".into());
                        }
                        ops.push(Op::Math(MathOp::Abs, 1));
                    }
                    "floor" => {
                        if args.len() != 1 {
                            return Err("floor takes 1 argument".into());
                        }
                        ops.push(Op::Math(MathOp::Floor, 1));
                    }
                    "ceil" => {
                        if args.len() != 1 {
                            return Err("ceil takes 1 argument".into());
                        }
                        ops.push(Op::Math(MathOp::Ceil, 1));
                    }
                    "round" => {
                        if args.len() != 1 {
                            return Err("round takes 1 argument".into());
                        }
                        ops.push(Op::Math(MathOp::Round, 1));
                    }
                    "min" => {
                        if args.len() != 2 {
                            return Err("min takes 2 arguments".into());
                        }
                        ops.push(Op::Math(MathOp::Min, 2));
                    }
                    "max" => {
                        if args.len() != 2 {
                            return Err("max takes 2 arguments".into());
                        }
                        ops.push(Op::Math(MathOp::Max, 2));
                    }
                    _ => return Err(format!("Unknown function: {}", name)),
                }
            }
        }
        Ok(())
    }
}
