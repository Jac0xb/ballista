//! The checks the TypeScript SDK's Zod schema makes before its compiler runs: names, sizes,
//! counts, and the template-wide rules of `TemplateSchema`'s `superRefine`.

use std::collections::HashSet;

use ballista_common::template::{
    MAX_ACCOUNT_GROUPS, MAX_BATCH_STRIDE, MAX_CPI_ACCOUNTS, MAX_GROUP_EXCEPTS, MAX_GROUP_MATCHES,
    MAX_INPUTS, MAX_INPUT_BYTES, MAX_INPUT_VALUES, MAX_LOOPS, MAX_PDA_SEEDS, MAX_REGISTRIES,
    MAX_REGISTRY_SIZE, MAX_ROW_INPUTS, MAX_RUNTIME_ACCOUNTS,
};

use super::compile::registry_read_type;
use super::model::*;
use super::CompileError;

const MAX_STEPS: usize = 128;
const MAX_LOOP_STEPS: usize = 64;
const MAX_PARTS: usize = 64;
const MAX_LABEL_LENGTH: usize = 64;
const MAX_BATCH_ITERATIONS: i64 = 60;

fn fail<T>(message: impl Into<String>) -> Result<T, CompileError> {
    Err(CompileError::new(message))
}

/// A name the TypeScript SDK accepts: a letter or underscore, then letters, digits or
/// underscores.
fn identifier(name: &str, what: &str) -> Result<(), CompileError> {
    let mut characters = name.chars();
    let valid = characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && characters.all(|rest| rest.is_ascii_alphanumeric() || rest == '_');
    if !valid {
        return fail(format!(
            "{what} name {name:?} must start with a letter or underscore and hold only letters, digits and underscores"
        ));
    }
    Ok(())
}

fn input_type(name: &str, ty: Type) -> Result<(), CompileError> {
    if let Type::Bytes(length) = ty {
        if length < 1 || length as usize > MAX_INPUT_BYTES {
            return fail(format!(
                "input {name}: a bytes input holds 1 to {MAX_INPUT_BYTES} bytes, not {length}"
            ));
        }
    }
    Ok(())
}

fn account_ref(reference: &AccountRef) -> Result<(), CompileError> {
    identifier(reference.name(), "account")
}

fn label(label: Option<&str>) -> Result<(), CompileError> {
    if let Some(label) = label {
        if label.chars().count() > MAX_LABEL_LENGTH {
            return fail(format!(
                "label {label:?} is longer than {MAX_LABEL_LENGTH} characters"
            ));
        }
    }
    Ok(())
}

fn byte_read_length(length: usize, what: &str) -> Result<(), CompileError> {
    if !(1..=MAX_INPUT_BYTES).contains(&length) {
        return fail(format!(
            "{what} reads 1 to {MAX_INPUT_BYTES} bytes, not {length}"
        ));
    }
    Ok(())
}

fn offset_u32(offset: i64, what: &str) -> Result<(), CompileError> {
    if !(0..=i64::from(u32::MAX)).contains(&offset) {
        return fail(format!("{what} offset {offset} is not a u32"));
    }
    Ok(())
}

/// `GroupFilter`'s schema: one or two different programs, one to four matches, at most four
/// except keys.
fn group_filter(filter: &GroupFilter) -> Result<(), CompileError> {
    let programs = &filter.programs;
    if programs.is_empty() || programs.len() > 2 {
        return fail(format!(
            "A group filter names one or two programs, not {}",
            programs.len()
        ));
    }
    if programs.len() == 2 && programs[0] == programs[1] {
        return fail("A group filter names two different programs");
    }
    if filter.matches.is_empty() || filter.matches.len() > MAX_GROUP_MATCHES {
        return fail(format!(
            "A group filter holds 1 to {MAX_GROUP_MATCHES} matches, not {}",
            filter.matches.len()
        ));
    }
    if filter.except_keys.len() > MAX_GROUP_EXCEPTS {
        return fail(format!(
            "A group filter holds at most {MAX_GROUP_EXCEPTS} except keys, not {}",
            filter.except_keys.len()
        ));
    }
    for (_, value) in &filter.matches {
        expression(value)?;
    }
    for key in &filter.except_keys {
        expression(key)?;
    }
    Ok(())
}

fn expression(expr: &Expr) -> Result<(), CompileError> {
    match &expr.0 {
        Node::Input(name) => identifier(name, "input"),
        Node::RowInput(name) => identifier(name, "row input"),
        Node::Variable(name) => identifier(name, "variable"),
        Node::Literal(Literal::Bytes(bytes)) if bytes.len() > MAX_INPUT_BYTES => fail(format!(
            "a bytes constant holds at most {MAX_INPUT_BYTES} bytes, not {}",
            bytes.len()
        )),
        Node::Literal(_) => Ok(()),
        Node::AccountField(account, _) => account_ref(account),
        Node::AccountData {
            account, offset, ..
        } => {
            account_ref(account)?;
            match offset.as_ref() {
                Offset::Static(offset) => offset_u32(*offset, "accountData"),
                Offset::Dynamic(offset) => expression(offset),
            }
        }
        Node::ReturnData { offset, .. } => offset_u32(*offset, "returnData"),
        Node::ClockSlot | Node::ClockUnixTimestamp | Node::LoopIndex => Ok(()),
        Node::Pda {
            program,
            seeds,
            bump,
        } => {
            account_ref(program)?;
            if seeds.is_empty() || seeds.len() > MAX_PDA_SEEDS {
                return fail(format!(
                    "PDA derivation requires 1 to {MAX_PDA_SEEDS} seeds, not {}",
                    seeds.len()
                ));
            }
            for seed in seeds {
                expression(seed)?;
            }
            if let Some(bump) = bump {
                expression(bump)?;
            }
            Ok(())
        }
        Node::Binary { left, right, .. } => {
            expression(left)?;
            expression(right)
        }
        Node::MultiplyDivide {
            left,
            right,
            divisor,
            ..
        } => {
            expression(left)?;
            expression(right)?;
            expression(divisor)
        }
        Node::PowerOfTen(value) | Node::Not(value) | Node::BytesLength(value) => expression(value),
        Node::Cast { value, .. } => expression(value),
        Node::Select {
            condition,
            if_true,
            if_false,
        } => {
            expression(condition)?;
            expression(if_true)?;
            expression(if_false)
        }
        Node::InstructionCount(sysvar) | Node::CurrentInstructionIndex(sysvar) => {
            account_ref(sysvar)
        }
        Node::Instruction { sysvar, index, .. } => {
            account_ref(sysvar)?;
            expression(index)
        }
        Node::InstructionAccount {
            sysvar,
            index,
            position,
            ..
        } => {
            account_ref(sysvar)?;
            expression(index)?;
            expression(position)
        }
        Node::InstructionData {
            sysvar,
            index,
            offset,
            ..
        } => {
            account_ref(sysvar)?;
            expression(index)?;
            expression(offset)
        }
        Node::InstructionDataBytes {
            sysvar,
            index,
            offset,
            length,
        } => {
            account_ref(sysvar)?;
            expression(index)?;
            expression(offset)?;
            byte_read_length(*length, "instructionDataBytes")
        }
        Node::AccountDataBytes {
            account,
            offset,
            length,
        } => {
            account_ref(account)?;
            expression(offset)?;
            byte_read_length(*length, "accountDataBytes")
        }
        Node::Registry { account, field } => {
            identifier(account, "account")?;
            identifier(field, "registry field")
        }
        Node::GroupLength(group) => identifier(group, "account group"),
        Node::GroupFilter { group, filter, .. } => {
            identifier(group, "account group")?;
            group_filter(filter)
        }
        Node::Invalid(message) => fail(message.clone()),
    }
}

fn parts(parts: &[DataPart], what: &str, minimum: usize) -> Result<(), CompileError> {
    if parts.len() < minimum || parts.len() > MAX_PARTS {
        return fail(format!(
            "{what} takes {minimum} to {MAX_PARTS} data parts, not {}",
            parts.len()
        ));
    }
    for part in parts {
        if let DataNode::Encoded(_, value) = &part.0 {
            expression(value)?;
        }
    }
    Ok(())
}

fn is_loop(step: &Step) -> Option<&Loop> {
    match &step.0 {
        StepNode::Loop(body) => Some(body),
        _ => None,
    }
}

fn steps(steps: &[Step], groups: &HashSet<&str>) -> Result<(), CompileError> {
    for step in steps {
        label(step.label_text())?;
        match &step.0 {
            StepNode::Require { condition, .. } => expression(condition)?,
            StepNode::Let { name, value, .. } | StepNode::Assign { name, value, .. } => {
                identifier(name, "variable")?;
                expression(value)?;
            }
            StepNode::Invoke(invoke) => {
                account_ref(&invoke.program)?;
                if invoke.accounts.len() > MAX_CPI_ACCOUNTS {
                    return fail(format!(
                        "an invocation passes at most {MAX_CPI_ACCOUNTS} accounts, not {}",
                        invoke.accounts.len()
                    ));
                }
                for account in &invoke.accounts {
                    account_ref(&account.account)?;
                }
                parts(&invoke.data, "an invocation", 0)?;
                if let Some(when) = &invoke.when {
                    expression(when)?;
                }
                if let Some(group) = &invoke.account_group {
                    identifier(group, "account group")?;
                    if !groups.contains(group.as_str()) {
                        return fail(format!("Unknown account group: {group}"));
                    }
                }
            }
            StepNode::Emit { parts: list, .. } => parts(list, "emit", 1)?,
            StepNode::SetReturnData { parts: list, .. } => parts(list, "setReturnData", 1)?,
            StepNode::SetRegistry {
                account,
                field,
                value,
                ..
            } => {
                identifier(account, "account")?;
                identifier(field, "registry field")?;
                expression(value)?;
            }
            StepNode::Loop(body) => {
                if let Some((count, max)) = &body.repeat {
                    expression(count)?;
                    if !(1..=255).contains(max) {
                        return fail(format!("repeat max must be 1 to 255, not {max}"));
                    }
                }
                if body.steps.is_empty() || body.steps.len() > MAX_LOOP_STEPS {
                    return fail(format!(
                        "a loop body holds 1 to {MAX_LOOP_STEPS} steps, not {}",
                        body.steps.len()
                    ));
                }
                let carry = body.carry.as_deref().unwrap_or_default();
                if carry.len() > 64 {
                    return fail("a loop carries at most 64 variables");
                }
                for name in carry {
                    identifier(name, "carried variable")?;
                }
                if body.steps.iter().any(|inner| is_loop(inner).is_some()) {
                    return fail("Nested iteration is not supported");
                }
                if carry.iter().collect::<HashSet<_>>().len() != carry.len() {
                    return fail("Carried variables must be unique");
                }
                self::steps(&body.steps, groups)?;
            }
            StepNode::Invalid(message) => return fail(message.clone()),
        }
    }
    Ok(())
}

fn account(name: &str, account: &Account) -> Result<(), CompileError> {
    identifier(name, "account")?;
    if let Some(misuse) = &account.misuse {
        return fail(format!("account {name}: {misuse}"));
    }
    if let Some(registry) = &account.registry {
        identifier(&registry.name, "registry")?;
        identifier(&registry.payer, "account")?;
        if let Some(key) = &registry.key {
            expression(key)?;
        }
    }
    Ok(())
}

pub(crate) fn template(template: &Template) -> Result<(), CompileError> {
    if let Some(duplicate) = template.duplicates.first() {
        return fail(format!("{duplicate} is declared twice"));
    }
    for (name, ty) in &template.inputs {
        identifier(name, "input")?;
        input_type(name, *ty)?;
    }
    if template.registries.len() > MAX_REGISTRIES {
        return fail(format!(
            "A template declares at most {MAX_REGISTRIES} registries"
        ));
    }
    for (name, fields) in &template.registries {
        identifier(name, "registry")?;
        let mut size = 0;
        let mut seen = HashSet::new();
        for (field, ty) in fields {
            identifier(field, "registry field")?;
            if !seen.insert(field.as_str()) {
                return fail(format!("registry {name}: field {field} is declared twice"));
            }
            let Some(read) = registry_read_type(*ty) else {
                return fail(format!(
                    "registry {name}: field {field:?} is a {}; a registry field is bool, u64, i64, u128 or pubkey",
                    ty.name()
                ));
            };
            size += read.width();
        }
        if !(1..=MAX_REGISTRY_SIZE).contains(&size) {
            return fail(format!(
                "registry {name}: A registry holds 1 to {MAX_REGISTRY_SIZE} bytes of fields"
            ));
        }
    }
    for (name, constraint) in &template.accounts {
        account(name, constraint)?;
    }
    let mut row_inputs = 0;
    let mut stride = 0;
    let mut max_iterations = 0;
    if let Some(batch) = &template.batch {
        if !(1..=MAX_BATCH_ITERATIONS).contains(&batch.max_iterations) {
            return fail(format!(
                "batch maxIterations must be 1 to {MAX_BATCH_ITERATIONS}, not {}",
                batch.max_iterations
            ));
        }
        if batch.min_iterations > MAX_BATCH_ITERATIONS {
            return fail(format!(
                "batch minIterations must be 0 to {MAX_BATCH_ITERATIONS}, not {}",
                batch.min_iterations
            ));
        }
        if batch.min_iterations > batch.max_iterations {
            return fail("minIterations cannot exceed maxIterations");
        }
        if batch.row.is_empty() || batch.row.len() > MAX_BATCH_STRIDE {
            return fail(format!(
                "a batch row holds 1 to {MAX_BATCH_STRIDE} accounts, not {}",
                batch.row.len()
            ));
        }
        let mut names = HashSet::new();
        for (name, constraint) in &batch.row {
            account(name, constraint)?;
            if !names.insert(name.as_str()) {
                return fail(format!("row account {name} is declared twice"));
            }
            if constraint.registry.is_some() {
                return fail(format!("{name}: registry accounts are fixed accounts"));
            }
        }
        if batch.row_inputs.len() > MAX_ROW_INPUTS {
            return fail(format!(
                "A batch row carries at most {MAX_ROW_INPUTS} inputs"
            ));
        }
        let mut names = HashSet::new();
        for (name, ty) in &batch.row_inputs {
            identifier(name, "row input")?;
            input_type(name, *ty)?;
            if !names.insert(name.as_str()) {
                return fail(format!("row input {name} is declared twice"));
            }
        }
        row_inputs = batch.row_inputs.len();
        stride = batch.row.len();
        max_iterations = batch.max_iterations as usize;
    }
    if template.account_groups.len() > MAX_ACCOUNT_GROUPS {
        return fail(format!(
            "A template declares at most {MAX_ACCOUNT_GROUPS} account groups"
        ));
    }
    let mut groups = HashSet::new();
    for group in &template.account_groups {
        identifier(group, "account group")?;
        if !groups.insert(group.as_str()) {
            return fail("Account group names must be unique");
        }
    }
    if template.steps.is_empty() || template.steps.len() > MAX_STEPS {
        return fail(format!(
            "A template holds 1 to {MAX_STEPS} top-level steps, not {}",
            template.steps.len()
        ));
    }
    if template.inputs.len() + row_inputs > MAX_INPUTS {
        return fail("Templates support at most 32 inputs including row inputs");
    }
    if template.inputs.len() + row_inputs * max_iterations > MAX_INPUT_VALUES {
        return fail("Fixed inputs plus row inputs times the maximum iterations exceed 256 values");
    }
    if template.accounts.len() + stride * max_iterations > MAX_RUNTIME_ACCOUNTS {
        return fail("Fixed accounts plus the maximum batch range exceeds 120 runtime accounts");
    }
    let loops: Vec<&Loop> = template.steps.iter().filter_map(is_loop).collect();
    let has_for_each = loops.iter().any(|body| body.repeat.is_none());
    if template.batch.is_some() != has_for_each {
        return fail(
            "A batch schema requires at least one top-level forEach step, and forEach requires a batch schema",
        );
    }
    if loops.len() > MAX_LOOPS {
        return fail("A template holds at most 8 top-level loops");
    }
    if template
        .steps
        .iter()
        .any(|step| matches!(step.0, StepNode::Assign { .. }))
    {
        return fail("assign is only valid inside a loop");
    }
    steps(&template.steps, &groups)
}
