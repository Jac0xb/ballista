//! Invariants checked on every run outcome, and the reference-model comparison. Every check is hard:
//! a violation fails the fuzz test with a replayable seed. That includes any disagreement with a
//! concrete prediction of the independent model ([`ballista_fuzz_gen::model`]): a model bug and an
//! executor bug look the same from here, and either is worth a failing seed.
//!
//! What a failed run may fail with:
//! - never an abort, a panic or an access violation ([`classify`]);
//! - never one of the runtime's account-rule errors in Ballista's own frame
//!   ([`ballista_enforcement_error`]): Ballista asking for a privilege it was not granted, or
//!   touching an account it may not;
//! - never a structural Ballista error from a template the verifier accepted ([`structural`], P40).

use std::collections::{HashMap, HashSet};

use ballista_common::template::*;
use ballista_fuzz_gen::model::{self, Accounts, CpiData, ExpectedMeta, InputVal, Prediction};
use ballista_fuzz_gen::scenario::{Kind, Scenario};
use ballista_fuzz_gen::template::{probe, Program, TemplatePlan, World, ALL_PROGRAMS};
use mollusk_svm::result::types::{TransactionProgramResult, TransactionResult};
use solana_account::Account;
use solana_instruction::error::InstructionError;
use solana_program_error::ProgramError;
use solana_pubkey::Pubkey;

use super::harness::{
    captured_cpis, log_trace, program_pubkey, Harness, LogTrace, RunOutcome, BALLISTA_ID, PROBE_COPY_ID, PROBE_ID,
};

/// What a check turned up, and what it actually compared, for the loop's floors.
#[derive(Debug, Default)]
pub struct Report {
    pub hard: Vec<String>,
    pub compared: Compared,
    /// Why the model did not compare a successful run, if it did not.
    pub skipped: Option<String>,
}

/// How much of one run the oracles checked. The fuzz loop sums these and fails when a sum falls
/// under its floor, so a generator or model change that quietly stops the comparisons fails too.
#[derive(Clone, Copy, Debug, Default)]
pub struct Compared {
    /// The model predicted the successful run concretely and every CPI was compared.
    pub run: bool,
    /// CPIs compared by program and accounts.
    pub cpis: usize,
    /// CPIs whose data the model knew and compared byte for byte.
    pub data: usize,
    /// Probe CPIs whose received signer and writable flags were compared.
    pub flags: usize,
    /// EMIT lines compared byte for byte.
    pub emits: usize,
    /// Return data compared byte for byte.
    pub return_data: bool,
    /// Loop passes the model ran on a compared run (each restores or carries registers).
    pub loop_passes: usize,
    /// The model predicted a failure from concrete values, and the run failed.
    pub predicted_failure: bool,
    /// A failure with an in-run Ballista code, classified as structural or value-dependent.
    pub classified_failure: bool,
}

impl Report {
    fn hard(&mut self, message: impl Into<String>) {
        self.hard.push(message.into());
    }
}

/// Runs every check against one outcome.
#[allow(clippy::too_many_arguments)]
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
    let trace = log_trace(outcome);

    classify(result, &mut report);
    ballista_enforcement_error(outcome, &trace, &mut report);
    structural_failure(plan, outcome, &trace, &mut report);

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
    }
    reference_model(harness, world, plan, scenario, inputs, outcome, &trace, &mut report);

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
            // `structural_failure` checks Ballista's codes.
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

/// Instruction errors a well-behaved run may legitimately return, beyond a `ProgramError`. The
/// account-rule errors here are acceptable only from another program's frame; see
/// [`ballista_enforcement_error`].
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

/// The runtime's errors for breaking an account rule: asking a CPI for a signer or writable
/// privilege the caller lacks (`PrivilegeEscalation`), and changing an account's data, lamports
/// or size without the right to. Another program may raise them, the probe or the System program
/// refusing what a template asked of it. In Ballista's own frame they mean a run asked for a
/// privilege its template's verified declarations and the run's account checks should have ruled
/// out (the CPI privilege ceiling, `trust-model.md#privileges`), or touched an account it must not:
/// Ballista writes only the registry entries it opened.
fn ballista_enforcement_error(outcome: &RunOutcome, trace: &LogTrace, report: &mut Report) {
    use InstructionError::*;
    let TransactionProgramResult::UnknownError(index, error) = &outcome.result.program_result else { return };
    let enforcement = matches!(
        error,
        PrivilegeEscalation
            | ReadonlyDataModified
            | ReadonlyLamportChange
            | ExternalAccountDataModified
            | ExternalAccountLamportSpend
            | ExecutableDataModified
            | ExecutableLamportChange
            | UnbalancedInstruction
    );
    if !enforcement || *index != outcome.top_index {
        return;
    }
    if let Some((program, height, message)) = &trace.first_failure {
        if *program == BALLISTA_ID {
            report.hard(format!("Ballista itself raised {error:?} (stack height {height}): {message}"));
        }
    }
}

/// P40: a template the verifier accepted never fails for a structural reason, one the verifier's
/// checks exclude, only for a value-dependent one (what a run meets: register values, accounts and
/// data, what a callee returns). A structural error means the verifier and the executor disagree.
///
/// The split is the documented one in `certora/ballista-specs/src/rules/oracle.rs`, kept here as
/// the fuzzer's own list so neither oracle silently follows the other:
///
/// | Kind | Structural when |
/// | --- | --- |
/// | 6002 `InvalidTemplateProgram`, 6011 `InvalidRegister`, 6016 `CpiDataTooLarge` | always |
/// | 6012 `TypeMismatch` | the failing instruction does not decode a `bool` from bytes |
/// | 6009 `InvalidRuntimeAccount` | not a typed account read at a dynamic offset, and not a typed read at a fixed offset after the run made a CPI |
/// | 6013-6015, 6017-6019, 6021-6026 | never (value-dependent) |
/// | 6000, 6001, 6003-6008, 6010, 6020 | not checked: raised before or outside execution, with an index, not a pc |
///
/// The one refinement over oracle.rs, whose split assumes the state a run validated before its
/// first instruction: a fixed-offset read is in bounds because validation checked the declared
/// minimum length, but a callee that owns the account may shrink it in a CPI (the probe's
/// `RESIZE_FIRST` does), after which the read legitimately fails (critic: "post-CPI
/// realloc/close/reassign"). So after any CPI that error counts as value-dependent.
fn structural(program: &ProgramView, kind: u32, pc: usize, made_cpi: bool) -> bool {
    let instruction = program.instructions.get(pc);
    match kind {
        6002 | 6011 | 6016 => true,
        6012 => !instruction.is_some_and(decodes_a_bool),
        6009 => match instruction {
            Some(record) if typed_account_read(record.opcode) => {
                record.flags & INSTRUCTION_FLAG_DYNAMIC_OFFSET == 0 && !made_cpi
            }
            Some(_) => true,
            None => false,
        },
        _ => false,
    }
}

/// The typed reads of account data.
fn typed_account_read(opcode: u8) -> bool {
    matches!(
        opcode,
        OP_READ_U8 | OP_READ_U16 | OP_READ_U32 | OP_READ_U64 | OP_READ_I64 | OP_READ_I32 | OP_READ_U128 | OP_READ_PUBKEY | OP_READ_BOOL
    )
}

/// Whether `instruction` decodes a `bool` from bytes, which fails on any byte but 0 and 1: a typed
/// account read, and a return-data, instruction-data or registry-field read whose read opcode is
/// `READ_BOOL` (the operand `a`, the immediate, and byte 2 of the immediate, respectively).
fn decodes_a_bool(instruction: &InstructionRecord) -> bool {
    match instruction.opcode {
        OP_READ_BOOL => true,
        OP_RETURN_DATA => instruction.a == OP_READ_BOOL,
        OP_READ_INSTRUCTION_DATA => instruction.immediate() == u64::from(OP_READ_BOOL),
        OP_READ_REGISTRY => (instruction.immediate() >> 16) as u8 == OP_READ_BOOL,
        _ => false,
    }
}

fn structural_failure(plan: &TemplatePlan, outcome: &RunOutcome, trace: &LogTrace, report: &mut Report) {
    let TransactionProgramResult::Failure(index, ProgramError::Custom(raw)) = &outcome.result.program_result else { return };
    let (kind, context) = (raw & 0xffff, (raw >> 16) as usize);
    if *index != outcome.top_index || !(6000..=6026).contains(&kind) {
        return;
    }
    if matches!(kind, 6000 | 6001 | 6003..=6008 | 6010 | 6020) {
        return;
    }
    report.compared.classified_failure = true;
    let Ok(program) = ProgramView::parse(&plan.bytes) else { return };
    if structural(&program, kind, context, trace.made_cpi) {
        let opcode = program.instructions.get(context).map(|record| record.opcode);
        report.hard(format!(
            "a verified template failed with structural error {kind} at pc {context} (opcode {opcode:?}, made a CPI: {})",
            trace.made_cpi
        ));
    }
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

/// The scenario as the reference model reads it: runtime accounts in slot order, in the state the
/// run meets them, with the flags Ballista sees.
struct ScenarioAccounts<'a> {
    scenario: &'a Scenario,
    /// Accounts a probe instruction before the run may have changed: the model treats their data
    /// and lamports as unknown.
    dirty: HashSet<[u8; 32]>,
    /// Ballista's own view of each runtime account's writable flag.
    writable: Vec<bool>,
}

impl<'a> ScenarioAccounts<'a> {
    fn new(scenario: &'a Scenario, outcome: &RunOutcome) -> Self {
        let mut dirty = HashSet::new();
        for extra in &scenario.before {
            // NOOP changes nothing; WRITE_FIRST and RESIZE_FIRST change the first account's data,
            // TRANSFER the first two accounts' lamports.
            if extra.data.first().is_some_and(|&op| op != probe::NOOP) {
                for &index in extra.accounts.iter().take(2) {
                    dirty.insert(scenario.pool[index].address);
                }
            }
        }
        let writable = ballista_writable(scenario, outcome);
        Self { scenario, dirty, writable }
    }

    /// The template and the Instructions sysvar hold live data (the finalized payload, the
    /// transaction's instructions) the generated pool does not mirror, so the model treats their
    /// data and lamports as unknown; key and owner stay predictable. So does an account a probe
    /// instruction before the run touched.
    fn unknown(&self, index: usize) -> bool {
        self.scenario
            .slots
            .get(index)
            .map(|&i| {
                matches!(self.scenario.pool[i].kind, Kind::Template | Kind::Sysvar) || self.dirty.contains(&self.scenario.pool[i].address)
            })
            .unwrap_or(false)
    }
}

/// Ballista's view of each runtime account's writable flag. A top-level run sees the transaction's
/// flag. A wrapped run sees what the probe forwarded: the transaction's flag, cleared where the
/// probe's policy demotes that forwarded position, and merged over every forwarded copy of the same
/// address, as the runtime merges a CPI's duplicate accounts.
fn ballista_writable(scenario: &Scenario, outcome: &RunOutcome) -> Vec<bool> {
    let pool = |index: usize| &scenario.pool[index];
    let policy = match (outcome.wrapped, scenario.wrap) {
        (true, Some(policy)) => policy,
        _ => return scenario.slots.iter().map(|&index| pool(index).writable).collect(),
    };
    // The probe forwards the template first, then every runtime account; `reinvoke` demotes
    // forwarded position `n` when bit `n` of the policy is set, for `n` below 8.
    let forwarded: Vec<usize> = std::iter::once(scenario.template).chain(scenario.slots.iter().copied()).collect();
    let mut by_address: HashMap<[u8; 32], bool> = HashMap::new();
    for (position, &index) in forwarded.iter().enumerate() {
        let demoted = position < 8 && policy & (1 << position) != 0;
        *by_address.entry(pool(index).address).or_default() |= pool(index).writable && !demoted;
    }
    scenario.slots.iter().map(|&index| by_address[&pool(index).address]).collect()
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
        if self.unknown(index) {
            return None;
        }
        self.scenario.slots.get(index).map(|&i| self.scenario.pool[i].lamports)
    }
    fn data(&self, index: usize) -> Option<&[u8]> {
        if self.unknown(index) {
            return None;
        }
        self.scenario.slots.get(index).map(|&i| self.scenario.pool[i].data.as_slice())
    }
    fn is_writable(&self, index: usize) -> Option<bool> {
        self.writable.get(index).copied()
    }
    fn executable(&self, index: usize) -> Option<bool> {
        self.scenario.slots.get(index).map(|&i| self.scenario.pool[i].executable)
    }
    fn count(&self) -> usize {
        self.scenario.slots.len()
    }
}

/// The flags the callee sees for each of a CPI's accounts: the runtime merges a CPI's duplicate
/// accounts, so every copy of an address carries the union of the flags its metas asked for.
fn received_flags(metas: &[ExpectedMeta]) -> Vec<u8> {
    let mut merged: HashMap<[u8; 32], u8> = HashMap::new();
    for meta in metas {
        *merged.entry(meta.address).or_default() |= u8::from(meta.signer) | 2 * u8::from(meta.writable);
    }
    metas.iter().map(|meta| merged[&meta.address]).collect()
}

/// Compares the run with what the independent model predicts, whenever the model can follow the
/// run with concrete control flow. Not for templates with registry opens (the model does not
/// predict an entry's creation calls) or for scenarios with a deliberate mutation (which change the
/// account layout or the run data the model reads).
///
/// A successful run must match a [`Prediction::Calls`] exactly: the same CPIs in the same order,
/// each to the same program with the same accounts, the same data where the model knows it, and,
/// for a probe call, the same flags as the probe logged receiving; and the same return data. A
/// [`Prediction::Fails`] must meet a failed run.
#[allow(clippy::too_many_arguments)]
fn reference_model(
    harness: &Harness,
    world: &World,
    plan: &TemplatePlan,
    scenario: &Scenario,
    inputs: &[InputVal],
    outcome: &RunOutcome,
    trace: &LogTrace,
    report: &mut Report,
) {
    let succeeded = outcome.result.program_result.is_ok();
    let skip = |report: &mut Report, reason: &str| {
        if succeeded {
            report.skipped = Some(reason.to_string());
        }
    };
    if !harness.has_probe {
        return skip(report, "no probe");
    }
    if !plan.opens.is_empty() {
        return skip(report, "registry opens");
    }
    if scenario.mutation.is_some() {
        return skip(report, "mutation");
    }
    let Ok(program) = ProgramView::parse(&plan.bytes) else { return skip(report, "unparsable") };
    if std::env::var("FV_DUMP").is_ok() {
        eprintln!("dump: iterations={} groups={:?} fixed={} rows={} inputs={:?}", scenario.iterations, scenario.group_lengths, plan.fixed.len(), plan.row.len(), inputs);
        for (slot, &i) in scenario.slots.iter().enumerate() {
            let spec = &scenario.pool[i];
            eprintln!("  slot {slot}: kind={:?} exec={} data_len={} writable={} signer={}", spec.kind, spec.executable, spec.data.len(), spec.writable, spec.signer);
        }
        for (index, record) in program.instructions.iter().enumerate() {
            eprintln!("  [{index}] op={} dst={} a={} b={} c={} flags={} imm={}", record.opcode, record.dst, record.a, record.b, record.c, record.flags, record.immediate());
        }
    }
    let accounts = ScenarioAccounts::new(scenario, outcome);
    let prediction = model::predict(
        &program,
        &accounts,
        inputs,
        scenario.iterations,
        &scenario.group_lengths,
        super::harness::CLOCK_SLOT,
        super::harness::CLOCK_TIMESTAMP,
    );
    let (expected, emits, return_data, loop_passes) = match prediction {
        Prediction::Calls { cpis, emits, return_data, loop_passes, .. } => (cpis, emits, return_data, loop_passes),
        Prediction::Indeterminate(reason) => return skip(report, reason),
        Prediction::Fails(reason) => {
            if succeeded {
                report.hard(format!("the model predicted a failure ({reason}) but the run succeeded"));
            } else {
                report.compared.predicted_failure = true;
            }
            return;
        }
    };
    if !succeeded {
        // The model predicts only what the template computes; a run also fails for reasons it does
        // not follow (account validation, a callee's refusal, compute), so a failure is no
        // disagreement here. Its code was classified above.
        return;
    }
    let captured = captured_cpis(outcome);
    if captured.len() != expected.len() {
        let captured_programs: Vec<_> = captured.iter().map(|cpi| program_of(world, &cpi.program)).collect();
        report.hard(format!(
            "CPI count: model {} vs run {} (iterations={}, calls={:?}, captured={:?})",
            expected.len(),
            captured.len(),
            scenario.iterations,
            plan.calls,
            captured_programs,
        ));
        return;
    }
    let mut probe_calls = trace.probe_flags.iter();
    for (index, (want, got)) in expected.iter().zip(&captured).enumerate() {
        let want_program = Pubkey::new_from_array(want.program);
        if want_program != got.program {
            report.hard(format!("CPI {index} program: model {want_program} vs run {}", got.program));
            continue;
        }
        let want_accounts: Vec<Pubkey> = want.accounts.iter().map(|meta| Pubkey::new_from_array(meta.address)).collect();
        if want_accounts != got.accounts {
            report.hard(format!("CPI {index} accounts: model {want_accounts:?} vs run {:?}", got.accounts));
            continue;
        }
        if let CpiData::Concrete(data) = &want.data {
            if data != &got.data {
                report.hard(format!("CPI {index} data: model {:02x?} vs run {:02x?}", data, got.data));
            }
            report.compared.data += 1;
        }
        if got.program == PROBE_ID || got.program == PROBE_COPY_ID {
            let want_flags = received_flags(&want.accounts);
            match probe_calls.next() {
                Some(flags) if *flags == want_flags => report.compared.flags += 1,
                Some(flags) => report.hard(format!(
                    "CPI {index} flags (bit 0 signer, bit 1 writable): model {want_flags:?} vs probe received {flags:?}"
                )),
                None => report.hard(format!("CPI {index} to the probe logged no flags")),
            }
        }
        report.compared.cpis += 1;
    }
    // Every EMIT line, in order: the run's own Program data lines other than the run event.
    let (lines, _) = run_data_lines(outcome);
    let logged: Vec<&Vec<u8>> = lines.iter().filter(|line| !line.starts_with(b"BEV")).collect();
    if logged.len() != emits.len() {
        report.hard(format!("EMIT count: model {} vs run {}", emits.len(), logged.len()));
    } else {
        for (index, (want, got)) in emits.iter().zip(logged).enumerate() {
            if let CpiData::Concrete(bytes) = want {
                if bytes != got {
                    report.hard(format!("EMIT {index}: model {bytes:02x?} vs run {got:02x?}"));
                }
                report.compared.emits += 1;
            }
        }
    }
    // The run's return data, when the template sets it and no later instruction (or a wrapping
    // probe) replaces it, is exactly what the model encoded.
    if plan.sets_return_data && scenario.after.is_empty() && !outcome.wrapped {
        if let CpiData::Concrete(bytes) = return_data {
            if bytes != outcome.result.return_data {
                report.hard(format!("return data: model {:02x?} vs run {:02x?}", bytes, outcome.result.return_data));
            }
            report.compared.return_data = true;
        }
    }
    report.compared.run = true;
    report.compared.loop_passes = loop_passes;
}

/// The output rules (reference/language.md, Output; wire-format.md, Run event), checked against
/// the run's own `Program data:` lines — those logged while the run's invocation is innermost, not
/// a nested run's. Every `EMIT` line starts with one of the template's literal tags, never `BEV`;
/// a template that sets `PROGRAM_FLAG_EMIT_EVENT` logs exactly one 47-byte `BEV1` event, last,
/// naming this template and the row count; one that does not, logs none. Every line is at most
/// 1,024 bytes, and the run's return data too.
fn output_rules(plan: &TemplatePlan, scenario: &Scenario, template: &Pubkey, outcome: &RunOutcome, report: &mut Report) {
    let (lines, malformed) = run_data_lines(outcome);
    for problem in malformed {
        report.hard(problem);
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
                report.hard(format!(
                    "malformed run event {:02x?} (expected rows {iterations}, template {:02x?}, len {}, wrap {:?})",
                    event,
                    &template.to_bytes()[..4],
                    event.len(),
                    outcome.wrapped
                ));
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

/// The run's own `Program data:` lines, decoded, in order: those logged while the run's invocation
/// is innermost, not a nested run's. Also what is malformed about any of them.
fn run_data_lines(outcome: &RunOutcome) -> (Vec<Vec<u8>>, Vec<String>) {
    use base64::Engine as _;
    let run_height = outcome.ballista_cpi_height - 1;
    let ballista = super::harness::BALLISTA_ID.to_string();
    let mut stack: Vec<(String, u32)> = Vec::new();
    let mut lines: Vec<Vec<u8>> = Vec::new();
    let mut malformed = Vec::new();
    for line in &outcome.logs {
        if let Some(fields) = line.strip_prefix("Program data: ") {
            let innermost_is_run = stack.last().is_some_and(|(id, height)| *id == ballista && *height == run_height);
            if innermost_is_run {
                let parts: Vec<&str> = fields.split(' ').collect();
                if parts.len() != 1 {
                    malformed.push(format!("a run's Program data line holds {} fields, not one", parts.len()));
                }
                match base64::engine::general_purpose::STANDARD.decode(parts[0]) {
                    Ok(bytes) => lines.push(bytes),
                    Err(_) => malformed.push("a run's Program data line is not base64".to_string()),
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
    (lines, malformed)
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
