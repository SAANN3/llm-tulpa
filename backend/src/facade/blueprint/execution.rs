use super::{
    definition::{Comparison, Input, Literal, NodeKind, PathPart},
    runs::{NodeStatus, RunState},
    template::{self, Part},
};
use std::{cmp::Ordering, collections::BTreeMap, time::Duration};
use tokio_util::sync::CancellationToken;

pub(super) const MAX_VALUE_BYTES: usize = 1024 * 1024;
pub(super) const MAX_STATE_BYTES: usize = 4 * MAX_VALUE_BYTES;

pub(super) struct Outcome {
    pub port: Option<String>,
    pub outputs: BTreeMap<String, Literal>,
}

pub(super) fn supported(kind: &NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Entry
            | NodeKind::Tool { .. }
            | NodeKind::Generate { .. }
            | NodeKind::Agent { .. }
            | NodeKind::Plugin { .. }
            | NodeKind::Sleep { .. }
            | NodeKind::If { .. }
            | NodeKind::Switch { .. }
            | NodeKind::Format { .. }
            | NodeKind::Return { .. }
    )
}

pub(super) async fn execute(
    kind: &NodeKind,
    state: &RunState,
    cancel: &CancellationToken,
) -> Result<Outcome, String> {
    let mut values = Values {
        state,
        remaining: MAX_VALUE_BYTES,
    };
    let mut result = Outcome {
        port: Some("out".into()),
        outputs: BTreeMap::new(),
    };
    match kind {
        NodeKind::Entry => {}
        NodeKind::Sleep { seconds } => {
            let seconds = values.resolve(seconds, 0)?;
            let seconds = match seconds {
                Literal::Integer(v) => v as f64,
                Literal::Number(v) => v,
                _ => return Err("sleep duration must be a number".into()),
            };
            let duration = Duration::try_from_secs_f64(seconds)
                .map_err(|_| "sleep duration must be finite, nonnegative and representable")?;
            let until = tokio::time::Instant::now()
                .checked_add(duration)
                .ok_or("sleep deadline is too far in the future")?;
            tokio::select! { biased;
                _ = cancel.cancelled() => return Err("run was stopped".into()),
                _ = tokio::time::sleep_until(until) => {}
            }
        }
        NodeKind::If {
            left,
            operator,
            right,
        } => {
            let left = values.resolve(left, 0)?;
            let right = values.resolve(right, 0)?;
            let condition = match operator {
                Comparison::Equal => equal(&left, &right),
                Comparison::NotEqual => !equal(&left, &right),
                Comparison::Contains => match (&left, &right) {
                    (Literal::String(a), Literal::String(b)) => a.contains(b),
                    _ => return Err("contains requires two strings".into()),
                },
                other => {
                    let order =
                        numeric_cmp(&left, &right).ok_or("ordering requires finite numbers")?;
                    match other {
                        Comparison::Greater => order.is_gt(),
                        Comparison::GreaterOrEqual => !order.is_lt(),
                        Comparison::Less => order.is_lt(),
                        Comparison::LessOrEqual => !order.is_gt(),
                        _ => unreachable!(),
                    }
                }
            };
            result.port = Some(if condition { "true" } else { "false" }.into());
        }
        NodeKind::Switch { value, cases } => {
            let value = values.resolve(value, 0)?;
            result.port = Some(
                cases
                    .iter()
                    .find(|c| equal(&value, &c.value))
                    .map(|c| c.port.clone())
                    .unwrap_or_else(|| "default".into()),
            );
        }
        NodeKind::Format {
            template,
            arguments,
        } => {
            let mut arguments_text = BTreeMap::new();
            for (name, input) in arguments {
                let value = values.resolve(input, 0)?;
                let text = match value {
                    Literal::String(v) => v,
                    other => serde_json::to_string(&other).map_err(|_| "could not format value")?,
                };
                arguments_text.insert(name.as_str(), text);
            }
            let mut text = String::new();
            for part in template::parse(template)? {
                let piece = match part {
                    Part::Text(t) => t,
                    Part::Argument(name) => arguments_text
                        .get(name)
                        .ok_or_else(|| format!("missing template argument '{name}'"))?
                        .as_str(),
                };
                if text.len().saturating_add(piece.len()) > MAX_VALUE_BYTES {
                    return Err("formatted text exceeds 1 MiB".into());
                }
                text.push_str(piece);
            }
            result.outputs.insert("text".into(), Literal::String(text));
        }
        NodeKind::Return { values: inputs } => {
            for (name, input) in inputs {
                result
                    .outputs
                    .insert(name.clone(), values.resolve(input, 0)?);
            }
            result.port = None;
        }
        _ => return Err("this node does not have an executor yet".into()),
    }
    Ok(result)
}

pub(super) struct Values<'a> {
    pub state: &'a RunState,
    pub remaining: usize,
}
impl Values<'_> {
    pub fn fields(&mut self, fields: &BTreeMap<String, Input>, depth: usize) -> Result<BTreeMap<String, Literal>, String> {
        let mut result = BTreeMap::new();
        for (name, input) in fields {
            spend(&mut self.remaining, name.len(), depth)?;
            result.insert(name.clone(), self.resolve(input, depth + 1)?);
        }
        Ok(result)
    }
    pub fn resolve(&mut self, input: &Input, depth: usize) -> Result<Literal, String> {
        spend(&mut self.remaining, 1, depth)?;
        match input {
            Input::Literal { value } => copy_value(value, &mut self.remaining, depth + 1),
            Input::Array { items } => Ok(Literal::Array(
                items
                    .iter()
                    .map(|v| self.resolve(v, depth + 1))
                    .collect::<Result<_, _>>()?,
            )),
            Input::Object { fields } => Ok(Literal::Object(self.fields(fields, depth)?)),
            Input::Output {
                node_id,
                port,
                path,
            } => {
                let node = self
                    .state
                    .nodes
                    .get(node_id)
                    .filter(|n| n.status == NodeStatus::Completed)
                    .ok_or_else(|| format!("output from '{node_id}' is not available"))?;
                let mut value = node
                    .outputs
                    .get(port)
                    .ok_or_else(|| format!("output '{node_id}.{port}' is missing"))?;
                for part in path {
                    value = match (value, part) {
                        (Literal::Object(fields), PathPart::Field(name)) => fields.get(name),
                        (Literal::Array(items), PathPart::Index(index)) => {
                            items.get(*index as usize)
                        }
                        _ => None,
                    }
                    .ok_or_else(|| format!("output path in '{node_id}.{port}' does not exist"))?;
                }
                copy_value(value, &mut self.remaining, depth + 1)
            }
        }
    }
}
fn spend(remaining: &mut usize, bytes: usize, depth: usize) -> Result<(), String> {
    if depth > 128 {
        return Err("value nesting is too deep".into());
    }
    *remaining = remaining
        .checked_sub(bytes)
        .ok_or("node input values exceed 1 MiB")?;
    Ok(())
}
fn copy_value(value: &Literal, remaining: &mut usize, depth: usize) -> Result<Literal, String> {
    spend(remaining, 1, depth)?;
    Ok(match value {
        Literal::String(s) => {
            spend(remaining, s.len(), depth)?;
            Literal::String(s.clone())
        }
        Literal::Array(items) => Literal::Array(
            items
                .iter()
                .map(|v| copy_value(v, remaining, depth + 1))
                .collect::<Result<_, _>>()?,
        ),
        Literal::Object(fields) => {
            let mut result = BTreeMap::new();
            for (name, value) in fields {
                spend(remaining, name.len(), depth)?;
                result.insert(name.clone(), copy_value(value, remaining, depth + 1)?);
            }
            Literal::Object(result)
        }
        _ => value.clone(),
    })
}
fn numeric_cmp(a: &Literal, b: &Literal) -> Option<Ordering> {
    match (a, b) {
        (Literal::Integer(a), Literal::Integer(b)) => Some(a.cmp(b)),
        (Literal::Number(a), Literal::Number(b)) if a.is_finite() && b.is_finite() => {
            a.partial_cmp(b)
        }
        (Literal::Integer(a), Literal::Number(b)) => int_float(*a, *b),
        (Literal::Number(a), Literal::Integer(b)) => int_float(*b, *a).map(Ordering::reverse),
        _ => None,
    }
}
fn int_float(integer: i64, number: f64) -> Option<Ordering> {
    if !number.is_finite() {
        return None;
    }
    // Avoid rounding adjacent integers above 2^53 to the same f64.
    if number >= -(i64::MIN as f64) {
        return Some(Ordering::Less);
    }
    if number < i64::MIN as f64 {
        return Some(Ordering::Greater);
    }
    let order = integer.cmp(&(number as i64));
    Some(if order == Ordering::Equal {
        0.0f64.partial_cmp(&number.fract())?
    } else {
        order
    })
}
fn equal(a: &Literal, b: &Literal) -> bool {
    if let Some(order) = numeric_cmp(a, b) {
        return order == Ordering::Equal;
    }
    match (a, b) {
        (Literal::Null, Literal::Null) => true,
        (Literal::Boolean(a), Literal::Boolean(b)) => a == b,
        (Literal::String(a), Literal::String(b)) => a == b,
        (Literal::Array(a), Literal::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equal(a, b))
        }
        (Literal::Object(a), Literal::Object(b)) => {
            a.len() == b.len() && a.iter().all(|(k, a)| b.get(k).is_some_and(|b| equal(a, b)))
        }
        _ => false,
    }
}
