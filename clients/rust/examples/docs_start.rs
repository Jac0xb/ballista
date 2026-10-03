//! The Rust code on the Getting started, Template lifecycle and Transaction v1 pages
//! (`docs/guide/`), which include each region by name.
//!
//! On the pages the regions run in order, as the body of the reader's `main`: Getting started's,
//! then Template lifecycle's, then Transaction v1's. Here each page's regions sit in a function
//! whose first lines, outside the regions, bring in what the earlier pages defined.
//!
//! The pages also use the Solana client crates that Getting started's install line adds:
//! solana-rpc-client, solana-keypair, solana-signer, solana-transaction,
//! solana-transaction-error, solana-commitment-config and solana-message. This crate doesn't depend
//! on them, so the modules at the end of this file stand in for them, with the same paths and
//! signatures for every item the regions use. That way `cargo test` compiles each region exactly as
//! the pages show it, and `tests/docs_start.rs` measures every transaction the upload code builds.
//! Against the real crates, at the versions the install line pins, the same regions compile and
//! run unchanged; keep the stand-ins' signatures the same as theirs.

#![allow(dead_code)]

use std::error::Error;

use solana_keypair::Keypair;
use solana_program::instruction::Instruction;
use solana_rpc_client::rpc_client::RpcClient;
use solana_rpc_client_api::client_error::Result as ClientResult;
use solana_signature::Signature;
use solana_signer::Signer;

/// Nothing to run: the functions below exist to be compiled and tested.
fn main() {}

/// Getting started, steps 1 to 5. Under test it stops before the first RPC call and returns the
/// template it defined, which `tests/docs_start.rs` checks.
pub fn getting_started() -> Result<Vec<u8>, Box<dyn Error>> {
    // #region connect
    use solana_commitment_config::CommitmentConfig;
    use solana_keypair::Keypair;
    use solana_program::instruction::Instruction;
    use solana_rpc_client::rpc_client::RpcClient;
    use solana_signer::Signer;
    use solana_transaction::Transaction;

    // The local validator's default RPC address.
    let rpc = RpcClient::new_with_commitment(
        "http://127.0.0.1:8899".to_string(),
        CommitmentConfig::confirmed(),
    );

    // A new wallet with 1 SOL from the validator's faucet.
    let funded_signer = || -> Result<Keypair, Box<dyn std::error::Error>> {
        let signer = Keypair::new();
        let signature = rpc.request_airdrop(&signer.pubkey(), 1_000_000_000)?;
        rpc.poll_for_signature(&signature)?;
        Ok(signer)
    };

    // Send instructions in one legacy transaction that `fee_payer` signs and pays for.
    let send = |fee_payer: &Keypair, instructions: &[Instruction]| {
        let blockhash = rpc.get_latest_blockhash()?;
        let transaction = Transaction::new_signed_with_payer(
            instructions,
            Some(&fee_payer.pubkey()),
            &[fee_payer],
            blockhash,
        );
        rpc.send_and_confirm_transaction(&transaction)
    };
    // #endregion connect

    // #region define
    use ballista_sdk::{
        ballista_common::template::{
            ACCOUNT_EXECUTABLE, ACCOUNT_SIGNER, ACCOUNT_WRITABLE, DATA_REG_U64, OP_GT, OP_SUB,
            VALUE_U64,
        },
        ProgramBuilder, Segment, SYSTEM_PROGRAM_ID,
    };

    let mut builder = ProgramBuilder::new();
    let system = builder.account(
        ACCOUNT_EXECUTABLE,
        Some(SYSTEM_PROGRAM_ID.to_bytes()),
        None,
        0,
    );
    let vault = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let destination = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    let reserve_input = builder.input(VALUE_U64, 0);

    let reserve = builder.load_input(reserve_input);
    let balance = builder.account_lamports(vault);
    let above_reserve = builder.binary(OP_GT, balance, reserve);
    builder.require(above_reserve);
    let amount = builder.binary(OP_SUB, balance, reserve);
    let discriminator = builder.blob(&[2, 0, 0, 0]); // SystemInstruction::Transfer
    let transfer = builder.cpi(
        system,
        &[
            (vault, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (destination, ACCOUNT_WRITABLE),
        ],
        &[
            Segment::Literal(discriminator),
            Segment::Register(DATA_REG_U64, amount),
        ],
    );
    builder.invoke(transfer, None);

    let payload = builder.build()?;
    // #endregion define
    if cfg!(test) {
        return Ok(payload);
    }

    // #region upload
    use ballista_sdk::{create_template_instruction, find_template_pda};

    // The creator uploads the template and pays the rent for its account.
    let creator = funded_signer()?;
    let create = create_template_instruction(creator.pubkey(), 7, &payload);
    send(&creator, &[create])?;
    // A template's address comes from its creator's address and its template ID.
    let (template, _) = find_template_pda(&creator.pubkey(), 7);
    println!("uploaded to {template}");
    // #endregion upload

    // #region run
    use ballista_sdk::{run_instruction, RunInputs};
    use solana_program::instruction::AccountMeta;

    // The vault is the account the template sweeps. It signs the run and pays the fee.
    let vault_keypair = funded_signer()?;
    // Any account can receive the lamports; here, another new wallet.
    let destination_pubkey = funded_signer()?.pubkey();

    // The caller picks the reserve; the template works out the amount.
    let sweep_instruction = |reserve: u64| {
        run_instruction(
            template,
            vec![
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
                AccountMeta::new(vault_keypair.pubkey(), true),
                AccountMeta::new(destination_pubkey, false),
            ],
            &RunInputs::new().u64(reserve).finish(),
        )
    };

    send(&vault_keypair, &[sweep_instruction(2_000_000)])?;
    // Prints 2000000: the vault keeps exactly the reserve.
    println!("vault keeps {}", rpc.get_balance(&vault_keypair.pubkey())?);
    // #endregion run

    // #region failure
    use ballista_sdk::{ballista_common::template::ProgramView, decode_ballista_error};
    use solana_program::instruction::InstructionError;
    use solana_transaction_error::TransactionError;

    // The vault now holds less than 5,000,000 lamports, so the check fails.
    let error = send(&vault_keypair, &[sweep_instruction(5_000_000)]).unwrap_err();
    if let Some(TransactionError::InstructionError(_, InstructionError::Custom(code))) =
        error.get_transaction_error()
    {
        if let Some(decoded) = decode_ballista_error(code) {
            println!("{code}: {} (context {})", decoded.name, decoded.context);
            // 202623: RequirementFailed (context 3)

            // For RequirementFailed, the context is the program counter.
            let pc = usize::from(decoded.context);
            let failing = ProgramView::parse(&payload)?.instructions[pc];
            println!("failed at opcode {}", failing.opcode); // 40, the require
        }
    }
    // #endregion failure

    Ok(payload)
}

/// Template lifecycle's upload in pieces, after Getting started, with its `send`, `creator` and
/// `payload`.
pub fn chunked_upload(
    send: impl Fn(&Keypair, &[Instruction]) -> ClientResult<Signature>,
    creator: Keypair,
    payload: Vec<u8>,
) -> Result<(), Box<dyn Error>> {
    use ballista_sdk::find_template_pda;

    // #region chunked
    use ballista_sdk::{
        begin_template_instruction, finalize_template_instruction, template_hash,
        write_template_chunk_instruction,
    };

    // Each instruction goes in its own legacy transaction, which fits a write of at most
    // 1,023 bytes of template.
    const CHUNK: usize = 1_000;
    let creator_key = creator.pubkey();
    let (template, _) = find_template_pda(&creator_key, 42);
    let (length, hash) = (payload.len() as u32, template_hash(&payload));
    let begin = begin_template_instruction(creator_key, 42, length, hash);
    send(&creator, &[begin])?;
    for (index, chunk) in payload.chunks(CHUNK).enumerate() {
        let offset = (index * CHUNK) as u32;
        let write = write_template_chunk_instruction(creator_key, template, offset, chunk);
        send(&creator, &[write])?;
    }
    let finalize = finalize_template_instruction(creator_key, template);
    send(&creator, &[finalize])?;
    // #endregion chunked
    Ok(())
}

/// Template lifecycle's resumed upload, after its upload in pieces stopped partway.
pub fn resume_upload(
    rpc: &RpcClient,
    send: impl Fn(&Keypair, &[Instruction]) -> ClientResult<Signature>,
    creator: Keypair,
    payload: Vec<u8>,
) -> Result<(), Box<dyn Error>> {
    use ballista_sdk::{
        finalize_template_instruction, find_template_pda, write_template_chunk_instruction,
    };
    const CHUNK: usize = 1_000;

    // #region resume
    use ballista_sdk::ballista_common::template::TemplateAccount;

    // Write only the bytes the account doesn't have yet, then finalize.
    let creator_key = creator.pubkey();
    let (template, _) = find_template_pda(&creator_key, 42);
    let data = rpc.get_account_data(&template)?;
    let written = TemplateAccount::parse(&data)?.header().written_len();
    for (index, chunk) in payload[written..].chunks(CHUNK).enumerate() {
        let offset = (written + index * CHUNK) as u32;
        let write = write_template_chunk_instruction(creator_key, template, offset, chunk);
        send(&creator, &[write])?;
    }
    let finalize = finalize_template_instruction(creator_key, template);
    send(&creator, &[finalize])?;
    // #endregion resume
    Ok(())
}

/// Transaction v1: send `run`, a run instruction such as Getting started's
/// `sweep_instruction(...)`, with limits measured by simulating the same transaction.
pub fn send_in_version_1(
    rpc: &RpcClient,
    payer: Keypair,
    run: Instruction,
    measured_compute_units: u32,
    measured_loaded_bytes: u32,
) -> Result<(), Box<dyn Error>> {
    // #region v1
    use solana_message::{v1, VersionedMessage};
    use solana_transaction::versioned::VersionedTransaction;

    // Both limits default to zero, so set each one.
    let message = VersionedMessage::V1(v1::Message::try_compile_with_config(
        &payer.pubkey(),
        &[run],
        rpc.get_latest_blockhash()?,
        v1::TransactionConfig::empty()
            .with_compute_unit_limit(measured_compute_units)
            .with_loaded_accounts_data_size_limit(measured_loaded_bytes),
    )?);
    let transaction = VersionedTransaction::try_new(message, &[&payer])?;
    rpc.send_and_confirm_transaction(&transaction)?;
    // #endregion v1
    Ok(())
}

/// Getting started, step 6: a template that calls your own program. `tests/docs_start.rs` holds
/// it to the bytes the TypeScript compiler produces for `examples/start/own-program.ts`.
pub fn own_program() -> Result<Vec<u8>, Box<dyn Error>> {
    // #region own-program
    use ballista_sdk::{
        anchor_discriminator,
        ballista_common::template::{
            ACCOUNT_EXECUTABLE, ACCOUNT_SIGNER, ACCOUNT_WRITABLE, DATA_REG_U64, VALUE_U64,
        },
        ProgramBuilder, Segment,
    };
    use solana_program::pubkey::Pubkey;

    // Your program's address. This one is a placeholder.
    let my_program = Pubkey::from_str_const("MyProgram1111111111111111111111111111111111");

    let mut builder = ProgramBuilder::new();
    let program = builder.account(ACCOUNT_EXECUTABLE, Some(my_program.to_bytes()), None, 0);
    let vault = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    let authority = builder.account(ACCOUNT_SIGNER, None, None, 0);
    let amount_input = builder.input(VALUE_U64, 0);

    let amount = builder.load_input(amount_input);
    // An Anchor instruction's data: its 8-byte discriminator, then its arguments.
    let discriminator = builder.blob(&anchor_discriminator("deposit"));
    let deposit = builder.cpi(
        program,
        &[(vault, ACCOUNT_WRITABLE), (authority, ACCOUNT_SIGNER)],
        &[
            Segment::Literal(discriminator),
            Segment::Register(DATA_REG_U64, amount),
        ],
    );
    builder.invoke(deposit, None);

    let payload = builder.build()?;
    // #endregion own-program
    Ok(payload)
}

// ------------------------------------------------------- stand-ins for the Solana client crates
//
// Only the items the regions use, with the real crates' signatures. A stand-in never talks to a
// validator: its methods are never called here, except `Keypair::new` and `Signer::pubkey`, which
// the tests use to name a fee payer.

pub mod solana_commitment_config {
    pub struct CommitmentConfig;

    impl CommitmentConfig {
        pub fn confirmed() -> Self {
            Self
        }
    }
}

pub mod solana_signer {
    use solana_program::pubkey::Pubkey;

    pub trait Signer {
        fn pubkey(&self) -> Pubkey;
    }

    #[derive(Debug)]
    pub struct SignerError;

    impl std::fmt::Display for SignerError {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("signer error")
        }
    }

    impl std::error::Error for SignerError {}
}

pub mod solana_keypair {
    use solana_program::pubkey::Pubkey;

    /// Holds only an address: these stand-ins never sign.
    pub struct Keypair(Pubkey);

    impl Keypair {
        pub fn new() -> Self {
            Self(Pubkey::new_unique())
        }
    }

    impl super::solana_signer::Signer for Keypair {
        fn pubkey(&self) -> Pubkey {
            self.0
        }
    }
}

pub mod solana_signature {
    #[derive(Debug, Default)]
    pub struct Signature;
}

pub mod solana_transaction_error {
    use solana_program::instruction::InstructionError;

    pub enum TransactionError {
        InstructionError(u8, InstructionError),
    }
}

pub mod solana_message {
    #[derive(Debug)]
    pub struct CompileError;

    impl std::fmt::Display for CompileError {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("compile error")
        }
    }

    impl std::error::Error for CompileError {}

    pub enum VersionedMessage {
        V1(v1::Message),
    }

    pub mod v1 {
        use solana_program::{hash::Hash, instruction::Instruction, pubkey::Pubkey};

        pub struct TransactionConfig;

        impl TransactionConfig {
            pub const fn empty() -> Self {
                Self
            }

            pub const fn with_compute_unit_limit(self, _limit: u32) -> Self {
                self
            }

            pub const fn with_loaded_accounts_data_size_limit(self, _limit: u32) -> Self {
                self
            }
        }

        pub struct Message;

        impl Message {
            pub fn try_compile_with_config(
                _payer: &Pubkey,
                _instructions: &[Instruction],
                _recent_blockhash: Hash,
                _config: TransactionConfig,
            ) -> Result<Self, super::CompileError> {
                unimplemented!("stand-in")
            }
        }
    }
}

pub mod solana_transaction {
    use solana_program::{hash::Hash, instruction::Instruction, pubkey::Pubkey};

    use super::solana_keypair::Keypair;

    pub struct Transaction;

    impl Transaction {
        pub fn new_signed_with_payer(
            _instructions: &[Instruction],
            _payer: Option<&Pubkey>,
            _signing_keypairs: &[&Keypair],
            _recent_blockhash: Hash,
        ) -> Self {
            unimplemented!("stand-in")
        }
    }

    pub mod versioned {
        use super::super::{
            solana_keypair::Keypair, solana_message::VersionedMessage, solana_signer::SignerError,
        };

        pub struct VersionedTransaction;

        impl VersionedTransaction {
            pub fn try_new(
                _message: VersionedMessage,
                _keypairs: &[&Keypair],
            ) -> Result<Self, SignerError> {
                unimplemented!("stand-in")
            }
        }
    }
}

pub mod solana_rpc_client_api {
    pub mod client_error {
        use super::super::solana_transaction_error::TransactionError;

        #[derive(Debug)]
        pub struct ClientError;

        impl ClientError {
            pub fn get_transaction_error(&self) -> Option<TransactionError> {
                unimplemented!("stand-in")
            }
        }

        impl std::fmt::Display for ClientError {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("client error")
            }
        }

        impl std::error::Error for ClientError {}

        pub type Result<T> = std::result::Result<T, ClientError>;
    }
}

pub mod solana_rpc_client {
    pub mod rpc_client {
        use solana_program::{hash::Hash, pubkey::Pubkey};

        use super::super::{
            solana_commitment_config::CommitmentConfig,
            solana_rpc_client_api::client_error::Result as ClientResult,
            solana_signature::Signature,
            solana_transaction::{versioned::VersionedTransaction, Transaction},
        };

        /// What `send_and_confirm_transaction` accepts.
        pub trait SerializableTransaction {}
        impl SerializableTransaction for Transaction {}
        impl SerializableTransaction for VersionedTransaction {}

        pub struct RpcClient;

        impl RpcClient {
            pub fn new_with_commitment<U: ToString>(
                _url: U,
                _commitment: CommitmentConfig,
            ) -> Self {
                Self
            }

            pub fn request_airdrop(
                &self,
                _pubkey: &Pubkey,
                _lamports: u64,
            ) -> ClientResult<Signature> {
                unimplemented!("stand-in")
            }

            pub fn poll_for_signature(&self, _signature: &Signature) -> ClientResult<()> {
                unimplemented!("stand-in")
            }

            pub fn get_latest_blockhash(&self) -> ClientResult<Hash> {
                unimplemented!("stand-in")
            }

            pub fn send_and_confirm_transaction(
                &self,
                _transaction: &impl SerializableTransaction,
            ) -> ClientResult<Signature> {
                unimplemented!("stand-in")
            }

            pub fn get_balance(&self, _pubkey: &Pubkey) -> ClientResult<u64> {
                unimplemented!("stand-in")
            }

            pub fn get_account_data(&self, _pubkey: &Pubkey) -> ClientResult<Vec<u8>> {
                unimplemented!("stand-in")
            }
        }
    }
}
