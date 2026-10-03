//! Invariants checked on every run outcome, and the reference-model comparison for successful
//! runs. A hard violation returns `Err` and fails the fuzz test with a replayable seed; a model
//! divergence is collected as a soft finding (a model bug could cause it) and only reported.

use std::collections::HashSet;

use ballista_common::template::{REGISTRY_ENTRY_MAGIC, REGISTRY_ENTRY_VERSION};
use ballista_fuzz_gen::model::{self, Accounts, CpiData, InputVal, Prediction};
use ballista_fuzz_gen::scenario::{Kind, Scenario};
use ballista_fuzz_gen::template::{Program, TemplatePlan, World, ALL_PROGRAMS};
use mollusk_svm::result::types::{TransactionProgramResult, TransactionResult};
use solana_account::Account;
use solana_instruction::error::InstructionError;
use solana_pubkey::Pubkey;

use super::harness::{captured_cpis, program_pubkey, Harness, RunOutcome};

/// What a check turned up. A hard finding fails the test; a soft one is reported for triage.
#[derive(Debug, Default)]
pub struct Report {
    pub hard: Vec<String>,
    pub soft: Vec<String>,
    /// Whether the reference model predicted this run concretely and its CPIs were compared.
    pub model_compared: bool,
    /// How many CPIs the comparison matched one for one.
    pub cpis_compared: usize,
}

impl Report {
    fn hard(&mut self, message: impl Into<String>) {
        self.hard.push(message.into());
    }
    fn soft(&mut self, message: impl Into<String>) {
        self.soft.push(message.into());
    }
}

/// Runs every check against one outcome.
pub fn check(
    harness: &Harness,
    world: &World,
    plan: &TemplatePlan,
    scenario: &Scenario,
    template: &Pubkey,
    finalized_before: &Account,
    inputs: &[InputVal],
    outcome: &RunOutcome,
) -> Report {
    let mut report = Report::default();
    let result = &outcome.result;

    classify(result, &mut report);

    let succeeded = result.program_result.is_ok();

    // Template account: after any run its data, owner and length are unchanged. Its lamports may
    // change (a run never writes a finalized template, but a transfer could fund its address).
    if let Some(after) = result_account(result, template) {
        if after.data != finalized_before.data {
            report.hard(format!("finalized template data changed: {} bytes -> {}", finalized_before.data.len(), after.data.len()));
        }
        if after.owner != finalized_before.owner {
            report.hard(format!("finalized template owner changed to {}", after.owner));
        }
    }

    // Registry entries Ballista owns keep an intact header after the run.
    for spec in &scenario.pool {
        if !matches!(spec.kind, Kind::Entry { .. }) {
            continue;
        }
        let key = Pubkey::new_from_array(spec.address);
        if let Some(after) = result_account(result, &key) {
            if after.owner == Pubkey::new_from_array(world.ballista) && after.data.len() >= 8 {
                let magic = &after.data[..4];
                if magic != REGISTRY_ENTRY_MAGIC || after.data[4] != REGISTRY_ENTRY_VERSION {
                    report.hard(format!("registry entry {key} lost its header after a run"));
                }
            }
        }
    }

    if succeeded {
        lamports_conserved(outcome, &mut report);
        readonly_unchanged(outcome, &mut report);
        entries_change_only_in_own_runs(world, template, outcome, &mut report);
        opened_entries_distinct(plan, scenario, &mut report);
        output_rules(plan, scenario, template, outcome, &mut report);
        account_set_and_programs(world, scenario, template, outcome, &mut report);
        reference_model(harness, world, plan, scenario, template, inputs, outcome, &mut report);
    }

    report
}

/// A run must end in success or a documented failure, never an abort, a panic, or an access
/// violation. Compute exhaustion, instruction-trace overflow, call depth and borrow failures from
/// aliasing are legitimate (critic 3), so they are acceptable, not findings.
fn classify(result: &TransactionResult, report: &mut Report) {
    match &result.program_result {
        TransactionProgramResult::Success => {}
        TransactionProgramResult::Failure(_, _) => {
            // A `ProgramError`, including every Ballista custom code and a callee's own code.
        }
        TransactionProgramResult::UnknownError(index, error) => {
            if matches!(error, InstructionError::ProgramFailedToComplete) {
                // A VM fault: a panic, an access violation, or an unhandled abort in the program.
                report.hard(format!(
                    "instruction {index} aborted (ProgramFailedToComplete): a panic or access violation"
                ));
            } else if !acceptable_instruction_error(error) {
                report.hard(format!("instruction {index} failed with an undocumented error: {error:?}"));
            }
        }
    }
}

/// Instruction errors a well-behaved run may legitimately return, beyond a `ProgramError`.
fn acceptable_instruction_error(error: &InstructionError) -> bool {
    use InstructionError::*;
    matches!(
        error,
        Custom(_)
            | MissingRequiredSignature
            | MissingAccount
            | PrivilegeEscalation
            | ReadonlyLamportChange
            | ReadonlyDataModified
            | ExternalAccountLamportSpend
            | ExternalAccountDataModified
            | AccountDataSizeChanged
            | AccountDataTooSmall
            | AccountBorrowFailed
            | AccountBorrowOutstanding
            | ReentrancyNotAllowed
            // A callee (the probe asked to invoke a non-program, say) that is not loaded.
            | UnsupportedProgramId
            | UnsupportedSysvar
            | IllegalOwner
            | ComputationalBudgetExceeded
            | MaxAccountsDataAllocationsExceeded
            | MaxInstructionTraceLengthExceeded
            | CallDepth
            | Immutable
            | InvalidRealloc
            | ExecutableDataModified
            | ExecutableLamportChange
            | AccountNotRentExempt
            | InvalidAccountOwner
            | UnbalancedInstruction
            | InvalidError
    )
}

fn result_account<'a>(result: &'a TransactionResult, key: &Pubkey) -> Option<&'a Account> {
    result.resulting_accounts.iter().find(|(k, _)| k == key).map(|(_, account)| account)
}

/// Total lamports over every account the transaction loaded are unchanged (a cheap sanity check;
/// the runtime enforces this per instruction). Compared against the true pre-run balances the
/// harness snapshotted, since the finalized template's rent is not in the generated pool.
fn lamports_conserved(outcome: &RunOutcome, report: &mut Report) {
    // Only accounts the run transaction referenced can change; it returns exactly those in
    // `resulting_accounts`. Sum each such account's pre- and post-run balance. Accounts in the
    // store the run never touched (the creator from upload, say) appear in neither sum.
    let mut before: u128 = 0;
    let mut after: u128 = 0;
    for (key, account) in &outcome.result.resulting_accounts {
        if let Some(pre) = outcome.before_accounts.get(key) {
            before += pre.lamports as u128;
            after += account.lamports as u128;
        }
    }
    if before != after {
        report.hard(format!("lamports not conserved: {before} -> {after}"));
    }
}

/// An account the transaction marks read-only is unchanged after it: lamports, data, owner and the
/// executable flag (a sanity check; the runtime enforces it).
fn readonly_unchanged(outcome: &RunOutcome, report: &mut Report) {
    let Some(message) = &outcome.result.message else { return };
    let keys = message.account_keys();
    for (index, key) in keys.iter().enumerate() {
        if message.is_writable(index) {
            continue;
        }
        let (Some(before), Some(after)) = (outcome.before_accounts.get(key), result_account(&outcome.result, key)) else { continue };
        if before.lamports != after.lamports || before.data != after.data || before.owner != after.owner || before.executable != after.executable {
            report.hard(format!("read-only account {key} changed"));
        }
    }
}

/// A registry entry changes only in a run of its own template: every Ballista-owned entry whose
/// bytes the run changed names this template in its header. A run that invokes Ballista (a nested
/// run of possibly another template) is exempt, since that template may write its own entries.
fn entries_change_only_in_own_runs(world: &World, template: &Pubkey, outcome: &RunOutcome, report: &mut Report) {
    let ballista = Pubkey::new_from_array(world.ballista);
    if captured_cpis(outcome).iter().any(|cpi| cpi.program == ballista) {
        return;
    }
    for (key, after) in &outcome.result.resulting_accounts {
        if after.owner != ballista || !after.data.starts_with(&REGISTRY_ENTRY_MAGIC) || after.data.len() < 40 {
            continue;
        }
        let unchanged = outcome.before_accounts.get(key).is_some_and(|before| before.data == after.data && before.owner == after.owner);
        if !unchanged && after.data[8..40] != template.to_bytes() {
            report.hard(format!("entry {key} of another template changed in this template's run"));
        }
    }
}

/// On success every open ran (opens sit at the root, and a failing instruction fails the run), so
/// the entry accounts the opens named are pairwise distinct: a second open of an open entry fails.
fn opened_entries_distinct(plan: &TemplatePlan, scenario: &Scenario, report: &mut Report) {
    let mut seen = HashSet::new();
    for open in &plan.opens {
        let Some(&index) = scenario.slots.get(open.entry) else { continue };
        if !seen.insert(scenario.pool[index].address) {
            report.hard(format!("a run succeeded although two opens named one entry account (slot {})", open.entry));
        }
    }
}

/// Every account a run's CPI passes is one the run received, and every program it calls is one the
/// template declared. Ballista cannot conjure an account or call an undeclared program.
fn account_set_and_programs(
    world: &World,
    scenario: &Scenario,
    template: &Pubkey,
    outcome: &RunOutcome,
    report: &mut Report,
) {
    let mut allowed: HashSet<Pubkey> = scenario.slots.iter().map(|&index| Pubkey::new_from_array(scenario.pool[index].address)).collect();
    allowed.insert(*template);
    let programs: HashSet<Pubkey> = ALL_PROGRAMS.iter().map(|&program| program_pubkey(world, program)).collect();

    for cpi in captured_cpis(outcome) {
        if !programs.contains(&cpi.program) {
            report.hard(format!("run called an undeclared program {}", cpi.program));
        }
        for account in &cpi.accounts {
            if !allowed.contains(account) {
                report.hard(format!("run passed an account it never received: {account}"));
            }
        }
    }
}

/// The scenario as the reference model reads it: runtime accounts in slot order, pre-run state.
struct ScenarioAccounts<'a> {
    scenario: &'a Scenario,
}

impl ScenarioAccounts<'_> {
    /// The template, registry-entry and Instructions-sysvar accounts hold live data (the finalized
    /// payload, the entry header, the transaction's instructions) the generated pool does not
    /// mirror, so the model treats their data and lamports as unknown; key and owner stay
    /// predictable.
    fn live_only(&self, index: usize) -> bool {
        self.scenario
            .slots
            .get(index)
            .map(|&i| matches!(self.scenario.pool[i].kind, Kind::Template | Kind::Entry { .. } | Kind::Sysvar))
            .unwrap_or(false)
    }
}

impl Accounts for ScenarioAccounts<'_> {
    fn key(&self, index: usize) -> Option<[u8; 32]> {
        self.scenario.slots.get(index).map(|&i| self.scenario.pool[i].address)
    }
    fn owner(&self, index: usize) -> Option<[u8; 32]> {
        // Mollusk synthesizes the Instructions sysvar owned by the Sysvar program; the generated
        // pool does not know that owner, so it is unknown to the model.
        let &i = self.scenario.slots.get(index)?;
        if matches!(self.scenario.pool[i].kind, Kind::Sysvar) {
            return None;
        }
        Some(self.scenario.pool[i].owner)
    }
    fn lamports(&self, index: usize) -> Option<u64> {
        if self.live_only(index) {
            return None;
        }
        self.scenario.slots.get(index).map(|&i| self.scenario.pool[i].lamports)
    }
    fn data(&self, index: usize) -> Option<&[u8]> {
        if self.live_only(index) {
            return None;
        }
        self.scenario.slots.get(index).map(|&i| self.scenario.pool[i].data.as_slice())
    }
    fn is_writable(&self, index: usize) -> Option<bool> {
        self.scenario.slots.get(index).map(|&i| self.scenario.pool[i].writable)
    }
    fn executable(&self, index: usize) -> Option<bool> {
        self.scenario.slots.get(index).map(|&i| self.scenario.pool[i].executable)
    }
    fn count(&self) -> usize {
        self.scenario.slots.len()
    }
}

/// Compares the run's cross-program invocations with what the independent model predicts, when the
/// model ran the template with concrete control flow. Restricted to templates with no registry
/// opens (whose entry-creation CPIs the model does not predict) and at most one account group
/// (the model forwards only a single group exactly).
fn reference_model(
    harness: &Harness,
    world: &World,
    plan: &TemplatePlan,
    scenario: &Scenario,
    _template: &Pubkey,
    inputs: &[InputVal],
    outcome: &RunOutcome,
    report: &mut Report,
) {
    // The model predicts a run from the template's declarations. It cannot follow a scenario whose
    // deliberate mutation changed the account layout or iteration count, nor registry-open runs
    // (whose entry-creation CPIs it does not model), nor more than one account group.
    if !plan.opens.is_empty() || plan.groups > 1 || scenario.mutation.is_some() || !harness.has_probe {
        return;
    }
    let bytes = &plan.bytes;
    let Ok(program) = ballista_common::template::ProgramView::parse(bytes) else { return };
    if std::env::var("FV_DUMP").is_ok() {
        eprintln!("dump: iterations={} groups={} fixed={} rows={} inputs={:?}", scenario.iterations, plan.groups, plan.fixed.len(), plan.row.len(), inputs);
        for (slot, &i) in scenario.slots.iter().enumerate() {
            let spec = &scenario.pool[i];
            eprintln!("  slot {slot}: kind={:?} exec={} data_len={} writable={}", spec.kind, spec.executable, spec.data.len(), spec.writable);
        }
        for (index, record) in program.instructions.iter().enumerate() {
            eprintln!("  [{index}] op={} dst={} a={} b={} c={} imm={}", record.opcode, record.dst, record.a, record.b, record.c, record.immediate());
        }
    }
    let accounts = ScenarioAccounts { scenario };
    let prediction = model::predict(
        &program,
        &accounts,
        inputs,
        scenario.iterations,
        super::harness::CLOCK_SLOT,
        super::harness::CLOCK_TIMESTAMP,
    );
    let (expected, return_data) = match prediction {
        Prediction::Calls { cpis, return_data, .. } => (cpis, return_data),
        Prediction::Indeterminate(_) => return,
        Prediction::Fails(reason) => {
            report.soft(format!("model predicted failure ({reason}) but the run succeeded"));
            return;
        }
    };
    let captured = captured_cpis(outcome);
    report.model_compared = true;
    report.cpis_compared = expected.len().min(captured.len());
    if captured.len() != expected.len() {
        let captured_programs: Vec<_> = captured.iter().map(|cpi| program_of(world, &cpi.program)).collect();
        report.soft(format!(
            "CPI count mismatch: model {} vs run {} (iterations={}, calls={:?}, captured={:?})",
            expected.len(),
            captured.len(),
            scenario.iterations,
            plan.calls,
            captured_programs,
        ));
        return;
    }
    for (index, (want, got)) in expected.iter().zip(&captured).enumerate() {
        let want_program = Pubkey::new_from_array(want.program);
        if want_program != got.program {
            report.soft(format!("CPI {index} program: model {want_program} vs run {}", got.program));
            continue;
        }
        let want_accounts: Vec<Pubkey> = want.accounts.iter().map(|meta| Pubkey::new_from_array(meta.address)).collect();
        if want_accounts != got.accounts {
            report.soft(format!("CPI {index} accounts differ: model {want_accounts:?} vs run {:?}", got.accounts));
            continue;
        }
        if let CpiData::Concrete(data) = &want.data {
            if data != &got.data {
                report.soft(format!("CPI {index} data: model {:02x?} vs run {:02x?}", data, got.data));
            }
        }
    }
    // The run's return data, when the template sets it and no later instruction (or a wrapping
    // probe) replaces it, is exactly what the model encoded.
    if plan.sets_return_data && scenario.after.is_empty() && scenario.wrap.is_none() {
        if let CpiData::Concrete(bytes) = return_data {
            if bytes != outcome.result.return_data {
                report.soft(format!(
                    "return data: model {:02x?} vs run {:02x?}",
                    bytes, outcome.result.return_data
                ));
            }
        }
    }
    let _ = world;
}

/// The output rules (reference/language.md, Output; wire-format.md, Run event), checked against
/// the run's own `Program data:` lines — those logged while the run's invocation is innermost, not
/// a nested run's. Every `EMIT` line starts with one of the template's literal tags, never `BEV`;
/// a template that sets `PROGRAM_FLAG_EMIT_EVENT` logs exactly one 47-byte `BEV1` event, last,
/// naming this template and the row count; one that does not, logs none. Every line is at most
/// 1,024 bytes, and the run's return data too.
fn output_rules(plan: &TemplatePlan, scenario: &Scenario, template: &Pubkey, outcome: &RunOutcome, report: &mut Report) {
    use base64::Engine as _;
    let run_height = outcome.ballista_cpi_height - 1;
    let ballista = super::harness::BALLISTA_ID.to_string();
    let mut stack: Vec<(String, u32)> = Vec::new();
    let mut lines: Vec<Vec<u8>> = Vec::new();
    for line in &outcome.logs {
        if let Some(fields) = line.strip_prefix("Program data: ") {
            let innermost_is_run = stack.last().is_some_and(|(id, height)| *id == ballista && *height == run_height);
            if innermost_is_run {
                let parts: Vec<&str> = fields.split(' ').collect();
                if parts.len() != 1 {
                    report.hard(format!("a run's Program data line holds {} fields, not one", parts.len()));
                }
                match base64::engine::general_purpose::STANDARD.decode(parts[0]) {
                    Ok(bytes) => lines.push(bytes),
                    Err(_) => report.hard("a run's Program data line is not base64".to_string()),
                }
            }
            continue;
        }
        let Some(rest) = line.strip_prefix("Program ") else { continue };
        let Some((id, tail)) = rest.split_once(' ') else { continue };
        if let Some(height) = tail.strip_prefix("invoke [").and_then(|t| t.strip_suffix(']')) {
            stack.push((id.to_string(), height.parse().unwrap_or(0)));
        } else if tail == "success" || tail.starts_with("failed") {
            stack.pop();
        }
    }

    let events: Vec<&Vec<u8>> = lines.iter().filter(|bytes| bytes.starts_with(b"BEV")).collect();
    if plan.emits_event {
        if events.len() != 1 {
            report.hard(format!("a successful run of an event template logged {} run events", events.len()));
        } else {
            let event = events[0];
            if lines.last() != Some(event) {
                report.hard("the run event is not the run's last Program data line".to_string());
            }
            // The rows the run actually saw, from its own layout: a deliberate mutation can add or
            // drop a row account, so the generator's pre-mutation count is not the truth. The run
            // succeeded, so the division was exact and in range.
            let groups: usize = scenario.run_data.get(1..1 + plan.groups).map_or(0, |lengths| lengths.iter().map(|&n| n as usize).sum());
            let rows = scenario.slots.len().saturating_sub(plan.fixed.len()).saturating_sub(groups);
            let actual = if plan.row.is_empty() { 0 } else { rows / plan.row.len() };
            if actual != scenario.iterations && std::env::var("FV_DUMP").is_ok() {
                eprintln!("dump: rows {} differ from generated {} because of: {:?}", actual, scenario.iterations, scenario.mutation);
            }
            let iterations = u8::try_from(actual).unwrap_or(u8::MAX);
            if event.len() != 47
                || &event[..4] != b"BEV1"
                || event[4] != ballista_common::template::TEMPLATE_PROGRAM_VERSION
                || event[5] != iterations
                || event[15..] != template.to_bytes()
            {
                report.hard(format!("malformed run event {:02x?}", event));
            }
        }
    } else if !events.is_empty() {
        report.hard(format!("a template without the event flag logged {} BEV lines", events.len()));
    }
    for bytes in lines.iter().filter(|bytes| !bytes.starts_with(b"BEV")) {
        if bytes.len() > ballista_common::template::MAX_RETURN_DATA_LEN {
            report.hard(format!("an emit of {} bytes exceeds 1,024", bytes.len()));
        }
        if !plan.emit_tags.iter().any(|tag| bytes.starts_with(tag)) {
            report.hard(format!("an emit {:02x?} starts with none of the template's tags", &bytes[..bytes.len().min(8)]));
        }
    }
    if outcome.result.return_data.len() > ballista_common::template::MAX_RETURN_DATA_LEN {
        report.hard(format!("return data of {} bytes exceeds 1,024", outcome.result.return_data.len()));
    }
}

/// Decodes a scenario's run inputs into the model's input values, mirroring the executor's
/// decoding of the run data. Only the fixed inputs are needed; the model reads row inputs through
/// the same slice, which the harness supplies in full via [`InputVal`].
pub fn decode_inputs(plan: &TemplatePlan, scenario: &Scenario) -> Vec<InputVal> {
    // Re-decode from the run data so the model sees exactly what the program does. The run data is
    // `group lengths | fixed values | row values × iterations`.
    let mut cursor = plan.groups;
    let data = &scenario.run_data[1..]; // skip the IX_RUN discriminator
    let mut values = Vec::new();
    let mut read = |descriptors: &[ballista_fuzz_gen::template::Input], cursor: &mut usize| {
        for input in descriptors {
            if let Some(value) = decode_one(input.value_type, data, cursor) {
                values.push(value);
            }
        }
    };
    read(&plan.inputs, &mut cursor);
    for _ in 0..scenario.iterations {
        read(&plan.row_inputs, &mut cursor);
    }
    values
}

fn decode_one(value_type: u8, data: &[u8], cursor: &mut usize) -> Option<InputVal> {
    use ballista_common::template::*;
    let take = |cursor: &mut usize, n: usize| -> Option<&[u8]> {
        let slice = data.get(*cursor..*cursor + n)?;
        *cursor += n;
        Some(slice)
    };
    Some(match value_type {
        VALUE_BOOL => InputVal::Bool(take(cursor, 1)?[0] != 0),
        VALUE_U64 => InputVal::U64(u64::from_le_bytes(take(cursor, 8)?.try_into().ok()?)),
        VALUE_I64 => InputVal::I64(i64::from_le_bytes(take(cursor, 8)?.try_into().ok()?)),
        VALUE_U128 => InputVal::U128(u128::from_le_bytes(take(cursor, 16)?.try_into().ok()?)),
        VALUE_PUBKEY => InputVal::Pubkey(take(cursor, 32)?.try_into().ok()?),
        VALUE_BYTES => {
            let len = u16::from_le_bytes(take(cursor, 2)?.try_into().ok()?) as usize;
            InputVal::Bytes(take(cursor, len)?.to_vec())
        }
        _ => return None,
    })
}

/// Which concrete program a CPI's program address is, for diagnostics.
fn program_of(world: &World, address: &Pubkey) -> Option<Program> {
    ALL_PROGRAMS.into_iter().find(|&program| program_pubkey(world, program) == *address)
}
