pub mod compiler;
pub mod ops;

pub use compiler::Compiler;
pub use ops::Op;

#[derive(Debug, Default)]
pub struct VirtualMachine {
    stack: Vec<f64>,
}

impl VirtualMachine {
    pub fn new() -> Self {
        Self {
            stack: Vec::with_capacity(64),
        }
    }

    /// Evaluates a bytecode sequence against a specific module's parameters.
    /// Returns the result of the expression (usually 1.0/0.0 for logic, or a value for math).
    pub fn eval(&mut self, code: &[Op], params: &[f64]) -> Result<f64, String> {
        self.stack.clear();

        for op in code {
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
                Op::Div => self.binary_op(|a, b| if b == 0.0 { 0.0 } else { a / b })?,
                Op::Pow => self.binary_op(|a, b| a.powf(b))?,
                Op::Neg => {
                    let a = self.pop()?;
                    self.stack.push(-a);
                }
                // Logical/Relational (Return 1.0 for True, 0.0 for False)
                Op::Eq => self.binary_op(|a, b| {
                    if (a - b).abs() < f64::EPSILON {
                        1.0
                    } else {
                        0.0
                    }
                })?,
                Op::Ne => self.binary_op(|a, b| {
                    if (a - b).abs() >= f64::EPSILON {
                        1.0
                    } else {
                        0.0
                    }
                })?,
                Op::Gt => self.binary_op(|a, b| if a > b { 1.0 } else { 0.0 })?,
                Op::Lt => self.binary_op(|a, b| if a < b { 1.0 } else { 0.0 })?,
                Op::Ge => self.binary_op(|a, b| if a >= b { 1.0 } else { 0.0 })?,
                Op::Le => self.binary_op(|a, b| if a <= b { 1.0 } else { 0.0 })?,
                Op::And => self.binary_op(|a, b| if a != 0.0 && b != 0.0 { 1.0 } else { 0.0 })?,
                Op::Or => self.binary_op(|a, b| if a != 0.0 || b != 0.0 { 1.0 } else { 0.0 })?,
                Op::Not => {
                    let a = self.pop()?;
                    self.stack.push(if a == 0.0 { 1.0 } else { 0.0 });
                }
            }
        }

        self.stack
            .last()
            .copied()
            .ok_or("Empty stack result".to_string())
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
}
