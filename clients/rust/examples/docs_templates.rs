//! The guide and example pages' templates, written with the declarative API in
//! `ballista_sdk::template`: one function per example, each the Rust twin of the TypeScript file
//! of the same name in `clients/js/examples/docs/`.
//!
//! `tests/docs_examples.rs` compiles every one and checks the bytes against what the TypeScript
//! compiler produced: `fixtures/benchmarks.json` and `tests/fixtures/docs-examples.json`. The runs
//! are in `docs_runs.rs`.
//!
//! ```bash
//! cargo run -p ballista-sdk --example docs_templates
//! ```
//!
//! The pages include each function by its `#region` name.

#![allow(dead_code)]

use ballista_sdk::template::prelude::*;

fn main() {
    for (name, build) in ALL {
        let compiled = build()
            .compile()
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        println!(
            "{name:40} {:4} bytes  {:3} instructions  {:2} registers",
            compiled.stats.payload_bytes, compiled.stats.instructions, compiled.stats.registers
        );
    }
}

/// The fixture entry an example compiles to, and the function that defines it.
pub type Example = (&'static str, fn() -> Template);

pub const ALL: &[Example] = &[
    ("sweep-above-a-reserve", sweep_above_a_reserve),
    ("budgeted-payroll", budgeted_payroll),
    ("row-amounts", row_amounts),
    ("count-runs", count_runs),
    ("daily-limit-per-caller", daily_limit_per_caller),
    ("listed-callers-only", listed_callers_only),
    ("assert-create-then-transfer", assert_create_then_transfer),
    ("assert-recipient-ata", assert_recipient_ata),
    ("basis-point-revenue-split", basis_point_revenue_split),
    ("bounded-keeper-crank", bounded_keeper_crank),
    ("bounded-sol-payroll", bounded_sol_payroll),
    ("canonical-position-account", canonical_position_account),
    (
        "claim-only-when-there-is-something",
        claim_only_when_there_is_something,
    ),
    ("claim-then-distribute", claim_then_distribute),
    ("close-empty-token-accounts", close_empty_token_accounts),
    ("conditional-ata-setup", conditional_ata_setup),
    (
        "consolidate-only-the-funded-accounts",
        consolidate_only_the_funded_accounts,
    ),
    ("crank-once-per-waiting-entry", crank_once_per_waiting_entry),
    ("crank-only-the-ripe-entries", crank_only_the_ripe_entries),
    ("deadline-and-minimum-output", deadline_and_minimum_output),
    ("deadline-refund", deadline_refund),
    (
        "distribute-a-runtime-pot-pro-rata",
        distribute_a_runtime_pot_pro_rata,
    ),
    ("exact-lamport-delta", exact_lamport_delta),
    ("exact-token-debit", exact_token_debit),
    (
        "existing-account-token-payroll",
        existing_account_token_payroll,
    ),
    (
        "forward-the-whole-token-balance",
        forward_the_whole_token_balance,
    ),
    ("generic-cpi", generic_cpi),
    ("index-weighted-rewards", index_weighted_rewards),
    ("initialize-only-if-missing", initialize_only_if_missing),
    (
        "liquidate-only-when-unhealthy",
        liquidate_only_when_unhealthy,
    ),
    ("maximum-lamport-spend", maximum_lamport_spend),
    ("named-inputs", named_inputs),
    ("nested-swap-then-deposit", nested_swap_then_deposit),
    ("oracle-price-band", oracle_price_band),
    ("payment-agent", payment_agent),
    ("pinned-program-and-owner", pinned_program_and_owner),
    ("primary-or-fallback-route", primary_or_fallback_route),
    ("rebalance-three-swaps", rebalance_three_swaps),
    ("reserve-preserving-sweep", reserve_preserving_sweep),
    ("split-what-arrived", split_what_arrived),
    ("swap-and-return-what-arrived", swap_and_return_what_arrived),
    ("swap-then-deposit", swap_then_deposit),
    ("swap-through-a-checked-route", swap_through_a_checked_route),
    (
        "time-gated-governance-execution",
        time_gated_governance_execution,
    ),
    ("token-transfer", token_transfer_template),
    ("top-up-to-a-target", top_up_to_a_target),
    (
        "waterfall-until-the-money-runs-out",
        waterfall_until_the_money_runs_out,
    ),
];

// #region sweep-above-a-reserve
/// Move everything above `reserve` from the vault to the destination.
pub fn sweep_above_a_reserve() -> Template {
    Template::new()
        .input("reserve", Type::U64)
        .account("systemProgram", account::program(SYSTEM_PROGRAM_ID))
        .account("vault", account::signer().writable())
        .account("destination", account::writable())
        .step(step::let_("balance", lamports("vault")))
        .step(step::require(var("balance").gt(input("reserve"))))
        .step(system_transfer(
            "systemProgram",
            "vault",
            "destination",
            var("balance") - input("reserve"),
        ))
}
// #endregion sweep-above-a-reserve

// #region budgeted-payroll
/// Pay `amount` to every recipient, then require the total stays within `budget`.
pub fn budgeted_payroll() -> Template {
    Template::new()
        .input("amount", Type::U64)
        .input("budget", Type::U64)
        .account("systemProgram", account::program(SYSTEM_PROGRAM_ID))
        .account("treasury", account::signer().writable())
        .batch(
            Batch::new(30)
                .min_iterations(1)
                .account("recipient", account::writable()),
        )
        .step(step::let_("total", u64(0)))
        .step(
            step::for_each()
                .step(system_transfer(
                    "systemProgram",
                    "treasury",
                    account::iteration("recipient"),
                    input("amount"),
                ))
                .step(step::assign("total", var("total") + input("amount")))
                .carry("total"),
        )
        .step(step::require(var("total").lte(input("budget"))).label("withinBudget"))
}
// #endregion budgeted-payroll

// #region row-amounts
/// Pay each recipient its own amount, carried as a row input.
pub fn row_amounts() -> Template {
    Template::new()
        .account("systemProgram", account::program(SYSTEM_PROGRAM_ID))
        .account("treasury", account::signer().writable())
        .batch(
            Batch::new(30)
                .min_iterations(1)
                .account("recipient", account::writable())
                .input("amount", Type::U64),
        )
        .step(step::for_each().step(system_transfer(
            "systemProgram",
            "treasury",
            account::iteration("recipient"),
            row_input("amount"),
        )))
}
// #endregion row-amounts

// #region count-runs
/// Count each caller's runs, in an entry of their own.
pub fn count_runs() -> Template {
    Template::new()
        // Each registry and its fields: `runs`, with one u64 field, `count`.
        .registry("runs", [("count", Type::U64)])
        .account("caller", account::signer().writable())
        // The account holding the caller's entry in `runs`. Before the first step, every run checks
        // it, or creates it. `"caller"` pays the rent when a run creates the entry, and nothing
        // after that.
        .account(
            "callerRuns",
            account::registry("runs", "caller")
                // Keyed by the caller, who must sign, so a caller opens only their own entry.
                // Leave `.key` out for one entry that every run shares.
                .key(account_key("caller")),
        )
        // Creating an entry calls the System program.
        .account("systemProgram", account::system_program())
        // Read and write name the entry's account, not the registry, so a template can open two
        // entries of one registry. The write lands at once; if the run fails, Solana undoes it.
        .step(step::set_registry(
            "callerRuns",
            "count",
            registry("callerRuns", "count") + u64(1),
        ))
}
// #endregion count-runs

// #region daily-limit-per-caller
/// Send SOL, at most 1 SOL at once per caller, refilling over about a day.
pub fn daily_limit_per_caller() -> Template {
    Template::new()
        .input("amount", Type::U64)
        // The two fields rate_limit uses: `spent`, and `lastSpend`, a Unix time.
        .registry("limits", [("spent", Type::U64), ("lastSpend", Type::I64)])
        .account("caller", account::signer().writable())
        .account("recipient", account::writable())
        // Each caller's own entry, keyed by their address.
        .account(
            "callerLimit",
            account::registry("limits", "caller").key(account_key("caller")),
        )
        .account("systemProgram", account::system_program())
        // Each run refills `spent` by the seconds since `lastSpend` times the refill rate (not below
        // zero), adds `amount`, requires the total to be at most the cap, then writes both fields
        // back. Over the limit, the run fails with RequirementFailed (6015) at `withinRateLimit`,
        // and nothing moves.
        .steps(rate_limit(
            "callerLimit",      // the entry's account, not the registry
            u64(1_000_000_000), // cap: 1 SOL
            // 1 SOL refills in 86,401 seconds. The limit refills continuously, so over any 24 hours
            // a caller can send up to about 2 SOL: the full 1 SOL plus what refills.
            u64(11_574),
            input("amount"),
        ))
        .step(system_transfer(
            "systemProgram",
            "caller",
            "recipient",
            input("amount"),
        ))
}
// #endregion daily-limit-per-caller

// #region listed-callers-only
/// Only listed callers make the call. The author's runs add or remove a member instead.
pub fn listed_callers_only() -> Template {
    // Stand-ins so the example runs as written: replace AUTHOR with the author's address, and the
    // program and data with the call the list guards.
    const AUTHOR: [u8; 32] = [7; 32];
    const PROTOCOL_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID;
    const CALL_DATA: [u8; 12] = [2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0];

    // True only in the author's runs: `caller` must sign, so only the author can match AUTHOR. A
    // template has no `if` step; it branches with `select(condition, a, b)`, which gives `a` when
    // the condition is true and `b` otherwise.
    let is_author = account_key("caller").eq(pubkey(AUTHOR));

    Template::new()
        // Every run passes both inputs, but only the author's runs read them.
        .input("member", Type::Pubkey) // author's runs: whose entry to set
        .input("allow", Type::Bool) // author's runs: the flag to set
        .registry("allowed", [("ok", Type::Bool)])
        .account("caller", account::signer().writable())
        // One entry per run: the member's in the author's runs, the caller's own in everyone
        // else's, so only the author picks the key. The author pays the rent for each new member's
        // entry.
        .account(
            "entry",
            account::registry("allowed", "caller").key(select(
                &is_author,
                input("member"),
                account_key("caller"),
            )),
        )
        .account("systemProgram", account::system_program())
        .account("protocolProgram", account::program(PROTOCOL_PROGRAM))
        .account("pool", account::writable())
        // The author's runs write `allow` into the member's entry; `false` removes them, and the
        // entry stays, since entries are never closed. A write can't be skipped, so everyone else's
        // runs write back the flag already there, which changes nothing.
        .step(step::set_registry(
            "entry",
            "ok",
            select(&is_author, input("allow"), registry("entry", "ok")),
        ))
        // Everyone but the author must be listed. Anyone else fails at `listed` with
        // RequirementFailed (6015), and the entry their run created is undone too, so they pay no
        // rent.
        .step(step::require(is_author.clone().or(registry("entry", "ok"))).label("listed"))
        // The call the list guards. The author's runs skip it, so they only set flags.
        .step(
            step::invoke("protocolProgram")
                .writable_signer("caller")
                .writable("pool")
                .data(data::literal(CALL_DATA))
                .when(is_author.not()),
        )
}
// #endregion listed-callers-only

// #region assert-create-then-transfer
/// For each row: prove the destination is the recipient's ATA, create it if missing, then pay.
pub fn assert_create_then_transfer() -> Template {
    Template::new()
        .input("amount", Type::U64)
        .account(
            "associatedTokenProgram",
            account::program(ASSOCIATED_TOKEN_PROGRAM_ID),
        )
        .account("tokenProgram", account::program(TOKEN_PROGRAM_ID))
        .account("systemProgram", account::program(SYSTEM_PROGRAM_ID))
        .account(
            "mint",
            account::readonly()
                .owner(TOKEN_PROGRAM_ID)
                .min_data_length(82),
        )
        .account("payer", account::signer().writable())
        .account("authority", account::signer())
        .account(
            "source",
            account::writable()
                .owner(TOKEN_PROGRAM_ID)
                .min_data_length(165),
        )
        .batch(
            Batch::new(8)
                .min_iterations(1)
                // Each row is two accounts: the recipient's wallet, then its ATA.
                .account("recipient", account::readonly())
                .account("destinationAta", account::writable()),
        )
        .step(
            step::for_each()
                .step(assert_ata(
                    account::iteration("destinationAta"),
                    account::iteration("recipient"),
                    "mint",
                    "tokenProgram",
                    "associatedTokenProgram",
                ))
                .step(ensure_associated_token_account(AtaAccounts {
                    associated_token_program: "associatedTokenProgram".into(),
                    payer: "payer".into(),
                    associated_token_account: account::iteration("destinationAta"),
                    owner: account::iteration("recipient"),
                    mint: "mint".into(),
                    system_program: "systemProgram".into(),
                    token_program: "tokenProgram".into(),
                }))
                .step(token_transfer(
                    "tokenProgram",
                    "source",
                    account::iteration("destinationAta"),
                    "authority",
                    input("amount"),
                )),
        )
}
// #endregion assert-create-then-transfer

// #region assert-recipient-ata
/// Require `destinationAta` to be the associated token account of the recipient and mint.
pub fn assert_recipient_ata() -> Template {
    Template::new()
        .account(
            "associatedTokenProgram",
            account::program(ASSOCIATED_TOKEN_PROGRAM_ID),
        )
        .account("tokenProgram", account::readonly())
        .account("recipient", account::readonly())
        .account("mint", account::readonly())
        .account("destinationAta", account::writable())
        .step(assert_ata(
            "destinationAta",
            "recipient",
            "mint",
            "tokenProgram",
            "associatedTokenProgram",
        ))
}
// #endregion assert-recipient-ata

// #region basis-point-revenue-split
/// Split `total` lamports: `partnerBps` of it to the partner, the rest to the treasury.
pub fn basis_point_revenue_split() -> Template {
    Template::new()
        .input("total", Type::U64)
        .input("partnerBps", Type::U64)
        .account("systemProgram", account::program(SYSTEM_PROGRAM_ID))
        .account("source", account::signer().writable())
        .account("partner", account::writable())
        .account("treasury", account::writable())
        .step(step::require(input("partnerBps").lte(u64(10_000))))
        .step(step::let_(
            "partnerAmount",
            input("total") * input("partnerBps") / u64(10_000),
        ))
        .step(system_transfer(
            "systemProgram",
            "source",
            "partner",
            var("partnerAmount"),
        ))
        .step(system_transfer(
            "systemProgram",
            "source",
            "treasury",
            input("total") - var("partnerAmount"),
        ))
}
// #endregion basis-point-revenue-split

// #region bounded-keeper-crank
/// Call the crank instruction once for each (market, queue) row.
pub fn bounded_keeper_crank() -> Template {
    // Stand-ins so the example runs as written: replace them with the protocol's address and its
    // crank instruction data.
    const PROTOCOL_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID;
    const CRANK_DISCRIMINATOR: [u8; 4] = [2, 0, 0, 0];
    const CRANK_ARGUMENT: u64 = 1_000;

    Template::new()
        .account("protocolProgram", account::program(PROTOCOL_PROGRAM))
        .account("keeper", account::signer().writable())
        .batch(
            Batch::new(24)
                .min_iterations(1)
                // Each row is two accounts: a market, then its queue.
                .account("market", account::writable())
                .account("queue", account::writable()),
        )
        .step(
            step::for_each().step(
                step::invoke("protocolProgram")
                    .writable_signer("keeper")
                    .writable(account::iteration("market"))
                    .writable(account::iteration("queue"))
                    .data(data::literal(CRANK_DISCRIMINATOR))
                    .data(data::u64(u64(CRANK_ARGUMENT))),
            ),
        )
}
// #endregion bounded-keeper-crank

// #region bounded-sol-payroll
/// Pay `amount` lamports from the treasury to each of 1 to 30 recipients.
pub fn bounded_sol_payroll() -> Template {
    Template::new()
        .input("amount", Type::U64)
        .account("systemProgram", account::program(SYSTEM_PROGRAM_ID))
        .account("treasury", account::signer().writable())
        .batch(
            Batch::new(30)
                .min_iterations(1)
                .account("recipient", account::writable()),
        )
        .step(step::for_each().step(system_transfer(
            "systemProgram",
            "treasury",
            account::iteration("recipient"),
            input("amount"),
        )))
}
// #endregion bounded-sol-payroll

// #region canonical-position-account
/// Require `position` to be the PDA the protocol derives from ("position", owner, positionId).
pub fn canonical_position_account() -> Template {
    // Stand-ins so the example runs as written: replace them with your protocol's address and the
    // instruction to call on the position.
    const PROTOCOL_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID;
    const INSTRUCTION_DISCRIMINATOR: [u8; 4] = [2, 0, 0, 0];
    const INSTRUCTION_ARGUMENT: u64 = 10_000;

    Template::new()
        .input("positionId", Type::Bytes(8))
        .account("protocolProgram", account::program(PROTOCOL_PROGRAM))
        .account("owner", account::signer().writable())
        .account(
            "position",
            account::writable()
                .owner(PROTOCOL_PROGRAM)
                .min_data_length(128),
        )
        .step(assert_pda(
            "position",
            "protocolProgram",
            [bytes(b"position"), key("owner"), input("positionId")],
        ))
        .step(
            step::invoke("protocolProgram")
                .writable_signer("owner")
                .writable("position")
                .data(data::literal(INSTRUCTION_DISCRIMINATOR))
                .data(data::u64(u64(INSTRUCTION_ARGUMENT))),
        )
}
// #endregion canonical-position-account

// #region claim-only-when-there-is-something
/// Call the claim instruction only when the rewards account shows a pending amount.
pub fn claim_only_when_there_is_something() -> Template {
    // Stand-ins so the example runs as written: replace them with the rewards program's address,
    // its claim instruction data, and the offset of the pending amount in its rewards account.
    const REWARDS_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID;
    const CLAIM_DISCRIMINATOR: [u8; 4] = [2, 0, 0, 0];
    const CLAIM_ARGUMENT: u64 = 10_000;
    const PENDING_OFFSET: u32 = 8;

    Template::new()
        .account("rewardsProgram", account::program(REWARDS_PROGRAM))
        .account(
            "rewards",
            account::readonly()
                .owner(REWARDS_PROGRAM)
                .min_data_length(128),
        )
        .account("claimant", account::signer().writable())
        .account("destination", account::writable())
        .step(
            step::invoke("rewardsProgram")
                .writable_signer("claimant")
                .writable("destination")
                .data(data::literal(CLAIM_DISCRIMINATOR))
                .data(data::u64(u64(CLAIM_ARGUMENT)))
                .when(account_data("rewards", PENDING_OFFSET, ReadType::U64).gt(u64(0))),
        )
}
// #endregion claim-only-when-there-is-something

// #region claim-then-distribute
/// Claim once, then pay `amountPerRecipient` tokens to each row's token account.
pub fn claim_then_distribute() -> Template {
    // Stand-ins so the example runs as written: replace them with the rewards program's address
    // and its claim instruction data.
    const REWARDS_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID;
    const CLAIM_DISCRIMINATOR: [u8; 4] = [2, 0, 0, 0];
    const CLAIM_ARGUMENT: u64 = 10_000;

    Template::new()
        .input("amountPerRecipient", Type::U64)
        .account("rewardsProgram", account::program(REWARDS_PROGRAM))
        .account("tokenProgram", account::program(TOKEN_PROGRAM_ID))
        .account("claimer", account::signer().writable())
        .account("pool", account::writable())
        .account(
            "treasuryTokens",
            account::writable()
                .owner(TOKEN_PROGRAM_ID)
                .min_data_length(165),
        )
        .account("authority", account::signer())
        .batch(
            Batch::new(16).min_iterations(1).account(
                "recipientTokens",
                account::writable()
                    .owner(TOKEN_PROGRAM_ID)
                    .min_data_length(165),
            ),
        )
        .step(
            step::invoke("rewardsProgram")
                .writable_signer("claimer")
                .writable("pool")
                .data(data::literal(CLAIM_DISCRIMINATOR))
                .data(data::u64(u64(CLAIM_ARGUMENT))),
        )
        .step(step::for_each().step(token_transfer(
            "tokenProgram",
            "treasuryTokens",
            account::iteration("recipientTokens"),
            "authority",
            input("amountPerRecipient"),
        )))
}
// #endregion claim-then-distribute

// #region close-empty-token-accounts
/// Close each row's token account whose balance is zero; skip the others.
pub fn close_empty_token_accounts() -> Template {
    /// SPL Token `CloseAccount`: a single discriminator byte, no arguments.
    const CLOSE_ACCOUNT: [u8; 1] = [9];

    Template::new()
        .account("tokenProgram", account::program(TOKEN_PROGRAM_ID))
        .account("rentDestination", account::writable())
        .account("authority", account::signer())
        .batch(
            Batch::new(16).min_iterations(1).account(
                "tokenAccount",
                account::writable()
                    .owner(TOKEN_PROGRAM_ID)
                    .min_data_length(165),
            ),
        )
        .step(
            step::for_each().step(
                step::invoke("tokenProgram")
                    .program_address(TOKEN_PROGRAM_ID)
                    .writable(account::iteration("tokenAccount"))
                    .writable("rentDestination")
                    .signer("authority")
                    .data(data::literal(CLOSE_ACCOUNT))
                    .when(
                        account_data(account::iteration("tokenAccount"), 64, ReadType::U64)
                            .eq(u64(0)),
                    ),
            ),
        )
}
// #endregion close-empty-token-accounts

// #region conditional-ata-setup
/// Create the wallet's ATA with the ATA program's `Create`, only if it does not exist yet.
pub fn conditional_ata_setup() -> Template {
    Template::new()
        .account(
            "associatedTokenProgram",
            account::program(ASSOCIATED_TOKEN_PROGRAM_ID),
        )
        .account("tokenProgram", account::program(TOKEN_PROGRAM_ID))
        .account("systemProgram", account::program(SYSTEM_PROGRAM_ID))
        .account(
            "mint",
            account::readonly()
                .owner(TOKEN_PROGRAM_ID)
                .min_data_length(82),
        )
        .account("payer", account::signer().writable())
        .account("wallet", account::readonly())
        .account("ata", account::writable())
        .step(ensure_associated_token_account(AtaAccounts {
            associated_token_program: "associatedTokenProgram".into(),
            payer: "payer".into(),
            associated_token_account: "ata".into(),
            owner: "wallet".into(),
            mint: "mint".into(),
            system_program: "systemProgram".into(),
            token_program: "tokenProgram".into(),
        }))
}
// #endregion conditional-ata-setup

// #region consolidate-only-the-funded-accounts
/// Move each row's whole token balance into the vault, skipping empty accounts.
pub fn consolidate_only_the_funded_accounts() -> Template {
    Template::new()
        .account("tokenProgram", account::program(TOKEN_PROGRAM_ID))
        .account(
            "vault",
            account::writable()
                .owner(TOKEN_PROGRAM_ID)
                .min_data_length(165),
        )
        .account("authority", account::signer())
        .batch(
            Batch::new(8).min_iterations(1).account(
                "source",
                account::writable()
                    .owner(TOKEN_PROGRAM_ID)
                    .min_data_length(165),
            ),
        )
        .step(
            step::for_each()
                .step(step::let_(
                    "amount",
                    account_data(account::iteration("source"), 64, ReadType::U64),
                ))
                .step(
                    token_transfer(
                        "tokenProgram",
                        account::iteration("source"),
                        "vault",
                        "authority",
                        var("amount"),
                    )
                    .when(var("amount").gt(u64(0))),
                ),
        )
}
// #endregion consolidate-only-the-funded-accounts

// #region crank-once-per-waiting-entry
/// Crank the queue once for each waiting entry, at most eight times.
pub fn crank_once_per_waiting_entry() -> Template {
    // Stand-ins so the example runs as written: replace them with the queue program's address,
    // its crank instruction data, and the offset of the waiting count in its queue account.
    const QUEUE_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID;
    const CRANK_DATA: [u8; 12] = [2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0];
    const WAITING_OFFSET: u32 = 8;

    Template::new()
        .account("queueProgram", account::program(QUEUE_PROGRAM))
        .account("keeper", account::signer().writable())
        .account("queue", account::writable().owner(QUEUE_PROGRAM))
        .step(
            step::repeat(
                account_data("queue", WAITING_OFFSET, ReadType::U64).min(u64(8)),
                8,
            )
            .step(
                step::invoke("queueProgram")
                    .writable_signer("keeper")
                    .writable("queue")
                    .data(data::literal(CRANK_DATA)),
            ),
        )
}
// #endregion crank-once-per-waiting-entry

// #region crank-only-the-ripe-entries
/// Settle each queue entry whose deadline has passed; skip the rest.
pub fn crank_only_the_ripe_entries() -> Template {
    // Stand-ins so the example runs as written: replace them with the queue program's address,
    // its settle instruction data, and the offset of the deadline in its entry account.
    const QUEUE_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID;
    const SETTLE_DISCRIMINATOR: [u8; 4] = [2, 0, 0, 0];
    const SETTLE_ARGUMENT: u64 = 10_000;
    const DEADLINE_OFFSET: u32 = 8;

    Template::new()
        .account("queueProgram", account::program(QUEUE_PROGRAM))
        .account("keeper", account::signer().writable())
        .batch(
            Batch::new(8).min_iterations(1).account(
                "entry",
                account::writable()
                    .owner(QUEUE_PROGRAM)
                    .min_data_length(128),
            ),
        )
        .step(
            step::for_each().step(
                step::invoke("queueProgram")
                    .writable_signer("keeper")
                    .writable(account::iteration("entry"))
                    .data(data::literal(SETTLE_DISCRIMINATOR))
                    .data(data::u64(u64(SETTLE_ARGUMENT)))
                    .when(
                        account_data(account::iteration("entry"), DEADLINE_OFFSET, ReadType::I64)
                            .lte(clock_unix_timestamp()),
                    ),
            ),
        )
}
// #endregion crank-only-the-ripe-entries

// #region deadline-and-minimum-output
/// Forward the client's swap only if the quote is unexpired and promises at least `minimumOut`.
pub fn deadline_and_minimum_output() -> Template {
    // Stand-in so the example runs as written: replace it with the swap program's address.
    const SWAP_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID;

    Template::new()
        .input("deadline", Type::I64)
        .input("quotedOut", Type::U64)
        .input("minimumOut", Type::U64)
        .input("routeData", Type::Bytes(256))
        .account("swapProgram", account::program(SWAP_PROGRAM))
        .account("payer", account::signer().writable())
        .account("pool", account::writable())
        .step(step::require(
            clock_unix_timestamp()
                .lte(input("deadline"))
                .and(input("quotedOut").gte(input("minimumOut"))),
        ))
        .step(
            step::invoke("swapProgram")
                .writable_signer("payer")
                .writable("pool")
                .data(data::bytes(input("routeData"))),
        )
}
// #endregion deadline-and-minimum-output

// #region deadline-refund
/// Refund the customer only if the run executes at or before `deadline`.
pub fn deadline_refund() -> Template {
    Template::new()
        .input("refundAmount", Type::U64)
        .input("deadline", Type::I64)
        .account("systemProgram", account::program(SYSTEM_PROGRAM_ID))
        .account("escrowAuthority", account::signer().writable())
        .account("customer", account::writable())
        .step(
            system_transfer(
                "systemProgram",
                "escrowAuthority",
                "customer",
                input("refundAmount"),
            )
            .when(clock_unix_timestamp().lte(input("deadline"))),
        )
}
// #endregion deadline-refund

// #region distribute-a-runtime-pot-pro-rata
/// Pay each holder `weightBps` of the vault's balance above `reserve`.
pub fn distribute_a_runtime_pot_pro_rata() -> Template {
    Template::new()
        .input("reserve", Type::U64)
        .account("systemProgram", account::program(SYSTEM_PROGRAM_ID))
        .account("vault", account::signer().writable())
        .batch(
            Batch::new(8)
                .min_iterations(1)
                .account("holder", account::writable())
                .input("weightBps", Type::U64),
        )
        .step(step::let_("pot", lamports("vault") - input("reserve")))
        .step(step::for_each().step(system_transfer(
            "systemProgram",
            "vault",
            account::iteration("holder"),
            var("pot") * row_input("weightBps") / u64(10_000),
        )))
}
// #endregion distribute-a-runtime-pot-pro-rata

// #region exact-lamport-delta
/// Transfer `amount` lamports, then require the sender's balance fell by exactly that much.
pub fn exact_lamport_delta() -> Template {
    Template::new()
        .input("amount", Type::U64)
        .account("systemProgram", account::program(SYSTEM_PROGRAM_ID))
        .account("sender", account::signer().writable())
        .account("recipient", account::writable())
        // A run accepts one account in both slots, so require two different accounts.
        .step(
            step::require(account_key("sender").ne(account_key("recipient")))
                .label("distinctAccounts"),
        )
        .step(step::snapshot("before", lamports("sender")))
        .step(system_transfer(
            "systemProgram",
            "sender",
            "recipient",
            input("amount"),
        ))
        .step(step::require(
            lamports("sender").eq(snapshot("before") - input("amount")),
        ))
}
// #endregion exact-lamport-delta

// #region exact-token-debit
/// Transfer `amount` tokens, then require the source fell by exactly that much.
pub fn exact_token_debit() -> Template {
    Template::new()
        .input("amount", Type::U64)
        .account("tokenProgram", account::program(TOKEN_PROGRAM_ID))
        .account(
            "source",
            account::writable()
                .owner(TOKEN_PROGRAM_ID)
                .min_data_length(165),
        )
        .account(
            "destination",
            account::writable()
                .owner(TOKEN_PROGRAM_ID)
                .min_data_length(165),
        )
        .account("authority", account::signer())
        .step(step::snapshot(
            "before",
            account_data("source", 64, ReadType::U64),
        ))
        .step(token_transfer(
            "tokenProgram",
            "source",
            "destination",
            "authority",
            input("amount"),
        ))
        .step(step::require(
            account_data("source", 64, ReadType::U64).eq(snapshot("before") - input("amount")),
        ))
}
// #endregion exact-token-debit

// #region existing-account-token-payroll
/// Pay `amount` tokens from one source to each of 1 to 32 existing token accounts.
pub fn existing_account_token_payroll() -> Template {
    Template::new()
        .input("amount", Type::U64)
        .account("tokenProgram", account::program(TOKEN_PROGRAM_ID))
        .account(
            "source",
            account::writable()
                .owner(TOKEN_PROGRAM_ID)
                .min_data_length(165),
        )
        .account("authority", account::signer())
        .batch(
            Batch::new(32).min_iterations(1).account(
                "destination",
                account::writable()
                    .owner(TOKEN_PROGRAM_ID)
                    .min_data_length(165),
            ),
        )
        .step(step::for_each().step(token_transfer(
            "tokenProgram",
            "source",
            account::iteration("destination"),
            "authority",
            input("amount"),
        )))
}
// #endregion existing-account-token-payroll

// #region forward-the-whole-token-balance
/// Move a token account's entire balance, read during the run, to another token account.
pub fn forward_the_whole_token_balance() -> Template {
    /// SPL Token account layout: the balance is the u64 at byte 64 of a 165-byte account.
    const TOKEN_ACCOUNT_AMOUNT_OFFSET: u32 = 64;
    const TOKEN_ACCOUNT_LENGTH: u32 = 165;

    Template::new()
        .account("tokenProgram", account::program(TOKEN_PROGRAM_ID))
        // Token-owned and 165 bytes or more: a token account, or a multisig the transfer refuses.
        .account(
            "source",
            account::writable()
                .owner(TOKEN_PROGRAM_ID)
                .min_data_length(TOKEN_ACCOUNT_LENGTH),
        )
        .account(
            "destination",
            account::writable()
                .owner(TOKEN_PROGRAM_ID)
                .min_data_length(TOKEN_ACCOUNT_LENGTH),
        )
        .account("authority", account::signer())
        .step(step::let_(
            "balance",
            account_data("source", TOKEN_ACCOUNT_AMOUNT_OFFSET, ReadType::U64),
        ))
        .step(step::require(var("balance").gt(u64(0))))
        .step(token_transfer(
            "tokenProgram",
            "source",
            "destination",
            "authority",
            var("balance"),
        ))
}
// #endregion forward-the-whole-token-balance

// #region generic-cpi
/// A CPI built from parts: literal bytes, an encoded `u64`, and caller bytes, only when `enabled`.
pub fn generic_cpi() -> Template {
    // Placeholders: replace them with your program's address and its instruction discriminator.
    const MY_PROGRAM: [u8; 32] = [7; 32];
    const MY_DISCRIMINATOR: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];

    Template::new()
        .input("amount", Type::U64)
        .input("clientPayload", Type::Bytes(128))
        .input("enabled", Type::Bool)
        .account("program", account::program(MY_PROGRAM))
        .account("vault", account::writable())
        .account("authority", account::signer())
        .step(
            step::invoke("program")
                .writable("vault")
                .signer("authority")
                .data(data::literal(MY_DISCRIMINATOR))
                .data(data::u64(input("amount")))
                .data(data::bytes(input("clientPayload")))
                .when(input("enabled")),
        )
}
// #endregion generic-cpi

// #region index-weighted-rewards
/// Pay the recipient in row `i` (counting from 0) `(i + 1) × base` lamports.
pub fn index_weighted_rewards() -> Template {
    Template::new()
        .input("base", Type::U64)
        .account("systemProgram", account::program(SYSTEM_PROGRAM_ID))
        .account("treasury", account::signer().writable())
        .batch(
            Batch::new(30)
                .min_iterations(1)
                .account("recipient", account::writable()),
        )
        .step(step::for_each().step(system_transfer(
            "systemProgram",
            "treasury",
            account::iteration("recipient"),
            (loop_index() + u64(1)) * input("base"),
        )))
}
// #endregion index-weighted-rewards

// #region initialize-only-if-missing
/// Call the initialize instruction only when the position account holds no data yet.
pub fn initialize_only_if_missing() -> Template {
    // Stand-ins so the example runs as written: replace them with your program's address and its
    // initialize instruction data.
    const PROTOCOL_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID;
    const INITIALIZE_DISCRIMINATOR: [u8; 4] = [2, 0, 0, 0];
    const INITIALIZE_ARGUMENT: u64 = 10_000;

    Template::new()
        .account("protocolProgram", account::program(PROTOCOL_PROGRAM))
        .account("payer", account::signer().writable())
        .account("position", account::writable())
        .step(
            step::invoke("protocolProgram")
                .writable_signer("payer")
                .writable("position")
                .data(data::literal(INITIALIZE_DISCRIMINATOR))
                .data(data::u64(u64(INITIALIZE_ARGUMENT)))
                .when(is_empty("position")),
        )
}
// #endregion initialize-only-if-missing

// #region liquidate-only-when-unhealthy
/// Liquidate only when the position's health value is below `threshold`.
pub fn liquidate_only_when_unhealthy() -> Template {
    // Stand-ins so the example runs as written: replace them with the lending program's address,
    // its liquidate instruction data, and the offset of the health value in its position account.
    const LENDING_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID;
    const LIQUIDATE_DISCRIMINATOR: [u8; 4] = [2, 0, 0, 0];
    const LIQUIDATE_ARGUMENT: u64 = 10_000;
    const HEALTH_OFFSET: u32 = 8;

    Template::new()
        .input("threshold", Type::U64)
        .account("lendingProgram", account::program(LENDING_PROGRAM))
        .account(
            "position",
            account::readonly()
                .owner(LENDING_PROGRAM)
                .min_data_length(128),
        )
        .account("liquidator", account::signer().writable())
        .account("vault", account::writable())
        .step(
            step::invoke("lendingProgram")
                .writable_signer("liquidator")
                .writable("vault")
                .data(data::literal(LIQUIDATE_DISCRIMINATOR))
                .data(data::u64(u64(LIQUIDATE_ARGUMENT)))
                .when(
                    account_data("position", HEALTH_OFFSET, ReadType::U64).lt(input("threshold")),
                ),
        )
}
// #endregion liquidate-only-when-unhealthy

// #region maximum-lamport-spend
/// Make the call, then fail the run if the payer's balance fell by more than `maximumSpend`.
pub fn maximum_lamport_spend() -> Template {
    // Stand-ins so the example runs as written: replace them with the protocol call to protect.
    const PROTOCOL_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID;
    const INSTRUCTION_DISCRIMINATOR: [u8; 4] = [2, 0, 0, 0];
    const INSTRUCTION_ARGUMENT: u64 = 10_000;

    Template::new()
        .input("maximumSpend", Type::U64)
        .account("protocolProgram", account::program(PROTOCOL_PROGRAM))
        .account("payer", account::signer().writable())
        .account("pool", account::writable())
        .step(step::snapshot("before", lamports("payer")))
        .step(
            step::invoke("protocolProgram")
                .writable_signer("payer")
                .writable("pool")
                .data(data::literal(INSTRUCTION_DISCRIMINATOR))
                .data(data::u64(u64(INSTRUCTION_ARGUMENT))),
        )
        .step(step::require(
            (snapshot("before") - lamports("payer")).lte(input("maximumSpend")),
        ))
}
// #endregion maximum-lamport-spend

// #region named-inputs
/// The inputs from "Named inputs" on the expressions page, which shows them alone. The step is
/// there only so the template compiles.
pub fn named_inputs() -> Template {
    Template::new()
        .input("amount", Type::U64)
        .input("deadline", Type::I64)
        .input("enabled", Type::Bool)
        .input("routeData", Type::Bytes(512))
        .step(step::require(input("enabled")))
}
// #endregion named-inputs

// #region nested-swap-then-deposit
/// Run the inner swap template, then deposit exactly what it returned.
pub fn nested_swap_then_deposit() -> Template {
    // Stand-ins so the example runs as written: replace them with the swap and vault programs, the
    // vault's deposit discriminator, and the inner template's address (`find_template_pda` gives
    // it).
    const SWAP_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID;
    const VAULT_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID;
    const DEPOSIT_DISCRIMINATOR: [u8; 4] = [2, 0, 0, 0];
    const INNER_TEMPLATE: [u8; 32] = [7; 32];

    Template::new()
        // The inner run's data after the `run` tag.
        .input("innerRun", Type::Bytes(512))
        .account("ballista", account::program(ballista_sdk::ID))
        .account("innerTemplate", account::readonly().address(INNER_TEMPLATE))
        .account("swapProgram", account::program(SWAP_PROGRAM))
        .account("vaultProgram", account::program(VAULT_PROGRAM))
        .account("payer", account::signer().writable())
        .account("pool", account::writable())
        .account("receivedTokens", account::writable())
        .step(
            step::invoke("ballista")
                // The inner template, then the inner run's accounts in the order it declares them.
                .readonly("innerTemplate")
                .readonly("swapProgram")
                .writable_signer("payer")
                .writable("pool")
                .writable("receivedTokens")
                .data(data::literal([IX_RUN]))
                .data(data::bytes(input("innerRun"))),
        )
        // Straight after the call.
        .step(step::let_("received", return_data(ReadType::U64)))
        .step(
            step::invoke("vaultProgram")
                .writable_signer("payer")
                .writable("pool")
                .data(data::literal(DEPOSIT_DISCRIMINATOR))
                .data(data::u64(var("received"))),
        )
}
// #endregion nested-swap-then-deposit

// #region oracle-price-band
/// Call the protocol only while the oracle's price lies within `[minimumPrice, maximumPrice]`.
pub fn oracle_price_band() -> Template {
    // Stand-ins so the example runs as written: replace them with the oracle program (the owner of
    // the price account), the price's offset in its layout, and the protocol call. A real template
    // also checks which feed the account holds and when its price was published.
    const ORACLE_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID;
    const PRICE_OFFSET: u32 = 8;
    const PROTOCOL_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID;
    const INSTRUCTION_DISCRIMINATOR: [u8; 4] = [2, 0, 0, 0];
    const INSTRUCTION_ARGUMENT: u64 = 10_000;

    let price = account_data("oracle", PRICE_OFFSET, ReadType::I64);

    Template::new()
        .input("minimumPrice", Type::I64)
        .input("maximumPrice", Type::I64)
        .account(
            "oracle",
            account::readonly()
                .owner(ORACLE_PROGRAM)
                .min_data_length(128),
        )
        .account("protocolProgram", account::program(PROTOCOL_PROGRAM))
        .account("payer", account::signer().writable())
        .account("pool", account::writable())
        .step(step::require(
            price
                .gte(input("minimumPrice"))
                .and(price.lte(input("maximumPrice"))),
        ))
        .step(
            step::invoke("protocolProgram")
                .writable_signer("payer")
                .writable("pool")
                .data(data::literal(INSTRUCTION_DISCRIMINATOR))
                .data(data::u64(u64(INSTRUCTION_ARGUMENT))),
        )
}
// #endregion oracle-price-band

// #region payment-agent
/// An agent pays at most 0.1 SOL at once and 1 SOL a day, and keeps 0.05 SOL in its wallet.
pub fn payment_agent() -> Template {
    Template::new()
        .input("amount", Type::U64)
        .registry("limits", [("spent", Type::U64), ("lastSpend", Type::I64)])
        .account("agent", account::signer().writable())
        .account("recipient", account::writable())
        .account(
            "agentLimit",
            account::registry("limits", "agent").key(account_key("agent")),
        )
        .account("systemProgram", account::system_program())
        .step(
            step::require(input("amount").lte(u64(100_000_000))) // 0.1 SOL
                .label("perPaymentCap"),
        )
        .steps(
            rate_limit(
                "agentLimit",       // the entry's account, not the registry
                u64(1_000_000_000), // cap: 1 SOL
                u64(11_574),        // refill per second: 1 SOL over 86,400 seconds, rounded down
                input("amount"),
            )
            .name("dailyCap"), // labels the check `withinDailyCap`
        )
        .step(system_transfer(
            "systemProgram",
            "agent",
            "recipient",
            input("amount"),
        ))
        .step(
            step::require(lamports("agent").gte(u64(50_000_000))) // 0.05 SOL
                .label("keepsReserve"),
        )
}
// #endregion payment-agent

// #region pinned-program-and-owner
/// Call one program, with the program address and the position's owner pinned in the schema.
pub fn pinned_program_and_owner() -> Template {
    // Stand-ins so the example runs as written: replace them with your protocol's address and the
    // instruction's discriminator.
    const PROTOCOL_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID;
    const INSTRUCTION_DISCRIMINATOR: [u8; 4] = [2, 0, 0, 0];

    Template::new()
        .input("amount", Type::U64)
        .account("protocolProgram", account::program(PROTOCOL_PROGRAM))
        .account("payer", account::signer().writable())
        .account(
            "position",
            account::writable()
                .owner(PROTOCOL_PROGRAM)
                .min_data_length(128),
        )
        .step(
            step::invoke("protocolProgram")
                .writable_signer("payer")
                .writable("position")
                .data(data::literal(INSTRUCTION_DISCRIMINATOR))
                .data(data::u64(input("amount"))),
        )
}
// #endregion pinned-program-and-owner

// #region primary-or-fallback-route
/// Call exactly one of two routes, chosen by `usePrimary`.
pub fn primary_or_fallback_route() -> Template {
    // Stand-ins so the example runs as written: replace them with the two routes' programs.
    const PRIMARY_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID;
    const FALLBACK_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID;

    let use_primary = input("usePrimary");

    Template::new()
        .input("usePrimary", Type::Bool)
        .input("primaryData", Type::Bytes(256))
        .input("fallbackData", Type::Bytes(256))
        .account("primaryProgram", account::program(PRIMARY_PROGRAM))
        .account("fallbackProgram", account::program(FALLBACK_PROGRAM))
        .account("payer", account::signer().writable())
        .account("pool", account::writable())
        .step(
            step::invoke("primaryProgram")
                .writable_signer("payer")
                .writable("pool")
                .data(data::bytes(input("primaryData")))
                .when(&use_primary),
        )
        .step(
            step::invoke("fallbackProgram")
                .writable_signer("payer")
                .writable("pool")
                .data(data::bytes(input("fallbackData")))
                .when(use_primary.not()),
        )
}
// #endregion primary-or-fallback-route

// #region rebalance-three-swaps
// SPL Token account layout: the owner is the pubkey at byte 32, the amount the u64 at byte 64.
const OWNER_OFFSET: u32 = 32;
const AMOUNT_OFFSET: u32 = 64;

/// The five steps of one optional swap: leg `A` uses sourceA, destinationA, routeA and group ammA.
fn swap_leg(leg: &str) -> Vec<Step> {
    let destination = format!("destination{leg}");
    let needed = var(format!("need{leg}"));
    vec![
        step::snapshot(
            format!("balance{leg}"),
            account_data(&destination, AMOUNT_OFFSET, ReadType::U64),
        ),
        step::let_(
            format!("need{leg}"),
            snapshot(format!("balance{leg}")).lt(input(format!("target{leg}"))),
        ),
        step::require(account_data(&destination, OWNER_OFFSET, ReadType::Pubkey).eq(key("user"))),
        step::invoke("jupiter")
            // Jupiter's leading accounts, in its order; the quote's pools follow as the group.
            .readonly("tokenProgram")
            .signer("user")
            .writable(format!("source{leg}"))
            .writable(&destination)
            .account_group(format!("amm{leg}"))
            .data(data::bytes(input(format!("route{leg}"))))
            .when(needed.clone())
            .into(),
        // Skipped, or the balance rose by at least the minimum.
        step::require(
            needed
                .not()
                .or((account_data(&destination, AMOUNT_OFFSET, ReadType::U64)
                    - snapshot(format!("balance{leg}")))
                .gte(input(format!("minOut{leg}")))),
        ),
    ]
}

/// Three optional swaps, each forwarding its own account group, each checked afterwards.
pub fn rebalance_three_swaps() -> Template {
    const JUPITER_V6: Pubkey =
        Pubkey::from_str_const("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4");
    // No fixed mint: the caller decides which tokens are involved, and the checks decide whether
    // the result is acceptable. The owner and size pins let the template read the accounts' data.
    let token_account = account::writable()
        .owner(TOKEN_PROGRAM_ID)
        .min_data_length(165);

    Template::new()
        .account("jupiter", account::program(JUPITER_V6))
        .account("tokenProgram", account::program(TOKEN_PROGRAM_ID))
        .account("user", account::signer().writable())
        .account("sourceA", token_account.clone())
        .account("destinationA", token_account.clone())
        .account("sourceB", token_account.clone())
        .account("destinationB", token_account.clone())
        .account("sourceC", token_account.clone())
        .account("destinationC", token_account)
        .input("routeA", Type::Bytes(256))
        .input("routeB", Type::Bytes(256))
        .input("routeC", Type::Bytes(256))
        .input("targetA", Type::U64)
        .input("targetB", Type::U64)
        .input("targetC", Type::U64)
        .input("minOutA", Type::U64)
        .input("minOutB", Type::U64)
        .input("minOutC", Type::U64)
        .account_group("ammA")
        .account_group("ammB")
        .account_group("ammC")
        .steps(swap_leg("A"))
        .steps(swap_leg("B"))
        .steps(swap_leg("C"))
}
// #endregion rebalance-three-swaps

// #region swap-through-a-checked-route
/// One swap over a caller-supplied route that may hold none of the user's other token accounts.
pub fn swap_through_a_checked_route() -> Template {
    const JUPITER_V6: Pubkey =
        Pubkey::from_str_const("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4");

    Template::new()
        .input("route", Type::Bytes(256))
        .account("jupiter", account::program(JUPITER_V6))
        .account("tokenProgram", account::program(TOKEN_PROGRAM_ID))
        .account("user", account::signer())
        .account("source", account::writable())
        .account("destination", account::writable())
        .account_group("amm")
        .step(
            step::require(
                group_any(
                    "amm",
                    // A Token or Token-2022 account whose owner, the pubkey at byte 32, is the
                    // user...
                    GroupFilter::new()
                        .program(TOKEN_PROGRAM_ID)
                        .program(TOKEN_2022_PROGRAM_ID)
                        .equals(32, key("user"))
                        // ...other than the two this swap is meant to touch.
                        .except_key(key("source"))
                        .except_key(key("destination")),
                )
                .not(),
            )
            .label("noOtherUserTokenAccount"),
        )
        .step(
            step::invoke("jupiter")
                .readonly("tokenProgram")
                .signer("user")
                .writable("source")
                .writable("destination")
                .account_group("amm")
                .data(data::bytes(input("route"))),
        )
}
// #endregion swap-through-a-checked-route

// #region reserve-preserving-sweep
/// Move up to `cap` lamports to the vault without taking the payer below `reserve`.
pub fn reserve_preserving_sweep() -> Template {
    Template::new()
        .input("reserve", Type::U64)
        .input("cap", Type::U64)
        .account("systemProgram", account::program(SYSTEM_PROGRAM_ID))
        .account("payer", account::signer().writable())
        .account("vault", account::writable())
        .step(step::snapshot("before", lamports("payer")))
        .step(step::require(snapshot("before").gte(input("reserve"))))
        .step(system_transfer(
            "systemProgram",
            "payer",
            "vault",
            (snapshot("before") - input("reserve")).min(input("cap")),
        ))
        .step(step::require(lamports("payer").gte(input("reserve"))))
}
// #endregion reserve-preserving-sweep

// #region split-what-arrived
/// Pay a partner `shareBps` of the vault's balance above `reserve`, and the treasury the rest.
pub fn split_what_arrived() -> Template {
    Template::new()
        .input("reserve", Type::U64)
        .input("shareBps", Type::U64)
        .account("systemProgram", account::program(SYSTEM_PROGRAM_ID))
        .account("vault", account::signer().writable())
        .account("partner", account::writable())
        .account("treasury", account::writable())
        .step(step::let_(
            "distributable",
            lamports("vault") - input("reserve"),
        ))
        .step(step::let_(
            "partnerShare",
            var("distributable") * input("shareBps") / u64(10_000),
        ))
        .step(system_transfer(
            "systemProgram",
            "vault",
            "partner",
            var("partnerShare"),
        ))
        .step(system_transfer(
            "systemProgram",
            "vault",
            "treasury",
            var("distributable") - var("partnerShare"),
        ))
}
// #endregion split-what-arrived

// #region swap-and-return-what-arrived
/// Swap, require at least `minimumOut` arrived, and return what arrived as a `u64`.
pub fn swap_and_return_what_arrived() -> Template {
    // A stand-in so the example runs as written: replace it with the swap program.
    const SWAP_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID;

    let received_balance = account_data("receivedTokens", 64, ReadType::U64);

    Template::new()
        .input("minimumOut", Type::U64)
        .input("swapData", Type::Bytes(256))
        .account("swapProgram", account::program(SWAP_PROGRAM))
        .account("payer", account::signer().writable())
        .account("pool", account::writable())
        .account(
            "receivedTokens",
            account::writable()
                .owner(TOKEN_PROGRAM_ID)
                .min_data_length(165),
        )
        .step(step::snapshot("before", received_balance.clone()))
        .step(
            step::invoke("swapProgram")
                .writable_signer("payer")
                .writable("pool")
                .data(data::bytes(input("swapData"))),
        )
        .step(step::let_(
            "received",
            received_balance - snapshot("before"),
        ))
        .step(step::require(var("received").gte(input("minimumOut"))))
        // Last, after every call: a call would clear it.
        .step(step::set_return_data([data::u64(var("received"))]))
}
// #endregion swap-and-return-what-arrived

// #region swap-then-deposit
/// Swap, require at least `minimumOut` arrived, then deposit.
pub fn swap_then_deposit() -> Template {
    // Stand-ins so the example runs as written: replace them with the swap and vault programs.
    const SWAP_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID;
    const VAULT_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID;

    let received_balance = account_data("receivedTokens", 64, ReadType::U64);

    Template::new()
        .input("minimumOut", Type::U64)
        .input("swapData", Type::Bytes(256))
        .input("depositData", Type::Bytes(256))
        .account("swapProgram", account::program(SWAP_PROGRAM))
        .account("vaultProgram", account::program(VAULT_PROGRAM))
        .account("payer", account::signer().writable())
        .account("pool", account::writable())
        .account(
            "receivedTokens",
            account::writable()
                .owner(TOKEN_PROGRAM_ID)
                .min_data_length(165),
        )
        .step(step::snapshot("before", received_balance.clone()))
        .step(
            step::invoke("swapProgram")
                .writable_signer("payer")
                .writable("pool")
                .data(data::bytes(input("swapData"))),
        )
        // `received_balance` is read again here, after the swap.
        .step(step::require(
            (received_balance - snapshot("before")).gte(input("minimumOut")),
        ))
        .step(
            step::invoke("vaultProgram")
                .writable_signer("payer")
                .writable("pool")
                .data(data::bytes(input("depositData"))),
        )
}
// #endregion swap-then-deposit

// #region time-gated-governance-execution
/// Forward the execute instruction only once the proposal is approved and its time has come.
pub fn time_gated_governance_execution() -> Template {
    // Stand-ins so the example runs as written: replace them with your governance program's
    // address and the offsets of the two fields in its proposal account.
    const GOVERNANCE_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID;
    const APPROVED_OFFSET: u32 = 0;
    const TIME_OFFSET: u32 = 8;

    let approved = account_data("proposal", APPROVED_OFFSET, ReadType::Bool);
    let executable_after = account_data("proposal", TIME_OFFSET, ReadType::I64);

    Template::new()
        .input("executeData", Type::Bytes(256))
        .account("governanceProgram", account::program(GOVERNANCE_PROGRAM))
        .account(
            "proposal",
            account::readonly()
                .owner(GOVERNANCE_PROGRAM)
                .min_data_length(128),
        )
        .account("payer", account::signer().writable())
        .account("target", account::writable())
        .step(step::require(
            approved.and(clock_unix_timestamp().gte(executable_after)),
        ))
        .step(
            step::invoke("governanceProgram")
                .writable_signer("payer")
                .writable("target")
                .data(data::bytes(input("executeData"))),
        )
}
// #endregion time-gated-governance-execution

// #region token-transfer
/// One SPL Token transfer: the Token Program pinned, both token accounts pinned by owner and size.
pub fn token_transfer_template() -> Template {
    Template::new()
        .input("amount", Type::U64)
        .account("tokenProgram", account::program(TOKEN_PROGRAM_ID))
        .account("authority", account::signer())
        .account(
            "source",
            account::writable()
                .owner(TOKEN_PROGRAM_ID)
                .min_data_length(165),
        )
        .account(
            "destination",
            account::writable()
                .owner(TOKEN_PROGRAM_ID)
                .min_data_length(165),
        )
        .step(token_transfer(
            "tokenProgram",
            "source",
            "destination",
            "authority",
            input("amount"),
        ))
}
// #endregion token-transfer

// #region top-up-to-a-target
/// Bring the bot's balance up to `target` lamports; send nothing when it already has that much.
pub fn top_up_to_a_target() -> Template {
    Template::new()
        .input("target", Type::U64)
        .account("systemProgram", account::program(SYSTEM_PROGRAM_ID))
        .account("funder", account::signer().writable())
        .account("bot", account::writable())
        .step(step::let_("botBalance", lamports("bot")))
        .step(
            system_transfer(
                "systemProgram",
                "funder",
                "bot",
                // target - botBalance. The amount is worked out even when `when` skips the
                // transfer, so the `min` keeps it from going below zero, which would fail the run.
                input("target") - var("botBalance").min(input("target")),
            )
            .when(var("botBalance").lt(input("target"))),
        )
}
// #endregion top-up-to-a-target

// #region waterfall-until-the-money-runs-out
/// Pay creditors in row order, each the smaller of what it is owed and what is left.
pub fn waterfall_until_the_money_runs_out() -> Template {
    Template::new()
        .input("reserve", Type::U64)
        .account("systemProgram", account::program(SYSTEM_PROGRAM_ID))
        .account("treasury", account::signer().writable())
        .batch(
            Batch::new(8)
                .min_iterations(1)
                .account("creditor", account::writable())
                .input("owed", Type::U64),
        )
        .step(step::let_(
            "remaining",
            lamports("treasury") - input("reserve"),
        ))
        .step(
            step::for_each()
                .step(step::let_("pay", var("remaining").min(row_input("owed"))))
                .step(
                    system_transfer(
                        "systemProgram",
                        "treasury",
                        account::iteration("creditor"),
                        var("pay"),
                    )
                    .when(var("pay").gt(u64(0))),
                )
                .step(step::assign("remaining", var("remaining") - var("pay")))
                .carry("remaining"),
        )
}
// #endregion waterfall-until-the-money-runs-out
