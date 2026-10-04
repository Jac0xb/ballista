//! The Rust code on the Getting started and Template lifecycle pages (`docs/guide/`), which
//! include each region by name.
//!
//! On the pages the regions run in order, as the body of the reader's `main`: Getting started's,
//! then Template lifecycle's. Here each page's regions sit in a function whose first lines,
//! outside the regions, bring in what the earlier pages defined.
//!
//! The pages also use the Solana client crates that Getting started's install line adds:
//! solana-rpc-client, solana-keypair, solana-signer, solana-transaction,
//! solana-transaction-error and solana-commitment-config. This crate doesn't depend
//! on them, so the modules at the end of this file stand in for them, with the same paths and
//! signatures for every item the regions use. That way `cargo test` compiles each region exactly as
//! the pages show it, and `tests/docs_start.rs` measures every transaction the upload code builds.
//! Against the real crates, at the versions the install line pins, the same regions compile and
//! run unchanged; keep the stand-ins' signatures the same as theirs.
//!
//! `getting-started/` is Getting started's steps 1 to 5 as a crate that runs against the real
//! crates: its `main` is the connect, define, upload, run and failure regions, which
//! `tests/docs_start.rs` holds to the ones here. Change them here, then copy them there.

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
    use ballista_sdk::template::prelude::*;

    let sweep = Template::new()
        // The caller picks the reserve, in lamports, on every run.
        .input("reserve", Type::U64)
        // Must be exactly the System program, so a caller can't swap in another.
        .account("systemProgram", account::program(SYSTEM_PROGRAM_ID))
        // The account swept. It signs, so only its owner can run the sweep.
        .account("vault", account::signer().writable())
        // Where the swept lamports go.
        .account("destination", account::writable())
        // Read the vault's balance while the transaction runs.
        .step(step::let_("balance", lamports("vault")))
        // Stop the whole run unless there is something above the reserve.
        .step(step::require(var("balance").gt(input("reserve"))).label("aboveReserve"))
        // Send everything above the reserve: balance minus reserve.
        .step(system_transfer(
            "systemProgram",
            "vault",
            "destination",
            var("balance") - input("reserve"),
        ));

    // Compile to the bytes you upload in step 3.
    let compiled = sweep.compile()?;
    // #endregion define
    if cfg!(test) {
        return Ok(compiled.bytes);
    }

    // #region upload
    use ballista_sdk::{create_template_instruction, find_template_pda};

    // The creator uploads the template and pays the rent for its account.
    let creator = funded_signer()?;
    let create = create_template_instruction(creator.pubkey(), 7, &compiled.bytes);
    send(&creator, &[create])?;
    // A template's address comes from its creator's address and its template ID.
    let (template, _) = find_template_pda(&creator.pubkey(), 7);
    println!("uploaded to {template}");
    // #endregion upload

    // #region run
    // The vault is the account the template sweeps. It signs the run and pays the fee.
    let vault = funded_signer()?;
    // Any account can receive the lamports; here, another new wallet.
    let destination = funded_signer()?.pubkey();

    // The caller picks the reserve; the template works out the amount.
    let sweep_instruction = |reserve: u64| {
        compiled
            .run(template)
            .input("reserve", reserve)
            .account("systemProgram", SYSTEM_PROGRAM_ID)
            .account("vault", vault.pubkey())
            .account("destination", destination)
            .instruction()
    };

    send(&vault, &[sweep_instruction(2_000_000)?])?;
    // Prints 2000000: the vault keeps exactly the reserve.
    println!("vault keeps {}", rpc.get_balance(&vault.pubkey())?);
    // #endregion run

    // #region failure
    use solana_program::instruction::InstructionError;
    use solana_transaction_error::TransactionError;

    // The vault now holds less than 5,000,000 lamports, so the check fails.
    let error = send(&vault, &[sweep_instruction(5_000_000)?]).unwrap_err();
    if let Some(TransactionError::InstructionError(_, InstructionError::Custom(code))) =
        error.get_transaction_error()
    {
        println!("{code} {:?}", compiled.explain_error(code));
        // 202623 Some("RequirementFailed at steps[1] (aboveReserve)")
    }
    // #endregion failure

    Ok(compiled.bytes)
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

/// Getting started, step 6: a template that calls your own program. `tests/docs_start.rs` holds
/// it to the bytes the TypeScript compiler produces for `examples/start/own-program.ts`.
pub fn own_program() -> Result<Vec<u8>, Box<dyn Error>> {
    // #region own-program
    use ballista_sdk::{anchor_discriminator, template::prelude::*};

    // Your program's address. This one is a placeholder.
    let my_program = pubkey!("MyProgram1111111111111111111111111111111111");

    let deposit = Template::new()
        .input("amount", Type::U64)
        .account("myProgram", account::program(my_program))
        .account("vault", account::writable())
        .account("authority", account::signer())
        .step(
            step::invoke("myProgram")
                .writable("vault")
                .signer("authority")
                // An Anchor instruction's data: its 8-byte discriminator, then its arguments.
                .data(data::literal(anchor_discriminator("deposit")))
                .data(data::u64(input("amount"))),
        );

    let compiled = deposit.compile()?;
    // #endregion own-program
    Ok(compiled.bytes)
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
            solana_signature::Signature, solana_transaction::Transaction,
        };

        /// What `send_and_confirm_transaction` accepts.
        pub trait SerializableTransaction {}
        impl SerializableTransaction for Transaction {}

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
