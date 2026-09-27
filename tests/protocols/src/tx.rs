//! Sign and send transactions, keep them within the packet size, and name the program that failed.

use {
    ballista_sdk::decode_ballista_error,
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
        }),
        Err(failed) => Err(Failure::new(failed, instructions)),
    }
}

/// The name and context of a failure Ballista raised itself: `("RequirementFailed", pc)`, say.
///
/// `None` when another program failed, even one whose code falls in Ballista's range.
pub fn ballista_error(failure: &Failure) -> Option<(&'static str, u16)> {
    if failure.program != ballista_sdk::ID {
        return None;
    }
    let decoded = decode_ballista_error(failure.code?)?;
    Some((decoded.name, decoded.context))
}

impl Failure {
    fn new(failed: FailedTransactionMetadata, instructions: &[Instruction]) -> Self {
        let FailedTransactionMetadata { err, meta } = failed;
        let (program, code) = match innermost_failure(&meta.logs) {
            Some(failure) => failure,
            None => {
                assert!(
                    !meta.logs.iter().any(|line| line == "Log truncated"),
                    "the logs were truncated before the failure; build the SVM with \
                     `with_log_bytes_limit(None)`\n{}",
                    meta.logs.join("\n")
                );
                // No `failed` line: the runtime rejected the instruction before its program ran.
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
            decode_hex, snapshot,
            template::upload,
            wallet::{fund, keypair, SOL},
        },
        ballista_sdk::{run_instruction, RunInputs, SYSTEM_PROGRAM_ID},
        solana_instruction::AccountMeta,
    };

    const SYSTEM_TRANSFER_HEX: &str = include_str!("../../../fixtures/system-transfer.hex");

    fn lines(logs: &[&str]) -> Vec<String> {
        logs.iter().map(|line| line.to_string()).collect()
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
