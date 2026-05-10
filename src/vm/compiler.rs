use crate::parser::ast::Expr;
use crate::vm::ops::{MathOp, Op};
use std::collections::HashMap;

/// Compiles a parser [`Expr`] into a flat bytecode program for the
/// [`crate::vm::VirtualMachine`].
///
/// Name resolution is layered: the reserved name `age` always emits
/// [`Op::LoadAge`]; otherwise the compiler first checks the parameter map
/// (emitting [`Op::LoadParam`]) and then the `#define` constant table
/// (inlining the value as [`Op::Push`]). Unknown names are an error.
pub struct Compiler<'a> {
    param_map: Vec<String>,
    constants: &'a HashMap<String, f64>,
}

impl<'a> Compiler<'a> {
    /// Creates a compiler bound to the given parameter list and constant table.
    ///
    /// `params` should list parameter names in the order they will appear in
    /// the runtime context frame: predecessor params first, then left context,
    /// then right context.
    pub fn new(params: Vec<String>, constants: &'a HashMap<String, f64>) -> Self {
        Self {
            param_map: params,
            constants,
        }
    }

    /// Compiles a single expression to bytecode.
    ///
    /// Returns `Err` on an unknown identifier, an unknown function name, or
    /// a function call with the wrong arity.
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
                let math_op = match name.as_str() {
                    "sin" => MathOp::Sin,
                    "cos" => MathOp::Cos,
                    "tan" => MathOp::Tan,
                    "sqrt" => MathOp::Sqrt,
                    "abs" => MathOp::Abs,
                    "floor" => MathOp::Floor,
                    "ceil" => MathOp::Ceil,
                    "round" => MathOp::Round,
                    "min" => MathOp::Min,
                    "max" => MathOp::Max,
                    _ => return Err(format!("Unknown function: {}", name)),
                };
                let expected_arity = math_op.arity() as usize;
                if args.len() != expected_arity {
                    return Err(format!(
                        "{} takes {} argument{}",
                        name,
                        expected_arity,
                        if expected_arity == 1 { "" } else { "s" }
                    ));
                }
                ops.push(Op::Math(math_op));
            }
        }
        Ok(())
    }
}
