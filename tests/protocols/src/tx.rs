//! Sign and send transactions, keep them within the packet size, and name the program that failed
//! and, when it was Ballista, the step.

use {
    crate::template::Example,
    ballista_sdk::decode_ballista_error,
    base64::{engine::general_purpose::STANDARD as BASE64, Engine},
    litesvm::{types::FailedTransactionMetadata, LiteSVM},
    solana_address::Address,
    solana_instruction::Instruction,
    solana_keypair::Keypair,
    solana_message::{v0, AddressLookupTableAccount, Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::{versioned::VersionedTransaction, InstructionError, TransactionError},
    std::{fmt, str::FromStr},
};

/// The most a transaction may take on the wire (`PACKET_DATA_SIZE`). LiteSVM does not enforce it,
/// so [`send`] does.
pub const PACKET_DATA_SIZE: usize = 1_232;

/// A transaction that landed.
pub struct Outcome {
    /// The runtime's log lines, as plain text.
    pub logs: Vec<String>,
    /// Compute units the whole transaction consumed.
    pub compute_units: u64,
    /// What the fee payer was charged, priority fee included.
    pub fee: u64,
    /// The signed transaction's size on the wire, in bytes.
    pub size: usize,
    /// The return data the transaction ended with.
    pub return_data: ReturnData,
}

/// A transaction's return data: what the last program to set it set. The runtime clears it as
/// each instruction starts, so it is the last instruction's, or empty.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReturnData {
    pub program: Address,
    pub data: Vec<u8>,
}

/// One `Program data:` line, as `sol_log_data` logs it: which program logged it, how deep in the
/// call stack that program ran, and the fields, decoded.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProgramData {
    pub program: Address,
    /// The invocation's stack height: 1 for a transaction's own instruction, 2 for a program it
    /// calls, and so on.
    pub height: usize,
    /// Each field of the line: Ballista's `EMIT` logs exactly one.
    pub fields: Vec<Vec<u8>>,
}

/// Every `Program data:` line in `logs`, attributed by following the `invoke` and `success` or
/// `failed` lines around it.
///
/// # Panics
///
/// If the logs do not nest: a line outside any invocation, a height that skips a level, or an
/// invalid base64 field.
pub fn program_data(logs: &[String]) -> Vec<ProgramData> {
    let mut stack: Vec<Address> = Vec::new();
    let mut logged = Vec::new();
    for line in logs {
        let Some(rest) = line.strip_prefix("Program ") else {
            continue;
        };
        if let Some(fields) = rest.strip_prefix("data: ") {
            let program = *stack
                .last()
                .unwrap_or_else(|| panic!("{line:?} is outside every invocation"));
            let fields = fields
                .split(' ')
                .map(|field| {
                    BASE64
                        .decode(field)
                        .unwrap_or_else(|error| panic!("{line:?} is not base64: {error}"))
                })
                .collect();
            logged.push(ProgramData {
                program,
                height: stack.len(),
                fields,
            });
        } else if let Some((program, height)) = invocation(rest) {
            assert_eq!(height, stack.len() + 1, "{line:?} skips a level");
            stack.push(program);
        } else if let Some((program, outcome)) = rest.split_once(' ') {
            let ended = outcome == "success" || outcome.starts_with("failed: ");
            if ended && Address::from_str(program).is_ok() {
                stack.pop();
            }
        }
    }
    logged
}

/// `<program> invoke [<height>]`, the rest of an invocation's first line.
fn invocation(rest: &str) -> Option<(Address, usize)> {
    let (program, height) = rest.split_once(" invoke [")?;
    let height = height.strip_suffix(']')?.parse().ok()?;
    Some((Address::from_str(program).ok()?, height))
}

/// The deepest stack height any program was invoked at in `logs`: 1 when no instruction calls
/// another program.
pub fn deepest_invocation(logs: &[String]) -> usize {
    logs.iter()
        .filter_map(|line| invocation(line.strip_prefix("Program ")?))
        .map(|(_, height)| height)
        .max()
        .unwrap_or(0)
}

impl Outcome {
    /// The compute units `program` consumed in its most expensive invocation, CPIs it made
    /// included: the largest of its `consumed` lines, which for a program called once is its
    /// outermost invocation's. `None` if it never ran.
    pub fn compute_units_of(&self, program: &Address) -> Option<u64> {
        units_of(&self.logs, program)
    }

    /// The compute units `program`'s outermost invocation spent on its own work: its `consumed`
    /// line less those of the programs it called directly. What a call costs its caller, such as
    /// passing the accounts, stays in the caller's share, and so does a builtin callee, which logs
    /// no `consumed` line. `None` if it never ran.
    pub fn own_compute_units_of(&self, program: &Address) -> Option<u64> {
        own_units_of(&self.logs, program)
    }
}

impl Failure {
    /// [`Outcome::compute_units_of`], for a failed transaction: what `program` consumed before it
    /// failed or its callee did.
    pub fn compute_units_of(&self, program: &Address) -> Option<u64> {
        units_of(&self.logs, program)
    }
}

/// The largest of `program`'s `consumed` lines in `logs`.
fn units_of(logs: &[String], program: &Address) -> Option<u64> {
    let prefix = format!("Program {program} consumed ");
    logs.iter()
        .filter_map(|line| {
            line.strip_prefix(&prefix)?
                .split_once(" of ")?
                .0
                .parse()
                .ok()
        })
        .max()
}

/// See [`Outcome::own_compute_units_of`].
fn own_units_of(logs: &[String], program: &Address) -> Option<u64> {
    let mut stack: Vec<Address> = Vec::new();
    // The stack height of `program`'s outermost invocation, once it has started.
    let mut outermost = None;
    let mut callees = 0;
    for line in logs {
        let Some(rest) = line.strip_prefix("Program ") else {
            continue;
        };
        if let Some((invoked, height)) = invocation(rest) {
            stack.push(invoked);
            if outermost.is_none() && invoked == *program {
                outermost = Some(height);
            }
        } else if let Some((consumer, units)) = rest.split_once(" consumed ") {
            // A program's own `Program log:` lines can contain the word too.
            let (Ok(consumer), Some(height)) = (Address::from_str(consumer), outermost) else {
                continue;
            };
            let units: u64 = units.split_once(" of ")?.0.parse().ok()?;
            if stack.len() == height + 1 {
                callees += units;
            } else if stack.len() == height && consumer == *program {
                return Some(units - callees);
            }
        } else if let Some((ended, outcome)) = rest.split_once(' ') {
            if (outcome == "success" || outcome.starts_with("failed: "))
                && Address::from_str(ended).is_ok()
            {
                stack.pop();
            }
        }
    }
    None
}

/// A transaction that failed in a program.
pub struct Failure {
    /// The innermost program that failed. When a CPI fails, every caller up the stack logs its
    /// own `failed` line repeating the callee's code, so the first such line names the program
    /// the code belongs to. Ballista's own codes (6000 and up) collide with other programs', so a
    /// code means nothing without this.
    pub program: Address,
    /// The program's custom error code, if it returned one.
    pub code: Option<u32>,
    /// The error as the runtime reported it for the transaction.
    pub err: TransactionError,
    /// The runtime's log lines, as plain text.
    pub logs: Vec<String>,
    /// A failed transaction still pays its fee.
    pub fee: u64,
}

/// Builds and signs a transaction for the SVM's current blockhash: a v0 message that looks up
/// keys in `tables` when any are given, a legacy message otherwise.
///
/// `payer` pays and signs first. `signers` holds every other signature the instructions need;
/// repeating the payer there is harmless.
///
/// # Panics
///
/// If the message cannot be compiled, or the keypairs are not exactly its signers.
pub fn transaction(
    svm: &LiteSVM,
    payer: &Keypair,
    signers: &[&Keypair],
    instructions: &[Instruction],
    tables: &[AddressLookupTableAccount],
) -> VersionedTransaction {
    let blockhash = svm.latest_blockhash();
    let payer_address = payer.pubkey();
    let message = if tables.is_empty() {
        VersionedMessage::Legacy(Message::new_with_blockhash(
            instructions,
            Some(&payer_address),
            &blockhash,
        ))
    } else {
        VersionedMessage::V0(
            v0::Message::try_compile(&payer_address, instructions, tables, blockhash)
                .unwrap_or_else(|error| panic!("compiling the v0 message failed: {error:?}")),
        )
    };
    let mut keypairs = vec![payer];
    for &signer in signers {
        if keypairs.iter().all(|kept| kept.pubkey() != signer.pubkey()) {
            keypairs.push(signer);
        }
    }
    VersionedTransaction::try_new(message, &keypairs).unwrap_or_else(|error| {
        panic!("signing failed ({error:?}): the keypairs must be exactly the message's signers")
    })
}

/// A signed transaction's size on the wire, in bytes.
pub fn wire_size(transaction: &VersionedTransaction) -> usize {
    let size = wincode::serialized_size(transaction).expect("a signed transaction serializes");
    usize::try_from(size).expect("the size fits in usize")
}

/// Signs `instructions` as [`transaction`] does and sends them.
///
/// The blockhash is expired first. LiteSVM's never advances on its own, so without that a second
/// identical transaction would be rejected as `AlreadyProcessed`.
///
/// # Panics
///
/// - If the signed transaction is larger than [`PACKET_DATA_SIZE`].
/// - If it fails without an instruction error: the fee payer cannot pay, an account would be left
///   below rent exemption, and the like. No scenario means to provoke those.
pub fn send(
    svm: &mut LiteSVM,
    payer: &Keypair,
    signers: &[&Keypair],
    instructions: &[Instruction],
    tables: &[AddressLookupTableAccount],
) -> Result<Outcome, Failure> {
    svm.expire_blockhash();
    let transaction = transaction(svm, payer, signers, instructions, tables);
    let size = wire_size(&transaction);
    assert!(
        size <= PACKET_DATA_SIZE,
        "the transaction is {size} bytes; at most {PACKET_DATA_SIZE} fit in a packet"
    );
    match svm.send_transaction(transaction) {
        Ok(meta) => Ok(Outcome {
            logs: meta.logs,
            compute_units: meta.compute_units_consumed,
            fee: meta.fee,
            size,
            return_data: ReturnData {
                program: meta.return_data.program_id,
                data: meta.return_data.data,
            },
        }),
        Err(failed) => Err(Failure::new(failed, instructions)),
    }
}

/// The name and context of a failure Ballista raised itself: `("RequirementFailed", pc)`, say.
///
/// `None` when another program failed, even one whose code falls in Ballista's range, and when
/// Ballista failed without a custom code of its own, such as a missing signature.
pub fn ballista_error(failure: &Failure) -> Option<(&'static str, u16)> {
    if failure.program != ballista_sdk::ID {
        return None;
    }
    let decoded = decode_ballista_error(failure.code?)?;
    Some((decoded.name, decoded.context))
}

/// Ballista's failures whose context is always the program counter of the instruction that failed.
/// The others carry an account or input index or a count (`InvalidRunInputs`,
/// `InvalidAccountRange`, `AccountConstraintFailed`, `CpiAccountLimitExceeded`), one or the other
/// (`InvalidTemplateProgram`), or nothing, being raised outside a run.
const FAILURES_AT_A_PC: [&str; 11] = [
    "InvalidRuntimeAccount",
    "InvalidRegister",
    "TypeMismatch",
    "ArithmeticOverflow",
    "DivisionByZero",
    "RequirementFailed",
    "CpiDataTooLarge",
    "InvalidPdaDerivation",
    "MissingReturnData",
    "ReturnDataMismatch",
    "LoopCountExceeded",
];

/// Asserts that Ballista itself failed with `kind` in the step `example` labels `label`.
///
/// The failure's program counter is looked up in the example's labels; a label is never turned
/// into a pc, since a step spans several.
///
/// # Panics
///
/// If the failure is another, or if `kind`'s context is not a program counter, so that no step
/// label can match it.
#[track_caller]
pub fn assert_ballista_failure(failure: &Failure, example: &Example, kind: &str, label: &str) {
    assert!(
        FAILURES_AT_A_PC.contains(&kind),
        "{kind} does not carry a program counter, so no step label can match it; \
         compare `ballista_error` with its context instead"
    );
    assert!(
        example.labels.values().any(|labelled| labelled == label),
        "the template has no step labelled {label:?}"
    );
    let Some((name, pc)) = ballista_error(failure) else {
        panic!("expected Ballista's {kind} in {label:?}, but {failure:?}");
    };
    let step = example.label_at(pc);
    assert!(
        name == kind && step == Some(label),
        "expected Ballista's {kind} in {label:?}, but it failed with {name} at pc {pc}, {}\n{failure:?}",
        step.map_or_else(
            || "an unlabelled instruction".to_string(),
            |step| format!("in {step:?}")
        )
    );
}

/// Asserts that the requirement `example` labels `label` failed; see [`assert_ballista_failure`].
#[track_caller]
pub fn assert_requirement_failed(failure: &Failure, example: &Example, label: &str) {
    assert_ballista_failure(failure, example, "RequirementFailed", label);
}

impl Failure {
    fn new(failed: FailedTransactionMetadata, instructions: &[Instruction]) -> Self {
        let FailedTransactionMetadata { err, meta } = failed;
        // Nothing is logged past the limit, so truncated logs may hold none of the failure's lines,
        // and the fallback below would blame the top-level instruction's program.
        assert!(
            !meta.logs.iter().any(|line| line == "Log truncated"),
            "the logs were truncated, so they cannot say which program failed; build the SVM with \
             `with_log_bytes_limit(None)`\n{}",
            meta.logs.join("\n")
        );
        let (program, code) = match innermost_failure(&meta.logs) {
            Some(failure) => failure,
            None => {
                // No `failed` line: either the runtime rejected the instruction before its program
                // ran, or the program logs nothing, as a precompile such as Ed25519 does. The
                // instruction index then names the program.
                let TransactionError::InstructionError(index, error) = &err else {
                    panic!(
                        "the transaction failed outside any program: {err:?}\n{}",
                        meta.logs.join("\n")
                    );
                };
                let code = match error {
                    InstructionError::Custom(code) => Some(*code),
                    _ => None,
                };
                (instructions[usize::from(*index)].program_id, code)
            }
        };
        Self {
            program,
            code,
            err,
            logs: meta.logs,
            fee: meta.fee,
        }
    }
}

/// The first `Program <id> failed: <error>` line, which belongs to the innermost program that
/// failed, with its custom code if the error is one.
fn innermost_failure(logs: &[String]) -> Option<(Address, Option<u32>)> {
    logs.iter().find_map(|line| {
        let (program, error) = line.strip_prefix("Program ")?.split_once(" failed: ")?;
        // `Program log: ... failed: ...` does not name a program.
        let program = Address::from_str(program).ok()?;
        let code = error
            .strip_prefix("custom program error: 0x")
            .and_then(|hex| u32::from_str_radix(hex, 16).ok());
        Some((program, code))
    })
}

impl fmt::Debug for Outcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            formatter,
            "landed: {} CU, {} bytes, fee {}",
            self.compute_units, self.size, self.fee
        )?;
        write_logs(formatter, &self.logs)
    }
}

impl fmt::Debug for Failure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} failed", self.program)?;
        if let Some(code) = self.code {
            write!(formatter, " with code {code} ({code:#x})")?;
        }
        if let Some((name, context)) = ballista_error(self) {
            write!(formatter, ", Ballista's {name} at {context}")?;
        }
        writeln!(formatter, ": {:?}", self.err)?;
        write_logs(formatter, &self.logs)
    }
}

fn write_logs(formatter: &mut fmt::Formatter<'_>, logs: &[String]) -> fmt::Result {
    logs.iter()
        .try_for_each(|line| writeln!(formatter, "  {line}"))
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::{
            decode_hex,
            snapshot::{self, Snapshot, SNAPSHOT_DIR},
            template::{examples, upload, Run},
            wallet::{self, fund, keypair, token_account, SOL},
        },
        ballista_sdk::{
            ballista_common::template::encode_error, run_instruction, RunInputs, SYSTEM_PROGRAM_ID,
            TOKEN_PROGRAM_ID,
        },
        litesvm::types::TransactionMetadata,
        solana_instruction::AccountMeta,
        solana_sdk_ids::compute_budget,
    };

    const SYSTEM_TRANSFER_HEX: &str = include_str!("../../../fixtures/system-transfer.hex");
    const JUPITER: Address = Address::from_str_const("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4");
    const REQUIREMENT_FAILED: u32 = 6015;

    fn lines(logs: &[&str]) -> Vec<String> {
        logs.iter().map(|line| line.to_string()).collect()
    }

    /// A failure as `send` would report it, for checks that need no transaction.
    fn failure(program: Address, code: u32) -> Failure {
        Failure {
            program,
            code: Some(code),
            err: TransactionError::InstructionError(0, InstructionError::Custom(code)),
            logs: vec![],
            fee: 5_000,
        }
    }

    /// Jupiter's slippage error is 6001, which is also Ballista's `InvalidTemplateAccount`: only
    /// the program tells them apart.
    #[test]
    fn another_program_s_code_in_ballista_s_range_is_not_ballista_s() {
        assert_eq!(
            decode_ballista_error(6001).map(|decoded| decoded.name),
            Some("InvalidTemplateAccount")
        );
        assert_eq!(ballista_error(&failure(JUPITER, 6001)), None);
        assert_eq!(
            ballista_error(&failure(ballista_sdk::ID, 6001)),
            Some(("InvalidTemplateAccount", 0))
        );
    }

    #[test]
    #[should_panic(expected = "the logs were truncated, so they cannot say which program failed")]
    fn truncated_logs_are_refused_even_with_a_failed_line() {
        let failed = FailedTransactionMetadata {
            err: TransactionError::InstructionError(0, InstructionError::Custom(1)),
            meta: TransactionMetadata {
                logs: lines(&[
                    "Program 11111111111111111111111111111111 invoke [1]",
                    "Program 11111111111111111111111111111111 failed: custom program error: 0x1",
                    "Log truncated",
                ]),
                ..TransactionMetadata::default()
            },
        };
        let _ = Failure::new(failed, &[]);
    }

    /// A malformed compute-budget instruction fails the transaction before any program runs, so
    /// no `failed` line names it. The instruction error's index does.
    #[test]
    fn a_failure_before_any_program_ran_is_blamed_on_its_instruction() {
        let mut svm = LiteSVM::new();
        let payer = keypair(b"ballista-protocol-tests-payer-01");
        fund(&mut svm, &payer.pubkey(), SOL);
        let mut transfer_data = vec![2, 0, 0, 0];
        transfer_data.extend_from_slice(&1u64.to_le_bytes());
        let transfer = Instruction {
            program_id: SYSTEM_PROGRAM_ID,
            accounts: vec![
                AccountMeta::new(payer.pubkey(), true),
                AccountMeta::new(payer.pubkey(), false),
            ],
            data: transfer_data,
        };
        // SetComputeUnitLimit without its limit.
        let malformed = Instruction {
            program_id: compute_budget::ID,
            accounts: vec![],
            data: vec![2],
        };
        let failure = send(&mut svm, &payer, &[], &[transfer, malformed], &[]).unwrap_err();
        assert!(failure.logs.is_empty(), "{failure:?}");
        assert_eq!(failure.program, compute_budget::ID);
        assert_eq!(failure.code, None);
        assert_eq!(
            failure.err,
            TransactionError::InstructionError(1, InstructionError::InvalidInstructionData)
        );
    }

    /// A requirement in a real template fails, and the fixture's labels name its step:
    /// `tokenSweepIntoSwap` will not sell a balance at its dust floor, and stops before Jupiter.
    #[test]
    fn a_failed_requirement_is_named_by_its_step() {
        let snapshot = Snapshot::load(SNAPSHOT_DIR);
        let usdc = snapshot.named("usdcMint");
        let wsol = snapshot.named("wsolMint");
        let jupiter = snapshot.named("jupiter");
        let mut svm = snapshot.into_svm();
        let seller = wallet::wallet();
        let creator = keypair(b"ballista-protocol-tests-creator1");
        for key in [seller.pubkey(), creator.pubkey()] {
            fund(&mut svm, &key, SOL);
        }
        let examples = examples();
        let sweep = &examples["tokenSweepIntoSwap"];
        let template = upload(&mut svm, &creator, 1, &sweep.payload);
        let source = token_account(&mut svm, &seller.pubkey(), &usdc, 1_000);
        let destination = token_account(&mut svm, &seller.pubkey(), &wsol, 0);
        let run = Run::new(template, sweep)
            .account("jupiter", jupiter, false, false)
            .account("tokenProgram", TOKEN_PROGRAM_ID, false, false)
            .account("seller", seller.pubkey(), true, true)
            .account("sourceAta", source, true, false)
            .account("destinationAta", destination, true, false)
            .input_bytes("routePlan", &[0; 4])
            .input_u64("quotedInAmount", 1_000)
            .input_u64("quotedOutAmount", 1)
            .input_u64("slippageBps", 50)
            .input_u64("platformFeeBps", 0)
            .input_u64("dustFloor", 1_000)
            .group("routeAccounts", [])
            .build();
        let failure = send(&mut svm, &seller, &[], &[run], &[]).unwrap_err();
        assert_requirement_failed(&failure, sweep, "worthSelling");
    }

    #[test]
    fn a_requirement_is_matched_at_any_pc_of_its_step() {
        let examples = examples();
        let sweep = &examples["tokenSweepIntoSwap"];
        let worth: Vec<u16> = sweep
            .labels
            .iter()
            .filter(|(_, label)| label.as_str() == "worthSelling")
            .map(|(pc, _)| *pc)
            .collect();
        assert!(worth.len() > 1, "{worth:?}");
        for pc in worth {
            let failed = failure(ballista_sdk::ID, encode_error(REQUIREMENT_FAILED, pc));
            assert_requirement_failed(&failed, sweep, "worthSelling");
            assert_ballista_failure(&failed, sweep, "RequirementFailed", "worthSelling");
        }
    }

    /// A failure at a `worthSelling` pc does not satisfy an assertion that expects
    /// `saleMetTheQuote`: the message names the step the pc actually belongs to. The pc itself
    /// comes from the fixture's own labels, since program counters move when the template
    /// changes.
    #[test]
    fn another_step_s_failure_does_not_match() {
        let examples = examples();
        let sweep = &examples["tokenSweepIntoSwap"];
        let pc = sweep
            .labels
            .iter()
            .find_map(|(&pc, label)| (label == "worthSelling").then_some(pc))
            .expect("tokenSweepIntoSwap has a worthSelling step");
        let label = sweep.label_at(pc).expect("pc is labelled");
        let failed = failure(ballista_sdk::ID, encode_error(REQUIREMENT_FAILED, pc));
        let expected = format!(
            "expected Ballista's RequirementFailed in \"saleMetTheQuote\", but it failed with \
             RequirementFailed at pc {pc}, in {label:?}"
        );
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            assert_requirement_failed(&failed, sweep, "saleMetTheQuote");
        }))
        .expect_err("assert_requirement_failed should have panicked");
        let message = panic
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| panic.downcast_ref::<&str>().map(|text| text.to_string()))
            .unwrap_or_else(|| panic!("the panic payload is not a string"));
        assert!(
            message.contains(&expected),
            "the panic message {message:?} does not contain {expected:?}"
        );
    }

    #[test]
    #[should_panic(
        expected = "expected Ballista's RequirementFailed in \"worthSelling\", but JUP6"
    )]
    fn another_program_s_failure_does_not_match() {
        let examples = examples();
        let failed = failure(JUPITER, encode_error(REQUIREMENT_FAILED, 10));
        assert_requirement_failed(&failed, &examples["tokenSweepIntoSwap"], "worthSelling");
    }

    #[test]
    #[should_panic(expected = "the template has no step labelled \"worthSellin\"")]
    fn a_misspelt_label_panics() {
        let examples = examples();
        let failed = failure(ballista_sdk::ID, encode_error(REQUIREMENT_FAILED, 10));
        assert_requirement_failed(&failed, &examples["tokenSweepIntoSwap"], "worthSellin");
    }

    /// Its context is the index of the account that failed its constraint, which a label lookup
    /// would read as a program counter.
    #[test]
    #[should_panic(expected = "AccountConstraintFailed does not carry a program counter")]
    fn a_failure_without_a_program_counter_has_no_step() {
        let examples = examples();
        let failed = failure(ballista_sdk::ID, encode_error(6020, 9));
        assert_ballista_failure(
            &failed,
            &examples["tokenSweepIntoSwap"],
            "AccountConstraintFailed",
            "worthSelling",
        );
    }

    #[test]
    fn the_first_failed_line_names_the_innermost_program() {
        let logs = lines(&[
            "Program BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD invoke [1]",
            "Program log: step failed: not a program id",
            "Program 11111111111111111111111111111111 invoke [2]",
            "Transfer: insufficient lamports 5, need 10",
            "Program 11111111111111111111111111111111 failed: custom program error: 0x1",
            "Program BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD consumed 900 of 200000 compute units",
            "Program BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD failed: custom program error: 0x1",
        ]);
        assert_eq!(innermost_failure(&logs), Some((SYSTEM_PROGRAM_ID, Some(1))));
    }

    #[test]
    fn a_failure_without_a_custom_code_has_none() {
        let logs = lines(&[
            "Program BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD invoke [1]",
            "Program BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD failed: missing required signature for instruction",
        ]);
        assert_eq!(innermost_failure(&logs), Some((ballista_sdk::ID, None)));
        assert_eq!(innermost_failure(&lines(&["Program log: done"])), None);
    }

    /// Jupiter calls itself to log its event, so it has two `consumed` lines; the outer one, which
    /// includes the inner, is its cost. Ballista's includes Jupiter's.
    #[test]
    fn a_program_s_compute_units_are_its_outermost_invocation_s() {
        let outcome = Outcome {
            logs: lines(&[
                "Program BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD invoke [1]",
                "Program JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4 invoke [2]",
                "Program log: consumed 7 of 8 compute units",
                "Program JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4 invoke [3]",
                "Program JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4 consumed 1500 of 90000 compute units",
                "Program JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4 success",
                "Program JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4 consumed 41000 of 130000 compute units",
                "Program JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4 success",
                "Program BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD consumed 52000 of 200000 compute units",
                "Program BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD success",
            ]),
            compute_units: 52_150,
            fee: 5_000,
            size: 700,
            return_data: ReturnData::default(),
        };
        assert_eq!(outcome.compute_units_of(&JUPITER), Some(41_000));
        assert_eq!(outcome.compute_units_of(&ballista_sdk::ID), Some(52_000));
        assert_eq!(outcome.compute_units_of(&SYSTEM_PROGRAM_ID), None);
    }

    /// A program's own work leaves out the programs it called directly, and only those: Ballista's
    /// leaves out Jupiter's outer invocation, which includes Jupiter's call to itself, and a token
    /// transfer. A `Program log:` line that mentions units is not a `consumed` line.
    #[test]
    fn a_program_s_own_compute_units_leave_out_its_callees() {
        let outcome = Outcome {
            logs: lines(&[
                "Program BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD invoke [1]",
                "Program JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4 invoke [2]",
                "Program JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4 invoke [3]",
                "Program JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4 consumed 1500 of 90000 compute units",
                "Program JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4 success",
                "Program log: consumed 7 of 8 compute units",
                "Program JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4 consumed 41000 of 130000 compute units",
                "Program JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4 success",
                "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA invoke [2]",
                "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA consumed 76 of 88000 compute units",
                "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA success",
                "Program BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD consumed 52000 of 200000 compute units",
                "Program BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD success",
            ]),
            compute_units: 52_000,
            fee: 5_000,
            size: 700,
            return_data: ReturnData::default(),
        };
        assert_eq!(
            outcome.own_compute_units_of(&ballista_sdk::ID),
            Some(52_000 - 41_000 - 76)
        );
        assert_eq!(outcome.own_compute_units_of(&JUPITER), Some(41_000 - 1_500));
        assert_eq!(outcome.own_compute_units_of(&SYSTEM_PROGRAM_ID), None);
    }

    /// A `Program data:` line belongs to the innermost invocation open around it: here a nested
    /// Ballista run's line, then Jupiter's, then the outer run's own after both returned.
    #[test]
    fn program_data_belongs_to_the_invocation_around_it() {
        let logs = lines(&[
            "Program BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD invoke [1]",
            "Program BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD invoke [2]",
            "Program JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4 invoke [3]",
            "Program data: AQI=",
            "Program JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4 success",
            "Program data: U0xDRQ==",
            "Program log: data: not a data line",
            "Program BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD success",
            "Program data: UEFJRA== AA==",
            "Program BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD success",
        ]);
        let logged = |program, height, fields: &[&[u8]]| ProgramData {
            program,
            height,
            fields: fields.iter().map(|field| field.to_vec()).collect(),
        };
        assert_eq!(
            program_data(&logs),
            [
                logged(JUPITER, 3, &[&[1, 2]]),
                logged(ballista_sdk::ID, 2, &[b"SLCE"]),
                logged(ballista_sdk::ID, 1, &[b"PAID", &[0]]),
            ]
        );
        assert_eq!(deepest_invocation(&logs), 3);
    }

    /// Ballista built from source in a bare SVM, with `fixtures/system-transfer.hex` uploaded.
    /// Accounts: `systemProgram` (pinned), `source` (signer, writable), `destination` (writable).
    /// Input: `amount: u64`.
    fn system_transfer() -> (LiteSVM, Address, Keypair, Address) {
        let mut svm = LiteSVM::new();
        snapshot::add_ballista(&mut svm);
        let creator = keypair(b"ballista-protocol-tests-creator1");
        let source = keypair(b"ballista-protocol-tests-sender-1");
        let destination = keypair(b"ballista-protocol-tests-receiver").pubkey();
        for key in [creator.pubkey(), source.pubkey(), destination] {
            fund(&mut svm, &key, SOL);
        }
        let template = upload(&mut svm, &creator, 1, &decode_hex(SYSTEM_TRANSFER_HEX));
        (svm, template, source, destination)
    }

    fn transfer(
        template: Address,
        source: &Keypair,
        destination: AccountMeta,
        amount: u64,
    ) -> Instruction {
        run_instruction(
            template,
            vec![
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
                AccountMeta::new(source.pubkey(), true),
                destination,
            ],
            &RunInputs::new().u64(amount).finish(),
        )
    }

    #[test]
    fn a_failing_cpi_is_blamed_on_the_program_it_called() {
        let (mut svm, template, source, destination) = system_transfer();
        // More than the source holds: the System program refuses inside Ballista's CPI.
        let run = transfer(
            template,
            &source,
            AccountMeta::new(destination, false),
            2 * SOL,
        );
        let failure = send(&mut svm, &source, &[], &[run], &[]).unwrap_err();

        assert_eq!(failure.program, SYSTEM_PROGRAM_ID, "{failure:?}");
        assert_eq!(
            failure.code,
            Some(1),
            "SystemError::ResultWithNegativeLamports"
        );
        assert_eq!(ballista_error(&failure), None);
        assert_eq!(
            failure.err,
            TransactionError::InstructionError(0, InstructionError::Custom(1))
        );
        // Ballista, the caller, repeats the code on its own `failed` line, after the System
        // program's: the last line would blame Ballista.
        let failed: Vec<(Address, Option<u32>)> = failure
            .logs
            .iter()
            .filter_map(|line| innermost_failure(std::slice::from_ref(line)))
            .collect();
        assert_eq!(
            failed,
            vec![(SYSTEM_PROGRAM_ID, Some(1)), (ballista_sdk::ID, Some(1))]
        );
        // The fee is charged all the same.
        assert!(failure.fee > 0);
        assert_eq!(svm.get_balance(&source.pubkey()), Some(SOL - failure.fee));
    }

    #[test]
    fn a_failure_in_ballista_decodes_to_its_name_and_context() {
        let (mut svm, template, source, destination) = system_transfer();
        // `destination` must be writable; it is account 2 of the run.
        let run = transfer(
            template,
            &source,
            AccountMeta::new_readonly(destination, false),
            1,
        );
        let failure = send(&mut svm, &source, &[], &[run], &[]).unwrap_err();

        assert_eq!(failure.program, ballista_sdk::ID, "{failure:?}");
        assert_eq!(
            ballista_error(&failure),
            Some(("AccountConstraintFailed", 2))
        );
    }

    #[test]
    fn identical_transactions_both_land() {
        let (mut svm, template, source, destination) = system_transfer();
        let run = transfer(
            template,
            &source,
            AccountMeta::new(destination, false),
            1_000,
        );
        // The payer repeated among the signers signs once.
        for signers in [&[][..], &[&source]] {
            send(&mut svm, &source, signers, std::slice::from_ref(&run), &[]).unwrap();
        }
        assert_eq!(svm.get_balance(&destination), Some(SOL + 2_000));
    }

    /// An instruction for a program the SVM does not hold.
    fn unknown_program(data_len: usize) -> (LiteSVM, Keypair, Instruction) {
        let mut svm = LiteSVM::new();
        let payer = keypair(b"ballista-protocol-tests-payer-01");
        fund(&mut svm, &payer.pubkey(), SOL);
        let instruction = Instruction {
            program_id: Address::new_from_array([7; 32]),
            accounts: vec![],
            data: vec![0; data_len],
        };
        (svm, payer, instruction)
    }

    #[test]
    #[should_panic(expected = "at most 1232 fit in a packet")]
    fn an_oversized_transaction_is_refused_before_sending() {
        let (mut svm, payer, instruction) = unknown_program(PACKET_DATA_SIZE);
        let _ = send(&mut svm, &payer, &[], &[instruction], &[]);
    }

    #[test]
    #[should_panic(expected = "the transaction failed outside any program")]
    fn a_failure_outside_any_program_panics() {
        let (mut svm, payer, instruction) = unknown_program(1);
        let _ = send(&mut svm, &payer, &[], &[instruction], &[]);
    }
}
