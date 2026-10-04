//! Getting started, steps 1 to 5 (`docs/guide/getting-started.md`), as one program against the
//! real Solana client crates. Start the local validator the page describes, then, from the
//! repository root, run:
//!
//!   cargo run --manifest-path clients/rust/examples/getting-started/Cargo.toml
//!
//! The body of `main` is the page's Rust regions, which live in `../docs_start.rs`; the test
//! `clients/rust/tests/docs_start.rs` keeps the two the same.

fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    Ok(())
}
