pub mod compiler;
pub mod ops;

pub use compiler::Compiler;
pub use ops::Op;

/// Hardened Float Equality: Handles scale variance using relative epsilon.
fn float_eq(a: f64, b: f64) -> bool {
    if a == b {
        return true;
    }
    let diff = (a - b).abs();
    let abs_a = a.abs();
    let abs_b = b.abs();
    diff / (abs_a + abs_b).min(f64::MAX) < 1e-10
}

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

    pub fn eval(&mut self, code: &[Op], params: &[f64]) -> Result<f64, String> {
        self.stack.clear();

        for op in code {
            if self.stack.len() > Self::MAX_STACK_SIZE {
                return Err("Stack overflow".into());
            }

            match op {
                Op::Push(val) => self.stack.push(*val),
                Op::LoadParam(idx) => {
                    let val = *params
                        .get(*idx as usize)
                        .ok_or("Parameter index out of bounds")?;
                    self.stack.push(val);
                }
                Op::Add => self.binary_op(|a, b| a + b)?,
                Op::Sub => self.binary_op(|a, b| a - b)?,
                Op::Mul => self.binary_op(|a, b| a * b)?,
                Op::Div => self.binary_op(|a, b| if b == 0.0 { f64::NAN } else { a / b })?,
                Op::Pow => self.binary_op(|a, b| a.powf(b))?,
                Op::Neg => {
                    let a = self.pop()?;
                    self.stack.push(-a);
                }
                Op::Eq => self.compare_op(|a, b| float_eq(a, b))?,
                Op::Ne => self.compare_op(|a, b| !float_eq(a, b))?,
                Op::Gt => self.compare_op(|a, b| a > b)?,
                Op::Lt => self.compare_op(|a, b| a < b)?,
                Op::Ge => self.compare_op(|a, b| a >= b)?,
                Op::Le => self.compare_op(|a, b| a <= b)?,
                Op::And => self.binary_op(|a, b| if a != 0.0 && b != 0.0 { 1.0 } else { 0.0 })?,
                Op::Or => self.binary_op(|a, b| if a != 0.0 || b != 0.0 { 1.0 } else { 0.0 })?,
                Op::Not => {
                    let a = self.pop()?;
                    self.stack.push(if a == 0.0 { 1.0 } else { 0.0 });
                }
            }
        }

        let res = self.stack.last().copied().ok_or("Empty stack result")?;
        if res.is_nan() {
            return Err("VM Runtime Error: Operation resulted in NaN".into());
        }
        Ok(res)
    }

    fn pop(&mut self) -> Result<f64, String> {
        self.stack.pop().ok_or("Stack underflow".to_string())
    }

    fn binary_op<F>(&mut self, f: F) -> Result<(), String>
    where
        F: FnOnce(f64, f64) -> f64,
    {
        let b = self.pop()?;
        let a = self.pop()?;
        self.stack.push(f(a, b));
        Ok(())
    }

    fn compare_op<F>(&mut self, f: F) -> Result<(), String>
    where
        F: FnOnce(f64, f64) -> bool,
    {
        let b = self.pop()?;
        let a = self.pop()?;
        // IEEE-754: Comparisons with NaN are False. No longer erroring.
        self.stack.push(if f(a, b) { 1.0 } else { 0.0 });
        Ok(())
    }
}
