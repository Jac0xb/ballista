//! Author Ballista templates from Rust with the shared `ProgramBuilder`.
//!
//! ```bash
//! cargo run -p ballista-sdk --example author_template
//! ```
//!
//! The builder emits the same bytecode as the TypeScript compiler. The first template below is
//! byte-identical to `fixtures/system-transfer.hex`, which the TypeScript suite writes.

use ballista_sdk::{
    ballista_common::template::{
        ProgramView, ACCOUNT_EXECUTABLE, ACCOUNT_SIGNER, ACCOUNT_WRITABLE, DATA_REG_U64,
        OP_ADD, OP_LTE, VALUE_U64,
    },
    create_template_instruction, find_template_pda, template_hash, ProgramBuilder, Segment,
    SYSTEM_PROGRAM_ID,
};
use solana_program::pubkey::Pubkey;

fn main() {
    let transfer = sol_transfer_template();
    let payroll = budgeted_payroll_template();
    let creator = Pubkey::new_unique();

    for (name, payload) in [("sol-transfer", transfer), ("budgeted-payroll", payroll)] {
        let program = ProgramView::parse(&payload).expect("payload parses");
        let stats = program.verify().expect("payload verifies");
        let (template, bump) = find_template_pda(&creator, 1);
        let create = create_template_instruction(creator, 1, &payload);
        println!("{name}");
        println!("  bytes            {}", payload.len());
        println!("  sha256           {}", hex(&template_hash(&payload)));
        println!("  instructions     {}", stats.instructions);
        println!("  registers        {}", stats.registers);
        println!("  expanded cpis    {}", stats.max_expanded_cpis);
        println!("  template pda     {template} (bump {bump})");
        println!("  create ix bytes  {}", create.data.len());
        println!("  payload hex      {}", hex(&payload));
    }
}

/// Transfer `lamports` from a signing sender to a recipient through the System Program.
fn sol_transfer_template() -> Vec<u8> {
    let mut builder = ProgramBuilder::new();
    // Fixed accounts, in the order callers must pass them after the template account.
    let system = builder.account(
        ACCOUNT_EXECUTABLE,
        Some(SYSTEM_PROGRAM_ID.to_bytes()),
        None,
        0,
    );
    let sender = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let recipient = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    // Typed run inputs, in the order callers must encode them.
    let lamports_input = builder.input(VALUE_U64, 0);

    let lamports = builder.load_input(lamports_input);
    let discriminator = builder.blob(&[2, 0, 0, 0]); // SystemInstruction::Transfer
    let transfer = builder.cpi(
        system,
        &[
            (sender, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (recipient, ACCOUNT_WRITABLE),
        ],
        &[
            Segment::Literal(discriminator),
            Segment::Register(DATA_REG_U64, lamports),
        ],
    );
    builder.invoke(transfer, None);
    builder.build().expect("transfer template builds")
}

/// Pay every row account `amount`, then require the total stays within `budget`. The running total
/// lives in a loop-carried register, so it survives across iterations and after the loop.
fn budgeted_payroll_template() -> Vec<u8> {
    let mut builder = ProgramBuilder::new();
    let system = builder.account(
        ACCOUNT_EXECUTABLE,
        Some(SYSTEM_PROGRAM_ID.to_bytes()),
        None,
        0,
    );
    let treasury = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let recipient = builder.row_account(ACCOUNT_WRITABLE, None, None, 0);
    builder.batch(30, 1); // up to 30 rows, at least one

    let amount_input = builder.input(VALUE_U64, 0);
    let budget_input = builder.input(VALUE_U64, 0);
    let amount = builder.load_input(amount_input);
    let budget = builder.load_input(budget_input);
    let total = builder.const_u64(0);

    let discriminator = builder.blob(&[2, 0, 0, 0]);
    let transfer = builder.cpi(
        system,
        &[
            (treasury, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (recipient, ACCOUNT_WRITABLE),
        ],
        &[
            Segment::Literal(discriminator),
            Segment::Register(DATA_REG_U64, amount),
        ],
    );
    builder.for_each(1 << total, |body| {
        body.invoke(transfer, None);
        let sum = body.binary(OP_ADD, total, amount);
        body.mov(total, sum);
    });
    let within_budget = builder.binary(OP_LTE, total, budget);
    builder.require(within_budget);
    builder.build().expect("payroll template builds")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
