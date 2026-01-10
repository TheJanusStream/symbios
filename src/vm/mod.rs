/* src/vm/mod.rs */
pub mod compiler;
pub mod ops;

pub use compiler::Compiler;
pub use ops::{MathOp, Op};
use std::fmt;

/// Robust floating point equality check.
#[inline]
pub fn float_eq(a: f64, b: f64) -> bool {
    if a == b {
        return true;
    }
    let abs_a = a.abs();
    let abs_b = b.abs();
    let diff = (a - b).abs();
    if a == 0.0 || b == 0.0 || (abs_a + abs_b < f64::MIN_POSITIVE) {
        return diff < (f64::EPSILON * 100.0);
    }
    diff / (abs_a + abs_b).min(f64::MAX) < 1e-8
}

#[derive(Debug, PartialEq)]
pub enum VMError {
    StackUnderflow,
    StackOverflow,
    MathError,
    ParamOutOfBounds,
    EmptyStack,
    RuntimeError(String),
}

impl fmt::Display for VMError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VMError::StackUnderflow => write!(f, "Stack underflow"),
            VMError::StackOverflow => write!(f, "Stack overflow"),
            VMError::MathError => write!(f, "Mathematical error (NaN/Inf)"),
            VMError::ParamOutOfBounds => write!(f, "Parameter index out of bounds"),
            VMError::EmptyStack => write!(f, "Stack empty at result time"),
            VMError::RuntimeError(msg) => write!(f, "Runtime error: {}", msg),
        }
    }
}

impl std::error::Error for VMError {}

#[derive(Debug, Default)]
pub struct VirtualMachine {
    stack: Vec<f64>,
}

impl VirtualMachine {
    const MAX_STACK_SIZE: usize = 256;

    pub fn new() -> Self {
        Self {
            stack: Vec::with_capacity(64),
        }
    }

    pub fn eval(
        &mut self,
        code: &[Op],
        params: &[f64],
        predecessor_age: f64,
    ) -> Result<f64, String> {
        self.stack.clear();

        for op in code {
            if self.stack.len() > Self::MAX_STACK_SIZE {
                return Err(VMError::StackOverflow.to_string());
            }

            match op {
                Op::Push(val) => self.stack.push(*val),
                Op::LoadParam(idx) => {
                    let val = *params
                        .get(*idx as usize)
                        .ok_or(VMError::ParamOutOfBounds.to_string())?;
                    self.stack.push(val);
                }
                Op::LoadAge => self.stack.push(predecessor_age),
                Op::Add => self.binary_op(|a, b| a + b).map_err(|e| e.to_string())?,
                Op::Sub => self.binary_op(|a, b| a - b).map_err(|e| e.to_string())?,
                Op::Mul => self.binary_op(|a, b| a * b).map_err(|e| e.to_string())?,
                Op::Div => self
                    .binary_op(|a, b| if b == 0.0 { f64::NAN } else { a / b })
                    .map_err(|e| e.to_string())?,
                Op::Pow => self
                    .binary_op(|a, b| a.powf(b))
                    .map_err(|e| e.to_string())?,
                Op::Neg => {
                    let a = self.pop().map_err(|e| e.to_string())?;
                    self.stack.push(-a);
                }
                Op::Eq => self
                    .compare_op(|a, b| float_eq(a, b))
                    .map_err(|e| e.to_string())?,
                Op::Ne => self
                    .compare_op(|a, b| !float_eq(a, b))
                    .map_err(|e| e.to_string())?,
                Op::Gt => self.compare_op(|a, b| a > b).map_err(|e| e.to_string())?,
                Op::Lt => self.compare_op(|a, b| a < b).map_err(|e| e.to_string())?,
                Op::Ge => self.compare_op(|a, b| a >= b).map_err(|e| e.to_string())?,
                Op::Le => self.compare_op(|a, b| a <= b).map_err(|e| e.to_string())?,
                Op::And => self
                    .binary_op(|a, b| if a != 0.0 && b != 0.0 { 1.0 } else { 0.0 })
                    .map_err(|e| e.to_string())?,
                Op::Or => self
                    .binary_op(|a, b| if a != 0.0 || b != 0.0 { 1.0 } else { 0.0 })
                    .map_err(|e| e.to_string())?,
                Op::Not => {
                    let a = self.pop().map_err(|e| e.to_string())?;
                    self.stack.push(if a == 0.0 { 1.0 } else { 0.0 });
                }
                Op::Math(op, arity) => {
                    // Pre-check stack depth
                    if self.stack.len() < *arity as usize {
                        return Err(VMError::StackUnderflow.to_string());
                    }

                    match op {
                        MathOp::Sin => self.unary_op(|a| a.sin()).map_err(|e| e.to_string())?,
                        MathOp::Cos => self.unary_op(|a| a.cos()).map_err(|e| e.to_string())?,
                        MathOp::Tan => self.unary_op(|a| a.tan()).map_err(|e| e.to_string())?,
                        MathOp::Sqrt => self
                            .unary_op(|a| if a < 0.0 { f64::NAN } else { a.sqrt() })
                            .map_err(|e| e.to_string())?,
                        MathOp::Abs => self.unary_op(|a| a.abs()).map_err(|e| e.to_string())?,
                        MathOp::Floor => self.unary_op(|a| a.floor()).map_err(|e| e.to_string())?,
                        MathOp::Ceil => self.unary_op(|a| a.ceil()).map_err(|e| e.to_string())?,
                        MathOp::Round => self.unary_op(|a| a.round()).map_err(|e| e.to_string())?,
                        MathOp::Min => {
                            self.binary_op(|a, b| a.min(b)).map_err(|e| e.to_string())?
                        }
                        MathOp::Max => {
                            self.binary_op(|a, b| a.max(b)).map_err(|e| e.to_string())?
                        }
                    }
                }
            }
        }

        let res = self
            .stack
            .last()
            .copied()
            .ok_or(VMError::EmptyStack.to_string())?;
        if res.is_nan() {
            return Err(VMError::MathError.to_string());
        }
        Ok(res)
    }

    fn pop(&mut self) -> Result<f64, VMError> {
        self.stack.pop().ok_or(VMError::StackUnderflow)
    }

    fn unary_op<F>(&mut self, op: F) -> Result<(), VMError>
    where
        F: Fn(f64) -> f64,
    {
        if self.stack.is_empty() {
            return Err(VMError::StackUnderflow);
        }
        let a = self.stack.pop().unwrap();
        let result = op(a);
        if result.is_nan() {
            return Err(VMError::MathError);
        }
        self.stack.push(result);
        Ok(())
    }

    fn binary_op<F>(&mut self, op: F) -> Result<(), VMError>
    where
        F: Fn(f64, f64) -> f64,
    {
        // FIX: Check stack depth BEFORE popping to prevent corruption
        if self.stack.len() < 2 {
            return Err(VMError::StackUnderflow);
        }

        let b = self.stack.pop().unwrap();
        let a = self.stack.pop().unwrap();

        let result = op(a, b);

        // FIX: Strictly treat NaN as a runtime error in derivation logic
        if result.is_nan() {
            return Err(VMError::MathError);
        }

        self.stack.push(result);
        Ok(())
    }

    fn compare_op<F>(&mut self, f: F) -> Result<(), VMError>
    where
        F: FnOnce(f64, f64) -> bool,
    {
        // Compare ops also need 2 args
        if self.stack.len() < 2 {
            return Err(VMError::StackUnderflow);
        }
        let b = self.stack.pop().unwrap();
        let a = self.stack.pop().unwrap();
        // IEEE-754: Comparisons with NaN are False. No longer erroring.
        self.stack.push(if f(a, b) { 1.0 } else { 0.0 });
        Ok(())
    }
}
