use super::{
    definition::{Comparison, Input, Literal, NodeKind, PathPart, Workflow},
    operations::Operations,
    validation,
    runs::{NodeStatus, RunState},
    template::{self, Part},
};
use std::{cmp::Ordering, collections::BTreeMap, time::Duration};
use tokio_util::sync::CancellationToken;

pub(super) const MAX_VALUE_BYTES: usize = 1024 * 1024;
pub(super) const MAX_STATE_BYTES: usize = 4 * MAX_VALUE_BYTES;

pub(super) struct Outcome {
    pub control_output: Option<String>,
    pub outputs: BTreeMap<String, Literal>,
    // Applied with the result so a stopped or oversized node cannot change a variable.
    pub assignment: Option<(String, Literal)>,
}

pub(super) fn supported(kind: &NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Entry
            | NodeKind::Variable { .. }
            | NodeKind::SetVariable { .. }
            | NodeKind::Calculate { .. }
            | NodeKind::Merge { .. }
            | NodeKind::Tool { .. }
            | NodeKind::Generate { .. }
            | NodeKind::Agent { .. }
            | NodeKind::Plugin { .. }
            | NodeKind::Sleep { .. }
            | NodeKind::If { .. }
            | NodeKind::While { .. }
            | NodeKind::ForEach { .. }
            | NodeKind::Switch { .. }
            | NodeKind::Format { .. }
            | NodeKind::Print { .. }
            | NodeKind::Html { .. }
            | NodeKind::Return { .. }
    )
}

pub(super) async fn execute(
    node_id: &str,
    kind: &NodeKind,
    workflow: &Workflow,
    state: &RunState,
    cancel: &CancellationToken,
) -> Result<Outcome, String> {
    let mut values = Values {
        state,
        remaining: MAX_VALUE_BYTES,
    };
    let mut result = Outcome {
        control_output: Some("out".into()),
        outputs: BTreeMap::new(),
        assignment: None,
    };
    match kind {
        NodeKind::Entry => {}
        NodeKind::Variable { .. } => {
            let value = state.variables.get(node_id).ok_or("variable is not initialized")?;
            result.outputs.insert("value".into(), copy_value(value, &mut values.remaining, 0)?);
        }
        NodeKind::SetVariable { variable_id, value } => {
            let schema = workflow.nodes.iter().find_map(|node| match &node.kind {
                NodeKind::Variable { value_type, .. } if node.id == *variable_id => Some(value_type),
                _ => None,
            }).ok_or("assignment must reference a Variable node")?;
            if !state.variables.contains_key(variable_id) { return Err("variable is not initialized".into()); }
            if !state.nodes.get(variable_id).is_some_and(|node| node.status == NodeStatus::Completed) {
                return Err("variable declaration has not executed".into());
            }
            let value = values.resolve(value, 0)?;
            validation::variable_value(&value, schema)?;
            result.assignment = Some((variable_id.clone(), value.clone()));
            result.outputs.insert("value".into(), value);
        }
        NodeKind::Calculate { operation } => {
            result.outputs.insert("value".into(), Operations::execute(operation, &mut values)?);
        }
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
            let condition = operator.evaluate(&left, &right)?;
            result.control_output = Some(if condition { "true" } else { "false" }.into());
        }
        NodeKind::Switch { value, cases } => {
            let value = values.resolve(value, 0)?;
            result.control_output = Some(
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
        NodeKind::Html { value } => {
            let html = values.resolve(value, 0)?;
            if !matches!(html, Literal::String(_)) {
                return Err("HTML requires a string".into());
            }
            result.outputs.insert("value".into(), html);
        }
        NodeKind::Print { value } => {
            result.outputs.insert("value".into(), values.resolve(value, 0)?);
        }
        NodeKind::Return { values: inputs } => {
            for (name, input) in inputs {
                result
                    .outputs
                    .insert(name.clone(), values.resolve(input, 0)?);
            }
            result.control_output = None;
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
            Input::Variable { node_id, path } => {
                if !self.state.nodes.get(node_id).is_some_and(|node| node.status == NodeStatus::Completed) {
                    return Err(format!("variable declaration '{node_id}' has not executed"));
                }
                let value = self.state.variables.get(node_id)
                    .ok_or_else(|| format!("variable '{node_id}' is not initialized"))?;
                let value = select(value, path).ok_or_else(|| format!("variable path in '{node_id}' does not exist"))?;
                copy_value(value, &mut self.remaining, depth + 1)
            }
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
                let value = node
                    .outputs
                    .get(port)
                    .ok_or_else(|| format!("output '{node_id}.{port}' is missing"))?;
                let value = select(value, path).ok_or_else(|| format!("output path in '{node_id}.{port}' does not exist"))?;
                copy_value(value, &mut self.remaining, depth + 1)
            }
        }
    }
}
fn select<'a>(mut value: &'a Literal, path: &[PathPart]) -> Option<&'a Literal> {
    for part in path {
        value = match (value, part) {
            (Literal::Object(fields), PathPart::Field(name)) => fields.get(name),
            (Literal::Array(items), PathPart::Index(index)) => items.get(*index as usize),
            _ => None,
        }?;
    }
    Some(value)
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

impl Comparison {
    pub(super) fn evaluate(&self, left: &Literal, right: &Literal) -> Result<bool, String> {
        Ok(match self {
            Comparison::Equal => equal(left, right),
            Comparison::NotEqual => !equal(left, right),
            Comparison::Contains => match (left, right) {
                (Literal::String(a), Literal::String(b)) => a.contains(b),
                _ => return Err("contains requires two strings".into()),
            },
            other => {
                let order =
                    numeric_cmp(left, right).ok_or("ordering requires finite numbers")?;
                match other {
                    Comparison::Greater => order.is_gt(),
                    Comparison::GreaterOrEqual => !order.is_lt(),
                    Comparison::Less => order.is_lt(),
                    Comparison::LessOrEqual => !order.is_gt(),
                    _ => unreachable!(),
                }
            }
        })
    }
}
