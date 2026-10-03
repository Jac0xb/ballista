//! The TypeScript compiler's tests (`clients/js/src/compiler.test.ts` and
//! `register-reuse.test.ts`), ported to the declarative Rust API: what it refuses, with the same
//! message, and the structure of what it accepts.
//!
//! Messages match the TypeScript compiler's except where the TypeScript one names its own API
//! (`unsafeUnpinned: true`, `expression.registry(...)`) or is a Zod schema issue: those carry the
//! Rust wording of `src/template/compile.rs` and `src/template/validate.rs`.

use ballista_sdk::ballista_common::template::{self as wire, ProgramView};
use ballista_sdk::template::prelude::*;
use ballista_sdk::{
    ASSOCIATED_TOKEN_PROGRAM_ID, ED25519_PROGRAM_ID, INSTRUCTIONS_SYSVAR_ID, SYSTEM_PROGRAM_ID,
    TOKEN_PROGRAM_ID,
};

const TAG: &[u8] = b"TAG1";
const HEADER_LENGTH: usize = 24;
const ACCOUNT_RECORD_LENGTH: usize = 8;
const NONE: u8 = 0xff;

fn address(byte: u8) -> Pubkey {
    Pubkey::new_from_array([byte; 32])
}

fn compile(template: Template) -> CompiledTemplate {
    match template.compile() {
        Ok(compiled) => compiled,
        Err(error) => panic!("expected the template to compile: {error}"),
    }
}

#[track_caller]
fn fails(template: Template, message: &str) {
    match template.compile() {
        Ok(_) => panic!("expected a failure containing {message:?}, but the template compiled"),
        Err(error) => assert!(
            error.message().contains(message),
            "expected a failure containing {message:?}, got {:?}",
            error.message()
        ),
    }
}

#[track_caller]
fn fails_starting(template: Template, prefix: &str) {
    let error = template.compile().expect_err("expected a failure");
    assert!(
        error.message().starts_with(prefix),
        "expected a failure starting {prefix:?}, got {:?}",
        error.message()
    );
}

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn u64_at(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}

/// Byte offset of instruction `pc` inside a compiled payload.
fn instruction_offset(compiled: &CompiledTemplate, pc: usize) -> usize {
    let accounts = compiled.stats.fixed_accounts + compiled.stats.batch_stride;
    let inputs = compiled.stats.inputs + compiled.stats.row_inputs;
    HEADER_LENGTH + accounts * ACCOUNT_RECORD_LENGTH + inputs * 4 + pc * 16
}

/// Every 16-byte instruction record of a compiled payload.
fn records(compiled: &CompiledTemplate) -> Vec<[u8; 16]> {
    (0..compiled.stats.instructions)
        .map(|pc| {
            let at = instruction_offset(compiled, pc);
            compiled.bytes[at..at + 16].try_into().unwrap()
        })
        .collect()
}

fn records_with(compiled: &CompiledTemplate, opcode: u8) -> Vec<[u8; 16]> {
    records(compiled)
        .into_iter()
        .filter(|record| record[0] == opcode)
        .collect()
}

fn immediate(record: &[u8; 16]) -> u64 {
    u64_at(record, 6)
}

/// A record's range immediate: where the range starts and its length.
fn range_of(record: &[u8; 16]) -> (usize, usize) {
    let value = immediate(record);
    ((value & 0xffff_ffff) as usize, (value >> 32) as usize)
}

/// The minimum data length recorded for fixed account `index`.
fn min_data_length(compiled: &CompiledTemplate, index: usize) -> u32 {
    u32_at(
        &compiled.bytes,
        HEADER_LENGTH + index * ACCOUNT_RECORD_LENGTH + 4,
    )
}

fn blob(compiled: &CompiledTemplate) -> &[u8] {
    let length = u16_at(&compiled.bytes, 18) as usize;
    &compiled.bytes[compiled.bytes.len() - length..]
}

/// Data segments as `(kind, register)`, and CPI descriptors' segment ranges as `(start, end)`.
type SegmentTables = (Vec<(u8, u8)>, Vec<(usize, usize)>);

/// The data segments, as (kind, register), and each CPI descriptor's segment range.
fn segment_tables(compiled: &CompiledTemplate) -> SegmentTables {
    let bytes = &compiled.bytes;
    let cpi_start = instruction_offset(compiled, compiled.stats.instructions);
    let cpi_accounts = u16_at(bytes, 12) as usize;
    let segment_start = cpi_start + compiled.stats.cpis * 12 + cpi_accounts * 2;
    let segments = (0..u16_at(bytes, 14) as usize)
        .map(|index| {
            let at = segment_start + index * 8;
            (bytes[at], bytes[at + 1])
        })
        .collect();
    let cpis = (0..compiled.stats.cpis)
        .map(|index| {
            let at = cpi_start + index * 12;
            (u16_at(bytes, at + 6) as usize, bytes[at + 5] as usize)
        })
        .collect();
    (segments, cpis)
}

fn verifies(compiled: &CompiledTemplate) {
    ProgramView::parse(&compiled.bytes)
        .expect("payload parses")
        .verify()
        .expect("payload verifies");
}

fn transfer() -> Template {
    Template::new()
        .input("amount", Type::U64)
        .account("systemProgram", account::program(SYSTEM_PROGRAM_ID))
        .account("source", account::signer().writable())
        .account("destination", account::writable())
        .step(system_transfer(
            "systemProgram",
            "source",
            "destination",
            input("amount"),
        ))
}

// ---------------------------------------------------------------------------- Ballista compiler

mod ballista_compiler {
    use super::*;

    #[test]
    fn compiles_the_system_transfer_with_stable_stats() {
        let compiled = compile(transfer());
        assert_eq!(compiled.stats.payload_bytes, 152);
        assert_eq!(compiled.stats.instructions, 2);
        assert_eq!(compiled.stats.cpis, 1);
        assert_eq!(compiled.stats.registers, 1);
        assert_eq!(compiled.stats.batch_min_iterations, 0);
        assert!(!compiled.stats.emit_event);
        assert_eq!(compiled.bytes[4], 1);
        let map: Vec<_> = compiled
            .source_map
            .iter()
            .map(|entry| (entry.pc, entry.path.as_str(), entry.label.clone()))
            .collect();
        // The input loads before the first step, so it carries its own path.
        assert_eq!(map, [(0, "inputs.amount", None), (1, "steps[0]", None)]);
        verifies(&compiled);
    }

    #[test]
    fn compiles_a_constant_size_30_recipient_batch_template() {
        let compiled = compile(
            Template::new()
                .input("amount", Type::U64)
                .account("systemProgram", account::program(SYSTEM_PROGRAM_ID))
                .account("source", account::signer().writable())
                .batch(Batch::new(30).account("recipient", account::writable()))
                .step(step::for_each().step(system_transfer(
                    "systemProgram",
                    "source",
                    account::iteration("recipient"),
                    input("amount"),
                ))),
        );
        assert_eq!(compiled.stats.batch_stride, 1);
        assert_eq!(compiled.stats.batch_max_iterations, 30);
        assert_eq!(compiled.stats.max_expanded_cpis, 30);
        assert_eq!(compiled.stats.cpis, 1);
        assert!(compiled.bytes.len() < 200);
    }

    #[test]
    fn guards_non_idempotent_associated_token_creation_on_account_emptiness() {
        let compiled = compile(
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
                        .address(address(4))
                        .owner(TOKEN_PROGRAM_ID)
                        .min_data_length(82),
                )
                .account(
                    "payer",
                    account::signer().writable().owner(SYSTEM_PROGRAM_ID),
                )
                .account("owner", account::readonly())
                .account("associatedTokenAccount", account::writable())
                .step(ensure_associated_token_account(AtaAccounts {
                    associated_token_program: "associatedTokenProgram".into(),
                    payer: "payer".into(),
                    associated_token_account: "associatedTokenAccount".into(),
                    owner: "owner".into(),
                    mint: "mint".into(),
                    system_program: "systemProgram".into(),
                    token_program: "tokenProgram".into(),
                })),
        );
        assert_eq!(compiled.stats.instructions, 2);
        assert_eq!(compiled.stats.cpis, 1);
        assert_eq!(compiled.stats.max_expanded_cpis, 1);
        // The guard is the emptiness check, compiled into the invoke's `b`.
        let all = records(&compiled);
        assert_eq!(all[0][0], wire::OP_ACCOUNT_IS_EMPTY);
        assert_eq!(all[1][0], wire::OP_INVOKE);
        assert_eq!(all[1][3], all[0][1]);
    }

    #[test]
    fn composes_invoke_guards_with_nested_and_and_or_expressions() {
        compile(
            Template::new()
                .input("enabled", Type::Bool)
                .input("withinLimit", Type::Bool)
                .input("force", Type::Bool)
                .account("program", account::executable().unsafe_unpinned())
                .step(
                    step::invoke("program").when(
                        input("enabled")
                            .and(input("withinLimit"))
                            .or(input("force")),
                    ),
                ),
        );
    }

    #[test]
    fn keeps_named_snapshots_in_registers_for_post_cpi_delta_assertions() {
        let compiled = compile(
            Template::new()
                .input("amount", Type::U64)
                .account("systemProgram", account::program(SYSTEM_PROGRAM_ID))
                .account("source", account::signer().writable())
                .account("destination", account::writable())
                .step(step::snapshot("before", lamports("source")))
                .step(system_transfer(
                    "systemProgram",
                    "source",
                    "destination",
                    input("amount"),
                ))
                .step(step::snapshot("after", lamports("source")))
                .step(step::require(
                    snapshot("after").eq(snapshot("before") - input("amount")),
                )),
        );
        // The amount input is read twice but loads once, before the first step.
        assert_eq!(compiled.stats.instructions, 7);
        assert_eq!(compiled.stats.registers, 5);
        fails(
            Template::new().step(step::require(snapshot("missing"))),
            "Unknown variable: missing",
        );
        fails(
            Template::new()
                .step(step::let_("value", bool(true)))
                .step(step::let_("value", bool(false)))
                .step(step::require(var("value"))),
            "Variable already defined: value",
        );
    }

    #[test]
    fn compiles_canonical_pda_and_ata_relationship_assertions() {
        let compiled = compile(
            Template::new()
                .account(
                    "associatedTokenProgram",
                    account::program(ASSOCIATED_TOKEN_PROGRAM_ID),
                )
                .account("tokenProgram", account::program(TOKEN_PROGRAM_ID))
                .account("owner", account::readonly())
                .account("mint", account::readonly())
                .account("associatedTokenAccount", account::readonly())
                .step(assert_ata(
                    "associatedTokenAccount",
                    "owner",
                    "mint",
                    "tokenProgram",
                    "associatedTokenProgram",
                )),
        );
        assert_eq!(compiled.stats.instructions, 7);
        assert_eq!(compiled.stats.registers, 6);
        assert_eq!(compiled.stats.cpis, 0);
        assert_eq!(u16_at(&compiled.bytes, 14), 3);
        assert_eq!(
            compiled.bytes[instruction_offset(&compiled, 4)],
            wire::OP_DERIVE_PDA
        );

        fails(
            Template::new()
                .input("seed", Type::Bytes(33))
                .account("program", account::program(address(9)))
                .account("candidate", account::readonly())
                .step(step::require(
                    key("candidate").eq(pda("program", [input("seed")])),
                )),
            "PDA seed can exceed 32 bytes",
        );
        fails(
            Template::new()
                .account("program", account::readonly())
                .account("candidate", account::readonly())
                .step(step::require(
                    key("candidate").eq(pda("program", [bytes([1])])),
                )),
            "PDA program account must require executable=true",
        );
    }

    #[test]
    fn a_supplied_bump_compiles_to_create_pda_and_must_be_a_u64() {
        let compiled = compile(
            Template::new()
                .input("bump", Type::U64)
                .account(
                    "associatedTokenProgram",
                    account::program(ASSOCIATED_TOKEN_PROGRAM_ID),
                )
                .account("tokenProgram", account::program(TOKEN_PROGRAM_ID))
                .account("mint", account::readonly())
                .account("owner", account::readonly())
                .account("ata", account::readonly())
                .step(
                    assert_ata(
                        "ata",
                        "owner",
                        "mint",
                        "tokenProgram",
                        "associatedTokenProgram",
                    )
                    .bump(input("bump")),
                ),
        );
        assert_eq!(compiled.stats.instructions, 8);
        assert_eq!(compiled.stats.registers, 7);
        assert_eq!(compiled.stats.cpis, 0);
        assert_eq!(
            compiled.bytes[instruction_offset(&compiled, 5)],
            wire::OP_CREATE_PDA
        );

        fails(
            Template::new()
                .account("program", account::program(address(9)))
                .account("candidate", account::readonly())
                .step(step::require(key("candidate").eq(pda_with_bump(
                    "program",
                    [bytes([1])],
                    key("candidate"),
                )))),
            "PDA bump",
        );
    }

    #[test]
    fn requires_pins_for_programs_and_data_reads_unless_the_author_opts_out() {
        fails(
            Template::new()
                .account("program", account::executable())
                .step(step::invoke("program")),
            "Invoke program account program must pin an address",
        );
        fails(
            Template::new()
                .account("program", account::executable())
                .account("candidate", account::readonly())
                .step(step::require(
                    key("candidate").eq(pda("program", [bytes([1])])),
                )),
            "PDA program account program must pin an address",
        );
        fails(
            Template::new()
                .account("holder", account::readonly())
                .step(step::require(
                    account_data("holder", 64, ReadType::U64).eq(u64(1)),
                )),
            "pins neither owner nor address",
        );
        // The Rust message names the Rust opt-out.
        fails(
            Template::new()
                .account("holder", account::readonly())
                .step(step::require(
                    account_data("holder", 64, ReadType::U64).eq(u64(1)),
                )),
            ".unsafe_unpinned()",
        );
        compile(
            Template::new()
                .account("program", account::executable().unsafe_unpinned())
                .account("holder", account::readonly().unsafe_unpinned())
                .step(step::require(
                    account_data("holder", 64, ReadType::U64).eq(u64(1)),
                ))
                .step(step::invoke("program")),
        );
        compile(
            Template::new()
                .account("holder", account::readonly().owner(TOKEN_PROGRAM_ID))
                .step(step::require(
                    account_data("holder", 64, ReadType::U64).eq(u64(1)),
                )),
        );
        fails(
            Template::new()
                .input("amount", Type::U64)
                .account("systemProgram", account::program(TOKEN_PROGRAM_ID))
                .account("source", account::signer().writable())
                .account("destination", account::writable())
                .step(system_transfer(
                    "systemProgram",
                    "source",
                    "destination",
                    input("amount"),
                )),
            "Invoke targets program",
        );
    }

    #[test]
    fn infers_the_minimum_data_length_from_static_reads() {
        let compiled = compile(
            Template::new()
                .account("holder", account::readonly().owner(TOKEN_PROGRAM_ID))
                .account(
                    "declared",
                    account::readonly()
                        .owner(TOKEN_PROGRAM_ID)
                        .min_data_length(100),
                )
                .step(step::require(
                    account_data("holder", 64, ReadType::U64).eq(account_data(
                        "declared",
                        32,
                        ReadType::U64,
                    )),
                )),
        );
        assert_eq!(min_data_length(&compiled, 0), 72);
        assert_eq!(min_data_length(&compiled, 1), 100);
    }

    #[test]
    fn compiles_loop_carried_sums_with_assign_and_move() {
        let compiled = compile(
            Template::new()
                .input("budget", Type::U64)
                .batch(Batch::new(3).account("recipient", account::readonly()))
                .step(step::let_("total", u64(0)))
                .step(
                    step::for_each()
                        .step(step::assign(
                            "total",
                            var("total") + lamports(account::iteration("recipient")),
                        ))
                        .carry("total"),
                )
                .step(step::require(var("total").lte(input("budget"))).label("withinBudget")),
        );
        assert_eq!(compiled.stats.instructions, 8);
        assert_eq!(compiled.stats.registers, 5);
        let for_each_pc = compiled
            .source_map
            .iter()
            .find(|entry| entry.path == "steps[1]")
            .unwrap()
            .pc;
        let record = instruction_offset(&compiled, for_each_pc);
        assert_eq!(compiled.bytes[record], wire::OP_FOREACH);
        assert_eq!(compiled.bytes[record + 2], 3);
        // Register 0 holds the hoisted budget input, so the carried total is register 1.
        assert_eq!(u64_at(&compiled.bytes, record + 6), 1 << 1);
        let moved = instruction_offset(&compiled, for_each_pc + 3);
        assert_eq!(compiled.bytes[moved], wire::OP_MOVE);
        assert_eq!(compiled.bytes[moved + 1], 1);
        let last = compiled.source_map.last().unwrap();
        assert_eq!(
            (last.pc, last.path.as_str(), last.label.as_deref()),
            (7, "steps[2]", Some("withinBudget"))
        );

        fails(
            Template::new()
                .step(step::let_("total", u64(0)))
                .step(step::assign("total", u64(1))),
            "assign is only valid inside a loop",
        );
        fails(
            Template::new()
                .batch(Batch::new(1).account("recipient", account::readonly()))
                .step(step::let_("total", u64(0)))
                .step(step::for_each().step(step::assign("total", u64(1)))),
            "must be listed in the loop's carry",
        );
        fails(
            Template::new()
                .batch(Batch::new(1).account("recipient", account::readonly()))
                .step(
                    step::for_each()
                        .step(step::require(bool(true)))
                        .carry("missing"),
                ),
            "must be defined before the loop",
        );
        fails(
            Template::new()
                .batch(Batch::new(1).account("recipient", account::readonly()))
                .step(step::let_("total", u64(0)))
                .step(
                    step::for_each()
                        .step(step::assign("total", bool(true)))
                        .carry("total"),
                ),
            "must keep its u64 type",
        );
    }

    #[test]
    fn encodes_minimum_iterations() {
        let compiled = compile(
            Template::new()
                .batch(
                    Batch::new(4)
                        .min_iterations(2)
                        .account("recipient", account::readonly()),
                )
                .step(step::for_each().step(step::require(bool(true)))),
        );
        assert_eq!(compiled.bytes[20], 2);
        assert_eq!(compiled.stats.batch_min_iterations, 2);
        fails(
            Template::new()
                .batch(
                    Batch::new(1)
                        .min_iterations(2)
                        .account("recipient", account::readonly()),
                )
                .step(step::for_each().step(step::require(bool(true)))),
            "minIterations cannot exceed maxIterations",
        );
    }

    #[test]
    fn compiles_dynamic_offset_reads_with_the_instruction_flag() {
        let compiled = compile(
            Template::new()
                .input("offset", Type::U64)
                .input("expected", Type::U64)
                .account("holder", account::readonly().owner(TOKEN_PROGRAM_ID))
                .step(step::require(
                    account_data("holder", input("offset"), ReadType::U64).eq(input("expected")),
                )),
        );
        // Both inputs load first, so the read is the third instruction, offset in register 0.
        let read = instruction_offset(&compiled, 2);
        assert_eq!(compiled.bytes[read], wire::OP_READ_U64);
        assert_eq!(compiled.bytes[read + 3], 0);
        assert_eq!(compiled.bytes[read + 5], 1);
        assert_eq!(u64_at(&compiled.bytes, read + 6), 0);
        assert_eq!(min_data_length(&compiled, 0), 0);

        fails(
            Template::new()
                .input("offset", Type::Bool)
                .account("holder", account::readonly().owner(TOKEN_PROGRAM_ID))
                .step(step::require(
                    account_data("holder", input("offset"), ReadType::U64).eq(u64(1)),
                )),
            "accountData offset requires u64",
        );
    }

    #[test]
    fn compiles_return_data_only_as_a_let_directly_after_an_unconditional_invoke() {
        let compiled = compile(
            Template::new()
                .account("tokenProgram", account::program(TOKEN_PROGRAM_ID))
                .account(
                    "mint",
                    account::readonly()
                        .owner(TOKEN_PROGRAM_ID)
                        .min_data_length(82),
                )
                .step(
                    step::invoke("tokenProgram")
                        .readonly("mint")
                        .data(data::literal([21])),
                )
                .step(step::let_("size", return_data(ReadType::U64)))
                .step(step::require(var("size").eq(u64(165)))),
        );
        // The comparison's constant is hoisted ahead of the invoke, so the read still follows it.
        let read = instruction_offset(&compiled, 2);
        assert_eq!(compiled.bytes[read], wire::OP_RETURN_DATA);
        assert_eq!(compiled.bytes[read + 2], wire::OP_READ_U64);
        assert_eq!(compiled.stats.instructions, 5);

        fails(
            Template::new().step(step::let_("size", return_data(ReadType::U64))),
            "directly after an unconditional invoke",
        );
        fails(
            Template::new()
                .input("go", Type::Bool)
                .account("program", account::executable().unsafe_unpinned())
                .step(step::invoke("program").when(input("go")))
                .step(step::let_("size", return_data(ReadType::U64))),
            "directly after an unconditional invoke",
        );
        fails(
            Template::new()
                .account("program", account::executable().unsafe_unpinned())
                .step(step::invoke("program"))
                .step(step::require(return_data(ReadType::U64).eq(u64(1)))),
            "must be the value of a let",
        );
        fails(
            Template::new()
                .account("program", account::executable().unsafe_unpinned())
                .step(step::invoke("program"))
                .step(step::let_("size", return_data_at(ReadType::U64, 1020))),
            "extends past 1024 bytes",
        );
    }

    #[test]
    fn records_a_source_map_that_reaches_into_loop_bodies() {
        let compiled = compile(
            Template::new()
                .input("amount", Type::U64)
                .account("systemProgram", account::program(SYSTEM_PROGRAM_ID))
                .account("source", account::signer().writable())
                .batch(Batch::new(2).account("recipient", account::writable()))
                .step(step::let_("amount", input("amount")).label("loadAmount"))
                .step(
                    step::for_each()
                        .step(step::require(var("amount").gt(u64(0))).label("positive"))
                        .step(
                            system_transfer(
                                "systemProgram",
                                "source",
                                account::iteration("recipient"),
                                var("amount"),
                            )
                            .label("pay"),
                        )
                        .label("payEveryone"),
                ),
        );
        let map: Vec<_> = compiled
            .source_map
            .iter()
            .map(|entry| (entry.pc, entry.path.as_str(), entry.label.as_deref()))
            .collect();
        assert_eq!(
            map,
            [
                (0, "inputs.amount", None),
                (1, "constants[0]", None),
                (2, "steps[1]", Some("payEveryone")),
                (3, "steps[1].steps[0]", Some("positive")),
                (4, "steps[1].steps[0]", Some("positive")),
                (5, "steps[1].steps[1]", Some("pay")),
            ]
        );
    }

    #[test]
    fn compiles_row_inputs_and_account_groups() {
        let template = || {
            Template::new()
                .input("fee", Type::U64)
                .account("systemProgram", account::program(SYSTEM_PROGRAM_ID))
                .account("treasury", account::signer().writable())
                .batch(
                    Batch::new(4)
                        .account("recipient", account::writable())
                        .input("amount", Type::U64),
                )
                .account_group("extra")
        };
        let compiled = compile(
            template().step(
                step::for_each().step(
                    step::invoke("systemProgram")
                        .writable_signer("treasury")
                        .writable(account::iteration("recipient"))
                        .account_group("extra")
                        .data(data::literal([2, 0, 0, 0]))
                        .data(data::u64(row_input("amount"))),
                ),
            ),
        );
        assert_eq!(compiled.bytes[21], 1); // row input count
        assert_eq!(compiled.bytes[22], 1); // account group count
        assert_eq!(compiled.stats.inputs, 1);
        assert_eq!(compiled.stats.row_inputs, 1);
        assert_eq!(compiled.stats.account_groups, 1);
        assert_eq!(compiled.row_input_order, ["amount"]);
        assert_eq!(compiled.account_group_order, ["extra"]);
        // Header, three account records, then two input records: fee then the row's amount.
        assert_eq!(compiled.bytes[48..56], [2, 0, 0, 0, 2, 0, 0, 0]);
        // FOREACH, then the row load whose operand a carries the iteration bit.
        assert_eq!(compiled.bytes[56], wire::OP_FOREACH);
        assert_eq!(compiled.bytes[72], wire::OP_LOAD_INPUT);
        assert_eq!(compiled.bytes[74], 0x80);
        // The CPI descriptor's second byte names group 0.
        assert_eq!(compiled.bytes[105], 0);

        // A row input outside every forEach. The template still needs its forEach.
        fails(
            template()
                .step(step::require(row_input("amount").eq(u64(1))))
                .step(step::for_each().step(step::require(bool(true)))),
            "only valid inside forEach",
        );
        fails(
            Template::new()
                .account("systemProgram", account::program(SYSTEM_PROGRAM_ID))
                .step(step::invoke("systemProgram").account_group("missing")),
            "Unknown account group",
        );
    }

    #[test]
    fn sets_the_event_flag_in_the_header() {
        let compiled = compile(transfer().emit_event());
        assert_eq!(compiled.bytes[17], 1);
        assert!(compiled.stats.emit_event);
    }

    #[test]
    fn rejects_nested_iteration_and_excessive_worst_case_cpi_expansion() {
        fails(
            Template::new()
                .batch(Batch::new(1).account("recipient", account::readonly()))
                .step(step::for_each().step(step::for_each().step(step::require(bool(true))))),
            "Nested iteration",
        );
        let pay = || {
            system_transfer(
                "systemProgram",
                "source",
                account::iteration("recipient"),
                input("amount"),
            )
        };
        fails(
            Template::new()
                .input("amount", Type::U64)
                .account("systemProgram", account::program(SYSTEM_PROGRAM_ID))
                .account("source", account::signer().writable())
                .batch(Batch::new(33).account("recipient", account::writable()))
                .step(step::for_each().step(pay()).step(pay())),
            "66 CPIs",
        );
    }
}

// --------------------------------------------------------------------------------- data segments

mod data_segments {
    use super::*;

    #[test]
    fn an_invocation_part_that_derives_a_pda_leaves_the_invocation_its_own_segments() {
        let compiled = compile(
            Template::new()
                .account("program", account::program(address(9)))
                .account("owner", account::readonly())
                .step(step::invoke("program").data(data::pubkey(pda("program", [key("owner")])))),
        );
        let (segments, cpis) = segment_tables(&compiled);
        let (start, length) = cpis[0];
        // Register 0 is the owner's key; register 1 is the derived address.
        assert_eq!(
            segments[start..start + length],
            [(wire::DATA_REG_PUBKEY, 1)]
        );
    }

    #[test]
    fn a_pda_seed_that_derives_a_pda_leaves_the_outer_derivation_its_own_segments() {
        let compiled = compile(
            Template::new()
                .account("vaultProgram", account::program(address(9)))
                .account("tokenProgram", account::program(TOKEN_PROGRAM_ID))
                .account(
                    "associatedTokenProgram",
                    account::program(ASSOCIATED_TOKEN_PROGRAM_ID),
                )
                .account("authority", account::readonly())
                .account("mint", account::readonly())
                .account("vaultTokens", account::readonly())
                .step(step::require(key("vaultTokens").eq(pda(
                    "associatedTokenProgram",
                    [
                        pda("vaultProgram", [key("authority")]),
                        key("tokenProgram"),
                        key("mint"),
                    ],
                )))),
        );
        let derives = records_with(&compiled, wire::OP_DERIVE_PDA);
        let (vault, ata) = (derives[0], derives[1]);
        let (start, length) = range_of(&ata);
        assert_eq!(vault[1], 2);
        assert_eq!(
            segment_tables(&compiled).0[start..start + length],
            [
                (wire::DATA_REG_PUBKEY, 2),
                (wire::DATA_REG_PUBKEY, 3),
                (wire::DATA_REG_PUBKEY, 4)
            ]
        );
    }

    #[test]
    fn an_output_part_that_derives_a_pda_leaves_the_output_its_own_segments() {
        for emit in [true, false] {
            let parts = vec![
                data::literal(TAG),
                data::pubkey(pda("program", [key("owner")])),
            ];
            let output = if emit {
                step::emit(parts)
            } else {
                step::set_return_data(parts)
            };
            let compiled = compile(
                Template::new()
                    .account("program", account::program(address(9)))
                    .account("owner", account::readonly())
                    .step(output),
            );
            let record = records(&compiled)
                .into_iter()
                .find(|record| record[0] == wire::OP_EMIT || record[0] == wire::OP_SET_RETURN_DATA)
                .unwrap();
            let (start, length) = range_of(&record);
            assert_eq!(
                segment_tables(&compiled).0[start..start + length],
                [(wire::DATA_LITERAL, NONE), (wire::DATA_REG_PUBKEY, 1)]
            );
        }
    }
}

// ---------------------------------------------------------------------------------- output steps

mod output_steps {
    use super::*;

    fn with_steps(inputs: &[(&str, Type)], batch: bool, steps: Vec<Step>) -> Template {
        let mut template = Template::new();
        for (name, ty) in inputs {
            template = template.input(*name, *ty);
        }
        template = template
            .account("systemProgram", account::program(SYSTEM_PROGRAM_ID))
            .account("payer", account::signer().writable());
        if batch {
            template = template.batch(Batch::new(2).account("recipient", account::writable()));
        }
        template.steps(steps)
    }

    fn pay(to: AccountRef) -> Step {
        system_transfer("systemProgram", "payer", to, u64(1)).into()
    }

    fn result() -> Step {
        step::set_return_data([data::literal([1])])
    }

    #[test]
    fn emit_and_set_return_data_lower_to_one_record_naming_a_contiguous_run_of_their_parts() {
        let compiled = compile(with_steps(
            &[("amount", Type::U64)],
            false,
            vec![
                step::emit([data::literal(TAG), data::u64(input("amount"))]).label("log"),
                step::set_return_data([data::pubkey(key("payer"))]).label("result"),
            ],
        ));
        let outputs: Vec<_> = records(&compiled)
            .into_iter()
            .filter(|record| record[0] == wire::OP_EMIT || record[0] == wire::OP_SET_RETURN_DATA)
            .collect();
        assert_eq!(
            outputs
                .iter()
                .map(|record| record[..6].to_vec())
                .collect::<Vec<_>>(),
            [
                vec![wire::OP_EMIT, NONE, NONE, NONE, NONE, 0],
                vec![wire::OP_SET_RETURN_DATA, NONE, NONE, NONE, NONE, 0],
            ]
        );
        assert_eq!(
            outputs.iter().map(range_of).collect::<Vec<_>>(),
            [(0, 2), (2, 1)]
        );
        assert_eq!(
            segment_tables(&compiled).0,
            [
                (wire::DATA_LITERAL, NONE),
                (wire::DATA_REG_U64, 0),
                (wire::DATA_REG_PUBKEY, 1)
            ]
        );
        let labelled: Vec<_> = compiled
            .source_map
            .iter()
            .filter(|entry| entry.label.is_some())
            .map(|entry| {
                (
                    entry.pc,
                    entry.path.as_str(),
                    entry.label.as_deref().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            labelled,
            [
                (1, "steps[0]", "log"),
                (2, "steps[1]", "result"),
                (3, "steps[1]", "result")
            ]
        );
    }

    #[test]
    fn emit_may_appear_anywhere_loops_and_invokes_included() {
        compile(with_steps(
            &[],
            true,
            vec![
                step::emit([data::literal(TAG)]),
                step::for_each()
                    .step(step::emit([data::literal(TAG), data::u64(loop_index())]))
                    .step(pay(account::iteration("recipient")))
                    .step(step::emit([data::literal(TAG), data::u64(loop_index())]))
                    .into(),
                step::emit([data::literal(TAG)]),
            ],
        ));
    }

    #[test]
    fn set_return_data_comes_once_outside_every_loop_after_every_invoke() {
        compile(with_steps(&[], false, vec![pay("payer".into()), result()]));
        compile(with_steps(
            &[],
            true,
            vec![
                step::for_each()
                    .step(pay(account::iteration("recipient")))
                    .into(),
                result(),
            ],
        ));
        fails(
            with_steps(&[], true, vec![step::for_each().step(result()).into()]),
            "setReturnData is not allowed inside a loop",
        );
        fails(
            with_steps(&[], false, vec![result(), result()]),
            "setReturnData may appear only once",
        );
        fails(
            with_steps(&[], false, vec![result(), pay("payer".into())]),
            "invoke cannot follow setReturnData",
        );
        fails(
            with_steps(
                &[],
                true,
                vec![
                    result(),
                    step::for_each()
                        .step(pay(account::iteration("recipient")))
                        .into(),
                ],
            ),
            "invoke cannot follow setReturnData",
        );
    }

    #[test]
    fn set_return_data_stays_out_of_count_loops_and_emit_may_run_in_them() {
        let count = [("n", Type::U64)];
        fails(
            with_steps(
                &count,
                false,
                vec![step::repeat(input("n"), 2).step(result()).into()],
            ),
            "setReturnData is not allowed inside a loop",
        );
        compile(with_steps(
            &count,
            false,
            vec![step::repeat(input("n"), 2)
                .step(step::emit([data::literal(TAG), data::u64(loop_index())]))
                .into()],
        ));
    }

    #[test]
    fn an_output_encodes_at_most_1024_bytes_counting_a_bytes_value_at_its_maximum_length() {
        let memo = [("memo", Type::Bytes(1024))];
        compile(with_steps(
            &memo,
            false,
            vec![step::set_return_data([data::bytes(input("memo"))])],
        ));
        fails(
            with_steps(
                &memo,
                false,
                vec![step::emit([data::literal(TAG), data::bytes(input("memo"))])],
            ),
            "emit can encode 1028 bytes; maximum is 1024",
        );
        fails(
            with_steps(
                &[],
                false,
                vec![step::set_return_data([data::literal([0u8; 1025])])],
            ),
            "setReturnData can encode 1025 bytes; maximum is 1024",
        );
    }

    #[test]
    fn an_output_takes_1_to_64_parts() {
        let parts = |count: usize| -> Vec<DataPart> {
            (0..count)
                .map(|index| {
                    if index == 0 {
                        data::literal(TAG)
                    } else {
                        data::literal([index as u8])
                    }
                })
                .collect()
        };
        for emit in [true, false] {
            let output = |parts: Vec<DataPart>| {
                if emit {
                    step::emit(parts)
                } else {
                    step::set_return_data(parts)
                }
            };
            let name = if emit { "emit" } else { "setReturnData" };
            fails(
                with_steps(&[], false, vec![output(parts(0))]),
                &format!("{name} takes 1 to 64 data parts, not 0"),
            );
            compile(with_steps(&[], false, vec![output(parts(64))]));
            fails(
                with_steps(&[], false, vec![output(parts(65))]),
                &format!("{name} takes 1 to 64 data parts, not 65"),
            );
        }
    }

    #[test]
    fn emit_starts_with_a_literal_tag_of_at_least_4_bytes_outside_the_run_event_family() {
        let amount = [("amount", Type::U64)];
        let value = || data::u64(input("amount"));
        let tag = |text: &[u8]| data::literal(text);
        let emit = |parts: Vec<DataPart>| with_steps(&amount, false, vec![step::emit(parts)]);
        let untagged = "emit must start with a literal tag of at least 4 bytes, so its log cannot pass for Ballista's run event";
        let reserved = "emit tag cannot start with \"BEV\": that tag family is reserved for Ballista's run event";

        for text in [&b"TAG1"[..], b"a longer tag", b"BEU1", b"bev1", b"XBEV"] {
            compile(emit(vec![tag(text), value()]));
            compile(emit(vec![tag(text)]));
        }
        fails(emit(vec![value()]), untagged);
        fails(emit(vec![value(), tag(b"TAG1")]), untagged);
        fails(emit(vec![tag(b"TAG"), value()]), untagged);
        fails(emit(vec![tag(b"TAG"), tag(b"1")]), untagged);
        for text in [&b"BEV1"[..], b"BEV2", b"BEV\0", b"BEVERAGE"] {
            fails(emit(vec![tag(text), value()]), reserved);
        }
        compile(with_steps(
            &amount,
            false,
            vec![step::set_return_data([value()])],
        ));
    }
}

// ------------------------------------------------------------------------------ math expressions

mod math_expressions {
    use super::*;

    fn with_steps(inputs: &[(&str, Type)], steps: Vec<Step>) -> Template {
        let mut template = Template::new();
        for (name, ty) in inputs {
            template = template.input(*name, *ty);
        }
        template.steps(steps)
    }

    #[test]
    fn multiply_divide_lowers_to_one_three_operand_record_rounding_by_opcode() {
        let compiled = compile(with_steps(
            &[("a", Type::U64), ("b", Type::U64), ("c", Type::U64)],
            vec![
                step::let_("down", multiply_divide(input("a"), input("b"), input("c"))),
                step::let_("up", multiply_divide_up(input("a"), input("b"), input("c"))),
                step::require(var("down").lte(var("up"))),
            ],
        ));
        let mul_divs: Vec<_> = records(&compiled)
            .into_iter()
            .filter(|record| record[0] == wire::OP_MUL_DIV || record[0] == wire::OP_MUL_DIV_CEIL)
            .collect();
        assert_eq!(
            mul_divs.iter().map(|record| record[0]).collect::<Vec<_>>(),
            [wire::OP_MUL_DIV, wire::OP_MUL_DIV_CEIL]
        );
        assert_eq!(mul_divs[0][2..5], [0, 1, 2]);
    }

    #[test]
    fn multiply_divide_rejects_mixed_or_signed_operands() {
        fails(
            with_steps(
                &[("a", Type::U64), ("b", Type::U128)],
                vec![step::let_(
                    "x",
                    multiply_divide(input("a"), input("a"), input("b")),
                )],
            ),
            "multiplyDivide",
        );
        fails(
            with_steps(
                &[("a", Type::I64)],
                vec![step::let_(
                    "x",
                    multiply_divide(input("a"), input("a"), input("a")),
                )],
            ),
            "multiplyDivide",
        );
    }

    #[test]
    fn shifts_take_an_unsigned_value_and_a_u64_amount_bitwise_ops_need_matching_unsigned_operands()
    {
        compile(with_steps(
            &[("a", Type::U128), ("n", Type::U64)],
            vec![
                step::let_("x", input("a") << input("n")),
                step::let_("y", input("a") >> input("n")),
                step::let_("z", input("a") ^ var("x")),
            ],
        ));
        fails(
            with_steps(
                &[("a", Type::I64), ("n", Type::U64)],
                vec![step::let_("x", shift_left(input("a"), input("n")))],
            ),
            "shiftLeft",
        );
        fails(
            with_steps(
                &[("a", Type::U64), ("b", Type::U128)],
                vec![step::let_("x", bit_and(input("a"), input("b")))],
            ),
            "bitAnd",
        );
    }

    #[test]
    fn remainder_works_on_every_numeric_type_power_of_ten_takes_a_u64_and_yields_a_u128() {
        for ty in [Type::U64, Type::I64, Type::U128] {
            let compiled = compile(with_steps(
                &[("a", ty)],
                vec![step::let_("r", input("a") % input("a"))],
            ));
            assert!(!records_with(&compiled, wire::OP_REM).is_empty());
        }
        compile(with_steps(
            &[("a", Type::I64), ("e", Type::U64)],
            vec![
                step::let_("r", remainder(input("a"), input("a"))),
                step::require(power_of_ten(input("e")).eq(u128(1_000))),
            ],
        ));
        fails(
            with_steps(
                &[("a", Type::U64), ("b", Type::I64)],
                vec![step::let_("x", remainder(input("a"), input("b")))],
            ),
            "remainder",
        );
        fails(
            with_steps(
                &[("a", Type::U128)],
                vec![step::let_("x", power_of_ten(input("a")))],
            ),
            "powerOfTen",
        );
    }

    #[test]
    fn an_i32_read_is_typed_i64_and_raises_the_data_floor_to_cover_its_four_bytes() {
        let compiled = compile(
            Template::new()
                .account("feed", account::readonly().owner(address(7)))
                .step(step::require(
                    account_data("feed", 89, ReadType::I32).lt(i64(0)),
                )),
        );
        assert!(!records_with(&compiled, wire::OP_READ_I32).is_empty());
        assert_eq!(min_data_length(&compiled, 0), 93);
    }
}

// ---------------------------------------------------------------- count loops and several loops

mod count_loops {
    use super::*;

    fn pay_accounts(template: Template) -> Template {
        template
            .account("systemProgram", account::program(SYSTEM_PROGRAM_ID))
            .account("source", account::signer().writable())
            .account("destination", account::writable())
    }

    fn pay_to(to: AccountRef) -> Step {
        system_transfer("systemProgram", "source", to, input("amount")).into()
    }

    fn pay() -> Step {
        pay_to("destination".into())
    }

    #[test]
    fn repeat_lowers_to_one_repeat_record() {
        let compiled = compile(
            pay_accounts(
                Template::new()
                    .input("rounds", Type::U64)
                    .input("amount", Type::U64),
            )
            .step(step::let_("total", u64(0)))
            .step(
                step::repeat(input("rounds"), 5)
                    .step(pay())
                    .step(step::assign("total", var("total") + loop_index()))
                    .carry("total")
                    .label("payRounds"),
            ),
        );
        let all = records(&compiled);
        let repeat_pc = all
            .iter()
            .position(|record| record[0] == wire::OP_REPEAT)
            .unwrap();
        let repeat = all[repeat_pc];
        // The two hoisted inputs are registers 0 and 1, and the constant total register 2.
        assert_eq!(
            repeat[1..6],
            [NONE, (all.len() - repeat_pc - 1) as u8, 0, 5, 0]
        );
        assert_eq!(immediate(&repeat), 1 << 2);
        let entry = &compiled.source_map[repeat_pc];
        assert_eq!(
            (entry.pc, entry.path.as_str(), entry.label.as_deref()),
            (repeat_pc, "steps[1]", Some("payRounds"))
        );
        assert_eq!(compiled.stats.max_expanded_cpis, 5);
        verifies(&compiled);
    }

    #[test]
    fn a_template_may_hold_several_loops_and_the_worst_case_adds_them_up() {
        let compiled = compile(
            pay_accounts(Template::new().input("amount", Type::U64))
                .batch(Batch::new(10).account("recipient", account::writable()))
                .step(pay())
                .step(step::for_each().step(pay_to(account::iteration("recipient"))))
                .step(step::repeat(u64(3), 4).step(pay()).step(pay()))
                .step(step::for_each().step(step::require(
                    lamports(account::iteration("recipient")).gt(u64(0)),
                ))),
        );
        assert_eq!(compiled.stats.max_expanded_cpis, 1 + 10 + 8);
        let loops: Vec<_> = records(&compiled)
            .into_iter()
            .map(|record| record[0])
            .filter(|&code| code == wire::OP_FOREACH || code == wire::OP_REPEAT)
            .collect();
        assert_eq!(loops, [wire::OP_FOREACH, wire::OP_REPEAT, wire::OP_FOREACH]);
        verifies(&compiled);

        fails(
            pay_accounts(Template::new().input("amount", Type::U64))
                .step(step::repeat(u64(1), 33).step(pay()).step(pay())),
            "66 CPIs",
        );
    }

    #[test]
    fn a_repeat_body_has_the_loop_index_but_no_rows() {
        let with_repeat = |body: Step| {
            pay_accounts(Template::new().input("amount", Type::U64))
                .batch(
                    Batch::new(2)
                        .account("recipient", account::writable())
                        .input("share", Type::U64),
                )
                .step(step::for_each().step(pay()))
                .step(step::repeat(u64(2), 2).step(body))
        };
        compile(with_repeat(step::require(loop_index().lt(u64(2)))));
        fails(
            with_repeat(step::require(row_input("share").eq(u64(1)))),
            "Row inputs are only valid inside forEach",
        );
        fails(
            with_repeat(pay_to(account::iteration("recipient"))),
            "Iteration accounts are only valid inside forEach",
        );
        fails(
            Template::new().step(step::require(loop_index().eq(u64(0)))),
            "loopIndex is only valid inside a loop",
        );
    }

    #[test]
    fn the_count_is_a_u64_evaluated_before_the_loop_and_the_maximum_is_1_to_255() {
        let counted = |count: Expr, max: u8| {
            Template::new()
                .input("rounds", Type::U64)
                .input("signed", Type::I64)
                .step(step::repeat(count, max).step(step::require(bool(true))))
        };
        let compiled = compile(counted(input("rounds") / u64(2), 255));
        let codes: Vec<_> = records(&compiled)
            .into_iter()
            .map(|record| record[0])
            .collect();
        let position = |code| codes.iter().position(|&other| other == code).unwrap();
        assert!(position(wire::OP_DIV) < position(wire::OP_REPEAT));
        fails(counted(input("signed"), 1), "repeat count requires u64");
        fails(
            counted(loop_index(), 1),
            "loopIndex is only valid inside a loop",
        );
        // The maximum is a `u8`, so 256 does not type-check; 0 is refused.
        fails(
            counted(input("rounds"), 0),
            "repeat max must be 1 to 255, not 0",
        );
    }

    #[test]
    fn loops_are_top_level_at_most_eight_and_a_batch_needs_a_for_each() {
        let repeat = || step::repeat(u64(1), 1).step(step::require(bool(true)));
        compile(Template::new().steps((0..8).map(|_| repeat())));
        fails(
            Template::new().steps((0..9).map(|_| repeat())),
            "at most 8 top-level loops",
        );
        fails(
            Template::new()
                .batch(Batch::new(1).account("recipient", account::readonly()))
                .step(step::for_each().step(repeat())),
            "Nested iteration",
        );
        fails(
            Template::new().step(step::repeat(u64(1), 1).step(repeat())),
            "Nested iteration",
        );
        fails(
            Template::new()
                .batch(Batch::new(1).account("recipient", account::readonly()))
                .step(repeat()),
            "requires at least one top-level forEach",
        );
        compile(
            Template::new()
                .batch(Batch::new(1).account("recipient", account::readonly()))
                .step(step::for_each().step(step::require(bool(true))))
                .step(step::for_each().step(step::require(bool(true)))),
        );
    }
}

// ----------------------------------------------------------------------------- carried variables

mod carried_variables {
    use super::*;

    #[test]
    fn each_carried_variable_gets_its_own_register_when_it_starts_from_a_value_something_else_reads(
    ) {
        let compiled = compile(
            Template::new()
                .input("start", Type::U64)
                .batch(Batch::new(3).account("holder", account::readonly()))
                .step(step::let_("sum", u64(0)))
                .step(step::let_("count", u64(0)))
                .step(step::let_("floor", input("start")))
                .step(
                    step::for_each()
                        .step(step::assign(
                            "sum",
                            var("sum") + lamports(account::iteration("holder")),
                        ))
                        .step(step::assign("count", var("count") + u64(1)))
                        .step(step::assign("floor", var("floor") + u64(1)))
                        .carry("sum")
                        .carry("count")
                        .carry("floor"),
                )
                .step(step::require(var("count").gt(u64(0))))
                .step(step::require(var("floor").gte(input("start")))),
        );
        let all = records(&compiled);
        assert_eq!(
            all[3..6]
                .iter()
                .map(|record| record[..3].to_vec())
                .collect::<Vec<_>>(),
            [
                vec![wire::OP_MOVE, 3, 1],
                vec![wire::OP_MOVE, 4, 1],
                vec![wire::OP_MOVE, 5, 0]
            ]
        );
        assert_eq!(all[6][0], wire::OP_FOREACH);
        assert_eq!(immediate(&all[6]), (1 << 3) | (1 << 4) | (1 << 5));
    }

    #[test]
    fn a_carried_variable_that_alone_reads_its_starting_value_keeps_its_register() {
        let compiled = compile(
            Template::new()
                .batch(Batch::new(3).account("holder", account::readonly()))
                .step(step::let_("total", u64(0)))
                .step(
                    step::for_each()
                        .step(step::assign(
                            "total",
                            var("total") + lamports(account::iteration("holder")),
                        ))
                        .carry("total"),
                ),
        );
        let all = records(&compiled);
        assert_eq!(all[1][0], wire::OP_FOREACH);
        assert_eq!(immediate(&all[1]), 1);
    }

    /// The payload the TypeScript compiler writes for `name` in
    /// `fixtures/compiler-fuzz-findings.json`. Machine-written JSON, so a string search is enough.
    fn finding_payload(name: &str) -> Vec<u8> {
        const FINDINGS: &str = include_str!("../../../fixtures/compiler-fuzz-findings.json");
        let entry = &FINDINGS[FINDINGS.find(&format!("\"{name}\": {{")).unwrap()..];
        let start = entry.find("\"payload\": \"").unwrap() + "\"payload\": \"".len();
        let hex = &entry[start..start + entry[start..].find('"').unwrap()];
        (0..hex.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
            .collect()
    }

    /// Finding `carried-alias` from the TypeScript compiler fuzzer, fixed in both compilers: a
    /// `let` or `snapshot` in a loop body of a carried variable bound the variable's own register,
    /// and the `assign` after it rewrote that register in place, so the snapshot took the new
    /// value. Each pass here snapshots the total, adds one, and requires the total to have grown
    /// by one from the snapshot, which the aliased snapshot failed on the first pass. The binding
    /// now takes a copy, and the bytes match `carriedAliasDocument` compiled by TypeScript.
    #[test]
    fn a_snapshot_of_a_carried_variable_keeps_its_value_after_an_assign() {
        let compiled = compile(
            Template::new()
                .input("passes", Type::U64)
                .step(step::let_("total", u64(0)))
                .step(
                    step::repeat(input("passes"), 4)
                        .step(step::snapshot("before", var("total")))
                        .step(step::assign("total", var("total") + u64(1)))
                        .step(
                            step::require((snapshot("before") + u64(1)).eq(var("total")))
                                .label("grewByOne"),
                        )
                        .carry("total"),
                )
                .step(step::set_return_data([data::u64(var("total"))])),
        );
        let all = records(&compiled);
        let repeat = all.iter().position(|record| record[0] == wire::OP_REPEAT).unwrap();
        let carried = immediate(&all[repeat]).trailing_zeros() as u8;
        // The body's first record copies the carried total into a register of its own.
        assert_eq!(all[repeat + 1][..3], [wire::OP_MOVE, all[repeat + 1][1], carried]);
        assert_ne!(all[repeat + 1][1], carried);
        assert_eq!(compiled.bytes, finding_payload("carried-alias"));
        verifies(&compiled);
    }

    /// The check `carried-alias` bypassed: a per-pass cap measured from a snapshot, with two
    /// assignments in the pass. Matches `perPassCapDocument` compiled by TypeScript.
    #[test]
    fn a_per_pass_cap_measured_from_a_snapshot_compiles_as_typescript_does() {
        let compiled = compile(
            Template::new()
                .input("passes", Type::U64)
                .input("amount", Type::U64)
                .input("fee", Type::U64)
                .step(step::let_("spent", u64(0)))
                .step(
                    step::repeat(input("passes"), 4)
                        .step(step::snapshot("before", var("spent")))
                        .step(step::assign("spent", var("spent") + input("amount")))
                        .step(step::assign("spent", var("spent") + input("fee")))
                        .step(
                            step::require((var("spent") - snapshot("before")).lte(u64(10)))
                                .label("perPassCap"),
                        )
                        .carry("spent"),
                )
                .step(step::set_return_data([data::u64(var("spent"))])),
        );
        assert_eq!(compiled.bytes, finding_payload("per-pass-cap"));
        verifies(&compiled);
    }
}

// --------------------------------------------------------------------- introspection expressions

mod introspection {
    use super::*;

    const SYSVAR: &str = "instructions";

    fn with_steps(steps: Vec<Step>) -> Template {
        Template::new()
            .input("index", Type::U64)
            .input("offset", Type::U64)
            .account(SYSVAR, account::readonly().address(INSTRUCTIONS_SYSVAR_ID))
            .steps(steps)
    }

    /// The a, b, c and immediate of every record with `opcode`.
    fn operands_of(compiled: &CompiledTemplate, opcode: u8) -> Vec<(u8, u8, u8, u64)> {
        records_with(compiled, opcode)
            .iter()
            .map(|record| (record[2], record[3], record[4], immediate(record)))
            .collect()
    }

    #[test]
    fn each_expression_lowers_to_its_opcode_with_the_sysvar_in_a_and_u64_registers_in_b_and_c() {
        let index = || input("index");
        let offset = || input("offset");
        let compiled = compile(with_steps(vec![
            step::let_("count", instruction_count(SYSVAR)),
            step::let_("current", current_instruction_index(SYSVAR)),
            step::let_("program", instruction_program(SYSVAR, index())),
            step::let_("accounts", instruction_account_count(SYSVAR, index())),
            step::let_("key", instruction_account(SYSVAR, index(), offset())),
            step::let_(
                "flags",
                instruction_account_flags(SYSVAR, index(), offset()),
            ),
            step::let_("length", instruction_data_length(SYSVAR, index())),
            step::let_(
                "word",
                instruction_data(SYSVAR, index(), offset(), ReadType::I32),
            ),
            step::let_(
                "bytes",
                instruction_data_bytes(SYSVAR, index(), offset(), 12),
            ),
            step::require(bytes_length(var("bytes")).eq(u64(12))),
        ]));
        assert_eq!(
            operands_of(&compiled, wire::OP_INSTRUCTION_COUNT),
            [(0, NONE, NONE, 0)]
        );
        assert_eq!(
            operands_of(&compiled, wire::OP_INSTRUCTION_INDEX),
            [(0, NONE, NONE, 0)]
        );
        assert_eq!(
            operands_of(&compiled, wire::OP_INSTRUCTION_PROGRAM),
            [(0, 0, NONE, 0)]
        );
        assert_eq!(
            operands_of(&compiled, wire::OP_INSTRUCTION_ACCOUNT_COUNT),
            [(0, 0, NONE, 0)]
        );
        assert_eq!(
            operands_of(&compiled, wire::OP_INSTRUCTION_ACCOUNT),
            [(0, 0, 1, 0)]
        );
        assert_eq!(
            operands_of(&compiled, wire::OP_INSTRUCTION_ACCOUNT_FLAGS),
            [(0, 0, 1, 0)]
        );
        assert_eq!(
            operands_of(&compiled, wire::OP_INSTRUCTION_DATA_LEN),
            [(0, 0, NONE, 0)]
        );
        assert_eq!(
            operands_of(&compiled, wire::OP_READ_INSTRUCTION_DATA),
            [(0, 0, 1, u64::from(wire::OP_READ_I32))]
        );
        assert_eq!(
            operands_of(&compiled, wire::OP_READ_INSTRUCTION_BYTES),
            [(0, 0, 1, 12)]
        );
        assert_eq!(operands_of(&compiled, wire::OP_BYTES_LEN).len(), 1);
    }

    #[test]
    fn a_number_for_an_index_position_or_offset_becomes_a_shared_u64_constant() {
        let compiled = compile(with_steps(vec![step::require(
            instruction_data(SYSVAR, 2, 0, ReadType::U8).eq(instruction_data(
                SYSVAR,
                2,
                1,
                ReadType::U8,
            )),
        )]));
        assert_eq!(records_with(&compiled, wire::OP_CONST_U64).len(), 3);
    }

    #[test]
    fn results_are_typed() {
        compile(with_steps(vec![
            step::require(instruction_program(SYSVAR, 0).eq(pubkey(ED25519_PROGRAM_ID))),
            step::require(instruction_data(SYSVAR, 0, 0, ReadType::I32).lt(i64(0))),
        ]));
        fails(
            with_steps(vec![step::require(
                instruction_account(SYSVAR, 0, 0).eq(u64(0)),
            )]),
            "matching types",
        );
        let compiled = compile(
            with_steps(vec![step::invoke("program")
                .data(data::bytes(instruction_data_bytes(SYSVAR, 0, 0, 40)))
                .into()])
            .account("program", account::program(address(9))),
        );
        assert_eq!(compiled.stats.max_cpi_data_length, 40);
    }

    #[test]
    fn the_sysvar_must_be_a_fixed_account_pinned_to_its_address() {
        for other in [account::readonly(), account::readonly().address(address(3))] {
            fails(
                Template::new()
                    .account("other", other)
                    .step(step::require(instruction_count("other").eq(u64(1)))),
                "Instructions sysvar",
            );
        }
        fails(
            Template::new()
                .batch(Batch::new(1).account(
                    "rowSysvar",
                    account::readonly().address(INSTRUCTIONS_SYSVAR_ID),
                ))
                .step(step::for_each().step(step::require(
                    instruction_count(account::iteration("rowSysvar")).eq(u64(1)),
                ))),
            "Instructions sysvar",
        );
        fails(
            with_steps(vec![step::require(
                instruction_data_length(SYSVAR, i64(0)).eq(u64(1)),
            )]),
            "instruction index requires u64",
        );
    }

    #[test]
    fn account_data_bytes_reads_a_pinned_read_only_account() {
        let read = |constraint: Account| {
            Template::new()
                .account("mint", constraint)
                .step(step::require(
                    account_data_bytes("mint", 44, 1).eq(bytes([6])),
                ))
        };
        let compiled = compile(read(account::readonly().owner(TOKEN_PROGRAM_ID)));
        assert_eq!(
            operands_of(&compiled, wire::OP_READ_ACCOUNT_BYTES),
            [(0, 0, NONE, 1)]
        );
        fails(read(account::readonly()), "pins neither owner nor address");
        fails(
            read(account::writable().owner(TOKEN_PROGRAM_ID)),
            "declared writable",
        );
    }

    #[test]
    fn bytes_length_takes_bytes_byte_reads_take_1_to_1024_bytes() {
        fails(
            with_steps(vec![step::let_("n", bytes_length(input("index")))]),
            "bytesLength requires bytes",
        );
        fails(
            with_steps(vec![step::let_(
                "b",
                instruction_data_bytes(SYSVAR, 0, 0, 0),
            )]),
            "instructionDataBytes reads 1 to 1024 bytes, not 0",
        );
        fails(
            with_steps(vec![step::let_(
                "b",
                instruction_data_bytes(SYSVAR, 0, 0, 1025),
            )]),
            "instructionDataBytes reads 1 to 1024 bytes, not 1025",
        );
    }

    #[test]
    fn is_signer_and_is_writable_test_one_flag_bit() {
        let compiled = compile(with_steps(vec![
            step::require(instruction_account_is_signer(SYSVAR, 0, 1)),
            step::require(!instruction_account_is_writable(SYSVAR, 0, 1)),
        ]));
        assert_eq!(
            operands_of(&compiled, wire::OP_INSTRUCTION_ACCOUNT_FLAGS).len(),
            2
        );
        assert_eq!(records_with(&compiled, wire::OP_BIT_AND).len(), 2);
    }
}

// ------------------------------------------------------------------------------ ed25519Signature

mod ed25519 {
    use super::*;

    fn quote() -> Ed25519Signature {
        ed25519_signature(
            "instructions",
            current_instruction_index("instructions") - u64(1),
            key("maker"),
            40,
        )
        .name("quote")
    }

    fn with_steps(steps: Vec<Step>) -> Template {
        Template::new()
            .account(
                "instructions",
                account::readonly().address(INSTRUCTIONS_SYSVAR_ID),
            )
            .account("maker", account::signer())
            .steps(steps)
    }

    use ballista_sdk::template::Ed25519Signature;

    #[test]
    fn its_steps_check_the_program_the_header_and_the_key_and_bind_the_index_and_the_message() {
        let quote = quote();
        let mut steps = quote.steps();
        steps.push(step::require(quote.field(0, ReadType::U64).gt(u64(0))).label("pricePositive"));
        let compiled = compile(with_steps(steps));
        let labels: Vec<_> = compiled
            .source_map
            .iter()
            .filter_map(|entry| entry.label.as_deref())
            .collect();
        for label in [
            "quoteInstructionIndex",
            "quoteIsEd25519",
            "quoteIsOneSelfContainedSignature",
            "quoteIsBySigner",
            "quoteMessageOffset",
        ] {
            assert!(labels.contains(&label), "{label}");
        }
        assert_eq!(records_with(&compiled, wire::OP_INSTRUCTION_INDEX).len(), 1);
        let reads: Vec<_> = records_with(&compiled, wire::OP_READ_INSTRUCTION_DATA)
            .iter()
            .map(immediate)
            .collect();
        assert_eq!(
            reads,
            [
                wire::OP_READ_U128,
                wire::OP_READ_U16,
                wire::OP_READ_PUBKEY,
                wire::OP_READ_U16,
                wire::OP_READ_U64
            ]
            .map(u64::from)
        );
    }

    #[test]
    fn the_header_check_masks_the_count_the_three_instruction_indexes_and_the_message_size() {
        let compiled = compile(with_steps(quote().steps()));
        let blob = blob(&compiled);
        let constants: Vec<u128> = records_with(&compiled, wire::OP_CONST_U128)
            .iter()
            .map(|record| {
                let (offset, length) = range_of(record);
                u128::from_le_bytes(blob[offset..offset + length].try_into().unwrap())
            })
            .collect();
        let field = |offset: u32, width: u32, value: u128| {
            (
                ((1u128 << (width * 8)) - 1) << (offset * 8),
                value << (offset * 8),
            )
        };
        let parts = [
            field(0, 1, 1),
            field(4, 2, 0xffff),
            field(8, 2, 0xffff),
            field(12, 2, 40),
            field(14, 2, 0xffff),
        ];
        assert_eq!(
            constants,
            [
                parts.iter().fold(0, |mask, (part, _)| mask | part),
                parts.iter().fold(0, |expected, (_, part)| expected | part)
            ]
        );
    }

    #[test]
    fn fields_must_lie_inside_the_signed_message() {
        let quote = quote();
        let read = |expression: Expr| {
            let mut steps = quote.steps();
            steps.push(step::let_("value", expression));
            with_steps(steps)
        };
        compile(read(quote.field(32, ReadType::U64)));
        fails(
            read(quote.field(33, ReadType::U64)),
            "inside the 40-byte signed message",
        );
        fails(read(quote.field(9, ReadType::Pubkey)), "inside");
        // A negative offset does not type-check: `field` takes a `u32`.
    }

    #[test]
    fn the_signer_cannot_be_an_input_which_the_transaction_builder_chooses() {
        for signer in [input("maker"), row_input("maker")] {
            let signature = ed25519_signature("instructions", u64(0), signer, 40);
            fails(
                with_steps(signature.steps()).input("maker", Type::Pubkey),
                "builder cannot choose",
            );
        }
    }

    #[test]
    fn a_field_read_without_the_steps_does_not_compile() {
        fails(
            with_steps(vec![step::require(
                quote().field(0, ReadType::U64).gt(u64(0)),
            )]),
            "Unknown variable: quote",
        );
    }
}

// ----------------------------------------------------------------------------- registries: schema

mod registries_schema {
    use super::*;

    fn with_registries(registries: Vec<(String, Vec<(String, Type)>)>) -> Template {
        let mut template = Template::new();
        for (name, fields) in registries {
            template = template.registry(name, fields);
        }
        template.step(step::require(bool(true)))
    }

    fn fields(fields: &[(&str, Type)]) -> Vec<(String, Type)> {
        fields
            .iter()
            .map(|(name, ty)| (name.to_string(), *ty))
            .collect()
    }

    #[test]
    fn declares_registries_registry_accounts_and_the_system_program() {
        let compiled = compile(
            Template::new()
                .registry("limits", [("spent", Type::U64), ("lastSpend", Type::I64)])
                .account("caller", account::signer().writable())
                .account(
                    "limits",
                    account::registry("limits", "caller").key(account_key("caller")),
                )
                .account("systemProgram", account::system_program())
                .step(step::set_registry("limits", "spent", u64(1))),
        );
        assert_eq!(compiled.registry_order, ["limits"]);
        assert_eq!(compiled.registry_index("limits"), Some(0));
        assert_eq!(compiled.registry_index("missing"), None);
        verifies(&compiled);
    }

    #[test]
    fn a_registry_holds_1_to_512_bytes_of_the_five_writable_types_and_a_template_at_most_8() {
        fails(
            with_registries(vec![("empty".into(), vec![])]),
            "1 to 512 bytes",
        );
        fails(
            with_registries(vec![("bytes".into(), fields(&[("blob", Type::Bytes(8))]))]),
            "\"blob\" is a bytes; a registry field is bool, u64, i64, u128 or pubkey",
        );
        let sixteen: Vec<_> = (0..16)
            .map(|index| (format!("k{index}"), Type::Pubkey))
            .collect();
        compile(with_registries(vec![("full".into(), sixteen.clone())]));
        let mut over = sixteen;
        over.push(("flag".into(), Type::Bool));
        fails(
            with_registries(vec![("over".into(), over)]),
            "1 to 512 bytes",
        );
        let nine = (0..9)
            .map(|index| (format!("r{index}"), fields(&[("flag", Type::Bool)])))
            .collect();
        fails(with_registries(nine), "at most 8 registries");
    }

    #[test]
    fn registry_accounts_are_fixed_accounts() {
        fails(
            Template::new()
                .registry("limits", [("spent", Type::U64)])
                .account("caller", account::signer().writable())
                .batch(Batch::new(2).account("entry", account::registry("limits", "caller")))
                .step(step::for_each().step(step::require(bool(true)))),
            "registry accounts are fixed accounts",
        );
    }
}

// --------------------------------------------------------------------------- registries: compiler

mod registries_compiler {
    use super::*;

    fn base_accounts() -> Vec<(&'static str, Account)> {
        vec![
            ("caller", account::signer().writable()),
            (
                "mine",
                account::registry("limits", "caller").key(account_key("caller")),
            ),
            (
                "theirs",
                account::registry("limits", "caller").key(input("owner")),
            ),
            ("global", account::registry("flags", "caller")),
            ("systemProgram", account::system_program()),
        ]
    }

    fn template(accounts: Vec<(&'static str, Account)>, steps: Vec<Step>) -> Template {
        let mut template = Template::new()
            .input("owner", Type::Pubkey)
            .registry("flags", [("on", Type::Bool)])
            .registry(
                "limits",
                [
                    ("spent", Type::U64),
                    ("lastSpend", Type::I64),
                    ("holder", Type::Pubkey),
                ],
            );
        for (name, constraint) in accounts {
            template = template.account(name, constraint);
        }
        template.steps(steps)
    }

    fn base(steps: Vec<Step>) -> Template {
        template(base_accounts(), steps)
    }

    /// The base accounts with `name` replaced by `constraint`, or appended when absent.
    fn replaced(name: &'static str, constraint: Account) -> Vec<(&'static str, Account)> {
        let mut accounts = base_accounts();
        match accounts.iter_mut().find(|(other, _)| *other == name) {
            Some(slot) => slot.1 = constraint,
            None => accounts.push((name, constraint)),
        }
        accounts
    }

    fn read(account: &str, field: &str) -> Vec<Step> {
        vec![step::require(registry(account, field).eq(u64(0)))]
    }

    #[test]
    fn opens_every_entry_before_the_first_step_in_declaration_order() {
        let compiled = compile(base(vec![step::require(
            registry("global", "on").eq(bool(false)),
        )]));
        let opens = records_with(&compiled, wire::OP_OPEN_REGISTRY);
        assert_eq!(
            opens[0][..6],
            [wire::OP_OPEN_REGISTRY, NONE, 1, opens[0][3], 0, 0]
        );
        assert_eq!(immediate(&opens[0]), 1 | (48 << 8) | (4 << 24));
        assert_eq!(opens[1][2], 2);
        assert_eq!(immediate(&opens[1]), 1 | (48 << 8) | (4 << 24));
        assert_eq!(opens[2][2], 3);
        assert_eq!(opens[2][3], NONE);
        assert_eq!(immediate(&opens[2]), (1 << 8) | (4 << 24));
        let codes: Vec<_> = records(&compiled)
            .into_iter()
            .map(|record| record[0])
            .collect();
        let last_open = codes
            .iter()
            .rposition(|&code| code == wire::OP_OPEN_REGISTRY)
            .unwrap();
        let first_read = codes
            .iter()
            .position(|&code| code == wire::OP_READ_REGISTRY)
            .unwrap();
        assert!(last_open < first_read);
        assert_eq!(compiled.stats.max_expanded_cpis, 9);
        verifies(&compiled);
    }

    #[test]
    fn reads_and_writes_fields_at_their_packed_offsets_and_types() {
        let compiled = compile(base(vec![
            step::let_("holder", registry("theirs", "holder")),
            step::set_registry("mine", "lastSpend", clock_unix_timestamp()),
            step::set_registry("mine", "holder", var("holder")),
        ]));
        let read = records_with(&compiled, wire::OP_READ_REGISTRY)[0];
        assert_eq!(read[2], 2);
        assert_eq!(
            immediate(&read),
            16 | (u64::from(wire::OP_READ_PUBKEY) << 16)
        );
        let writes: Vec<_> = records_with(&compiled, wire::OP_WRITE_REGISTRY)
            .iter()
            .map(|record| (record[1], record[3], immediate(record)))
            .collect();
        assert_eq!(
            writes,
            [
                (NONE, 1, 8 | (u64::from(wire::OP_READ_I64) << 16)),
                (NONE, 1, 16 | (u64::from(wire::OP_READ_PUBKEY) << 16)),
            ]
        );
    }

    #[test]
    fn refuses_what_the_verifier_or_the_run_would_refuse() {
        fails(
            base(read("caller", "spent")),
            "caller is not a registry account",
        );
        fails(base(read("mine", "missing")), "limits has no field missing");
        fails(
            base(vec![step::set_registry("mine", "spent", i64(1))]),
            "spent is a u64",
        );
        let without_system: Vec<_> = base_accounts()
            .into_iter()
            .filter(|(name, _)| *name != "systemProgram")
            .collect();
        fails(
            template(without_system, read("mine", "spent")),
            "pinned to the System program",
        );
        fails(
            template(replaced("caller", account::signer()), read("mine", "spent")),
            "payer caller must be a fixed account declared signer and writable",
        );
        fails(
            template(
                replaced("stray", account::registry("nothing", "caller")),
                read("mine", "spent"),
            ),
            "Unknown registry: nothing",
        );
        for misdeclared in [
            account::registry("limits", "caller").signer(),
            account::registry("limits", "caller").executable(),
            account::registry("limits", "caller").address(SYSTEM_PROGRAM_ID),
            account::registry("limits", "caller").owner(SYSTEM_PROGRAM_ID),
            account::registry("limits", "caller").min_data_length(1),
        ] {
            fails(
                template(replaced("mine", misdeclared), read("mine", "spent")),
                "mine must be declared only writable",
            );
        }
        // TypeScript also refuses `{ registry: { ... } }` without `writable`; `account::registry`
        // always declares it writable, so the Rust API cannot express that declaration.
        fails(
            base(vec![step::invoke("systemProgram")
                .program_address(SYSTEM_PROGRAM_ID)
                .writable("mine")
                .data(data::literal([2, 0, 0, 0]))
                .into()]),
            "mine is a registry entry: a CPI that passes it writable fails with RegistryReentry",
        );
    }

    #[test]
    fn a_key_reads_only_the_fields_of_entries_declared_before_it() {
        let keyed_by = |entry: &'static str, key: Expr| {
            template(
                replaced(entry, account::registry("limits", "caller").key(key)),
                vec![step::require(registry("global", "on"))],
            )
        };
        let chained = compile(keyed_by("theirs", registry("mine", "holder")));
        let codes: Vec<_> = records(&chained)
            .into_iter()
            .map(|record| record[0])
            .collect();
        let read = codes
            .iter()
            .position(|&code| code == wire::OP_READ_REGISTRY)
            .unwrap();
        assert!(
            read > codes
                .iter()
                .position(|&code| code == wire::OP_OPEN_REGISTRY)
                .unwrap()
        );
        assert!(
            read < codes
                .iter()
                .rposition(|&code| code == wire::OP_OPEN_REGISTRY)
                .unwrap()
        );
        fails(
            keyed_by("mine", registry("theirs", "holder")),
            "mine's key reads theirs, whose entry opens after mine's: declare theirs before mine",
        );
        fails(
            keyed_by("mine", registry("mine", "holder")),
            "mine's key reads its own entry, which opens only once its key is known",
        );
        let nested = select(
            registry("global", "on"),
            account_key("caller"),
            input("owner"),
        );
        fails_starting(keyed_by("theirs", nested), "theirs's key reads global,");
    }

    #[test]
    fn reads_an_entrys_data_only_through_its_fields() {
        let message =
            "mine is a registry entry: read its fields with expr::registry(\"mine\", field)";
        fails(
            base(vec![step::require(
                account_data("mine", 72, ReadType::U64).eq(u64(0)),
            )]),
            message,
        );
        fails(
            base(vec![step::let_("raw", account_data_bytes("mine", 72, 8))]),
            message,
        );
        compile(base(vec![step::require(lamports("mine").gt(u64(0)))]));
    }
}

// ------------------------------------------------------------------------------------- rateLimit

mod rate_limits {
    use super::*;

    fn limited(steps: Vec<Step>) -> Template {
        Template::new()
            .input("amount", Type::U64)
            .registry("limits", [("spent", Type::U64), ("lastSpend", Type::I64)])
            .account("caller", account::signer().writable())
            .account(
                "limits",
                account::registry("limits", "caller").key(account_key("caller")),
            )
            .account("systemProgram", account::system_program())
            .steps(steps)
    }

    const INLINE: &str = "must be written inline, from literals and arithmetic";

    #[track_caller]
    fn refuses(cap: Expr, refill: Expr, field: &str) {
        fails(
            limited(rate_limit("limits", cap, refill, input("amount")).steps()),
            &format!("{field} {INLINE}"),
        );
    }

    #[track_caller]
    fn accepts(cap: Expr) {
        compile(limited(
            rate_limit("limits", cap, u64(11_574), input("amount")).steps(),
        ));
    }

    #[test]
    fn refills_in_u128_requires_within_rate_limit_and_writes_both_fields_back() {
        let steps = rate_limit("limits", u64(1_000_000_000), u64(11_574), input("amount")).steps();
        assert_eq!(steps.len(), 8);
        let compiled = compile(limited(steps));
        // The caller supplies the amount alone.
        assert_eq!(compiled.input_order, ["amount"]);
        assert!(compiled
            .source_map
            .iter()
            .any(|entry| entry.label.as_deref() == Some("withinRateLimit")));
        assert_eq!(records_with(&compiled, wire::OP_READ_REGISTRY).len(), 2);
        assert_eq!(records_with(&compiled, wire::OP_WRITE_REGISTRY).len(), 2);
        // Five u128 casts: the spent amount, the elapsed time, the rate, the new amount, the cap.
        assert_eq!(records_with(&compiled, wire::OP_CAST_U128).len(), 5);
        assert_eq!(compiled.stats.max_expanded_cpis, 3);
        verifies(&compiled);
    }

    #[test]
    fn names_its_variables_and_requirement_after_name_and_takes_other_field_names() {
        let compiled = compile(
            Template::new()
                .registry("limits", [("used", Type::U64), ("at", Type::I64)])
                .account("caller", account::signer().writable())
                .account("limits", account::registry("limits", "caller"))
                .account("systemProgram", account::system_program())
                .steps(
                    rate_limit("limits", u64(10), u64(1), u64(1))
                        .spent("used")
                        .last_spend("at")
                        .name("daily"),
                )
                .step(step::require(var("dailyLast").lte(var("dailyNow")))),
        );
        assert!(compiled
            .source_map
            .iter()
            .any(|entry| entry.label.as_deref() == Some("withinDaily")));
        // `at` (offset 8, i64) is read first, then `used` (offset 0); `used` then `at` are written.
        let reads: Vec<_> = records_with(&compiled, wire::OP_READ_REGISTRY)
            .iter()
            .map(|record| immediate(record) & 0xffff)
            .collect();
        assert_eq!(reads, [8, 0]);
        let writes: Vec<_> = records_with(&compiled, wire::OP_WRITE_REGISTRY)
            .iter()
            .map(|record| immediate(record) & 0xffff)
            .collect();
        assert_eq!(writes, [0, 8]);
    }

    #[test]
    fn refuses_a_cap_built_from_an_input_even_nested_inside_an_expression() {
        refuses(u64(2) * input("cap"), u64(11_574), "cap");
    }

    #[test]
    fn refuses_a_refill_per_second_built_from_a_row_input() {
        refuses(u64(1_000_000_000), row_input("rate"), "refillPerSecond");
    }

    #[test]
    fn refuses_a_cap_that_grows_with_the_clock_or_a_rate_that_follows_a_loop_index() {
        refuses(u64(1_000) + clock_slot(), u64(11_574), "cap");
        refuses(u64(1_000_000_000), loop_index(), "refillPerSecond");
    }

    #[test]
    fn refuses_a_cap_laundered_through_a_variable() {
        refuses(var("cap"), u64(11_574), "cap");
    }

    #[test]
    fn refuses_a_cap_read_from_the_instructions_sysvar() {
        refuses(
            instruction_data("instructions", u64(0), u64(0), ReadType::U64),
            u64(11_574),
            "cap",
        );
    }

    #[test]
    fn refuses_a_cap_read_from_an_account_field() {
        refuses(lamports("caller"), u64(11_574), "cap");
    }

    #[test]
    fn accepts_a_cap_built_from_arithmetic_over_literals() {
        accepts(u64(1_000) * u64(1_000_000));
    }

    #[test]
    fn accepts_a_cap_read_from_a_registry_field() {
        accepts(registry("limits", "spent"));
    }
}

// -------------------------------------------------------------------------------- register reuse

mod register_reuse {
    use super::*;

    /// `count` checks that read two balances and compare them: three values each, each read once.
    fn balance_checks(count: usize) -> Vec<Step> {
        (0..count)
            .map(|index| {
                step::require(lamports("left").lte(lamports("right")))
                    .label(format!("check{index}"))
            })
            .collect()
    }

    fn accounts() -> Template {
        Template::new()
            .account("left", account::readonly())
            .account("right", account::readonly())
    }

    #[test]
    fn a_template_whose_values_fit_in_64_registers_takes_one_per_value_in_order() {
        let compiled = compile(
            accounts()
                .steps(balance_checks(21))
                .step(step::let_("slot", clock_slot())),
        );
        assert_eq!(compiled.stats.registers, 64);
        let written: Vec<_> = records(&compiled)
            .into_iter()
            .map(|record| record[1])
            .filter(|&dst| dst != NONE)
            .collect();
        assert_eq!(written, (0..64).collect::<Vec<u8>>());
        verifies(&compiled);
    }

    #[test]
    fn past_64_a_value_takes_the_lowest_register_whose_value_was_read_for_the_last_time() {
        let compiled = compile(accounts().steps(balance_checks(22)));
        assert_eq!(compiled.stats.registers, 3);
        assert_eq!(compiled.bytes[9], 3);
        let shape: Vec<_> = records(&compiled)[..8]
            .iter()
            .map(|record| [record[0], record[1], record[2], record[3]])
            .collect();
        let check = [
            [wire::OP_ACCOUNT_LAMPORTS, 0, 0, NONE],
            [wire::OP_ACCOUNT_LAMPORTS, 1, 1, NONE],
            [wire::OP_LTE, 2, 0, 1],
            [wire::OP_REQUIRE, NONE, 2, NONE],
        ];
        assert_eq!(shape, [check, check].concat());
        verifies(&compiled);
    }

    #[test]
    fn too_many_values_live_at_once_names_the_busiest_step() {
        // 65 values all read by one final step: none can share a register.
        let mut template = accounts();
        for index in 0..65 {
            template = template.step(step::let_(format!("v{index}"), clock_slot()));
        }
        let sum = (1..65).fold(var("v0"), |total, index| total + var(format!("v{index}")));
        fails(
            template.step(step::require(sum.gt(u64(0)))),
            "Template uses more than",
        );
    }
}

// ---------------------------------------------------------------------- account group filters

mod account_group_filters {
    use super::*;
    use ballista_sdk::TOKEN_2022_PROGRAM_ID;

    const GROUP_FILTER_FIXTURE: &str = include_str!("../../../fixtures/group-filter.hex");

    fn define(steps: Vec<Step>) -> Template {
        Template::new()
            .input("mint", Type::Pubkey)
            .input("amount", Type::U64)
            .account("user", account::signer())
            .account("destination", account::readonly())
            .account_group("route")
            .steps(steps)
    }

    fn owned_by_user() -> GroupFilter {
        GroupFilter::new()
            .program(TOKEN_PROGRAM_ID)
            .equals(32, account_key("user"))
    }

    fn decode_hex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
            .collect()
    }

    /// `fixtures.test.ts`'s `group-filter`, which the Mollusk suite runs.
    #[test]
    fn compiles_the_typescript_fixture_byte_for_byte() {
        let user_token_accounts = GroupFilter::new()
            .program(TOKEN_PROGRAM_ID)
            .program(TOKEN_2022_PROGRAM_ID)
            .equals(0, input("mint"))
            .equals(32, account_key("user"));
        let template = Template::new()
            .input("mint", Type::Pubkey)
            .account("user", account::signer())
            .account("destination", account::readonly())
            .account_group("route")
            .step(step::set_return_data([
                data::u64(group_length("route")),
                data::u64(group_count("route", user_token_accounts.clone())),
                data::bool(group_any("route", user_token_accounts.clone())),
                data::u64(group_count(
                    "route",
                    user_token_accounts.except_key(account_key("destination")),
                )),
                data::u64(group_count(
                    "route",
                    GroupFilter::new()
                        .program(TOKEN_PROGRAM_ID)
                        .min_data_length(165)
                        .equals(64, u64(1_000)),
                )),
            ]));
        let compiled = compile(template);
        assert_eq!(compiled.bytes, decode_hex(GROUP_FILTER_FIXTURE.trim()));
        verifies(&compiled);
    }

    #[test]
    fn group_length_reads_the_group_by_its_index() {
        let compiled =
            compile(define(vec![step::let_("n", group_length("second"))]).account_group("second"));
        let [record] = records_with(&compiled, wire::OP_GROUP_LENGTH)[..] else {
            panic!("one GROUP_LENGTH");
        };
        assert_eq!([record[2], record[3], record[4]], [1, NONE, NONE]);
        fails(
            define(vec![step::let_("n", group_length("missing"))]),
            "Unknown account group: missing",
        );
    }

    #[test]
    fn a_filter_packs_its_segments_counts_and_floor() {
        let compiled = compile(define(vec![
            step::require(
                group_any(
                    "route",
                    GroupFilter::new()
                        .program(TOKEN_PROGRAM_ID)
                        .program(TOKEN_2022_PROGRAM_ID)
                        .equals(0, input("mint"))
                        .equals(32, account_key("user"))
                        .except_key(account_key("destination")),
                )
                .not(),
            ),
            step::let_(
                "count",
                group_count("route", owned_by_user().min_data_length(165)),
            ),
        ]));
        let [any] = records_with(&compiled, wire::OP_GROUP_ANY)[..] else {
            panic!("one GROUP_ANY");
        };
        assert_eq!([any[2], any[3], any[4]], [0, 0, 1]);
        assert_eq!(immediate(&any), 2 << 16 | 1 << 24 | 64 << 32);
        let [count] = records_with(&compiled, wire::OP_GROUP_COUNT)[..] else {
            panic!("one GROUP_COUNT");
        };
        assert_eq!([count[2], count[3], count[4]], [0, 0, NONE]);
        assert_eq!(immediate(&count), 3 | 1 << 16 | 165 << 32);
        let (segments, _) = segment_tables(&compiled);
        let kinds: Vec<u8> = segments.iter().map(|(kind, _)| *kind).collect();
        assert_eq!(kinds, [wire::DATA_REG_PUBKEY; 4]);
        verifies(&compiled);
    }

    #[test]
    fn match_values_have_a_fixed_width_and_the_floor_covers_every_match() {
        let any =
            |filter: GroupFilter| define(vec![step::let_("found", group_any("route", filter))]);
        verifies(&compile(any(owned_by_user().equals(64, input("amount")))));
        fails(
            any(GroupFilter::new().program(TOKEN_PROGRAM_ID).equals(0, bytes([1]))),
            "groupAny match at offset 0: a match value is a bool, u64, i64, u128 or pubkey, not bytes",
        );
        fails(
            any(owned_by_user().except_key(input("amount"))),
            "groupAny exceptKeys requires pubkey; received u64",
        );
        fails(
            any(owned_by_user().min_data_length(63)),
            "groupAny: minDataLength 63 is shorter than the matches, which read to byte 64",
        );
        verifies(&compile(any(owned_by_user().min_data_length(64))));
    }

    #[test]
    fn the_schema_bounds_the_programs_matches_and_except_keys() {
        let any =
            |filter: GroupFilter| define(vec![step::let_("found", group_any("route", filter))]);
        fails(
            any(GroupFilter::new().equals(32, account_key("user"))),
            "A group filter names one or two programs, not 0",
        );
        fails(
            any(owned_by_user().program(TOKEN_PROGRAM_ID)),
            "A group filter names two different programs",
        );
        fails(
            any(owned_by_user().program(address(1)).program(address(2))),
            "A group filter names one or two programs, not 3",
        );
        fails(
            any(GroupFilter::new().program(TOKEN_PROGRAM_ID)),
            "A group filter holds 1 to 4 matches, not 0",
        );
        let five = (0..5).fold(GroupFilter::new().program(TOKEN_PROGRAM_ID), |filter, _| {
            filter.equals(0, account_key("user"))
        });
        fails(any(five), "A group filter holds 1 to 4 matches, not 5");
        let five = (0..5).fold(owned_by_user(), |filter, _| {
            filter.except_key(account_key("user"))
        });
        fails(
            any(five),
            "A group filter holds at most 4 except keys, not 5",
        );
        fails(
            define(vec![step::let_(
                "found",
                group_any("bad name", owned_by_user()),
            )]),
            "account group name \"bad name\"",
        );
    }

    #[test]
    fn register_reuse_renames_a_filters_values() {
        // 35 constants and 34 additions take more than 64 registers, so the compiler renumbers
        // them; the filter's match value is the last sum.
        let mut steps: Vec<Step> = (0..35u64)
            .map(|index| step::let_(format!("v{index}"), u64(index)))
            .collect();
        let sum = (0..35).fold(u64(0), |total, index| total + var(format!("v{index}")));
        steps.push(step::let_(
            "found",
            group_any(
                "route",
                GroupFilter::new().program(TOKEN_PROGRAM_ID).equals(64, sum),
            ),
        ));
        let compiled = compile(define(steps));
        assert!(compiled.stats.registers <= 64);
        let (segments, _) = segment_tables(&compiled);
        let last_add = records_with(&compiled, wire::OP_ADD).pop().unwrap();
        assert_eq!(segments[0].1, last_add[1]);
        verifies(&compiled);
    }
}
