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
        // Iterative postorder walk via an explicit work stack, so deep
        // left-nested chains (e.g., `1+1+...+1` parsed flat) cannot exhaust
        // the call stack. Each node either emits its leaf op directly or
        // queues an Emit task to fire after its children are visited.
        enum Task<'a> {
            Visit(&'a Expr),
            Emit(Op),
        }

        let mut ops = Vec::new();
        let mut stack: Vec<Task> = vec![Task::Visit(expr)];

        while let Some(task) = stack.pop() {
            match task {
                Task::Emit(op) => ops.push(op),
                Task::Visit(expr) => match expr {
                    Expr::Number(val) => ops.push(Op::Push(*val)),
                    Expr::Variable(name) => {
                        if name == "age" {
                            ops.push(Op::LoadAge);
                        } else if let Some(idx) = self.param_map.iter().position(|p| p == name) {
                            ops.push(Op::LoadParam(idx as u16));
                        } else if let Some(val) = self.constants.get(name) {
                            ops.push(Op::Push(*val));
                        } else {
                            return Err(format!("Unknown parameter or constant: {}", name));
                        }
                    }
                    Expr::Not(inner) => {
                        stack.push(Task::Emit(Op::Not));
                        stack.push(Task::Visit(inner));
                    }
                    Expr::Neg(inner) => {
                        stack.push(Task::Emit(Op::Neg));
                        stack.push(Task::Visit(inner));
                    }
                    // Binary operators: emit op last (postorder); push right
                    // before left so left is popped (visited) first.
                    Expr::Add(l, r) => {
                        stack.push(Task::Emit(Op::Add));
                        stack.push(Task::Visit(r));
                        stack.push(Task::Visit(l));
                    }
                    Expr::Sub(l, r) => {
                        stack.push(Task::Emit(Op::Sub));
                        stack.push(Task::Visit(r));
                        stack.push(Task::Visit(l));
                    }
                    Expr::Mul(l, r) => {
                        stack.push(Task::Emit(Op::Mul));
                        stack.push(Task::Visit(r));
                        stack.push(Task::Visit(l));
                    }
                    Expr::Div(l, r) => {
                        stack.push(Task::Emit(Op::Div));
                        stack.push(Task::Visit(r));
                        stack.push(Task::Visit(l));
                    }
                    Expr::Pow(l, r) => {
                        stack.push(Task::Emit(Op::Pow));
                        stack.push(Task::Visit(r));
                        stack.push(Task::Visit(l));
                    }
                    Expr::Eq(l, r) => {
                        stack.push(Task::Emit(Op::Eq));
                        stack.push(Task::Visit(r));
                        stack.push(Task::Visit(l));
                    }
                    Expr::Ne(l, r) => {
                        stack.push(Task::Emit(Op::Ne));
                        stack.push(Task::Visit(r));
                        stack.push(Task::Visit(l));
                    }
                    Expr::Gt(l, r) => {
                        stack.push(Task::Emit(Op::Gt));
                        stack.push(Task::Visit(r));
                        stack.push(Task::Visit(l));
                    }
                    Expr::Lt(l, r) => {
                        stack.push(Task::Emit(Op::Lt));
                        stack.push(Task::Visit(r));
                        stack.push(Task::Visit(l));
                    }
                    Expr::Ge(l, r) => {
                        stack.push(Task::Emit(Op::Ge));
                        stack.push(Task::Visit(r));
                        stack.push(Task::Visit(l));
                    }
                    Expr::Le(l, r) => {
                        stack.push(Task::Emit(Op::Le));
                        stack.push(Task::Visit(r));
                        stack.push(Task::Visit(l));
                    }
                    Expr::And(l, r) => {
                        stack.push(Task::Emit(Op::And));
                        stack.push(Task::Visit(r));
                        stack.push(Task::Visit(l));
                    }
                    Expr::Or(l, r) => {
                        stack.push(Task::Emit(Op::Or));
                        stack.push(Task::Visit(r));
                        stack.push(Task::Visit(l));
                    }
                    Expr::Call(name, args) => {
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
                        stack.push(Task::Emit(Op::Math(math_op)));
                        for arg in args.iter().rev() {
                            stack.push(Task::Visit(arg));
                        }
                    }
                },
            }
        }

        Ok(ops)
    }
}
