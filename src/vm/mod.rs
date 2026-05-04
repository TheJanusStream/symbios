/* src/vm/mod.rs */
pub mod compiler;
pub mod decompiler;
pub mod ops;

pub use compiler::Compiler;
pub use decompiler::{Decompiler, decompile, decompile_with_params};
pub use ops::{MathOp, Op};
use std::fmt;

/// Default *relative* tolerance used by [`float_eq`] and the VM's comparison ops.
///
/// Chosen as a balance: tight enough to surface meaningful inequality between
/// distinct user-visible parameter values (which are typically O(1)–O(10^3) in
/// L-system grammars and rarely require resolution finer than ~8 significant
/// decimal digits), and loose enough to absorb the few ULPs of round-off that
/// accumulate across a single rule's arithmetic chain.
///
/// Deep parametric derivations (50+ generations) where round-off compounds may
/// need a looser value; for those workflows construct the VM with
/// [`VirtualMachine::with_epsilon`] or call [`VirtualMachine::set_epsilon`].
pub const DEFAULT_RELATIVE_EPSILON: f64 = 1e-8;

/// Robust floating point equality check using [`DEFAULT_RELATIVE_EPSILON`].
///
/// See [`float_eq_eps`] for the parameterized variant.
#[inline]
pub fn float_eq(a: f64, b: f64) -> bool {
    float_eq_eps(a, b, DEFAULT_RELATIVE_EPSILON)
}

/// Robust floating point equality check with a caller-supplied relative
/// tolerance. Falls back to an absolute `f64::EPSILON * 100.0` test when either
/// operand is zero or both are subnormal, since relative tolerance is
/// ill-defined near zero.
#[inline]
pub fn float_eq_eps(a: f64, b: f64, relative_eps: f64) -> bool {
    if a == b {
        return true;
    }
    let abs_a = a.abs();
    let abs_b = b.abs();
    let diff = (a - b).abs();
    if a == 0.0 || b == 0.0 || (abs_a + abs_b < f64::MIN_POSITIVE) {
        return diff < (f64::EPSILON * 100.0);
    }
    diff / (abs_a + abs_b).min(f64::MAX) < relative_eps
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

#[derive(Debug)]
pub struct VirtualMachine {
    stack: Vec<f64>,
    /// Relative tolerance used by [`Op::Eq`], [`Op::Ne`], and the
    /// epsilon-aware [`Op::Gt`]/[`Op::Lt`]/[`Op::Ge`]/[`Op::Le`] ops.
    ///
    /// Defaults to [`DEFAULT_RELATIVE_EPSILON`]. Override via
    /// [`VirtualMachine::with_epsilon`] / [`VirtualMachine::set_epsilon`] for
    /// workloads (e.g. deep parametric chains) where the default would treat
    /// numerically distinct values as equal.
    relative_epsilon: f64,
}

impl Default for VirtualMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl VirtualMachine {
    const MAX_STACK_SIZE: usize = 256;

    pub fn new() -> Self {
        Self {
            stack: Vec::with_capacity(64),
            relative_epsilon: DEFAULT_RELATIVE_EPSILON,
        }
    }

    /// Constructs a VM with a custom relative epsilon for floating-point
    /// equality. Non-finite or non-positive values fall back to
    /// [`DEFAULT_RELATIVE_EPSILON`] (silent: a non-positive epsilon would make
    /// `Eq` fire only on bit-exact equality, which is almost never what the
    /// caller intended).
    pub fn with_epsilon(relative_eps: f64) -> Self {
        let mut vm = Self::new();
        vm.set_epsilon(relative_eps);
        vm
    }

    /// Sets the relative epsilon for floating-point comparisons.
    ///
    /// Non-finite or non-positive values are clamped to
    /// [`DEFAULT_RELATIVE_EPSILON`].
    pub fn set_epsilon(&mut self, relative_eps: f64) {
        self.relative_epsilon = if relative_eps.is_finite() && relative_eps > 0.0 {
            relative_eps
        } else {
            DEFAULT_RELATIVE_EPSILON
        };
    }

    /// Returns the relative epsilon currently in use.
    pub fn epsilon(&self) -> f64 {
        self.relative_epsilon
    }

    pub fn eval(
        &mut self,
        code: &[Op],
        params: &[f64],
        predecessor_age: f64,
    ) -> Result<f64, String> {
        self.stack.clear();

        for op in code {
            if self.stack.len() >= Self::MAX_STACK_SIZE {
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
                Op::Eq => {
                    let eps = self.relative_epsilon;
                    self.compare_op(|a, b| float_eq_eps(a, b, eps))
                        .map_err(|e| e.to_string())?
                }
                Op::Ne => {
                    let eps = self.relative_epsilon;
                    self.compare_op(|a, b| !float_eq_eps(a, b, eps))
                        .map_err(|e| e.to_string())?
                }
                // Epsilon-aware comparisons for mathematical consistency:
                // If float_eq(a, b), then Ge/Le must be true and Gt/Lt must be false.
                Op::Gt => {
                    let eps = self.relative_epsilon;
                    self.compare_op(|a, b| a > b && !float_eq_eps(a, b, eps))
                        .map_err(|e| e.to_string())?
                }
                Op::Lt => {
                    let eps = self.relative_epsilon;
                    self.compare_op(|a, b| a < b && !float_eq_eps(a, b, eps))
                        .map_err(|e| e.to_string())?
                }
                Op::Ge => {
                    let eps = self.relative_epsilon;
                    self.compare_op(|a, b| a >= b || float_eq_eps(a, b, eps))
                        .map_err(|e| e.to_string())?
                }
                Op::Le => {
                    let eps = self.relative_epsilon;
                    self.compare_op(|a, b| a <= b || float_eq_eps(a, b, eps))
                        .map_err(|e| e.to_string())?
                }
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
                Op::Math(math_op) => {
                    // Pre-check stack depth using authoritative arity from MathOp
                    let arity = math_op.arity() as usize;
                    if self.stack.len() < arity {
                        return Err(VMError::StackUnderflow.to_string());
                    }

                    match math_op {
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

        Self::sanitize(res).map_err(|e| e.to_string())
    }

    fn pop(&mut self) -> Result<f64, VMError> {
        self.stack.pop().ok_or(VMError::StackUnderflow)
    }

    /// Clamps non-finite results: NaN becomes MathError, Infinity is clamped
    /// to f64::MAX/-f64::MAX. This prevents transient singularities (e.g. division
    /// by near-zero) from killing entire derivations in evolutionary runs.
    #[inline]
    fn sanitize(result: f64) -> Result<f64, VMError> {
        if result.is_nan() {
            Err(VMError::MathError)
        } else if result == f64::INFINITY {
            Ok(f64::MAX)
        } else if result == f64::NEG_INFINITY {
            Ok(f64::MIN)
        } else {
            Ok(result)
        }
    }

    fn unary_op<F>(&mut self, op: F) -> Result<(), VMError>
    where
        F: Fn(f64) -> f64,
    {
        if self.stack.is_empty() {
            return Err(VMError::StackUnderflow);
        }
        let a = self.stack.pop().unwrap();
        let result = Self::sanitize(op(a))?;
        self.stack.push(result);
        Ok(())
    }

    fn binary_op<F>(&mut self, op: F) -> Result<(), VMError>
    where
        F: Fn(f64, f64) -> f64,
    {
        if self.stack.len() < 2 {
            return Err(VMError::StackUnderflow);
        }

        let b = self.stack.pop().unwrap();
        let a = self.stack.pop().unwrap();

        let result = Self::sanitize(op(a, b))?;

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
