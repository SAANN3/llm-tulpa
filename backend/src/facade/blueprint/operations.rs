use super::{
    catalog::ValueType,
    definition::{Literal, Operation},
    execution::{MAX_VALUE_BYTES, Values},
};

pub(super) struct Operations;
impl Operations {
    pub fn output_type(operation: &Operation, inputs: &[ValueType]) -> Result<ValueType, String> {
        use ValueType::{Array, Boolean, Dynamic, Integer, Number, String as Text};
        let expect = |index: usize, types: &[ValueType]| {
            let actual = inputs[index];
            if actual == Dynamic || types.contains(&actual) {
                Ok(())
            } else {
                Err(format!(
                    "operation argument '{}' must be {types:?}, got {actual:?}",
                    operation.inputs()[index].0
                ))
            }
        };
        Ok(match operation {
            Operation::Add { .. }
            | Operation::Subtract { .. }
            | Operation::Multiply { .. }
            | Operation::Divide { .. }
            | Operation::Remainder { .. } => {
                expect(0, &[Integer, Number])?;
                expect(1, &[Integer, Number])?;
                if matches!(operation, Operation::Divide { .. }) {
                    Number
                } else if inputs.contains(&Dynamic) {
                    Dynamic
                } else if inputs.iter().all(|t| *t == Integer) {
                    Integer
                } else {
                    Number
                }
            }
            Operation::And { .. } | Operation::Or { .. } => {
                expect(0, &[Boolean])?;
                expect(1, &[Boolean])?;
                Boolean
            }
            Operation::Not { .. } => {
                expect(0, &[Boolean])?;
                Boolean
            }
            Operation::Length { .. } => {
                expect(0, &[Array, Text])?;
                Integer
            }
            Operation::Get { .. } => {
                expect(0, &[Array])?;
                expect(1, &[Integer])?;
                Dynamic
            }
            Operation::Append { .. } => {
                expect(0, &[Array])?;
                Array
            }
            Operation::Concat { .. } => {
                expect(0, &[Array])?;
                expect(1, &[Array])?;
                Array
            }
        })
    }

    pub fn execute(operation: &Operation, values: &mut Values<'_>) -> Result<Literal, String> {
        let mut inputs = operation
            .inputs()
            .into_iter()
            .map(|(_, input)| values.resolve(input, 0))
            .collect::<Result<Vec<_>, _>>()?;
        Self::output_type(
            operation,
            &inputs.iter().map(ValueType::literal).collect::<Vec<_>>(),
        )?;
        let result = match operation {
            Operation::Add { .. }
            | Operation::Subtract { .. }
            | Operation::Multiply { .. }
            | Operation::Divide { .. }
            | Operation::Remainder { .. } => Self::numeric(operation, &inputs[0], &inputs[1])?,
            Operation::And { .. } | Operation::Or { .. } => {
                let (Literal::Boolean(left), Literal::Boolean(right)) = (&inputs[0], &inputs[1])
                else {
                    unreachable!()
                };
                Literal::Boolean(if matches!(operation, Operation::And { .. }) {
                    *left && *right
                } else {
                    *left || *right
                })
            }
            Operation::Not { .. } => {
                let Literal::Boolean(value) = inputs[0] else {
                    unreachable!()
                };
                Literal::Boolean(!value)
            }
            Operation::Length { .. } => {
                let length = match &inputs[0] {
                    Literal::String(text) => text.chars().count(),
                    Literal::Array(items) => items.len(),
                    _ => unreachable!(),
                };
                Literal::Integer(i64::try_from(length).map_err(|_| "length exceeds integer range")?)
            }
            Operation::Get { .. } => {
                let index = match inputs[1] {
                    Literal::Integer(index) => usize::try_from(index)
                        .map_err(|_| "list index must be nonnegative and representable")?,
                    _ => unreachable!(),
                };
                let Literal::Array(mut items) = inputs.remove(0) else {
                    unreachable!()
                };
                if index >= items.len() {
                    return Err("list index is out of bounds".into());
                }
                items.remove(index)
            }
            Operation::Append { .. } | Operation::Concat { .. } => {
                let right = inputs.pop().unwrap();
                let Literal::Array(mut left) = inputs.pop().unwrap() else {
                    unreachable!()
                };
                if matches!(operation, Operation::Append { .. }) {
                    left.push(right);
                } else {
                    let Literal::Array(right) = right else {
                        unreachable!()
                    };
                    left.extend(right);
                }
                Literal::Array(left)
            }
        };
        if serde_json::to_vec(&result)
            .map_err(|_| "could not serialize operation result")?
            .len()
            > MAX_VALUE_BYTES
        {
            return Err("operation result exceeds 1 MiB".into());
        }
        Ok(result)
    }

    fn numeric(operation: &Operation, left: &Literal, right: &Literal) -> Result<Literal, String> {
        if !matches!(operation, Operation::Divide { .. }) {
            if let (Literal::Integer(left), Literal::Integer(right)) = (left, right) {
                if matches!(operation, Operation::Remainder { .. }) && *right == 0 {
                    return Err("division by zero".into());
                }
                let value = match operation {
                    Operation::Add { .. } => left.checked_add(*right),
                    Operation::Subtract { .. } => left.checked_sub(*right),
                    Operation::Multiply { .. } => left.checked_mul(*right),
                    Operation::Remainder { .. } => left.checked_rem(*right),
                    _ => unreachable!(),
                }
                .ok_or("integer overflow")?;
                return Ok(Literal::Integer(value));
            }
        }
        let left = Self::number(left)?;
        let right = Self::number(right)?;
        if matches!(
            operation,
            Operation::Divide { .. } | Operation::Remainder { .. }
        ) && right == 0.0
        {
            return Err("division by zero".into());
        }
        let value = match operation {
            Operation::Add { .. } => left + right,
            Operation::Subtract { .. } => left - right,
            Operation::Multiply { .. } => left * right,
            Operation::Divide { .. } => left / right,
            Operation::Remainder { .. } => left % right,
            _ => unreachable!(),
        };
        if !value.is_finite() {
            return Err("numeric result must be finite".into());
        }
        Ok(Literal::Number(value))
    }

    fn number(value: &Literal) -> Result<f64, String> {
        match value {
            Literal::Number(value) if value.is_finite() => Ok(*value),
            // Mixed arithmetic must not silently round a large integer before calculating.
            Literal::Integer(value) if (*value as f64) as i128 == *value as i128 => {
                Ok(*value as f64)
            }
            Literal::Integer(_) => Err("integer cannot be represented exactly as a number".into()),
            _ => Err("expected a finite number".into()),
        }
    }
}
