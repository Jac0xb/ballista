pub mod cases;
mod ceilings;

mod benchmarks;
mod pda_equivalence;
#[cfg(feature = "cu-profile")]
mod phases;
mod profile;

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, collections::HashMap, rc::Rc};

    use base64::{engine::general_purpose::STANDARD, Engine as _};

    use ballista_common::instruction::{
        IX_BEGIN_TEMPLATE, IX_CANCEL_TEMPLATE, IX_CREATE_TEMPLATE, IX_FINALIZE_TEMPLATE, IX_RUN,
        IX_WRITE_TEMPLATE_CHUNK,
    };
    use ballista_common::template::{
        AccountConstraint, CpiAccountRecord, CpiDescriptor, DataSegment, InputDescriptor,
        InstructionRecord, ProgramBuilder, ProgramHeader, ProgramView, PubkeyRecord, Segment,
        TemplateAccount, ACCOUNT_EXECUTABLE, ACCOUNT_SIGNER, ACCOUNT_WRITABLE, DATA_LITERAL,
        DATA_REG_PUBKEY, DATA_REG_U64, ITERATION_ACCOUNT_BIT, MAX_CPI_DATA_LEN, MAX_LOOPS,
        MAX_PDA_SEEDS, MAX_REGISTERS, MAX_ROW_INPUTS, NO_INDEX, OP_ACCOUNT_IS_EMPTY, OP_ACCOUNT_KEY,
        OP_ACCOUNT_LAMPORTS, OP_ADD, OP_DERIVE_PDA, OP_EQ, OP_FOREACH, OP_INVOKE, OP_LOAD_INPUT,
        OP_LTE, OP_NE, OP_READ_I32, OP_READ_U64, OP_REPEAT, OP_REQUIRE, OP_SUB, VALUE_BOOL,
        VALUE_I64, VALUE_U64,
    };
    use mollusk_svm::result::types::{TransactionProgramResult, TransactionResult};
    use mollusk_svm::{program::loader_keys::LOADER_V3, Mollusk, MolluskContext};
    use mollusk_svm_programs_memo::memo;
    use mollusk_svm_programs_token::{associated_token, token};
    use solana_account::{Account, ReadableAccount};
    use solana_instruction::{AccountMeta, Instruction};
    use solana_program_option::COption;
    use solana_pubkey::{pubkey, Pubkey};
    use solana_sdk_ids::{system_program, sysvar};
    use solana_svm_log_collector::LogCollector;
    use spl_token_interface::state::{Account as TokenAccount, AccountState, Mint};
    use zerocopy::{Immutable, IntoBytes};

    const BALLISTA_ELF: &[u8] = include_bytes!("../../../target/deploy/ballista.so");
    const ID: Pubkey = pubkey!("BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD");
    const TEMPLATE_SEED: &[u8] = b"template";

    #[test]
    fn one_shot_open_run_guards_and_privileges() {
        let creator = Pubkey::new_unique();
        let runner = Pubkey::new_unique();
        let recipient = Pubkey::new_unique();
        let mut accounts = funded_accounts([creator, runner, recipient], 10_000_000_000);
        let context = context(std::mem::take(&mut accounts));

        let payload = system_transfer_template(None, true);
        let create = create_template_instruction(creator, 1, &payload);
        let create_result = context.process_instruction(&create);
        assert!(create_result.program_result.is_ok(), "{create_result:#?}");
        let (template, _) = find_template_pda(&creator, 1);
        {
            let store = context.account_store.borrow();
            let template_account = store.get(&template).expect("template account");
            let decoded = TemplateAccount::parse(template_account.data()).expect("template account");
            assert!(decoded.finalized_program().is_ok());
            assert_eq!(decoded.payload(), payload);
        }

        let starting_runner = lamports(&context, runner);
        let starting_recipient = lamports(&context, recipient);
        let amount = 55_000u64;
        let mut input = vec![1];
        input.extend_from_slice(&amount.to_le_bytes());
        let run = run_instruction(
            template,
            vec![
                AccountMeta::new_readonly(system_program::id(), false),
                AccountMeta::new(runner, true),
                AccountMeta::new(recipient, false),
            ],
            &input,
        );
        let run_result = context.process_instruction(&run);
        assert!(run_result.program_result.is_ok(), "{run_result:#?}");
        assert_eq!(lamports(&context, runner), starting_runner - amount);
        assert_eq!(lamports(&context, recipient), starting_recipient + amount);
        eprintln!(
            "single transfer compute units: {}",
            run_result.compute_units_consumed
        );

        let false_guard = run_instruction(
            template,
            vec![
                AccountMeta::new_readonly(system_program::id(), false),
                AccountMeta::new(runner, true),
                AccountMeta::new(recipient, false),
            ],
            &[0, 1, 0, 0, 0, 0, 0, 0, 0],
        );
        let before_guard = lamports(&context, runner);
        assert!(context
            .process_instruction(&false_guard)
            .program_result
            .is_err());
        assert_eq!(lamports(&context, runner), before_guard);

        let missing_signer = run_instruction(
            template,
            vec![
                AccountMeta::new_readonly(system_program::id(), false),
                AccountMeta::new(runner, false),
                AccountMeta::new(recipient, false),
            ],
            &input,
        );
        assert!(context
            .process_instruction(&missing_signer)
            .program_result
            .is_err());

        let readonly_destination = run_instruction(
            template,
            vec![
                AccountMeta::new_readonly(system_program::id(), false),
                AccountMeta::new(runner, true),
                AccountMeta::new_readonly(recipient, false),
            ],
            &input,
        );
        assert!(context
            .process_instruction(&readonly_destination)
            .program_result
            .is_err());

        let wrong_program_address = run_instruction(
            template,
            vec![
                AccountMeta::new_readonly(token::ID, false),
                AccountMeta::new(runner, true),
                AccountMeta::new(recipient, false),
            ],
            &input,
        );
        assert!(context
            .process_instruction(&wrong_program_address)
            .program_result
            .is_err());

        let immutable_write = write_template_chunk_instruction(creator, template, 0, &[9]);
        assert!(context
            .process_instruction(&immutable_write)
            .program_result
            .is_err());
    }

    #[test]
    fn chunked_upload_resume_wrong_offsets_hash_and_cancel() {
        let creator = Pubkey::new_unique();
        let wrong_creator = Pubkey::new_unique();
        let context = context(funded_accounts([creator, wrong_creator], 10_000_000_000));
        let payload = system_transfer_template(None, false);
        let hash = template_hash(&payload);
        let (template, _) = find_template_pda(&creator, 10);

        let begin = begin_template_instruction(creator, 10, payload.len() as u32, hash);
        assert!(context.process_instruction(&begin).program_result.is_ok());
        let incomplete_finalize = finalize_template_instruction(creator, template);
        assert!(context
            .process_instruction(&incomplete_finalize)
            .program_result
            .is_err());
        let wrong_offset = write_template_chunk_instruction(creator, template, 1, &payload[..17]);
        assert!(context
            .process_instruction(&wrong_offset)
            .program_result
            .is_err());
        let wrong_author =
            write_template_chunk_instruction(wrong_creator, template, 0, &payload[..17]);
        assert!(context
            .process_instruction(&wrong_author)
            .program_result
            .is_err());

        let first = write_template_chunk_instruction(creator, template, 0, &payload[..17]);
        let second = write_template_chunk_instruction(creator, template, 17, &payload[17..]);
        assert!(context.process_instruction(&first).program_result.is_ok());
        assert!(context.process_instruction(&second).program_result.is_ok());
        assert!(context
            .process_instruction(&finalize_template_instruction(creator, template))
            .program_result
            .is_ok());

        let bad_payload = system_transfer_template(Some(1), false);
        let (bad_template, _) = find_template_pda(&creator, 11);
        assert!(context
            .process_instruction(&begin_template_instruction(
                creator,
                11,
                bad_payload.len() as u32,
                [7; 32],
            ))
            .program_result
            .is_ok());
        assert!(context
            .process_instruction(&write_template_chunk_instruction(
                creator,
                bad_template,
                0,
                &bad_payload,
            ))
            .program_result
            .is_ok());
        assert!(context
            .process_instruction(&finalize_template_instruction(creator, bad_template))
            .program_result
            .is_err());

        let before_cancel = lamports(&context, creator);
        assert!(context
            .process_instruction(&cancel_template_instruction(creator, bad_template))
            .program_result
            .is_ok());
        assert!(lamports(&context, creator) > before_cancel);
    }

    #[test]
    fn thirty_recipient_batch_bounds_and_atomic_rollback() {
        let creator = Pubkey::new_unique();
        let runner = Pubkey::new_unique();
        let recipients: Vec<_> = (0..31).map(|_| Pubkey::new_unique()).collect();
        let mut accounts = funded_accounts([creator, runner], 10_000_000_000);
        for recipient in &recipients {
            accounts.insert(*recipient, Account::new(0, 0, &system_program::id()));
        }
        let context = context(accounts);
        let payload = system_transfer_template(Some(30), false);
        assert!(context
            .process_instruction(&create_template_instruction(creator, 20, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 20);

        let eight_amount = 5_000u64;
        let mut eight_metas = vec![
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new(runner, true),
        ];
        eight_metas.extend(
            recipients[..8]
                .iter()
                .map(|recipient| AccountMeta::new(*recipient, false)),
        );
        let eight_result = context.process_instruction(&run_instruction(
            template,
            eight_metas,
            &eight_amount.to_le_bytes(),
        ));
        assert!(eight_result.program_result.is_ok(), "{eight_result:#?}");
        eprintln!(
            "8 transfer compute units: {}",
            eight_result.compute_units_consumed
        );

        let amount = 10_000u64;
        let mut metas = vec![
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new(runner, true),
        ];
        metas.extend(
            recipients[..30]
                .iter()
                .map(|recipient| AccountMeta::new(*recipient, false)),
        );
        let result =
            context.process_instruction(&run_instruction(template, metas, &amount.to_le_bytes()));
        assert!(result.program_result.is_ok(), "{result:#?}");
        for (index, recipient) in recipients[..30].iter().enumerate() {
            let expected = amount + if index < 8 { eight_amount } else { 0 };
            assert_eq!(lamports(&context, *recipient), expected);
        }
        eprintln!(
            "30 transfer compute units: {}",
            result.compute_units_consumed
        );

        let mut excessive = vec![
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new(runner, true),
        ];
        excessive.extend(
            recipients
                .iter()
                .map(|recipient| AccountMeta::new(*recipient, false)),
        );
        assert!(context
            .process_instruction(&run_instruction(template, excessive, &amount.to_le_bytes(),))
            .program_result
            .is_err());

        let rollback_runner = Pubkey::new_unique();
        let rollback_a = Pubkey::new_unique();
        let rollback_b = Pubkey::new_unique();
        context
            .account_store
            .borrow_mut()
            .insert(rollback_runner, Account::new(100, 0, &system_program::id()));
        context
            .account_store
            .borrow_mut()
            .insert(rollback_a, Account::new(0, 0, &system_program::id()));
        context
            .account_store
            .borrow_mut()
            .insert(rollback_b, Account::new(0, 0, &system_program::id()));
        let rollback = run_instruction(
            template,
            vec![
                AccountMeta::new_readonly(system_program::id(), false),
                AccountMeta::new(rollback_runner, true),
                AccountMeta::new(rollback_a, false),
                AccountMeta::new(rollback_b, false),
            ],
            &75u64.to_le_bytes(),
        );
        assert!(context
            .process_instruction(&rollback)
            .program_result
            .is_err());
        assert_eq!(lamports(&context, rollback_runner), 100);
        assert_eq!(lamports(&context, rollback_a), 0);
        assert_eq!(lamports(&context, rollback_b), 0);
    }

    #[test]
    fn existing_account_token_batch() {
        let creator = Pubkey::new_unique();
        let authority = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        let source = Pubkey::new_unique();
        let destinations: Vec<_> = (0..8).map(|_| Pubkey::new_unique()).collect();
        let mut accounts = funded_accounts([creator, authority], 10_000_000_000);
        accounts.insert(
            source,
            token::create_account_for_token_account(token_account_state(mint, authority, 1_000_000)),
        );
        for destination in &destinations {
            accounts.insert(
                *destination,
                token::create_account_for_token_account(token_account_state(mint, *destination, 0)),
            );
        }
        let context = context(accounts);
        let payload = token_transfer_template(8);
        assert!(context
            .process_instruction(&create_template_instruction(creator, 30, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 30);
        let mut metas = vec![
            AccountMeta::new_readonly(token::ID, false),
            AccountMeta::new(source, false),
            AccountMeta::new_readonly(authority, true),
        ];
        metas.extend(
            destinations
                .iter()
                .map(|destination| AccountMeta::new(*destination, false)),
        );
        let amount = 12_500u64;
        let result =
            context.process_instruction(&run_instruction(template, metas, &amount.to_le_bytes()));
        assert!(result.program_result.is_ok(), "{result:#?}");
        assert_eq!(token_amount(&context, source), 1_000_000 - amount * 8);
        for destination in destinations {
            assert_eq!(token_amount(&context, destination), amount);
        }

        let wrong_owner = Pubkey::new_unique();
        context.account_store.borrow_mut().insert(
            wrong_owner,
            Account::new(1_000_000, 165, &system_program::id()),
        );
        let wrong_owner_run = run_instruction(
            template,
            vec![
                AccountMeta::new_readonly(token::ID, false),
                AccountMeta::new(source, false),
                AccountMeta::new_readonly(authority, true),
                AccountMeta::new(wrong_owner, false),
            ],
            &amount.to_le_bytes(),
        );
        let wrong_owner_result = context.process_instruction(&wrong_owner_run);
        // Runtime account 3 (the first row) fails its owner constraint: 6020 with the index.
        assert_eq!(
            custom_code(&wrong_owner_result),
            Some((3 << 16) | 6020),
            "{wrong_owner_result:#?}"
        );
    }

    #[test]
    fn stride_two_conditional_non_idempotent_ata_create_then_transfer() {
        let creator = Pubkey::new_unique();
        let authority = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        let source = Pubkey::new_unique();
        let owner_missing = Pubkey::new_unique();
        let owner_existing = Pubkey::new_unique();
        let (missing_ata, _) = associated_token::create_account_for_associated_token_account(
            token_account_state(mint, owner_missing, 0),
        );
        let (existing_ata, existing_account) =
            associated_token::create_account_for_associated_token_account(token_account_state(
                mint,
                owner_existing,
                0,
            ));
        let mut accounts = funded_accounts(
            [creator, authority, owner_missing, owner_existing],
            10_000_000_000,
        );
        accounts.insert(
            mint,
            token::create_account_for_mint(Mint {
                mint_authority: COption::Some(authority),
                supply: 1_000_000,
                decimals: 6,
                is_initialized: true,
                freeze_authority: COption::None,
            }),
        );
        accounts.insert(
            source,
            token::create_account_for_token_account(token_account_state(mint, authority, 1_000_000)),
        );
        accounts.insert(existing_ata, existing_account);
        let context = context(accounts);

        let unguarded_existing_create = Instruction {
            program_id: associated_token::ID,
            accounts: vec![
                AccountMeta::new(creator, true),
                AccountMeta::new(existing_ata, false),
                AccountMeta::new_readonly(owner_existing, false),
                AccountMeta::new_readonly(mint, false),
                AccountMeta::new_readonly(system_program::id(), false),
                AccountMeta::new_readonly(token::ID, false),
            ],
            data: Vec::new(),
        };
        let unguarded_result = context.process_instruction(&unguarded_existing_create);
        assert!(
            unguarded_result.program_result.is_err(),
            "ordinary ATA Create must fail when the account exists: {unguarded_result:#?}"
        );

        let payload = ata_then_transfer_template(4);
        assert!(context
            .process_instruction(&create_template_instruction(creator, 31, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 31);
        let amount = 8_000u64;
        let result = context.process_instruction(&run_instruction(
            template,
            vec![
                AccountMeta::new_readonly(associated_token::ID, false),
                AccountMeta::new_readonly(token::ID, false),
                AccountMeta::new_readonly(system_program::id(), false),
                AccountMeta::new(creator, true),
                AccountMeta::new(source, false),
                AccountMeta::new_readonly(authority, true),
                AccountMeta::new_readonly(mint, false),
                AccountMeta::new_readonly(owner_missing, false),
                AccountMeta::new(missing_ata, false),
                AccountMeta::new_readonly(owner_existing, false),
                AccountMeta::new(existing_ata, false),
            ],
            &amount.to_le_bytes(),
        ));
        assert!(result.program_result.is_ok(), "{result:#?}");
        assert_eq!(token_amount(&context, missing_ata), amount);
        assert_eq!(token_amount(&context, existing_ata), amount);
        assert_eq!(token_amount(&context, source), 1_000_000 - amount * 2);

        let source_before_mismatch = token_amount(&context, source);
        let mismatched_owner_and_ata = context.process_instruction(&run_instruction(
            template,
            vec![
                AccountMeta::new_readonly(associated_token::ID, false),
                AccountMeta::new_readonly(token::ID, false),
                AccountMeta::new_readonly(system_program::id(), false),
                AccountMeta::new(creator, true),
                AccountMeta::new(source, false),
                AccountMeta::new_readonly(authority, true),
                AccountMeta::new_readonly(mint, false),
                AccountMeta::new_readonly(owner_existing, false),
                AccountMeta::new(missing_ata, false),
            ],
            &amount.to_le_bytes(),
        ));
        assert!(mismatched_owner_and_ata.program_result.is_err());
        assert_eq!(token_amount(&context, source), source_before_mismatch);
    }

    /// A carried register accumulates across rows and is readable after the loop, so a template
    /// can enforce a budget over a whole batch. Zero rows are rejected once a minimum is declared.
    #[test]
    fn carried_sum_enforces_a_budget_across_rows() {
        let creator = Pubkey::new_unique();
        let rows: Vec<Pubkey> = (0..3).map(|_| Pubkey::new_unique()).collect();
        let mut accounts = funded_accounts([creator], 10_000_000_000);
        for (index, row) in rows.iter().enumerate() {
            accounts.insert(*row, Account::new((index as u64 + 1) * 100, 0, &system_program::id()));
        }
        let context = context(accounts);

        let build = |min_iterations: u8| {
            let mut builder = ProgramBuilder::new();
            let row = builder.row_account(0, None, None, 0);
            builder.batch(3, min_iterations);
            let budget_input = builder.input(VALUE_U64, 0);
            let budget = builder.load_input(budget_input);
            let total = builder.const_u64(0);
            builder.for_each(1 << total, |body| {
                let lamports = body.account_lamports(row);
                let sum = body.binary(OP_ADD, total, lamports);
                body.mov(total, sum);
            });
            let within = builder.binary(OP_LTE, total, budget);
            builder.require(within);
            let payload = builder.build().expect("builds");
            ProgramView::parse(&payload)
                .and_then(|program| program.verify())
                .expect("verifies");
            payload
        };

        let payload = build(0);
        assert!(context
            .process_instruction(&create_template_instruction(creator, 50, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 50);
        let metas: Vec<AccountMeta> = rows
            .iter()
            .map(|row| AccountMeta::new_readonly(*row, false))
            .collect();

        let exact = context.process_instruction(&run_instruction(
            template,
            metas.clone(),
            &600u64.to_le_bytes(),
        ));
        assert!(exact.program_result.is_ok(), "{exact:#?}");

        let short = context.process_instruction(&run_instruction(
            template,
            metas.clone(),
            &599u64.to_le_bytes(),
        ));
        // Instruction 7 is the `require`; 6015 is RequirementFailed.
        assert_eq!(custom_code(&short), Some((7 << 16) | 6015), "{short:#?}");

        let empty = context.process_instruction(&run_instruction(
            template,
            Vec::new(),
            &0u64.to_le_bytes(),
        ));
        assert!(empty.program_result.is_ok(), "zero rows leave the total at zero");

        let payload = build(1);
        assert!(context
            .process_instruction(&create_template_instruction(creator, 51, &payload))
            .program_result
            .is_ok());
        let (strict, _) = find_template_pda(&creator, 51);
        let rejected = context.process_instruction(&run_instruction(
            strict,
            Vec::new(),
            &0u64.to_le_bytes(),
        ));
        // 6010 is InvalidAccountRange; the context is the iteration count that was rejected.
        assert_eq!(custom_code(&rejected), Some(6010), "{rejected:#?}");
        let one_row = context.process_instruction(&run_instruction(
            strict,
            metas[..1].to_vec(),
            &100u64.to_le_bytes(),
        ));
        assert!(one_row.program_result.is_ok(), "{one_row:#?}");
    }

    /// A dynamic-offset read takes its offset from a register, so one template can read a field
    /// whose position the caller supplies at run time.
    #[test]
    fn dynamic_offset_reads_use_the_register_value() {
        let creator = Pubkey::new_unique();
        let authority = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        let token_account = Pubkey::new_unique();
        let mut accounts = funded_accounts([creator], 10_000_000_000);
        accounts.insert(
            token_account,
            token::create_account_for_token_account(token_account_state(mint, authority, 4_242)),
        );
        let context = context(accounts);

        let mut builder = ProgramBuilder::new();
        let holder = builder.account(0, None, Some(token::ID.to_bytes()), 0);
        let offset_input = builder.input(VALUE_U64, 0);
        let expected_input = builder.input(VALUE_U64, 0);
        let offset = builder.load_input(offset_input);
        let expected = builder.load_input(expected_input);
        let value = builder.read_dynamic(OP_READ_U64, holder, offset);
        let matches = builder.binary(OP_EQ, value, expected);
        builder.require(matches);
        let payload = builder.build().expect("builds");
        ProgramView::parse(&payload)
            .and_then(|program| program.verify())
            .expect("verifies");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 52, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 52);
        let metas = vec![AccountMeta::new_readonly(token_account, false)];
        let inputs = |offset: u64, expected: u64| {
            let mut bytes = offset.to_le_bytes().to_vec();
            bytes.extend_from_slice(&expected.to_le_bytes());
            bytes
        };

        let amount_at_64 =
            context.process_instruction(&run_instruction(template, metas.clone(), &inputs(64, 4_242)));
        assert!(amount_at_64.program_result.is_ok(), "{amount_at_64:#?}");

        let misaligned =
            context.process_instruction(&run_instruction(template, metas.clone(), &inputs(65, 4_242)));
        assert_eq!(custom_code(&misaligned), Some((4 << 16) | 6015), "{misaligned:#?}");

        let past_the_end =
            context.process_instruction(&run_instruction(template, metas, &inputs(200, 0)));
        assert_eq!(custom_code(&past_the_end), Some((2 << 16) | 6009), "{past_the_end:#?}");
    }

    /// SPL Token's GetAccountDataSize sets the account size as return data. The template reads it
    /// straight after the CPI; a callee that sets nothing leaves nothing to read.
    #[test]
    fn return_data_is_readable_right_after_the_invoke_that_set_it() {
        let creator = Pubkey::new_unique();
        let authority = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let recipient = Pubkey::new_unique();
        let mut accounts = funded_accounts([creator, payer, recipient], 10_000_000_000);
        accounts.insert(
            mint,
            token::create_account_for_mint(Mint {
                mint_authority: COption::Some(authority),
                supply: 0,
                decimals: 6,
                is_initialized: true,
                freeze_authority: COption::None,
            }),
        );
        let context = context(accounts);

        let mut builder = ProgramBuilder::new();
        let token_program = builder.account(ACCOUNT_EXECUTABLE, Some(token::ID.to_bytes()), None, 0);
        let mint_account = builder.account(0, None, Some(token::ID.to_bytes()), 82);
        let literal = builder.blob(&[21]);
        let cpi = builder.cpi(token_program, &[(mint_account, 0)], &[Segment::Literal(literal)]);
        builder.invoke(cpi, None);
        let size = builder.return_data(OP_READ_U64, 0);
        let expected = builder.const_u64(165);
        let same = builder.binary(OP_EQ, size, expected);
        builder.require(same);
        let payload = builder.build().expect("builds");
        ProgramView::parse(&payload)
            .and_then(|program| program.verify())
            .expect("verifies");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 60, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 60);
        let result = context.process_instruction(&run_instruction(
            template,
            vec![
                AccountMeta::new_readonly(token::ID, false),
                AccountMeta::new_readonly(mint, false),
            ],
            &[],
        ));
        assert!(result.program_result.is_ok(), "{result:#?}");

        let mut builder = ProgramBuilder::new();
        let system = builder.account(
            ACCOUNT_EXECUTABLE,
            Some(system_program::id().to_bytes()),
            None,
            0,
        );
        let source = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        let destination = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        let mut data = vec![2, 0, 0, 0];
        data.extend_from_slice(&1u64.to_le_bytes());
        let literal = builder.blob(&data);
        let cpi = builder.cpi(
            system,
            &[
                (source, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
                (destination, ACCOUNT_WRITABLE),
            ],
            &[Segment::Literal(literal)],
        );
        builder.invoke(cpi, None);
        let value = builder.return_data(OP_READ_U64, 0);
        let same = builder.binary(OP_EQ, value, value);
        builder.require(same);
        let payload = builder.build().expect("builds");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 61, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 61);
        let result = context.process_instruction(&run_instruction(
            template,
            vec![
                AccountMeta::new_readonly(system_program::id(), false),
                AccountMeta::new(payer, true),
                AccountMeta::new(recipient, false),
            ],
            &[],
        ));
        // Instruction 1 is the return-data read; 6018 is MissingReturnData.
        assert_eq!(custom_code(&result), Some((1 << 16) | 6018), "{result:#?}");
    }

    /// The event flag adds one data log after a successful run and changes nothing else. Mollusk
    /// records program logs once it is given a log collector, so the event is read back from its
    /// `Program data:` line.
    #[test]
    fn event_flag_does_not_change_run_semantics() {
        let creator = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let recipient = Pubkey::new_unique();
        let mut context = context(funded_accounts([creator, payer, recipient], 10_000_000_000));
        let logger = LogCollector::new_ref();
        context.mollusk.logger = Some(logger.clone());

        let mut builder = ProgramBuilder::new();
        builder.flags(ballista_common::template::PROGRAM_FLAG_EMIT_EVENT);
        let system = builder.account(
            ACCOUNT_EXECUTABLE,
            Some(system_program::id().to_bytes()),
            None,
            0,
        );
        let source = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        let destination = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        let amount = builder.const_u64(1_000);
        let literal = builder.blob(&[2, 0, 0, 0]);
        let cpi = builder.cpi(
            system,
            &[
                (source, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
                (destination, ACCOUNT_WRITABLE),
            ],
            &[
                Segment::Literal(literal),
                Segment::Register(DATA_REG_U64, amount),
            ],
        );
        builder.invoke(cpi, None);
        let payload = builder.build().expect("builds");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 62, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 62);
        let before = lamports(&context, recipient);
        let result = context.process_instruction(&run_instruction(
            template,
            vec![
                AccountMeta::new_readonly(system_program::id(), false),
                AccountMeta::new(payer, true),
                AccountMeta::new(recipient, false),
            ],
            &[],
        ));
        assert!(result.program_result.is_ok(), "{result:#?}");
        assert_eq!(lamports(&context, recipient), before + 1_000);
        // Magic, bytecode version, iterations, invokes reached, the mask of those that ran, and
        // the template address.
        let mut event = b"BEV1".to_vec();
        event.extend_from_slice(&[1, 0, 1]);
        event.extend_from_slice(&1u64.to_le_bytes());
        event.extend_from_slice(template.as_ref());
        assert_eq!(program_data(&logger), vec![event]);
        eprintln!(
            "transfer with event compute units: {}",
            result.compute_units_consumed
        );
    }

    /// Anyone can send lamports to a predictable template address before it is created. Creation
    /// must tolerate that instead of letting dust block the ID forever.
    #[test]
    fn create_template_succeeds_on_a_prefunded_pda() {
        let creator = Pubkey::new_unique();
        let (dusted, _) = find_template_pda(&creator, 77);
        let (overfunded, _) = find_template_pda(&creator, 78);
        let mut accounts = funded_accounts([creator], 10_000_000_000);
        accounts.insert(dusted, Account::new(1, 0, &system_program::id()));
        accounts.insert(overfunded, Account::new(5_000_000_000, 0, &system_program::id()));
        let context = context(accounts);
        let payload = system_transfer_template(None, false);

        let result = context.process_instruction(&create_template_instruction(creator, 77, &payload));
        assert!(result.program_result.is_ok(), "{result:#?}");
        {
            let store = context.account_store.borrow();
            let account = store.get(&dusted).expect("template account");
            assert_eq!(account.owner(), &ID);
            let decoded = TemplateAccount::parse(account.data()).expect("template parses");
            assert!(decoded.finalized_program().is_ok());
        }

        // An address already holding more than rent exemption costs the creator nothing.
        let before = lamports(&context, creator);
        let result = context.process_instruction(&create_template_instruction(creator, 78, &payload));
        assert!(result.program_result.is_ok(), "{result:#?}");
        assert_eq!(lamports(&context, creator), before);
        assert_eq!(lamports(&context, overfunded), 5_000_000_000);

        // The chunked path takes the same route.
        let (chunked, _) = find_template_pda(&creator, 79);
        context
            .account_store
            .borrow_mut()
            .insert(chunked, Account::new(1, 0, &system_program::id()));
        let hash = template_hash(&payload);
        assert!(context
            .process_instruction(&begin_template_instruction(creator, 79, payload.len() as u32, hash))
            .program_result
            .is_ok());
        assert!(context
            .process_instruction(&write_template_chunk_instruction(creator, chunked, 0, &payload))
            .program_result
            .is_ok());
        assert!(context
            .process_instruction(&finalize_template_instruction(creator, chunked))
            .program_result
            .is_ok());
    }

    /// Fifty-eight system transfers each carrying 1,000 bytes of instruction data. The system
    /// program's bincode decoder tolerates trailing bytes, so the padding is accepted. Per-CPI
    /// allocation with a bump allocator would need about 60 KB against a 32 KB heap; scratch
    /// reuse keeps it constant.
    #[test]
    fn many_large_cpis_fit_in_the_default_heap() {
        let creator = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let rows: Vec<Pubkey> = (0..58).map(|_| Pubkey::new_unique()).collect();
        let mut accounts = funded_accounts([creator, payer], 10_000_000_000);
        for row in &rows {
            accounts.insert(*row, Account::new(0, 0, &system_program::id()));
        }
        let context = context(accounts);

        let mut builder = ProgramBuilder::new();
        let system = builder.account(
            ACCOUNT_EXECUTABLE,
            Some(system_program::id().to_bytes()),
            None,
            0,
        );
        let source = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        let recipient = builder.row_account(ACCOUNT_WRITABLE, None, None, 0);
        builder.batch(58, 0);
        let mut data = vec![2, 0, 0, 0];
        data.extend_from_slice(&0u64.to_le_bytes());
        data.resize(1_000, 0);
        let literal = builder.blob(&data);
        let cpi = builder.cpi(
            system,
            &[
                (source, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
                (recipient, ACCOUNT_WRITABLE),
            ],
            &[Segment::Literal(literal)],
        );
        builder.for_each(0, |body| body.invoke(cpi, None));
        let payload = builder.build().expect("builds");
        ProgramView::parse(&payload)
            .and_then(|program| program.verify())
            .expect("verifies");

        assert!(context
            .process_instruction(&create_template_instruction(creator, 40, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 40);
        let mut metas = vec![
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new(payer, true),
        ];
        metas.extend(rows.iter().map(|row| AccountMeta::new(*row, false)));
        let result = context.process_instruction(&run_instruction(template, metas, &[]));
        assert!(result.program_result.is_ok(), "{result:#?}");
        eprintln!(
            "58 transfers with 1000-byte data compute units: {}",
            result.compute_units_consumed
        );
    }

    /// Two fifteen-seed PDA derivations in each of 59 rows. Per-derivation seed vectors would
    /// need well over 100 KB of bump-allocated heap.
    #[test]
    fn pda_derivation_in_every_row_fits_in_the_default_heap() {
        let creator = Pubkey::new_unique();
        let rows: Vec<Pubkey> = (0..59).map(|_| Pubkey::new_unique()).collect();
        let mut accounts = funded_accounts([creator], 10_000_000_000);
        for row in &rows {
            accounts.insert(*row, Account::new(0, 0, &system_program::id()));
        }
        let context = context(accounts);

        let mut builder = ProgramBuilder::new();
        let system = builder.account(
            ACCOUNT_EXECUTABLE,
            Some(system_program::id().to_bytes()),
            None,
            0,
        );
        let row = builder.row_account(0, None, None, 0);
        builder.batch(59, 0);
        builder.for_each(0, |body| {
            let key = body.account_key(row);
            let seeds = vec![Segment::Register(DATA_REG_PUBKEY, key); MAX_PDA_SEEDS];
            let first = body.derive_pda(system, &seeds);
            let second = body.derive_pda(system, &seeds[..1]);
            let first_differs = body.binary(OP_NE, first, key);
            let second_differs = body.binary(OP_NE, second, key);
            body.require(first_differs);
            body.require(second_differs);
        });
        let payload = builder.build().expect("builds");
        ProgramView::parse(&payload)
            .and_then(|program| program.verify())
            .expect("verifies");

        assert!(context
            .process_instruction(&create_template_instruction(creator, 41, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 41);
        let mut metas = vec![AccountMeta::new_readonly(system_program::id(), false)];
        metas.extend(rows.iter().map(|row| AccountMeta::new_readonly(*row, false)));
        let result = context.process_instruction(&run_instruction(template, metas, &[]));
        assert!(result.program_result.is_ok(), "{result:#?}");
        eprintln!(
            "118 PDA derivations compute units: {}",
            result.compute_units_consumed
        );
    }

    /// Templates compiled by the TypeScript SDK run unchanged on the Rust program. Each fixture is
    /// exercised through a real scenario, so compiler and executor cannot drift apart silently.
    #[test]
    fn typescript_compiled_fixtures_run_end_to_end() {
        let creator = Pubkey::new_unique();
        let authority = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let owner = Pubkey::new_unique();
        let holder = Pubkey::new_unique();
        let (ata, _) = associated_token::create_account_for_associated_token_account(
            token_account_state(mint, owner, 0),
        );
        let rows: Vec<Pubkey> = (0..3).map(|_| Pubkey::new_unique()).collect();
        let mut accounts = funded_accounts([creator, payer, owner], 10_000_000_000);
        accounts.insert(
            mint,
            token::create_account_for_mint(Mint {
                mint_authority: COption::Some(authority),
                supply: 0,
                decimals: 6,
                is_initialized: true,
                freeze_authority: COption::None,
            }),
        );
        accounts.insert(
            holder,
            token::create_account_for_token_account(token_account_state(mint, authority, 4_242)),
        );
        for (index, row) in rows.iter().enumerate() {
            accounts.insert(*row, Account::new((index as u64 + 1) * 100, 0, &system_program::id()));
        }
        let context = context(accounts);

        // ensure-ata: assert the ATA relationship, create when missing, and be a no-op afterwards.
        let payload = fixture("ensure-ata");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 70, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 70);
        let metas = vec![
            AccountMeta::new_readonly(associated_token::ID, false),
            AccountMeta::new_readonly(token::ID, false),
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(owner, false),
            AccountMeta::new(ata, false),
        ];
        let first = context.process_instruction(&run_instruction(template, metas.clone(), &[]));
        assert!(first.program_result.is_ok(), "{first:#?}");
        assert_eq!(token_amount(&context, ata), 0);
        let repeat = context.process_instruction(&run_instruction(template, metas.clone(), &[]));
        assert!(repeat.program_result.is_ok(), "{repeat:#?}");
        assert!(
            repeat.compute_units_consumed < first.compute_units_consumed / 3,
            "the guarded repeat skips the CPI: {} vs {}",
            repeat.compute_units_consumed,
            first.compute_units_consumed
        );
        let mut wrong_owner = metas.clone();
        wrong_owner[5] = AccountMeta::new_readonly(payer, false);
        let mismatch = context.process_instruction(&run_instruction(template, wrong_owner, &[]));
        // The assertion is instruction 4 (three key reads, one derivation, then the compare and require).
        assert_eq!(custom_code(&mismatch), Some((6 << 16) | 6015), "{mismatch:#?}");

        // carry-sum: total row lamports against a budget, with at least one row required.
        let payload = fixture("carry-sum");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 71, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 71);
        let row_metas: Vec<AccountMeta> = rows
            .iter()
            .map(|row| AccountMeta::new_readonly(*row, false))
            .collect();
        let within = context.process_instruction(&run_instruction(
            template,
            row_metas.clone(),
            &600u64.to_le_bytes(),
        ));
        assert!(within.program_result.is_ok(), "{within:#?}");
        let over = context.process_instruction(&run_instruction(
            template,
            row_metas,
            &599u64.to_le_bytes(),
        ));
        assert_eq!(
            decode_kind(&over),
            Some(6015),
            "budget breach is a failed require: {over:#?}"
        );
        let empty = context.process_instruction(&run_instruction(template, Vec::new(), &0u64.to_le_bytes()));
        assert_eq!(custom_code(&empty), Some(6010), "min iterations of one rejects zero rows");

        // return-data: read the token account size that GetAccountDataSize returns.
        let payload = fixture("return-data");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 72, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 72);
        let sized = context.process_instruction(&run_instruction(
            template,
            vec![
                AccountMeta::new_readonly(token::ID, false),
                AccountMeta::new_readonly(mint, false),
            ],
            &[],
        ));
        assert!(sized.program_result.is_ok(), "{sized:#?}");

        // dynamic-read: the caller supplies the offset of the field to compare.
        let payload = fixture("dynamic-read");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 73, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 73);
        let mut inputs = 64u64.to_le_bytes().to_vec();
        inputs.extend_from_slice(&4_242u64.to_le_bytes());
        let read = context.process_instruction(&run_instruction(
            template,
            vec![AccountMeta::new_readonly(holder, false)],
            &inputs,
        ));
        assert!(read.program_result.is_ok(), "{read:#?}");

        // pinned-mint-read: a fixed-offset read whose minimum data length the compiler inferred.
        let payload = fixture("pinned-mint-read");
        let pinned_mint = Pubkey::new_from_array([4; 32]);
        context.account_store.borrow_mut().insert(
            pinned_mint,
            token::create_account_for_mint(Mint {
                mint_authority: COption::None,
                supply: 0,
                decimals: 6,
                is_initialized: true,
                freeze_authority: COption::None,
            }),
        );
        assert!(context
            .process_instruction(&create_template_instruction(creator, 74, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 74);
        let decimals = context.process_instruction(&run_instruction(
            template,
            vec![AccountMeta::new_readonly(pinned_mint, false)],
            &[],
        ));
        assert!(decimals.program_result.is_ok(), "{decimals:#?}");
        let short = Pubkey::new_unique();
        context
            .account_store
            .borrow_mut()
            .insert(short, Account::new(1_000_000, 10, &token::ID));
        let too_short = context.process_instruction(&run_instruction(
            template,
            vec![AccountMeta::new_readonly(pinned_mint, false)],
            &[1],
        ));
        assert_eq!(custom_code(&too_short), Some(6008), "trailing input bytes are rejected");
    }

    /// Supplying the canonical bump turns the derivation into one `create_program_address` call
    /// instead of a search down from 255, and a wrong bump still fails the assertion.
    #[test]
    fn a_supplied_bump_derives_once_and_still_rejects_substitutes() {
        let creator = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        // A shallow owner finds its bump on the first attempt; a deep one costs several more.
        let ata_of = |owner: &Pubkey| {
            Pubkey::find_program_address(
                &[owner.as_ref(), token::ID.as_ref(), mint.as_ref()],
                &associated_token::ID,
            )
        };
        let shallow = std::iter::repeat_with(Pubkey::new_unique)
            .find(|owner| ata_of(owner).1 == 255)
            .expect("a first-attempt bump exists");
        let deep = std::iter::repeat_with(Pubkey::new_unique)
            .find(|owner| ata_of(owner).1 <= 251)
            .expect("a fourth-attempt bump exists");

        let mut accounts = funded_accounts([creator], 10_000_000_000);
        accounts.insert(mint, Account::new(1_000_000, 0, &system_program::id()));
        for owner in [shallow, deep] {
            accounts.insert(owner, Account::new(1_000_000, 0, &system_program::id()));
            accounts.insert(ata_of(&owner).0, Account::new(1_000_000, 0, &system_program::id()));
        }
        let context = context(accounts);

        let searched = fixture("assert-ata");
        let supplied = fixture("assert-ata-with-bump");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 90, &searched))
            .program_result
            .is_ok());
        assert!(context
            .process_instruction(&create_template_instruction(creator, 91, &supplied))
            .program_result
            .is_ok());
        let (searched_template, _) = find_template_pda(&creator, 90);
        let (supplied_template, _) = find_template_pda(&creator, 91);

        let metas = |owner: Pubkey| {
            vec![
                AccountMeta::new_readonly(associated_token::ID, false),
                AccountMeta::new_readonly(token::ID, false),
                AccountMeta::new_readonly(owner, false),
                AccountMeta::new_readonly(mint, false),
                AccountMeta::new_readonly(ata_of(&owner).0, false),
            ]
        };
        let run = |template: Pubkey, owner: Pubkey, inputs: &[u8]| {
            let result = context.process_instruction(&run_instruction(template, metas(owner), inputs));
            assert!(result.program_result.is_ok(), "{result:#?}");
            result.compute_units_consumed
        };

        let shallow_bump = (ata_of(&shallow).1 as u64).to_le_bytes();
        let deep_bump = (ata_of(&deep).1 as u64).to_le_bytes();
        let shallow_searched = run(searched_template, shallow, &[]);
        let deep_searched = run(searched_template, deep, &[]);
        let shallow_supplied = run(supplied_template, shallow, &shallow_bump);
        let deep_supplied = run(supplied_template, deep, &deep_bump);
        eprintln!(
            "ATA assertion compute units: searched {shallow_searched} (bump 255) /              {deep_searched} (bump {}), supplied {shallow_supplied} / {deep_supplied}",
            ata_of(&deep).1
        );

        // One derivation costs the same however deep the canonical bump is.
        assert!(
            shallow_supplied.abs_diff(deep_supplied) < 50,
            "a supplied bump is flat: {shallow_supplied} vs {deep_supplied}"
        );
        // The search pays for every bump it rejects, and the deep owner's rejects at least four:
        // a SHA-256 of the 150-byte preimage and a curve check, about 330 units each.
        const PER_ATTEMPT: u64 = 300;
        assert!(
            deep_searched > shallow_searched + 4 * PER_ATTEMPT,
            "the search grows with depth: {deep_searched} vs {shallow_searched}"
        );
        assert!(
            deep_supplied + 3 * PER_ATTEMPT < deep_searched,
            "the supplied bump skips that search: {deep_supplied} vs {deep_searched}"
        );

        // A bump one below the canonical one either lands on the curve, which is not a valid
        // program address, or derives a different one; both reject the substituted account.
        let wrong = context.process_instruction(&run_instruction(
            supplied_template,
            metas(deep),
            &(ata_of(&deep).1 as u64 - 1).to_le_bytes(),
        ));
        assert!(
            matches!(decode_kind(&wrong), Some(6015 | 6017)),
            "{wrong:#?}"
        );

        // A bump that does not fit in a byte is rejected before any derivation.
        let oversized = context.process_instruction(&run_instruction(
            supplied_template,
            metas(deep),
            &256u64.to_le_bytes(),
        ));
        assert_eq!(decode_kind(&oversized), Some(6017), "{oversized:#?}");
    }

    /// Errors raised by an invoked program reach the caller untouched, so they are never mistaken
    /// for Ballista's own codes.
    #[test]
    fn callee_errors_pass_through_unchanged() {
        let creator = Pubkey::new_unique();
        let context = context(funded_accounts([creator], 10_000_000_000));

        // An unpinned program account lets the caller substitute any executable.
        let mut builder = ProgramBuilder::new();
        let program = builder.account(ACCOUNT_EXECUTABLE, None, None, 0);
        let literal = builder.blob(&[0xff, 0xfe]);
        let cpi = builder.cpi(program, &[], &[Segment::Literal(literal)]);
        builder.invoke(cpi, None);
        let payload = builder.build().expect("builds");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 80, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 80);
        let result = context.process_instruction(&run_instruction(
            template,
            vec![AccountMeta::new_readonly(memo::ID, false)],
            &[],
        ));
        // Memo rejects invalid UTF-8 with the builtin InvalidInstructionData, not a custom code.
        assert_eq!(
            result.program_result,
            mollusk_svm::result::ProgramResult::Failure(
                solana_program_error::ProgramError::InvalidInstructionData
            ),
            "{result:#?}"
        );
        assert_eq!(custom_code(&result), None);
    }

    /// Ballista may invoke itself, so one template can run another as a step.
    #[test]
    fn nested_template_runs_through_cpi() {
        let creator = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let recipient = Pubkey::new_unique();
        let context = context(funded_accounts([creator, payer, recipient], 10_000_000_000));

        let inner_payload = system_transfer_template(None, false);
        assert!(context
            .process_instruction(&create_template_instruction(creator, 81, &inner_payload))
            .program_result
            .is_ok());
        let (inner, _) = find_template_pda(&creator, 81);

        let mut builder = ProgramBuilder::new();
        let ballista = builder.account(ACCOUNT_EXECUTABLE, Some(ID.to_bytes()), None, 0);
        let template = builder.account(0, None, Some(ID.to_bytes()), 80);
        let system = builder.account(ACCOUNT_EXECUTABLE, Some(system_program::id().to_bytes()), None, 0);
        let source = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        let destination = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        let mut data = vec![IX_RUN];
        data.extend_from_slice(&2_500u64.to_le_bytes());
        let literal = builder.blob(&data);
        let cpi = builder.cpi(
            ballista,
            &[
                (template, 0),
                (system, 0),
                (source, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
                (destination, ACCOUNT_WRITABLE),
            ],
            &[Segment::Literal(literal)],
        );
        builder.invoke(cpi, None);
        let outer_payload = builder.build().expect("builds");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 82, &outer_payload))
            .program_result
            .is_ok());
        let (outer, _) = find_template_pda(&creator, 82);
        let before = lamports(&context, recipient);
        let result = context.process_instruction(&run_instruction(
            outer,
            vec![
                AccountMeta::new_readonly(ID, false),
                AccountMeta::new_readonly(inner, false),
                AccountMeta::new_readonly(system_program::id(), false),
                AccountMeta::new(payer, true),
                AccountMeta::new(recipient, false),
            ],
            &[],
        ));
        assert!(result.program_result.is_ok(), "{result:#?}");
        assert_eq!(lamports(&context, recipient), before + 2_500);
        eprintln!(
            "nested template run compute units: {}",
            result.compute_units_consumed
        );
    }

    /// Sixty runtime accounts are the ceiling; the sixty-first is rejected with the count.
    #[test]
    fn one_hundred_twenty_runtime_accounts_are_the_ceiling() {
        let creator = Pubkey::new_unique();
        let rows: Vec<Pubkey> = (0..120).map(|_| Pubkey::new_unique()).collect();
        let mut accounts = funded_accounts([creator], 10_000_000_000);
        for row in &rows {
            accounts.insert(*row, Account::new(0, 0, &system_program::id()));
        }
        let context = context(accounts);
        let mut builder = ProgramBuilder::new();
        let row = builder.row_account(0, None, None, 0);
        builder.batch(120, 0);
        builder.for_each(0, |body| {
            let key = body.account_key(row);
            let same = body.binary(OP_EQ, key, key);
            body.require(same);
        });
        let payload = builder.build().expect("builds");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 83, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 83);
        let metas: Vec<AccountMeta> = rows
            .iter()
            .map(|row| AccountMeta::new_readonly(*row, false))
            .collect();
        let full = context.process_instruction(&run_instruction(template, metas.clone(), &[]));
        assert!(full.program_result.is_ok(), "{full:#?}");
        let mut one_more = metas;
        one_more.push(AccountMeta::new_readonly(creator, false));
        let rejected = context.process_instruction(&run_instruction(template, one_more, &[]));
        assert_eq!(custom_code(&rejected), Some((121 << 16) | 6010), "{rejected:#?}");
    }

    /// Each batch row carries its own input values, so one template pays a different amount to
    /// each recipient without staging the amounts in an account.
    #[test]
    fn row_inputs_pay_a_different_amount_per_recipient() {
        let creator = Pubkey::new_unique();
        let treasury = Pubkey::new_unique();
        let recipients: Vec<Pubkey> = (0..3).map(|_| Pubkey::new_unique()).collect();
        let mut accounts = funded_accounts([creator, treasury], 10_000_000_000);
        for recipient in &recipients {
            accounts.insert(*recipient, Account::new(1_000_000, 0, &system_program::id()));
        }
        let context = context(accounts);

        let mut builder = ProgramBuilder::new();
        let system = builder.account(ACCOUNT_EXECUTABLE, Some([0; 32]), None, 0);
        let from = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        let recipient = builder.row_account(ACCOUNT_WRITABLE, None, None, 0);
        builder.batch(3, 1);
        let amount_input = builder.row_input(VALUE_U64, 0);
        let discriminator = builder.blob(&[2, 0, 0, 0]);
        builder.for_each(0, |body| {
            let amount = body.load_input(amount_input);
            let transfer = body.cpi(
                system,
                &[
                    (from, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
                    (recipient, ACCOUNT_WRITABLE),
                ],
                &[
                    Segment::Literal(discriminator),
                    Segment::Register(DATA_REG_U64, amount),
                ],
            );
            body.invoke(transfer, None);
        });
        let payload = builder.build().expect("builds");
        ProgramView::parse(&payload)
            .and_then(|program| program.verify())
            .expect("verifies");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 60, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 60);

        let mut metas = vec![
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new(treasury, true),
        ];
        metas.extend(recipients.iter().map(|recipient| AccountMeta::new(*recipient, false)));
        let amounts = [1_000u64, 2_000, 3_000];
        let mut inputs = Vec::new();
        for amount in amounts {
            inputs.extend_from_slice(&amount.to_le_bytes());
        }
        let before = lamports(&context, treasury);
        let run = context.process_instruction(&run_instruction(template, metas.clone(), &inputs));
        assert!(run.program_result.is_ok(), "{run:#?}");
        for (recipient, amount) in recipients.iter().zip(amounts) {
            assert_eq!(lamports(&context, *recipient), 1_000_000 + amount);
        }
        assert_eq!(lamports(&context, treasury), before - 6_000);
        eprintln!(
            "row-input payroll compute units: {}",
            run.compute_units_consumed
        );

        // Two rows of values for three rows of accounts: the third value (index 2) is missing.
        let short = context.process_instruction(&run_instruction(template, metas, &inputs[..16]));
        assert_eq!(custom_code(&short), Some((2 << 16) | 6008), "{short:#?}");
    }

    /// A batch reuses the invocation it built for the previous row only when nothing in the body
    /// can change it. Here the amount is derived from the loop index, so every row must send
    /// different bytes even though the account list and the program never change.
    #[test]
    fn invocation_data_derived_in_the_loop_is_rebuilt_every_row() {
        let creator = Pubkey::new_unique();
        let treasury = Pubkey::new_unique();
        let recipients: Vec<Pubkey> = (0..4).map(|_| Pubkey::new_unique()).collect();
        let mut accounts = funded_accounts([creator, treasury], 10_000_000_000);
        for recipient in &recipients {
            accounts.insert(*recipient, Account::new(1_000_000, 0, &system_program::id()));
        }
        let context = context(accounts);

        let mut builder = ProgramBuilder::new();
        let system = builder.account(ACCOUNT_EXECUTABLE, Some([0; 32]), None, 0);
        let from = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        let recipient = builder.row_account(ACCOUNT_WRITABLE, None, None, 0);
        builder.batch(4, 1);
        let base = builder.const_u64(1_000);
        let discriminator = builder.blob(&[2, 0, 0, 0]);
        builder.for_each(0, |body| {
            let index = body.op(ballista_common::template::OP_LOOP_INDEX, NO_INDEX, NO_INDEX, NO_INDEX, 0);
            let amount = body.binary(OP_ADD, base, index);
            let transfer = body.cpi(
                system,
                &[
                    (from, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
                    (recipient, ACCOUNT_WRITABLE),
                ],
                &[
                    Segment::Literal(discriminator),
                    Segment::Register(DATA_REG_U64, amount),
                ],
            );
            body.invoke(transfer, None);
        });
        let payload = builder.build().expect("builds");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 95, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 95);

        let mut metas = vec![
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new(treasury, true),
        ];
        metas.extend(recipients.iter().map(|key| AccountMeta::new(*key, false)));
        let run = context.process_instruction(&run_instruction(template, metas, &[]));
        assert!(run.program_result.is_ok(), "{run:#?}");
        for (index, key) in recipients.iter().enumerate() {
            assert_eq!(
                lamports(&context, *key),
                1_000_000 + 1_000 + index as u64,
                "row {index} received the previous row's amount"
            );
        }
    }

    /// The waterfall pays creditors in order until the money runs out: each row's payment is
    /// capped by what the rows before it left, and the rows past the money are skipped rather
    /// than failing the run. No instruction sequence expresses this, because every amount
    /// depends on a balance that only exists during execution.
    #[test]
    fn waterfall_pays_in_order_and_stops_when_the_money_runs_out() {
        let creator = Pubkey::new_unique();
        let treasury = Pubkey::new_unique();
        let creditors: Vec<Pubkey> = (0..4).map(|_| Pubkey::new_unique()).collect();
        let mut accounts = funded_accounts([creator], 10_000_000_000);
        accounts.insert(treasury, Account::new(5_000_000_000, 0, &system_program::id()));
        for creditor in &creditors {
            accounts.insert(*creditor, Account::new(1_000_000, 0, &system_program::id()));
        }
        let context = context(accounts);

        let payload = fixture("waterfall-payout");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 96, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 96);

        // Two and a half claims' worth of money against four claims of 1,000 each.
        let reserve = 5_000_000_000u64 - 2_500;
        let mut inputs = reserve.to_le_bytes().to_vec();
        for _ in 0..creditors.len() {
            inputs.extend_from_slice(&1_000u64.to_le_bytes());
        }
        let mut metas = vec![
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new(treasury, true),
        ];
        metas.extend(creditors.iter().map(|key| AccountMeta::new(*key, false)));
        let run = context.process_instruction(&run_instruction(template, metas, &inputs));
        assert!(run.program_result.is_ok(), "{run:#?}");

        // Paid in full, in full, the remainder, then nothing.
        let paid: Vec<u64> = creditors
            .iter()
            .map(|key| lamports(&context, *key) - 1_000_000)
            .collect();
        assert_eq!(paid, vec![1_000, 1_000, 500, 0], "waterfall order");
        assert_eq!(lamports(&context, treasury), 5_000_000_000 - 2_500);
    }

    /// A CPI that names an account group receives the group's accounts after its declared ones.
    /// The System Program ignores accounts past the two a transfer reads, which makes it a
    /// convenient callee for observing the forwarding.
    #[test]
    fn account_groups_are_forwarded_after_declared_accounts() {
        let creator = Pubkey::new_unique();
        let runner = Pubkey::new_unique();
        let recipient = Pubkey::new_unique();
        let extras: Vec<Pubkey> = (0..2).map(|_| Pubkey::new_unique()).collect();
        let mut accounts = funded_accounts([creator, runner], 10_000_000_000);
        accounts.insert(recipient, Account::new(1_000_000, 0, &system_program::id()));
        for extra in &extras {
            accounts.insert(*extra, Account::new(0, 0, &system_program::id()));
        }
        let context = context(accounts);

        let mut builder = ProgramBuilder::new();
        builder.account_groups(1);
        let system = builder.account(ACCOUNT_EXECUTABLE, Some([0; 32]), None, 0);
        let from = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        let to = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        let amount_input = builder.input(VALUE_U64, 0);
        let amount = builder.load_input(amount_input);
        let discriminator = builder.blob(&[2, 0, 0, 0]);
        let transfer = builder.cpi_with_group(
            system,
            &[
                (from, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
                (to, ACCOUNT_WRITABLE),
            ],
            &[
                Segment::Literal(discriminator),
                Segment::Register(DATA_REG_U64, amount),
            ],
            0,
        );
        builder.invoke(transfer, None);
        let payload = builder.build().expect("builds");
        ProgramView::parse(&payload)
            .and_then(|program| program.verify())
            .expect("verifies");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 61, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 61);

        let declared = vec![
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new(runner, true),
            AccountMeta::new(recipient, false),
        ];
        let mut with_group = declared.clone();
        with_group.extend(extras.iter().map(|extra| AccountMeta::new_readonly(*extra, false)));
        let mut inputs = vec![2u8];
        inputs.extend_from_slice(&5_000u64.to_le_bytes());
        let run = context.process_instruction(&run_instruction(template, with_group, &inputs));
        assert!(run.program_result.is_ok(), "{run:#?}");
        assert_eq!(lamports(&context, recipient), 1_005_000);

        // An empty group is a valid run.
        let mut empty = vec![0u8];
        empty.extend_from_slice(&5_000u64.to_le_bytes());
        let run = context.process_instruction(&run_instruction(template, declared.clone(), &empty));
        assert!(run.program_result.is_ok(), "{run:#?}");

        // Claiming a member that was not supplied fails the account layout (three accounts seen).
        let mut claims_one = vec![1u8];
        claims_one.extend_from_slice(&5_000u64.to_le_bytes());
        let rejected =
            context.process_instruction(&run_instruction(template, declared.clone(), &claims_one));
        assert_eq!(custom_code(&rejected), Some((3 << 16) | 6010), "{rejected:#?}");

        // Run data without the prefix is malformed input.
        let missing = context.process_instruction(&run_instruction(template, declared, &[]));
        assert_eq!(custom_code(&missing), Some(6008), "{missing:#?}");
    }

    /// Batch rows and account groups share the runtime account list: rows come first, the groups
    /// after them, and the prefix tells them apart.
    #[test]
    fn account_groups_and_batch_rows_coexist() {
        let creator = Pubkey::new_unique();
        let runner = Pubkey::new_unique();
        let recipients: Vec<Pubkey> = (0..2).map(|_| Pubkey::new_unique()).collect();
        let extra = Pubkey::new_unique();
        let mut accounts = funded_accounts([creator, runner], 10_000_000_000);
        for recipient in &recipients {
            accounts.insert(*recipient, Account::new(1_000_000, 0, &system_program::id()));
        }
        accounts.insert(extra, Account::new(0, 0, &system_program::id()));
        let context = context(accounts);

        let mut builder = ProgramBuilder::new();
        builder.account_groups(1);
        let system = builder.account(ACCOUNT_EXECUTABLE, Some([0; 32]), None, 0);
        let from = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        let recipient = builder.row_account(ACCOUNT_WRITABLE, None, None, 0);
        builder.batch(4, 1);
        let amount_input = builder.input(VALUE_U64, 0);
        let amount = builder.load_input(amount_input);
        let discriminator = builder.blob(&[2, 0, 0, 0]);
        builder.for_each(0, |body| {
            let transfer = body.cpi_with_group(
                system,
                &[
                    (from, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
                    (recipient, ACCOUNT_WRITABLE),
                ],
                &[
                    Segment::Literal(discriminator),
                    Segment::Register(DATA_REG_U64, amount),
                ],
                0,
            );
            body.invoke(transfer, None);
        });
        let payload = builder.build().expect("builds");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 62, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 62);

        let mut metas = vec![
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new(runner, true),
        ];
        metas.extend(recipients.iter().map(|recipient| AccountMeta::new(*recipient, false)));
        metas.push(AccountMeta::new_readonly(extra, false));
        let mut inputs = vec![1u8];
        inputs.extend_from_slice(&7_000u64.to_le_bytes());
        let run = context.process_instruction(&run_instruction(template, metas, &inputs));
        assert!(run.program_result.is_ok(), "{run:#?}");
        for recipient in &recipients {
            assert_eq!(lamports(&context, *recipient), 1_007_000);
        }
    }

    /// Group accounts are forwarded without signer status even when the transaction signed for
    /// them, so a transfer whose payer arrives through a group is refused by the callee.
    #[test]
    fn account_group_members_never_sign() {
        let creator = Pubkey::new_unique();
        let runner = Pubkey::new_unique();
        let recipient = Pubkey::new_unique();
        let mut accounts = funded_accounts([creator, runner], 10_000_000_000);
        accounts.insert(recipient, Account::new(1_000_000, 0, &system_program::id()));
        let context = context(accounts);

        let mut builder = ProgramBuilder::new();
        builder.account_groups(1);
        let system = builder.account(ACCOUNT_EXECUTABLE, Some([0; 32]), None, 0);
        let amount_input = builder.input(VALUE_U64, 0);
        let amount = builder.load_input(amount_input);
        let discriminator = builder.blob(&[2, 0, 0, 0]);
        let transfer = builder.cpi_with_group(
            system,
            &[],
            &[
                Segment::Literal(discriminator),
                Segment::Register(DATA_REG_U64, amount),
            ],
            0,
        );
        builder.invoke(transfer, None);
        let payload = builder.build().expect("builds");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 63, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 63);

        let metas = vec![
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new(runner, true),
            AccountMeta::new(recipient, false),
        ];
        let mut inputs = vec![2u8];
        inputs.extend_from_slice(&5_000u64.to_le_bytes());
        let before = lamports(&context, runner);
        let result = context.process_instruction(&run_instruction(template, metas, &inputs));
        assert!(result.program_result.is_err(), "{result:#?}");
        assert_eq!(custom_code(&result), None, "the callee, not Ballista, refused: {result:#?}");
        assert_eq!(lamports(&context, runner), before);
    }

    /// Declared accounts plus the forwarded group must fit the 64-account CPI limit.
    #[test]
    fn cpi_account_limit_counts_the_group() {
        let creator = Pubkey::new_unique();
        let runner = Pubkey::new_unique();
        let recipient = Pubkey::new_unique();
        let extras: Vec<Pubkey> = (0..63).map(|_| Pubkey::new_unique()).collect();
        let mut accounts = funded_accounts([creator, runner], 10_000_000_000);
        accounts.insert(recipient, Account::new(1_000_000, 0, &system_program::id()));
        for extra in &extras {
            accounts.insert(*extra, Account::new(0, 0, &system_program::id()));
        }
        let context = context(accounts);

        let mut builder = ProgramBuilder::new();
        builder.account_groups(1);
        let system = builder.account(ACCOUNT_EXECUTABLE, Some([0; 32]), None, 0);
        let from = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        let to = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        let amount_input = builder.input(VALUE_U64, 0);
        let amount = builder.load_input(amount_input);
        let discriminator = builder.blob(&[2, 0, 0, 0]);
        let transfer = builder.cpi_with_group(
            system,
            &[
                (from, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
                (to, ACCOUNT_WRITABLE),
            ],
            &[
                Segment::Literal(discriminator),
                Segment::Register(DATA_REG_U64, amount),
            ],
            0,
        );
        builder.invoke(transfer, None);
        let payload = builder.build().expect("builds");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 64, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 64);

        let mut metas = vec![
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new(runner, true),
            AccountMeta::new(recipient, false),
        ];
        metas.extend(extras.iter().map(|extra| AccountMeta::new_readonly(*extra, false)));
        let mut inputs = vec![63u8];
        inputs.extend_from_slice(&5_000u64.to_le_bytes());
        let result = context.process_instruction(&run_instruction(template, metas, &inputs));
        // Instruction 1 is the invoke; 6021 is CpiAccountLimitExceeded with the total as context.
        assert_eq!(custom_code(&result), Some((65 << 16) | 6021), "{result:#?}");
    }

    /// The TypeScript compiler's row-input and account-group fixtures run against the program.
    #[test]
    fn row_input_and_account_group_fixtures_run_end_to_end() {
        let creator = Pubkey::new_unique();
        let treasury = Pubkey::new_unique();
        let recipients: Vec<Pubkey> = (0..3).map(|_| Pubkey::new_unique()).collect();
        let extras: Vec<Pubkey> = (0..2).map(|_| Pubkey::new_unique()).collect();
        let mut accounts = funded_accounts([creator, treasury], 10_000_000_000);
        for recipient in &recipients {
            accounts.insert(*recipient, Account::new(1_000_000, 0, &system_program::id()));
        }
        for extra in &extras {
            accounts.insert(*extra, Account::new(0, 0, &system_program::id()));
        }
        let context = context(accounts);

        // payroll-row-amounts: each row's amount travels in the run data.
        let payload = fixture("payroll-row-amounts");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 71, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 71);
        let mut metas = vec![
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new(treasury, true),
        ];
        metas.extend(recipients.iter().map(|recipient| AccountMeta::new(*recipient, false)));
        let mut inputs = Vec::new();
        for amount in [100u64, 200, 300] {
            inputs.extend_from_slice(&amount.to_le_bytes());
        }
        let run = context.process_instruction(&run_instruction(template, metas, &inputs));
        assert!(run.program_result.is_ok(), "{run:#?}");
        for (recipient, amount) in recipients.iter().zip([100u64, 200, 300]) {
            assert_eq!(lamports(&context, *recipient), 1_000_000 + amount);
        }

        // group-forward-transfer: two extra accounts ride along behind the declared ones.
        let payload = fixture("group-forward-transfer");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 72, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 72);
        let mut metas = vec![
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new(treasury, true),
            AccountMeta::new(recipients[0], false),
        ];
        metas.extend(extras.iter().map(|extra| AccountMeta::new_readonly(*extra, false)));
        let mut inputs = vec![2u8];
        inputs.extend_from_slice(&50u64.to_le_bytes());
        let run = context.process_instruction(&run_instruction(template, metas, &inputs));
        assert!(run.program_result.is_ok(), "{run:#?}");
        assert_eq!(lamports(&context, recipients[0]), 1_000_150);
    }

    /// Account reads observe the state a CPI leaves behind, including a reallocated data length.
    #[test]
    fn data_length_reflects_reallocation_after_a_cpi() {
        let creator = Pubkey::new_unique();
        let authority = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let owner = Pubkey::new_unique();
        let (ata, _) = associated_token::create_account_for_associated_token_account(
            token_account_state(mint, owner, 0),
        );
        let mut accounts = funded_accounts([creator, payer, owner], 10_000_000_000);
        accounts.insert(
            mint,
            token::create_account_for_mint(Mint {
                mint_authority: COption::Some(authority),
                supply: 0,
                decimals: 6,
                is_initialized: true,
                freeze_authority: COption::None,
            }),
        );
        let context = context(accounts);

        let mut builder = ProgramBuilder::new();
        let ata_program = builder.account(ACCOUNT_EXECUTABLE, Some(associated_token::ID.to_bytes()), None, 0);
        let token_program = builder.account(ACCOUNT_EXECUTABLE, Some(token::ID.to_bytes()), None, 0);
        let system = builder.account(ACCOUNT_EXECUTABLE, Some(system_program::id().to_bytes()), None, 0);
        let payer_account = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        let mint_account = builder.account(0, None, Some(token::ID.to_bytes()), 82);
        let owner_account = builder.account(0, None, None, 0);
        let ata_account = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        let before = builder.account_data_len(ata_account);
        let zero = builder.const_u64(0);
        let was_empty = builder.binary(OP_EQ, before, zero);
        builder.require(was_empty);
        let cpi = builder.cpi(
            ata_program,
            &[
                (payer_account, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
                (ata_account, ACCOUNT_WRITABLE),
                (owner_account, 0),
                (mint_account, 0),
                (system, 0),
                (token_program, 0),
            ],
            &[],
        );
        builder.invoke(cpi, None);
        let after = builder.account_data_len(ata_account);
        let expected = builder.const_u64(165);
        let grew = builder.binary(OP_EQ, after, expected);
        builder.require(grew);
        let slot = builder.clock_slot();
        let timestamp = builder.clock_timestamp();
        let epoch_start = builder.const_i64(0);
        let sane_time = builder.binary(ballista_common::template::OP_GTE, timestamp, epoch_start);
        builder.require(sane_time);
        let some_slot = builder.binary(ballista_common::template::OP_GTE, slot, zero);
        builder.require(some_slot);
        let payload = builder.build().expect("builds");
        ProgramView::parse(&payload)
            .and_then(|program| program.verify())
            .expect("verifies");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 84, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 84);
        let result = context.process_instruction(&run_instruction(
            template,
            vec![
                AccountMeta::new_readonly(associated_token::ID, false),
                AccountMeta::new_readonly(token::ID, false),
                AccountMeta::new_readonly(system_program::id(), false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(mint, false),
                AccountMeta::new_readonly(owner, false),
                AccountMeta::new(ata, false),
            ],
            &[],
        ));
        assert!(result.program_result.is_ok(), "{result:#?}");
        assert_eq!(context.account_store.borrow()[&ata].data().len(), 165);
    }

    /// The math opcodes as the TypeScript SDK compiles them, run on chain. Each requirement in the
    /// fixture compares one result with a constant worked out by hand, so a run that succeeds has
    /// computed all of them exactly; the failing runs show each failure is the documented one.
    #[test]
    fn typescript_math_fixture_computes_exact_results() {
        let creator = Pubkey::new_unique();
        let authority = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        let feed = Pubkey::new_unique();
        let mut accounts = funded_accounts([creator], 10_000_000_000);
        accounts.insert(
            feed,
            token::create_account_for_token_account(token_account_state(mint, authority, 0xffff_fff8)),
        );
        let context = context(accounts);
        let payload = fixture("math-ops");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 90, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 90);
        let run = |amount: u64, price: u64, divisor: u64, flags: u64, exponent: u64| {
            let mut inputs = Vec::new();
            for value in [amount, price, divisor, flags, exponent] {
                inputs.extend_from_slice(&value.to_le_bytes());
            }
            context.process_instruction(&run_instruction(
                template,
                vec![AccountMeta::new_readonly(feed, false)],
                &inputs,
            ))
        };

        let exact = run(1_000_003, 7, 3, 0xabcd, 18);
        assert!(exact.program_result.is_ok(), "{exact:#?}");
        eprintln!("math fixture compute units: {}", exact.compute_units_consumed);

        // A wrong power of ten fails its requirement, not the arithmetic.
        let wrong = run(1_000_003, 7, 3, 0xabcd, 17);
        assert_eq!(decode_kind(&wrong), Some(6015));
        // Division by zero and a power of ten past 10^38 fail as themselves.
        let zero = run(1_000_003, 7, 0, 0xabcd, 18);
        assert_eq!(decode_kind(&zero), Some(6014));
        let huge = run(1_000_003, 7, 3, 0xabcd, 39);
        assert_eq!(decode_kind(&huge), Some(6013));
    }

    /// The introspection opcodes as the TypeScript SDK compiles them, run as the middle of three
    /// instructions, between two memos, against the Instructions sysvar Mollusk builds from the
    /// whole transaction.
    #[test]
    fn typescript_introspection_fixture_reads_the_transaction() {
        let creator = Pubkey::new_unique();
        let signer = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        let mut accounts = funded_accounts([creator, signer], 10_000_000_000);
        accounts.insert(
            mint,
            token::create_account_for_mint(Mint {
                mint_authority: COption::None,
                supply: 0,
                decimals: 6,
                is_initialized: true,
                freeze_authority: COption::None,
            }),
        );
        let context = context(accounts);
        let payload = fixture("introspection");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 91, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 91);
        let before = Instruction {
            program_id: memo::ID,
            accounts: vec![AccountMeta::new_readonly(signer, true)],
            data: b"before".to_vec(),
        };
        let after = Instruction {
            program_id: memo::ID,
            accounts: vec![],
            data: b"after".to_vec(),
        };
        let run = |neighbour: u64, position: u64, data_offset: u64, writable_mint: bool| {
            let mut inputs = Vec::new();
            for value in [neighbour, position, data_offset] {
                inputs.extend_from_slice(&value.to_le_bytes());
            }
            let mint_meta = if writable_mint {
                AccountMeta::new(mint, false)
            } else {
                AccountMeta::new_readonly(mint, false)
            };
            let ballista = run_instruction(
                template,
                vec![
                    AccountMeta::new_readonly(sysvar::instructions::id(), false),
                    AccountMeta::new_readonly(signer, true),
                    mint_meta,
                ],
                &inputs,
            );
            context.process_transaction_instructions(&[before.clone(), ballista, after.clone()])
        };

        let read = run(0, 0, 44, false);
        assert!(read.program_result.is_ok(), "{read:#?}");
        let memos = context.process_transaction_instructions(&[before.clone(), after.clone()]);
        eprintln!(
            "introspection fixture compute units: {}",
            read.compute_units_consumed - memos.compute_units_consumed
        );

        // Every failure is the run's, instruction 1, at the step whose read is out of range.
        let failure = |label: &str, kind: u32| Some((1, kind, label.to_owned()));
        assert_eq!(
            fixture_failure(&run(3, 0, 44, false), "introspection"),
            failure("neighbourIsMemo", 6023),
            "there is no fourth instruction"
        );
        assert_eq!(
            fixture_failure(&run(0, 1, 44, false), "introspection"),
            failure("memoNamesItsSigner", 6023),
            "the memo names one account"
        );
        assert_eq!(
            fixture_failure(&run(0, 0, 82, false), "introspection"),
            failure("mintHasSixDecimals", 6023),
            "a mint is 82 bytes"
        );
        assert_eq!(
            fixture_failure(&run(0, 0, 44, true), "introspection"),
            failure("mintHasSixDecimals", 6024),
            "a writable account's bytes are not lent"
        );
        // The last memo exists but names no accounts, so the checks on it fail as requirements.
        assert_eq!(
            fixture_failure(&run(2, 0, 44, false), "introspection"),
            failure("oneAccount", 6015)
        );
    }

    /// A deterministic Ed25519 key pair and its address.
    fn ed25519_keypair(seed: u8) -> (ed25519_dalek::Keypair, Pubkey) {
        let secret = ed25519_dalek::SecretKey::from_bytes(&[seed; 32]).expect("a 32-byte secret");
        let public = ed25519_dalek::PublicKey::from(&secret);
        let address = Pubkey::new_from_array(public.to_bytes());
        (ed25519_dalek::Keypair { secret, public }, address)
    }

    /// An Ed25519 precompile instruction with `count` copies of one signature over `message`,
    /// laid out as `new_ed25519_instruction_with_signature` lays out one: the offsets, then the
    /// key, the signature and the message. Every offset names `index` as the instruction holding
    /// its bytes; `u16::MAX` is the precompile instruction itself.
    fn ed25519_instruction(
        keypair: &ed25519_dalek::Keypair,
        message: &[u8],
        count: u8,
        index: u16,
    ) -> Instruction {
        use ed25519_dalek::Signer;
        let signature = keypair.sign(message).to_bytes();
        let key_offset = 2 + 14 * count as u16;
        let signature_offset = key_offset + 32;
        let message_offset = signature_offset + 64;
        let mut data = vec![count, 0];
        for _ in 0..count {
            for field in [
                signature_offset,
                index,
                key_offset,
                index,
                message_offset,
                message.len() as u16,
                index,
            ] {
                data.extend_from_slice(&field.to_le_bytes());
            }
        }
        data.extend_from_slice(&keypair.public.to_bytes());
        data.extend_from_slice(&signature);
        data.extend_from_slice(message);
        Instruction {
            program_id: solana_sdk_ids::ed25519_program::id(),
            accounts: vec![],
            data,
        }
    }

    /// The 128-byte quote `signed-quote-settlement.ts` reads, after its tag.
    fn quote_message(
        price: u64,
        max_amount: u64,
        expiry: i64,
        taker: &Pubkey,
        base_mint: &Pubkey,
        quote_mint: &Pubkey,
    ) -> Vec<u8> {
        let mut message = Vec::with_capacity(128);
        message.extend_from_slice(b"BLSTQT01");
        message.extend_from_slice(&price.to_le_bytes());
        message.extend_from_slice(&max_amount.to_le_bytes());
        message.extend_from_slice(&expiry.to_le_bytes());
        for key in [taker, base_mint, quote_mint] {
            message.extend_from_slice(key.as_ref());
        }
        message
    }

    /// The signed-quote example as the TypeScript SDK compiles it, settling a quote whose
    /// signature the real Ed25519 precompile verifies. A tampered signature or message fails in the
    /// precompile, before Ballista runs; a valid one reaches the template, which binds it to the
    /// maker, the tag, the taker, the size, the expiry and the mints.
    #[test]
    fn signed_quote_settles_only_as_the_maker_signed() {
        let creator = Pubkey::new_unique();
        let (maker_key, maker) = ed25519_keypair(7);
        let (stranger_key, _) = ed25519_keypair(8);
        let taker = Pubkey::new_unique();
        let other_taker = Pubkey::new_unique();
        let (base_mint, quote_mint) = (Pubkey::new_unique(), Pubkey::new_unique());
        let taker_quote = Pubkey::new_unique();
        let maker_quote = Pubkey::new_unique();
        let maker_base = Pubkey::new_unique();
        let taker_base = Pubkey::new_unique();
        let diverted = Pubkey::new_unique();
        let mut accounts = funded_accounts([creator, maker, taker, other_taker], 10_000_000_000);
        for mint in [base_mint, quote_mint] {
            accounts.insert(
                mint,
                token::create_account_for_mint(Mint {
                    mint_authority: COption::None,
                    supply: 1_000_000_000_000,
                    decimals: 6,
                    is_initialized: true,
                    freeze_authority: COption::None,
                }),
            );
        }
        for (address, mint, owner, amount) in [
            (taker_quote, quote_mint, taker, 100_000_000),
            (maker_quote, quote_mint, maker, 0),
            (maker_base, base_mint, maker, 10_000_000),
            (taker_base, base_mint, taker, 0),
            (diverted, quote_mint, taker, 0),
        ] {
            accounts.insert(
                address,
                token::create_account_for_token_account(token_account_state(mint, owner, amount)),
            );
        }
        let mut context = context(accounts);
        context.mollusk.sysvars.clock.unix_timestamp = 1_800_000_000;
        let payload = fixture("signed-quote-settlement");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 92, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 92);
        let settle = |instructions: Vec<Instruction>, taker: Pubkey, payee: Pubkey, amount: u64| {
            let run = run_instruction(
                template,
                vec![
                    AccountMeta::new_readonly(sysvar::instructions::id(), false),
                    AccountMeta::new_readonly(token::ID, false),
                    AccountMeta::new_readonly(taker, true),
                    AccountMeta::new_readonly(maker, true),
                    AccountMeta::new(taker_quote, false),
                    AccountMeta::new(payee, false),
                    AccountMeta::new(maker_base, false),
                    AccountMeta::new(taker_base, false),
                ],
                &amount.to_le_bytes(),
            );
            let mut transaction = instructions;
            transaction.push(run);
            context.process_transaction_instructions(&transaction)
        };
        let expiry = 1_800_000_060;
        // 2.5 quote units per base unit, at most 4 base units.
        let message = quote_message(2_500_000, 4_000_000, expiry, &taker, &base_mint, &quote_mint);
        let quote = ed25519_instruction(&maker_key, &message, 1, u16::MAX);

        // A tampered signature, or one changed byte of the message it signs (the price), fails
        // in the precompile, instruction 0, with `InvalidSignature`; Ballista never runs.
        const SIGNATURE: usize = 2 + 14 + 32;
        const MESSAGE: usize = SIGNATURE + 64;
        let invalid_signature = Some((0, 2));
        let mut tampered = quote.clone();
        tampered.data[SIGNATURE] ^= 1;
        let rejected = settle(vec![tampered], taker, maker_quote, 3_000_001);
        assert_eq!(transaction_code(&rejected), invalid_signature, "{rejected:#?}");
        let mut repriced = quote.clone();
        repriced.data[MESSAGE + 8] ^= 1;
        let rejected = settle(vec![repriced], taker, maker_quote, 3_000_001);
        assert_eq!(transaction_code(&rejected), invalid_signature, "{rejected:#?}");

        // With no precompile at all the run is instruction 0, and the index of the one before it
        // underflows.
        let alone = settle(vec![], taker, maker_quote, 1);
        assert_eq!(
            fixture_failure(&alone, "signed-quote-settlement"),
            Some((0, 6013, "quoteInstructionIndex".to_owned())),
            "{alone:#?}"
        );

        // Each broken binding fails the run, instruction 1, at its own requirement.
        let failure = |label: &str| Some((1, 6015, label.to_owned()));
        let refused = |instructions: Vec<Instruction>, taker: Pubkey, payee: Pubkey, amount: u64| {
            fixture_failure(&settle(instructions, taker, payee, amount), "signed-quote-settlement")
        };
        let by_stranger = ed25519_instruction(&stranger_key, &message, 1, u16::MAX);
        assert_eq!(
            refused(vec![by_stranger], taker, maker_quote, 1),
            failure("quoteIsBySigner")
        );
        let twice = ed25519_instruction(&maker_key, &message, 2, u16::MAX);
        assert_eq!(
            refused(vec![twice], taker, maker_quote, 1),
            failure("quoteIsOneSelfContainedSignature")
        );
        // The signature's, the key's and the message's instruction index, each set to 0 on its
        // own. Index 0 is the precompile instruction here too, so it verifies; the template still
        // wants the explicit `u16::MAX` in all three.
        for field in [4, 8, 14] {
            let mut by_index = quote.clone();
            by_index.data[field..field + 2].copy_from_slice(&0u16.to_le_bytes());
            assert_eq!(
                refused(vec![by_index], taker, maker_quote, 1),
                failure("quoteIsOneSelfContainedSignature"),
                "the index at byte {field}"
            );
        }
        let short = ed25519_instruction(&maker_key, &message[..message.len() - 1], 1, u16::MAX);
        assert_eq!(
            refused(vec![short], taker, maker_quote, 1),
            failure("quoteIsOneSelfContainedSignature")
        );
        // A message of the quote's shape signed for something else, under another tag.
        let mut untagged = message.clone();
        untagged[..8].copy_from_slice(b"BLSTQT02");
        let untagged = ed25519_instruction(&maker_key, &untagged, 1, u16::MAX);
        assert_eq!(
            refused(vec![untagged], taker, maker_quote, 1),
            failure("quoteIsTagged")
        );
        // With a memo in between, the run is instruction 2 and the one before it is the memo.
        let memo = Instruction {
            program_id: memo::ID,
            accounts: vec![],
            data: b"between".to_vec(),
        };
        assert_eq!(
            refused(vec![quote.clone(), memo], taker, maker_quote, 1),
            Some((2, 6015, "quoteIsEd25519".to_owned()))
        );
        let stale =
            quote_message(2_500_000, 4_000_000, 1_799_999_999, &taker, &base_mint, &quote_mint);
        let stale = ed25519_instruction(&maker_key, &stale, 1, u16::MAX);
        assert_eq!(
            refused(vec![stale], taker, maker_quote, 1),
            failure("quoteHasNotExpired")
        );
        assert_eq!(
            refused(vec![quote.clone()], other_taker, maker_quote, 1),
            failure("quoteIsForThisTaker")
        );
        assert_eq!(
            refused(vec![quote.clone()], taker, maker_quote, 4_000_001),
            failure("withinTheQuotedSize")
        );
        let other_market =
            quote_message(2_500_000, 4_000_000, expiry, &taker, &base_mint, &base_mint);
        let other_market = ed25519_instruction(&maker_key, &other_market, 1, u16::MAX);
        assert_eq!(
            refused(vec![other_market], taker, maker_quote, 1),
            failure("paysInTheQuotedMint")
        );
        assert_eq!(
            refused(vec![quote.clone()], taker, diverted, 1),
            failure("paymentReachesTheMaker")
        );

        // The real thing: 3.000001 base units at 2.5, rounded up in the maker's favour.
        let settled = settle(vec![quote], taker, maker_quote, 3_000_001);
        assert!(settled.program_result.is_ok(), "{settled:#?}");
        eprintln!("signed-quote settlement compute units: {}", settled.compute_units_consumed);
        assert_eq!(token_amount(&context, taker_quote), 100_000_000 - 7_500_003);
        assert_eq!(token_amount(&context, maker_quote), 7_500_003);
        assert_eq!(token_amount(&context, maker_base), 10_000_000 - 3_000_001);
        assert_eq!(token_amount(&context, taker_base), 3_000_001);
    }

    /// `READ_I32` runs in `extended_instruction`, not the dispatch loop's read arm, so its dynamic
    /// offsets are checked there. A read in range sign-extends, up to the last four bytes. One that
    /// would end past the data fails at the read, and so does an offset of `u64::MAX`, which cannot
    /// even have the width added to it.
    #[test]
    fn i32_reads_at_dynamic_offsets_sign_extend_or_fail_at_the_read() {
        let creator = Pubkey::new_unique();
        let feed = Pubkey::new_unique();
        let mut accounts = funded_accounts([creator], 10_000_000_000);
        let mut feed_account = Account::new(1_000_000, 128, &system_program::id());
        feed_account.data[..4].copy_from_slice(&42i32.to_le_bytes());
        feed_account.data[124..].copy_from_slice(&i32::MIN.to_le_bytes());
        accounts.insert(feed, feed_account);
        let context = context(accounts);

        let mut builder = ProgramBuilder::new();
        let source = builder.account(0, None, None, 0);
        let offset_input = builder.input(VALUE_U64, 0);
        let expected_input = builder.input(VALUE_I64, 0);
        let offset = builder.load_input(offset_input);
        let expected = builder.load_input(expected_input);
        let value = builder.read_dynamic(OP_READ_I32, source, offset);
        let matches = builder.binary(OP_EQ, value, expected);
        builder.require(matches);
        let payload = builder.build().expect("builds");
        ProgramView::parse(&payload)
            .and_then(|program| program.verify())
            .expect("verifies");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 91, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 91);
        let metas = vec![AccountMeta::new_readonly(feed, false)];
        let inputs = |offset: u64, expected: i64| {
            let mut bytes = offset.to_le_bytes().to_vec();
            bytes.extend_from_slice(&expected.to_le_bytes());
            bytes
        };

        for (offset, expected) in [(0, 42), (124, i64::from(i32::MIN))] {
            let run = context.process_instruction(&run_instruction(
                template,
                metas.clone(),
                &inputs(offset, expected),
            ));
            assert!(run.program_result.is_ok(), "offset {offset}: {run:#?}");
        }
        // The read is instruction 2; 6009 is InvalidRuntimeAccount.
        for offset in [125, u64::MAX] {
            let run = context.process_instruction(&run_instruction(
                template,
                metas.clone(),
                &inputs(offset, 0),
            ));
            assert_eq!(
                custom_code(&run),
                Some((2 << 16) | 6009),
                "offset {offset}: {run:#?}"
            );
        }
    }

    /// Inside a FOREACH body, `READ_I32` reads each row's own account, at a fixed offset and at one
    /// a row input supplies: `extended_instruction` resolves row accounts from the loop context the
    /// dispatch loop hands it.
    #[test]
    fn i32_reads_resolve_each_rows_account_in_a_loop() {
        let creator = Pubkey::new_unique();
        let rows: Vec<Pubkey> = (0..3).map(|_| Pubkey::new_unique()).collect();
        let fixed_values = [-5i32, 7, -100];
        let dynamic_values = [1_000i32, -2_000, 3];
        let mut accounts = funded_accounts([creator], 10_000_000_000);
        for (index, row) in rows.iter().enumerate() {
            // Row `n` holds one i32 at offset 8 and another at 20 + 4n.
            let mut account = Account::new(1_000_000, 64, &system_program::id());
            account.data[8..12].copy_from_slice(&fixed_values[index].to_le_bytes());
            let offset = 20 + 4 * index;
            account.data[offset..offset + 4].copy_from_slice(&dynamic_values[index].to_le_bytes());
            accounts.insert(*row, account);
        }
        let context = context(accounts);

        let mut builder = ProgramBuilder::new();
        let row = builder.row_account(0, None, None, 64);
        builder.batch(3, 1);
        let offset_input = builder.row_input(VALUE_U64, 0);
        let total = builder.const_i64(0);
        builder.for_each(1u64 << total, |body| {
            let offset = body.load_input(offset_input);
            let fixed = body.read(OP_READ_I32, row, 8);
            let dynamic = body.read_dynamic(OP_READ_I32, row, offset);
            let both = body.binary(OP_ADD, fixed, dynamic);
            let next = body.binary(OP_ADD, total, both);
            body.mov(total, next);
        });
        let expected = builder.const_i64(-5 + 7 - 100 + 1_000 - 2_000 + 3);
        let same = builder.binary(OP_EQ, total, expected);
        builder.require(same);
        let payload = builder.build().expect("builds");
        ProgramView::parse(&payload)
            .and_then(|program| program.verify())
            .expect("verifies");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 92, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 92);
        let metas: Vec<AccountMeta> = rows
            .iter()
            .map(|row| AccountMeta::new_readonly(*row, false))
            .collect();
        let offsets = |offsets: [u64; 3]| {
            offsets
                .iter()
                .flat_map(|offset| offset.to_le_bytes())
                .collect::<Vec<u8>>()
        };

        let run = context.process_instruction(&run_instruction(
            template,
            metas.clone(),
            &offsets([20, 24, 28]),
        ));
        assert!(run.program_result.is_ok(), "{run:#?}");

        // The third row's offset runs past its 64 bytes: the dynamic read is instruction 4.
        let past =
            context.process_instruction(&run_instruction(template, metas, &offsets([20, 24, 61])));
        assert_eq!(custom_code(&past), Some((4 << 16) | 6009), "{past:#?}");
    }

    /// `RETURN_DATA` takes `READ_I32` as its width selector: four bytes at the offset,
    /// sign-extended. SPL Token's GetAccountDataSize sets 165 as an eight-byte `u64`, so offset 0
    /// reads 165, offset 4 reads its zero high half, and offset 5 would end past the data.
    #[test]
    fn return_data_reads_an_i32_at_an_offset() {
        let creator = Pubkey::new_unique();
        let authority = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        let mut accounts = funded_accounts([creator], 10_000_000_000);
        accounts.insert(
            mint,
            token::create_account_for_mint(Mint {
                mint_authority: COption::Some(authority),
                supply: 0,
                decimals: 6,
                is_initialized: true,
                freeze_authority: COption::None,
            }),
        );
        let context = context(accounts);

        for (template_id, offset, expected) in
            [(93u16, 0u64, Some(165i64)), (94, 4, Some(0)), (95, 5, None)]
        {
            let mut builder = ProgramBuilder::new();
            let token_program =
                builder.account(ACCOUNT_EXECUTABLE, Some(token::ID.to_bytes()), None, 0);
            let mint_account = builder.account(0, None, Some(token::ID.to_bytes()), 82);
            let literal = builder.blob(&[21]);
            let cpi = builder.cpi(
                token_program,
                &[(mint_account, 0)],
                &[Segment::Literal(literal)],
            );
            builder.invoke(cpi, None);
            let value = builder.return_data(OP_READ_I32, offset);
            let wanted = builder.const_i64(expected.unwrap_or(0));
            let same = builder.binary(OP_EQ, value, wanted);
            builder.require(same);
            let payload = builder.build().expect("builds");
            ProgramView::parse(&payload)
                .and_then(|program| program.verify())
                .expect("verifies");
            assert!(context
                .process_instruction(&create_template_instruction(creator, template_id, &payload))
                .program_result
                .is_ok());
            let (template, _) = find_template_pda(&creator, template_id);
            let run = context.process_instruction(&run_instruction(
                template,
                vec![
                    AccountMeta::new_readonly(token::ID, false),
                    AccountMeta::new_readonly(mint, false),
                ],
                &[],
            ));
            match expected {
                Some(_) => assert!(run.program_result.is_ok(), "offset {offset}: {run:#?}"),
                // Instruction 1 is the return-data read; 6018 is MissingReturnData, which is what a
                // read past the end of the return data reports.
                None => assert_eq!(
                    custom_code(&run),
                    Some((1 << 16) | 6018),
                    "offset {offset}: {run:#?}"
                ),
            }
        }
    }

    /// Count and row loops as the TypeScript SDK compiles them. A REPEAT pays `amount` once per
    /// round; then two FOREACH loops walk the same rows, the second checking each row against the
    /// total the first carried out of its loop.
    #[test]
    fn typescript_loops_fixture_runs_count_and_row_loops_in_sequence() {
        let creator = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let recipient = Pubkey::new_unique();
        let holders: Vec<Pubkey> = (0..5).map(|_| Pubkey::new_unique()).collect();
        let mut accounts = funded_accounts([creator, payer, recipient], 10_000_000_000);
        for (holder, lamports) in holders.iter().zip([100u64, 200, 300, 400, 301]) {
            accounts.insert(*holder, Account::new(lamports, 0, &system_program::id()));
        }
        let context = context(accounts);
        let payload = fixture("loops");
        let repeat_pc = ProgramView::parse(&payload)
            .unwrap()
            .instructions
            .iter()
            .position(|record| record.opcode == OP_REPEAT)
            .unwrap() as u32;
        assert!(context
            .process_instruction(&create_template_instruction(creator, 96, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 96);
        let run = |rounds: u64, rows: &[Pubkey]| {
            let mut metas = vec![
                AccountMeta::new_readonly(system_program::id(), false),
                AccountMeta::new(payer, true),
                AccountMeta::new(recipient, false),
            ];
            metas.extend(rows.iter().map(|row| AccountMeta::new_readonly(*row, false)));
            let mut inputs = rounds.to_le_bytes().to_vec();
            inputs.extend_from_slice(&1_000u64.to_le_bytes());
            context.process_instruction(&run_instruction(template, metas, &inputs))
        };

        // Each round pays once, none included, and the rows pass both row loops.
        for rounds in [0u64, 1, 4] {
            let before = lamports(&context, recipient);
            let result = run(rounds, &holders[..3]);
            assert!(result.program_result.is_ok(), "{rounds} rounds: {result:#?}");
            assert_eq!(lamports(&context, recipient), before + rounds * 1_000);
            if rounds == 4 {
                eprintln!("loops fixture, four rounds and three rows: {} CU", result.compute_units_consumed);
            }
        }

        // Five rounds is over the count loop's maximum of four: the run fails at the REPEAT.
        let before = lamports(&context, recipient);
        let over = run(5, &holders[..3]);
        assert_eq!(custom_code(&over), Some((repeat_pc << 16) | 6022), "{over:#?}");
        assert_eq!(lamports(&context, recipient), before);

        // A row holding more than half of the total fails the second row loop's check.
        let lopsided = run(1, &[holders[0], holders[1], holders[3]]);
        assert_eq!(decode_kind(&lopsided), Some(6015), "{lopsided:#?}");

        // The total is the rows' lamports and nothing more: twice 301 is one over 100 + 200 + 301.
        // Both accumulators start from `u64(0)`. Had they shared that constant's register, the
        // total would start from the 6 that four rounds leave in `indexSum`, and this run would
        // pass.
        let barely_over = run(4, &[holders[0], holders[1], holders[4]]);
        assert_eq!(decode_kind(&barely_over), Some(6015), "{barely_over:#?}");
    }

    /// Eight loops over sixty-four registers. The heap is a 32 KiB bump allocator that never frees;
    /// with 240 row input values and a 4 KiB invocation buffer already on it, a 2,560-byte register
    /// snapshot per loop would not fit, so the run passes only if every loop reuses one snapshot.
    #[test]
    fn eight_loops_share_one_register_snapshot_in_the_default_heap() {
        let creator = Pubkey::new_unique();
        let rows: Vec<Pubkey> = (0..30).map(|_| Pubkey::new_unique()).collect();
        let mut accounts = funded_accounts([creator], 10_000_000_000);
        for row in &rows {
            accounts.insert(*row, Account::new(1, 0, &system_program::id()));
        }
        let context = context(accounts);

        let mut builder = ProgramBuilder::new();
        let program = builder.account(
            ACCOUNT_EXECUTABLE,
            Some(system_program::id().to_bytes()),
            None,
            0,
        );
        builder.row_account(0, None, None, 0);
        builder.batch(30, 30);
        for _ in 0..MAX_ROW_INPUTS {
            builder.row_input(VALUE_BOOL, 0);
        }
        // Never invoked, but every descriptor sizes the invocation buffer: 4 KiB here.
        let padding = builder.blob(&[0; MAX_CPI_DATA_LEN]);
        builder.cpi(program, &[], &[Segment::Literal(padding)]);
        let count = builder.const_u64(1);
        builder.for_each(0, |body| {
            body.loop_index();
        });
        for _ in 1..MAX_LOOPS {
            builder.repeat(count, 1, 0, |body| {
                body.loop_index();
            });
        }
        while (builder.register_count() as usize) < MAX_REGISTERS {
            builder.register();
        }
        let payload = builder.build().expect("builds");
        ProgramView::parse(&payload)
            .and_then(|program| program.verify())
            .expect("verifies");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 97, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 97);
        let mut metas = vec![AccountMeta::new_readonly(system_program::id(), false)];
        metas.extend(rows.iter().map(|row| AccountMeta::new_readonly(*row, false)));
        let inputs = vec![0u8; rows.len() * MAX_ROW_INPUTS];
        let result = context.process_instruction(&run_instruction(template, metas, &inputs));
        assert!(result.program_result.is_ok(), "{result:#?}");
    }

    /// The output opcodes as the TypeScript SDK compiles them, run on chain. Each row logs its
    /// index and recipient right after a transfer whose data the next row sends again without
    /// encoding it, so a log that wrote the invocation's buffer would break the second transfer.
    /// The run then logs the memo after its tag and returns the total paid and the payer.
    #[test]
    fn typescript_output_fixture_logs_every_row_and_returns_the_total() {
        let creator = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let first = Pubkey::new_unique();
        let second = Pubkey::new_unique();
        let mut context = context(funded_accounts([creator, payer, first, second], 10_000_000_000));
        let payload = fixture("output");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 91, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 91);
        let logger = LogCollector::new_ref();
        context.mollusk.logger = Some(logger.clone());
        let before = (lamports(&context, first), lamports(&context, second));

        let result = context.process_instruction(&run_instruction(
            template,
            vec![
                AccountMeta::new_readonly(system_program::id(), false),
                AccountMeta::new(payer, true),
                AccountMeta::new(first, false),
                AccountMeta::new(second, false),
            ],
            &output_fixture_inputs(),
        ));
        assert!(result.program_result.is_ok(), "{result:#?}");
        assert_eq!(lamports(&context, first), before.0 + 1_000);
        assert_eq!(lamports(&context, second), before.1 + 1_000);

        let row = |index: u8, recipient: Pubkey| {
            let mut line = b"PAID".to_vec();
            line.push(index);
            line.extend_from_slice(recipient.as_ref());
            line
        };
        assert_eq!(
            program_data(&logger),
            vec![row(0, first), row(1, second), b"MEMOhello".to_vec()]
        );
        let mut returned = 2_000u64.to_le_bytes().to_vec();
        returned.extend_from_slice(payer.as_ref());
        assert_eq!(result.return_data, returned);
        eprintln!("output fixture compute units: {}", result.compute_units_consumed);
    }

    /// Return data survives the callee's return: a template that runs the output fixture through
    /// Ballista reads the total it set, directly after the invoke, then returns its own value.
    #[test]
    fn a_nested_template_reads_the_return_data_its_callee_set() {
        let creator = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let first = Pubkey::new_unique();
        let second = Pubkey::new_unique();
        let context = context(funded_accounts([creator, payer, first, second], 10_000_000_000));
        let inner_payload = fixture("output");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 93, &inner_payload))
            .program_result
            .is_ok());
        let (inner, _) = find_template_pda(&creator, 93);

        let mut builder = ProgramBuilder::new();
        let ballista = builder.account(ACCOUNT_EXECUTABLE, Some(ID.to_bytes()), None, 0);
        let template = builder.account(0, None, Some(ID.to_bytes()), 80);
        let system =
            builder.account(ACCOUNT_EXECUTABLE, Some(system_program::id().to_bytes()), None, 0);
        let source = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        let to_first = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        let to_second = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        let mut data = vec![IX_RUN];
        data.extend_from_slice(&output_fixture_inputs());
        let literal = builder.blob(&data);
        let cpi = builder.cpi(
            ballista,
            &[
                (template, 0),
                (system, 0),
                (source, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
                (to_first, ACCOUNT_WRITABLE),
                (to_second, ACCOUNT_WRITABLE),
            ],
            &[Segment::Literal(literal)],
        );
        builder.invoke(cpi, None);
        let paid = builder.return_data(OP_READ_U64, 0);
        let expected = builder.const_u64(2_000);
        let same = builder.binary(OP_EQ, paid, expected);
        builder.require(same);
        let doubled = builder.binary(OP_ADD, paid, paid);
        builder.set_return_data(&[Segment::Register(DATA_REG_U64, doubled)]);
        let outer_payload = builder.build().expect("builds");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 94, &outer_payload))
            .program_result
            .is_ok());
        let (outer, _) = find_template_pda(&creator, 94);

        let result = context.process_instruction(&run_instruction(
            outer,
            vec![
                AccountMeta::new_readonly(ID, false),
                AccountMeta::new_readonly(inner, false),
                AccountMeta::new_readonly(system_program::id(), false),
                AccountMeta::new(payer, true),
                AccountMeta::new(first, false),
                AccountMeta::new(second, false),
            ],
            &[],
        ));
        assert!(result.program_result.is_ok(), "{result:#?}");
        assert_eq!(result.return_data, 4_000u64.to_le_bytes());
    }

    /// Return data that a later invoke would erase is refused when the template is created, with
    /// the output error and the index of the instruction that sets it.
    #[test]
    fn return_data_set_before_an_invoke_is_rejected_at_create() {
        let creator = Pubkey::new_unique();
        let context = context(funded_accounts([creator], 10_000_000_000));
        let mut builder = ProgramBuilder::new();
        let system =
            builder.account(ACCOUNT_EXECUTABLE, Some(system_program::id().to_bytes()), None, 0);
        let value = builder.const_u64(1);
        let at = builder.set_return_data(&[Segment::Register(DATA_REG_U64, value)]);
        let cpi = builder.cpi(system, &[], &[]);
        builder.invoke(cpi, None);
        let payload = builder.build().expect("builds");
        let result =
            context.process_instruction(&create_template_instruction(creator, 95, &payload));
        assert_eq!(custom_code(&result), Some(((at as u32) << 16) | 6130), "{result:#?}");
    }

    /// A template cannot forge the run event: a log that starts with the event's tag family is
    /// refused when the template is created, even a byte-exact event for another template.
    #[test]
    fn a_log_that_copies_the_run_event_is_rejected_at_create() {
        let creator = Pubkey::new_unique();
        let context = context(funded_accounts([creator], 10_000_000_000));
        let (victim, _) = find_template_pda(&Pubkey::new_unique(), 7);
        let mut event = b"BEV1".to_vec();
        event.extend_from_slice(&[1, 0, 1]);
        event.extend_from_slice(&1u64.to_le_bytes());
        event.extend_from_slice(victim.as_ref());
        let mut builder = ProgramBuilder::new();
        let forged = builder.blob(&event);
        let at = builder.emit_data(&[Segment::Literal(forged)]);
        let payload = builder.build().expect("builds");
        let result =
            context.process_instruction(&create_template_instruction(creator, 96, &payload));
        assert_eq!(custom_code(&result), Some(((at as u32) << 16) | 6130), "{result:#?}");
    }

    /// Run data for the output fixture: 1,000 lamports per row and the memo `hello`.
    fn output_fixture_inputs() -> Vec<u8> {
        let mut inputs = 1_000u64.to_le_bytes().to_vec();
        inputs.extend_from_slice(&5u16.to_le_bytes());
        inputs.extend_from_slice(b"hello");
        inputs
    }

    /// The bytes of every `Program data:` line the collector recorded, in order. Ballista logs one
    /// field per line, which the runtime writes as base64.
    fn program_data(logger: &Rc<RefCell<LogCollector>>) -> Vec<Vec<u8>> {
        logger
            .borrow()
            .get_recorded_content()
            .iter()
            .filter_map(|line| line.strip_prefix("Program data: "))
            .map(|field| STANDARD.decode(field).expect("base64 field"))
            .collect()
    }

    /// Anything the verifier accepts must execute without a structural error. Generated programs
    /// contain no CPIs, so the only failures they may produce are value-dependent.
    #[test]
    fn generated_programs_never_hit_structural_errors() {
        use ballista_common::template::generate::{
            any_program, ALLOWED_RUNTIME_ERRORS, STRUCTURAL_RUNTIME_ERRORS,
        };
        use proptest::prelude::*;
        use proptest::test_runner::{Config, TestRunner};

        let cases = std::env::var("PROPTEST_CASES")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(48);
        let mut runner = TestRunner::new(Config {
            cases,
            ..Config::default()
        });
        runner
            .run(&any_program(), |program| {
                let creator = Pubkey::new_unique();
                let group_lengths = &[1u8, 2][..program.account_groups];
                let group_total: usize = group_lengths.iter().map(|len| *len as usize).sum();
                let total = program.fixed_accounts + program.row_accounts * program.max_iterations + group_total;
                let runtime: Vec<Pubkey> = (0..total).map(|_| Pubkey::new_unique()).collect();
                let mut accounts = funded_accounts([creator], 10_000_000_000);
                for (index, address) in runtime.iter().enumerate() {
                    accounts.insert(*address, Account::new(index as u64 * 1_000, 0, &system_program::id()));
                }
                let context = context(accounts);
                let created = context.process_instruction(&create_template_instruction(creator, 1, &program.bytes));
                prop_assert!(created.program_result.is_ok(), "finalize rejected a generated program: {created:#?}");
                let (template, _) = find_template_pda(&creator, 1);

                for iterations in [program.min_iterations, program.max_iterations] {
                    let count = program.fixed_accounts + program.row_accounts * iterations;
                    // Fixed accounts and rows first, then the group members from the end of the pool.
                    let metas: Vec<AccountMeta> = runtime[..count]
                        .iter()
                        .chain(runtime[total - group_total..].iter())
                        .map(|address| AccountMeta::new_readonly(*address, false))
                        .collect();
                    let inputs = program.run_inputs(iterations, group_lengths);
                    let result = context.process_instruction(&run_instruction(template, metas, &inputs));
                    if result.program_result.is_ok() {
                        continue;
                    }
                    let code = custom_code(&result);
                    let kind = code.map(|code| code & 0xffff);
                    prop_assert!(
                        kind.is_some_and(|kind| ALLOWED_RUNTIME_ERRORS.contains(&kind)),
                        "unexpected failure {code:?} (structural kinds are {STRUCTURAL_RUNTIME_ERRORS:?}): {result:#?}"
                    );
                }
                Ok(())
            })
            .unwrap_or_else(|failure| panic!("{failure}"));
    }

    /// Loads a compiler fixture written by `pnpm fixtures`.
    fn fixture(name: &str) -> Vec<u8> {
        let hex = match name {
            "ensure-ata" => include_str!("../../../fixtures/ensure-ata.hex"),
            "assert-ata" => include_str!("../../../fixtures/assert-ata.hex"),
            "assert-ata-with-bump" => include_str!("../../../fixtures/assert-ata-with-bump.hex"),
            "carry-sum" => include_str!("../../../fixtures/carry-sum.hex"),
            "waterfall-payout" => include_str!("../../../fixtures/waterfall-payout.hex"),
            "return-data" => include_str!("../../../fixtures/return-data.hex"),
            "dynamic-read" => include_str!("../../../fixtures/dynamic-read.hex"),
            "pinned-mint-read" => include_str!("../../../fixtures/pinned-mint-read.hex"),
            "system-transfer" => include_str!("../../../fixtures/system-transfer.hex"),
            "batch-transfer-30" => include_str!("../../../fixtures/batch-transfer-30.hex"),
            "payroll-row-amounts" => include_str!("../../../fixtures/payroll-row-amounts.hex"),
            "group-forward-transfer" => include_str!("../../../fixtures/group-forward-transfer.hex"),
            "math-ops" => include_str!("../../../fixtures/math-ops.hex"),
            "output" => include_str!("../../../fixtures/output.hex"),
            "loops" => include_str!("../../../fixtures/loops.hex"),
            "introspection" => include_str!("../../../fixtures/introspection.hex"),
            "rate-limited-transfer" => include_str!("../../../fixtures/rate-limited-transfer.hex"),
            "signed-quote-settlement" => {
                include_str!("../../../fixtures/signed-quote-settlement.hex")
            }
            other => panic!("unknown fixture {other}"),
        };
        let bytes: Vec<u8> = hex
            .trim()
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect();
        ProgramView::parse(&bytes)
            .and_then(|program| program.verify())
            .unwrap_or_else(|error| panic!("fixture {name} does not verify: {error}"));
        bytes
    }

    /// The instruction a transaction failed at and its custom error code, if it failed with one.
    fn transaction_code(result: &TransactionResult) -> Option<(usize, u32)> {
        match &result.program_result {
            TransactionProgramResult::Failure(
                index,
                solana_program_error::ProgramError::Custom(code),
            ) => Some((*index, *code)),
            _ => None,
        }
    }

    /// Where a transaction failed inside a compiler fixture's run: the instruction, the error kind,
    /// and the label of the step that emitted the failing program counter, from the source map
    /// `pnpm fixtures` records.
    fn fixture_failure(result: &TransactionResult, name: &str) -> Option<(usize, u32, String)> {
        let (index, code) = transaction_code(result)?;
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("../../../fixtures/manifest.json")).expect("manifest");
        let label = manifest[name]["sourceMap"]
            .as_array()?
            .iter()
            .find(|entry| entry["pc"] == code >> 16)?["label"]
            .as_str()?
            .to_owned();
        Some((index, code & 0xffff, label))
    }

    /// The error kind (low 16 bits) a run failed with, ignoring its context.
    fn decode_kind(result: &mollusk_svm::result::InstructionResult) -> Option<u32> {
        custom_code(result).map(|code| code & 0xffff)
    }

    /// The custom error code a run failed with, if it failed with one.
    fn custom_code(result: &mollusk_svm::result::InstructionResult) -> Option<u32> {
        match &result.program_result {
            mollusk_svm::result::ProgramResult::Failure(
                solana_program_error::ProgramError::Custom(code),
            ) => Some(*code),
            _ => None,
        }
    }

    fn context(accounts: HashMap<Pubkey, Account>) -> MolluskContext<HashMap<Pubkey, Account>> {
        let mut mollusk = Mollusk::default();
        mollusk.add_program_with_loader_and_elf(&ID, &LOADER_V3, BALLISTA_ELF);
        token::add_program(&mut mollusk);
        associated_token::add_program(&mut mollusk);
        memo::add_program(&mut mollusk);
        mollusk.with_context(accounts)
    }

    fn funded_accounts<const N: usize>(
        addresses: [Pubkey; N],
        lamports: u64,
    ) -> HashMap<Pubkey, Account> {
        addresses
            .into_iter()
            .map(|address| (address, Account::new(lamports, 0, &system_program::id())))
            .collect()
    }

    fn lamports(context: &MolluskContext<HashMap<Pubkey, Account>>, address: Pubkey) -> u64 {
        context.account_store.borrow()[&address].lamports()
    }

    fn token_amount(context: &MolluskContext<HashMap<Pubkey, Account>>, address: Pubkey) -> u64 {
        let store = context.account_store.borrow();
        let data = store[&address].data();
        u64::from_le_bytes(data[64..72].try_into().expect("token amount"))
    }

    fn token_account_state(mint: Pubkey, owner: Pubkey, amount: u64) -> TokenAccount {
        TokenAccount {
            mint,
            owner,
            amount,
            delegate: COption::None,
            state: AccountState::Initialized,
            is_native: COption::None,
            delegated_amount: 0,
            close_authority: COption::None,
        }
    }

    fn system_transfer_template(batch_max: Option<u8>, require_guard: bool) -> Vec<u8> {
        let batch = batch_max.is_some();
        let assert_delta = !batch && require_guard;
        let fixed_accounts = if batch { 2 } else { 3 };
        let input_count = if require_guard { 2 } else { 1 };
        let register_count = if assert_delta { 6 } else { input_count };
        let mut instructions = Vec::new();
        if require_guard {
            instructions.push(record(OP_LOAD_INPUT, 0, 0, 0, 0, 0));
            instructions.push(record(OP_REQUIRE, NO_INDEX, 0, 0, 0, 0));
            instructions.push(record(OP_LOAD_INPUT, 1, 1, 0, 0, 0));
        } else {
            instructions.push(record(OP_LOAD_INPUT, 0, 0, 0, 0, 0));
        }
        let amount_register = if require_guard { 1 } else { 0 };
        if assert_delta {
            // This register is the runtime representation of `step.snapshot("before", ...)`.
            instructions.push(record(OP_ACCOUNT_LAMPORTS, 2, 1, 0, 0, 0));
        }
        if batch {
            instructions.push(record(OP_FOREACH, NO_INDEX, 1, 0, 0, 0));
        }
        instructions.push(record(OP_INVOKE, NO_INDEX, 0, NO_INDEX, 0, 0));
        if assert_delta {
            instructions.push(record(OP_ACCOUNT_LAMPORTS, 3, 1, 0, 0, 0));
            instructions.push(record(OP_SUB, 4, 2, amount_register, 0, 0));
            instructions.push(record(OP_EQ, 5, 3, 4, 0, 0));
            instructions.push(record(OP_REQUIRE, NO_INDEX, 5, 0, 0, 0));
        }

        let header = ProgramHeader::new(
            fixed_accounts,
            u8::from(batch),
            batch_max.unwrap_or(0),
            0,
            input_count,
            register_count,
            instructions.len() as u8,
            1,
            2,
            2,
            1,
            0,
            4,
            0,
            0,
        );
        let mut account_constraints = vec![
            account_constraint(ACCOUNT_EXECUTABLE, 0),
            account_constraint(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, NO_INDEX),
        ];
        account_constraints.push(account_constraint(ACCOUNT_WRITABLE, NO_INDEX));
        let mut inputs = Vec::new();
        if require_guard {
            inputs.push(InputDescriptor {
                value_type: VALUE_BOOL,
                reserved: 0,
                max_len_le: [0; 2],
            });
        }
        inputs.push(InputDescriptor {
            value_type: VALUE_U64,
            reserved: 0,
            max_len_le: [0; 2],
        });
        let destination = if batch { ITERATION_ACCOUNT_BIT } else { 2 };
        let cpi = CpiDescriptor {
            program_account: 0,
            account_group: NO_INDEX,
            account_start_le: 0u16.to_le_bytes(),
            account_len: 2,
            segment_len: 2,
            segment_start_le: 0u16.to_le_bytes(),
            max_data_len_le: 12u16.to_le_bytes(),
            reserved1: [0; 2],
        };
        let cpi_accounts = [
            CpiAccountRecord {
                account: 1,
                flags: ACCOUNT_SIGNER | ACCOUNT_WRITABLE,
            },
            CpiAccountRecord {
                account: destination,
                flags: ACCOUNT_WRITABLE,
            },
        ];
        let segments = [
            DataSegment {
                kind: DATA_LITERAL,
                register: NO_INDEX,
                offset_le: 0u16.to_le_bytes(),
                len_le: 4u16.to_le_bytes(),
                reserved: [0; 2],
            },
            DataSegment {
                kind: DATA_REG_U64,
                register: amount_register,
                offset_le: [0; 2],
                len_le: [0; 2],
                reserved: [0; 2],
            },
        ];

        let mut bytes = Vec::new();
        bytes.extend_from_slice(header.as_bytes());
        append_records(&mut bytes, &account_constraints);
        append_records(&mut bytes, &inputs);
        append_records(&mut bytes, &instructions);
        append_records(&mut bytes, &[cpi]);
        append_records(&mut bytes, &cpi_accounts);
        append_records(&mut bytes, &segments);
        append_records(&mut bytes, &[PubkeyRecord { bytes: [0; 32] }]);
        bytes.extend_from_slice(&[2, 0, 0, 0]);
        ProgramView::parse(&bytes)
            .and_then(|program| program.verify())
            .expect("valid integration template");
        bytes
    }

    fn token_transfer_template(batch_max: u8) -> Vec<u8> {
        let header = ProgramHeader::new(3, 1, batch_max, 0, 1, 1, 3, 1, 3, 2, 1, 0, 1, 0, 0);
        let mut token_account_constraint = account_constraint(ACCOUNT_WRITABLE, NO_INDEX);
        token_account_constraint.owner_index = 0;
        let account_constraints = [
            account_constraint(ACCOUNT_EXECUTABLE, 0),
            token_account_constraint,
            account_constraint(ACCOUNT_SIGNER, NO_INDEX),
            token_account_constraint,
        ];
        let inputs = [InputDescriptor {
            value_type: VALUE_U64,
            reserved: 0,
            max_len_le: [0; 2],
        }];
        let instructions = [
            record(OP_LOAD_INPUT, 0, 0, 0, 0, 0),
            record(OP_FOREACH, NO_INDEX, 1, 0, 0, 0),
            record(OP_INVOKE, NO_INDEX, 0, NO_INDEX, 0, 0),
        ];
        let cpis = [CpiDescriptor {
            program_account: 0,
            account_group: NO_INDEX,
            account_start_le: [0; 2],
            account_len: 3,
            segment_len: 2,
            segment_start_le: [0; 2],
            max_data_len_le: 9u16.to_le_bytes(),
            reserved1: [0; 2],
        }];
        let cpi_accounts = [
            CpiAccountRecord {
                account: 1,
                flags: ACCOUNT_WRITABLE,
            },
            CpiAccountRecord {
                account: ITERATION_ACCOUNT_BIT,
                flags: ACCOUNT_WRITABLE,
            },
            CpiAccountRecord {
                account: 2,
                flags: ACCOUNT_SIGNER,
            },
        ];
        let segments = [
            DataSegment {
                kind: DATA_LITERAL,
                register: NO_INDEX,
                offset_le: [0; 2],
                len_le: 1u16.to_le_bytes(),
                reserved: [0; 2],
            },
            DataSegment {
                kind: DATA_REG_U64,
                register: 0,
                offset_le: [0; 2],
                len_le: [0; 2],
                reserved: [0; 2],
            },
        ];
        let mut bytes = Vec::new();
        bytes.extend_from_slice(header.as_bytes());
        append_records(&mut bytes, &account_constraints);
        append_records(&mut bytes, &inputs);
        append_records(&mut bytes, &instructions);
        append_records(&mut bytes, &cpis);
        append_records(&mut bytes, &cpi_accounts);
        append_records(&mut bytes, &segments);
        append_records(
            &mut bytes,
            &[PubkeyRecord {
                bytes: token::ID.to_bytes(),
            }],
        );
        bytes.push(3);
        ProgramView::parse(&bytes)
            .and_then(|program| program.verify())
            .expect("valid token batch template");
        bytes
    }

    fn ata_then_transfer_template(batch_max: u8) -> Vec<u8> {
        let header = ProgramHeader::new(7, 2, batch_max, 0, 1, 8, 12, 2, 9, 5, 3, 0, 1, 0, 0);
        let account_constraints = [
            account_constraint(ACCOUNT_EXECUTABLE, 0),
            account_constraint(ACCOUNT_EXECUTABLE, 1),
            account_constraint(ACCOUNT_EXECUTABLE, 2),
            account_constraint(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, NO_INDEX),
            account_constraint(ACCOUNT_WRITABLE, NO_INDEX),
            account_constraint(ACCOUNT_SIGNER, NO_INDEX),
            account_constraint(0, NO_INDEX),
            account_constraint(0, NO_INDEX),
            account_constraint(ACCOUNT_WRITABLE, NO_INDEX),
        ];
        let inputs = [InputDescriptor {
            value_type: VALUE_U64,
            reserved: 0,
            max_len_le: [0; 2],
        }];
        let instructions = [
            record(OP_LOAD_INPUT, 0, 0, 0, 0, 0),
            record(OP_FOREACH, NO_INDEX, 10, 0, 0, 0),
            record(OP_ACCOUNT_KEY, 1, 0x80, 0, 0, 0),
            record(OP_ACCOUNT_KEY, 2, 1, 0, 0, 0),
            record(OP_ACCOUNT_KEY, 3, 6, 0, 0, 0),
            record(OP_DERIVE_PDA, 4, 0, 0, 0, 3u64 << 32),
            record(OP_ACCOUNT_KEY, 5, 0x81, 0, 0, 0),
            record(OP_EQ, 6, 4, 5, 0, 0),
            record(OP_REQUIRE, NO_INDEX, 6, 0, 0, 0),
            record(OP_ACCOUNT_IS_EMPTY, 7, 0x81, 0, 0, 0),
            record(OP_INVOKE, NO_INDEX, 0, 7, 0, 0),
            record(OP_INVOKE, NO_INDEX, 1, NO_INDEX, 0, 0),
        ];
        let cpis = [
            CpiDescriptor {
                program_account: 0,
                account_group: NO_INDEX,
                account_start_le: [0; 2],
                account_len: 6,
                segment_len: 0,
                segment_start_le: [0; 2],
                max_data_len_le: [0; 2],
                reserved1: [0; 2],
            },
            CpiDescriptor {
                program_account: 1,
                account_group: NO_INDEX,
                account_start_le: 6u16.to_le_bytes(),
                account_len: 3,
                segment_len: 2,
                segment_start_le: 3u16.to_le_bytes(),
                max_data_len_le: 9u16.to_le_bytes(),
                reserved1: [0; 2],
            },
        ];
        let cpi_accounts = [
            CpiAccountRecord {
                account: 3,
                flags: ACCOUNT_SIGNER | ACCOUNT_WRITABLE,
            },
            CpiAccountRecord {
                account: 0x81,
                flags: ACCOUNT_WRITABLE,
            },
            CpiAccountRecord {
                account: 0x80,
                flags: 0,
            },
            CpiAccountRecord {
                account: 6,
                flags: 0,
            },
            CpiAccountRecord {
                account: 2,
                flags: 0,
            },
            CpiAccountRecord {
                account: 1,
                flags: 0,
            },
            CpiAccountRecord {
                account: 4,
                flags: ACCOUNT_WRITABLE,
            },
            CpiAccountRecord {
                account: 0x81,
                flags: ACCOUNT_WRITABLE,
            },
            CpiAccountRecord {
                account: 5,
                flags: ACCOUNT_SIGNER,
            },
        ];
        let segments = [
            DataSegment {
                kind: DATA_REG_PUBKEY,
                register: 1,
                offset_le: [0; 2],
                len_le: [0; 2],
                reserved: [0; 2],
            },
            DataSegment {
                kind: DATA_REG_PUBKEY,
                register: 2,
                offset_le: [0; 2],
                len_le: [0; 2],
                reserved: [0; 2],
            },
            DataSegment {
                kind: DATA_REG_PUBKEY,
                register: 3,
                offset_le: [0; 2],
                len_le: [0; 2],
                reserved: [0; 2],
            },
            DataSegment {
                kind: DATA_LITERAL,
                register: NO_INDEX,
                offset_le: [0; 2],
                len_le: 1u16.to_le_bytes(),
                reserved: [0; 2],
            },
            DataSegment {
                kind: DATA_REG_U64,
                register: 0,
                offset_le: [0; 2],
                len_le: [0; 2],
                reserved: [0; 2],
            },
        ];
        let mut bytes = Vec::new();
        bytes.extend_from_slice(header.as_bytes());
        append_records(&mut bytes, &account_constraints);
        append_records(&mut bytes, &inputs);
        append_records(&mut bytes, &instructions);
        append_records(&mut bytes, &cpis);
        append_records(&mut bytes, &cpi_accounts);
        append_records(&mut bytes, &segments);
        append_records(
            &mut bytes,
            &[
                PubkeyRecord {
                    bytes: associated_token::ID.to_bytes(),
                },
                PubkeyRecord {
                    bytes: token::ID.to_bytes(),
                },
                PubkeyRecord {
                    bytes: system_program::id().to_bytes(),
                },
            ],
        );
        bytes.push(3);
        ProgramView::parse(&bytes)
            .and_then(|program| program.verify())
            .expect("valid ATA + transfer template");
        bytes
    }

    fn account_constraint(flags: u8, address_index: u8) -> AccountConstraint {
        AccountConstraint {
            flags,
            address_index,
            owner_index: NO_INDEX,
            reserved: 0,
            min_data_len_le: [0; 4],
        }
    }

    fn record(opcode: u8, dst: u8, a: u8, b: u8, c: u8, immediate: u64) -> InstructionRecord {
        InstructionRecord {
            opcode,
            dst,
            a,
            b,
            c,
            flags: 0,
            immediate_le: immediate.to_le_bytes(),
            reserved: [0; 2],
        }
    }

    fn append_records<T: Immutable + IntoBytes>(output: &mut Vec<u8>, records: &[T]) {
        for record in records {
            output.extend_from_slice(record.as_bytes());
        }
    }

    fn find_template_pda(creator: &Pubkey, template_id: u16) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[TEMPLATE_SEED, creator.as_ref(), &template_id.to_le_bytes()],
            &ID,
        )
    }

    fn template_hash(payload: &[u8]) -> [u8; 32] {
        solana_sha256_hasher::hash(payload).to_bytes()
    }

    fn create_template_instruction(
        creator: Pubkey,
        template_id: u16,
        payload: &[u8],
    ) -> Instruction {
        let (template, _) = find_template_pda(&creator, template_id);
        let mut data = Vec::with_capacity(35 + payload.len());
        data.push(IX_CREATE_TEMPLATE);
        data.extend_from_slice(&template_id.to_le_bytes());
        data.extend_from_slice(&template_hash(payload));
        data.extend_from_slice(payload);
        Instruction {
            program_id: ID,
            accounts: vec![
                AccountMeta::new(creator, true),
                AccountMeta::new(template, false),
                AccountMeta::new_readonly(system_program::id(), false),
            ],
            data,
        }
    }

    fn begin_template_instruction(
        creator: Pubkey,
        template_id: u16,
        payload_len: u32,
        payload_hash: [u8; 32],
    ) -> Instruction {
        let (template, _) = find_template_pda(&creator, template_id);
        let mut data = Vec::with_capacity(39);
        data.push(IX_BEGIN_TEMPLATE);
        data.extend_from_slice(&template_id.to_le_bytes());
        data.extend_from_slice(&payload_len.to_le_bytes());
        data.extend_from_slice(&payload_hash);
        Instruction {
            program_id: ID,
            accounts: vec![
                AccountMeta::new(creator, true),
                AccountMeta::new(template, false),
                AccountMeta::new_readonly(system_program::id(), false),
            ],
            data,
        }
    }

    fn write_template_chunk_instruction(
        creator: Pubkey,
        template: Pubkey,
        offset: u32,
        bytes: &[u8],
    ) -> Instruction {
        let mut data = Vec::with_capacity(5 + bytes.len());
        data.push(IX_WRITE_TEMPLATE_CHUNK);
        data.extend_from_slice(&offset.to_le_bytes());
        data.extend_from_slice(bytes);
        Instruction {
            program_id: ID,
            accounts: vec![
                AccountMeta::new(creator, true),
                AccountMeta::new(template, false),
            ],
            data,
        }
    }

    fn finalize_template_instruction(creator: Pubkey, template: Pubkey) -> Instruction {
        Instruction {
            program_id: ID,
            accounts: vec![
                AccountMeta::new(creator, true),
                AccountMeta::new(template, false),
            ],
            data: vec![IX_FINALIZE_TEMPLATE],
        }
    }

    fn cancel_template_instruction(creator: Pubkey, template: Pubkey) -> Instruction {
        Instruction {
            program_id: ID,
            accounts: vec![
                AccountMeta::new(creator, true),
                AccountMeta::new(template, false),
            ],
            data: vec![IX_CANCEL_TEMPLATE],
        }
    }

    fn run_instruction(
        template: Pubkey,
        runtime_accounts: Vec<AccountMeta>,
        inputs: &[u8],
    ) -> Instruction {
        let mut data = Vec::with_capacity(1 + inputs.len());
        data.push(IX_RUN);
        data.extend_from_slice(inputs);
        let mut accounts = vec![AccountMeta::new_readonly(template, false)];
        accounts.extend(runtime_accounts);
        Instruction {
            program_id: ID,
            accounts,
            data,
        }
    }

    mod registry {
        use super::*;
        use ballista_common::template::{
            OP_READ_BOOL, OP_READ_I64, OP_READ_PUBKEY, OP_READ_U128, OP_READ_U64,
            SYSTEM_PROGRAM_ADDRESS, VALUE_BOOL, VALUE_I64, VALUE_PUBKEY, VALUE_U128, VALUE_U64,
        };

        const INVALID_REGISTRY_ENTRY: u32 = 6025;
        const REGISTRY_REENTRY: u32 = 6026;
        const ACCOUNT_CONSTRAINT_FAILED: u32 = 6020;

        /// The accounts every registry template here declares, in this order.
        struct Accounts {
            system: u8,
            payer: u8,
            entry: u8,
        }

        fn declare(builder: &mut ProgramBuilder) -> Accounts {
            Accounts {
                system: builder.account(ACCOUNT_EXECUTABLE, Some(SYSTEM_PROGRAM_ADDRESS), None, 0),
                payer: builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0),
                entry: builder.account(ACCOUNT_WRITABLE, None, None, 0),
            }
        }

        /// Opens registry 0 (16 bytes) keyed by the payer, adds the `u64` input to field 0 and
        /// writes the clock to field 8.
        fn counter() -> Vec<u8> {
            let mut builder = ProgramBuilder::new();
            let accounts = declare(&mut builder);
            let input = builder.input(VALUE_U64, 0);
            let amount = builder.load_input(input);
            let key = builder.account_key(accounts.payer);
            builder.open_registry(accounts.entry, Some(key), accounts.payer, 0, 16, accounts.system);
            let spent = builder.read_registry(accounts.entry, 0, OP_READ_U64);
            let total = builder.binary(OP_ADD, spent, amount);
            builder.write_registry(accounts.entry, 0, OP_READ_U64, total);
            let now = builder.clock_timestamp();
            builder.write_registry(accounts.entry, 8, OP_READ_I64, now);
            builder.build().unwrap()
        }

        fn entry_address(template: &Pubkey, index: u8, key: &Pubkey) -> Pubkey {
            Pubkey::find_program_address(&[b"registry", template.as_ref(), &[index], key.as_ref()], &ID).0
        }

        /// A context with `creator` and `payers` funded and `payload` uploaded as template `id`.
        fn setup(payload: &[u8], id: u16, payers: &[Pubkey]) -> (MolluskContext<HashMap<Pubkey, Account>>, Pubkey) {
            let creator = Pubkey::new_unique();
            let mut accounts = funded_accounts([creator], 10_000_000_000);
            for payer in payers {
                accounts.insert(*payer, Account::new(1_000_000_000, 0, &system_program::id()));
            }
            let mut context = context(accounts);
            context.mollusk.sysvars.clock.unix_timestamp = 1_800_000_000;
            let created = context.process_instruction(&create_template_instruction(creator, id, payload));
            assert!(created.program_result.is_ok(), "{created:#?}");
            (context, find_template_pda(&creator, id).0)
        }

        fn run(
            context: &MolluskContext<HashMap<Pubkey, Account>>,
            template: Pubkey,
            payer: Pubkey,
            entry: AccountMeta,
            amount: u64,
        ) -> mollusk_svm::result::InstructionResult {
            let metas = vec![
                AccountMeta::new_readonly(system_program::id(), false),
                AccountMeta::new(payer, true),
                entry,
            ];
            context.process_instruction(&run_instruction(template, metas, &amount.to_le_bytes()))
        }

        /// The error kind and program counter of a failed run.
        fn failure(result: &mollusk_svm::result::InstructionResult) -> (u32, u32) {
            let code = custom_code(result).unwrap_or_else(|| panic!("no custom error: {result:#?}"));
            (code & 0xffff, code >> 16)
        }

        fn account(context: &MolluskContext<HashMap<Pubkey, Account>>, address: Pubkey) -> Account {
            context.account_store.borrow()[&address].clone()
        }

        #[test]
        fn the_first_run_creates_the_entry_and_later_runs_reopen_it() {
            let payer = Pubkey::new_unique();
            let (context, template) = setup(&counter(), 1, &[payer]);
            let entry = entry_address(&template, 0, &payer);
            let rent = context.mollusk.sysvars.rent.minimum_balance(88);

            let result = run(&context, template, payer, AccountMeta::new(entry, false), 5);
            assert!(result.program_result.is_ok(), "{result:#?}");
            let created = account(&context, entry);
            assert_eq!(created.owner, ID);
            assert_eq!(created.lamports, rent, "an entry holds its rent and nothing else");
            assert_eq!(created.data.len(), 88);
            assert_eq!(&created.data[..8], b"BREG\x01\x00\x00\x00");
            assert_eq!(&created.data[8..40], template.as_ref());
            assert_eq!(&created.data[40..72], payer.as_ref());
            assert_eq!(created.data[72..80], 5u64.to_le_bytes());
            assert_eq!(created.data[80..88], 1_800_000_000i64.to_le_bytes());
            assert_eq!(lamports(&context, payer), 1_000_000_000 - rent, "the payer paid the rent");

            let result = run(&context, template, payer, AccountMeta::new(entry, false), 7);
            assert!(result.program_result.is_ok(), "{result:#?}");
            assert_eq!(account(&context, entry).data[72..80], 12u64.to_le_bytes());
            assert_eq!(lamports(&context, payer), 1_000_000_000 - rent, "no second charge");
        }

        #[test]
        fn a_pre_funded_address_is_topped_up_allocated_and_assigned() {
            for already in [1u64, 1_503_360, 2_000_000] {
                let payer = Pubkey::new_unique();
                let (context, template) = setup(&counter(), 2, &[payer]);
                let entry = entry_address(&template, 0, &payer);
                context
                    .account_store
                    .borrow_mut()
                    .insert(entry, Account::new(already, 0, &system_program::id()));
                let rent = context.mollusk.sysvars.rent.minimum_balance(88);
                let result = run(&context, template, payer, AccountMeta::new(entry, false), 5);
                assert!(result.program_result.is_ok(), "{already}: {result:#?}");
                let created = account(&context, entry);
                assert_eq!(created.owner, ID, "{already}");
                assert_eq!(created.data.len(), 88, "{already}");
                assert_eq!(created.lamports, rent.max(already), "{already}");
                assert_eq!(
                    lamports(&context, payer),
                    1_000_000_000 - rent.saturating_sub(already),
                    "{already}: the payer covers only the shortfall"
                );
                assert_eq!(created.data[72..80], 5u64.to_le_bytes(), "{already}");
            }
        }

        #[test]
        fn an_account_that_is_not_the_named_entry_fails() {
            let payer = Pubkey::new_unique();
            let other_payer = Pubkey::new_unique();
            let (context, template) = setup(&counter(), 3, &[payer, other_payer]);
            // `counter` loads its input (pc 0) and the payer's key (pc 1) before the open.
            let open_pc = 2;

            // Creation at an address that is not the entry's.
            let stranger = Pubkey::new_unique();
            let result = run(&context, template, payer, AccountMeta::new(stranger, false), 1);
            assert_eq!(failure(&result), (INVALID_REGISTRY_ENTRY, open_pc));

            // Create the payer's entry, then pass it for another payer: another key's entry.
            let entry = entry_address(&template, 0, &payer);
            assert!(run(&context, template, payer, AccountMeta::new(entry, false), 1).program_result.is_ok());
            let result = run(&context, template, other_payer, AccountMeta::new(entry, false), 1);
            assert_eq!(failure(&result), (INVALID_REGISTRY_ENTRY, open_pc));

            // The same code published at another address: another template's entry.
            let creator = Pubkey::new_unique();
            context.account_store.borrow_mut().insert(creator, Account::new(10_000_000_000, 0, &system_program::id()));
            assert!(context.process_instruction(&create_template_instruction(creator, 4, &counter())).program_result.is_ok());
            let copy = find_template_pda(&creator, 4).0;
            let result = run(&context, copy, payer, AccountMeta::new(entry, false), 1);
            assert_eq!(failure(&result), (INVALID_REGISTRY_ENTRY, open_pc));

            // An account another program owns.
            let foreign = Pubkey::new_unique();
            context.account_store.borrow_mut().insert(foreign, Account::new(1_000_000, 88, &Pubkey::new_unique()));
            let result = run(&context, template, payer, AccountMeta::new(foreign, false), 1);
            assert_eq!(failure(&result), (INVALID_REGISTRY_ENTRY, open_pc));

            // A read-only entry never reaches the open: the account check before the first
            // instruction refuses it, account 2, since the template declares the entry writable.
            let result = run(&context, template, payer, AccountMeta::new_readonly(entry, false), 1);
            assert_eq!(failure(&result), (ACCOUNT_CONSTRAINT_FAILED, 2));
        }

        /// A created entry is the header plus the size its template declares. (An existing entry of
        /// the wrong size needs a header that matches, which only the template's own entries have,
        /// and a template's sizes never change; the host test
        /// `an_existing_entry_opens_only_when_everything_matches` covers that check.)
        #[test]
        fn a_created_entry_is_the_header_plus_the_declared_size() {
            let mut builder = ProgramBuilder::new();
            let accounts = declare(&mut builder);
            let key = builder.account_key(accounts.payer);
            builder.open_registry(accounts.entry, Some(key), accounts.payer, 0, 24, accounts.system);
            let payer = Pubkey::new_unique();
            let (context, template) = setup(&builder.build().unwrap(), 5, &[payer]);
            let entry = entry_address(&template, 0, &payer);
            let metas = vec![
                AccountMeta::new_readonly(system_program::id(), false),
                AccountMeta::new(payer, true),
                AccountMeta::new(entry, false),
            ];
            let result = context.process_instruction(&run_instruction(template, metas, &[]));
            assert!(result.program_result.is_ok(), "{result:#?}");
            assert_eq!(account(&context, entry).data.len(), 72 + 24);
        }

        #[test]
        fn every_writable_width_round_trips() {
            let mut builder = ProgramBuilder::new();
            let accounts = declare(&mut builder);
            let key = builder.account_key(accounts.payer);
            builder.open_registry(accounts.entry, Some(key), accounts.payer, 1, 65, accounts.system);
            // bool at 0, u64 at 1, i64 at 9, u128 at 17, pubkey at 33.
            let fields = [
                (VALUE_BOOL, OP_READ_BOOL, 0),
                (VALUE_U64, OP_READ_U64, 1),
                (VALUE_I64, OP_READ_I64, 9),
                (VALUE_U128, OP_READ_U128, 17),
                (VALUE_PUBKEY, OP_READ_PUBKEY, 33),
            ];
            for (value_type, selector, offset) in fields {
                let input = builder.input(value_type, 0);
                let value = builder.load_input(input);
                builder.write_registry(accounts.entry, offset, selector, value);
                let read = builder.read_registry(accounts.entry, offset, selector);
                let same = builder.binary(OP_EQ, read, value);
                builder.require(same);
            }
            let payer = Pubkey::new_unique();
            let (context, template) = setup(&builder.build().unwrap(), 6, &[payer]);
            let entry = entry_address(&template, 1, &payer);
            let mut inputs = vec![1u8];
            inputs.extend_from_slice(&u64::MAX.to_le_bytes());
            inputs.extend_from_slice(&(-3i64).to_le_bytes());
            inputs.extend_from_slice(&(u128::MAX - 1).to_le_bytes());
            inputs.extend_from_slice(&[4; 32]);
            let metas = vec![
                AccountMeta::new_readonly(system_program::id(), false),
                AccountMeta::new(payer, true),
                AccountMeta::new(entry, false),
            ];
            let result = context.process_instruction(&run_instruction(template, metas, &inputs));
            assert!(result.program_result.is_ok(), "{result:#?}");
            assert_eq!(account(&context, entry).data[72..], inputs[..]);
        }

        /// A registry template that, after its open, invokes Ballista to run `inner` with the
        /// accounts `pass` names; `pass_entry` also passes its entry, writable.
        fn nested(pass_entry: bool) -> Vec<u8> {
            let mut builder = ProgramBuilder::new();
            let accounts = declare(&mut builder);
            let ballista = builder.account(ACCOUNT_EXECUTABLE, Some(ID.to_bytes()), None, 0);
            let inner = builder.account(0, None, None, 0);
            builder.open_registry(accounts.entry, None, accounts.payer, 0, 8, accounts.system);
            let mut cpi_accounts = vec![(inner, 0)];
            if pass_entry {
                cpi_accounts.push((accounts.entry, ACCOUNT_WRITABLE));
            }
            let data = builder.blob(&[IX_RUN]);
            let cpi = builder.cpi(ballista, &cpi_accounts, &[Segment::Literal(data)]);
            builder.invoke(cpi, None);
            builder.build().unwrap()
        }

        #[test]
        fn a_cpi_that_passes_an_open_entry_writable_fails_with_registry_reentry() {
            // The inner template asserts a constant and names no account.
            let mut inner = ProgramBuilder::new();
            let yes = inner.const_bool(true);
            inner.require(yes);
            let inner = inner.build().unwrap();
            for (pass_entry, id) in [(true, 7u16), (false, 8)] {
                let payer = Pubkey::new_unique();
                let (context, template) = setup(&nested(pass_entry), id, &[payer]);
                let creator = Pubkey::new_unique();
                context.account_store.borrow_mut().insert(creator, Account::new(10_000_000_000, 0, &system_program::id()));
                assert!(context.process_instruction(&create_template_instruction(creator, 1, &inner)).program_result.is_ok());
                let inner_template = find_template_pda(&creator, 1).0;
                let entry = entry_address(&template, 0, &Pubkey::default());
                let metas = vec![
                    AccountMeta::new_readonly(system_program::id(), false),
                    AccountMeta::new(payer, true),
                    AccountMeta::new(entry, false),
                    AccountMeta::new_readonly(ID, false),
                    AccountMeta::new_readonly(inner_template, false),
                ];
                let result = context.process_instruction(&run_instruction(template, metas, &[]));
                if pass_entry {
                    // The invoke is the second instruction, after the open.
                    assert_eq!(failure(&result), (REGISTRY_REENTRY, 1));
                } else {
                    assert!(result.program_result.is_ok(), "a CPI to Ballista that leaves the entry out runs: {result:#?}");
                }
            }
        }

        /// The kind and the source label of a failed run of fixture `name`.
        fn labeled(result: &mollusk_svm::result::InstructionResult, name: &str) -> (u32, String) {
            let (kind, pc) = failure(result);
            let manifest: serde_json::Value =
                serde_json::from_str(include_str!("../../../fixtures/manifest.json")).expect("manifest");
            let label = manifest[name]["sourceMap"]
                .as_array()
                .and_then(|entries| entries.iter().find(|entry| entry["pc"] == pc))
                .and_then(|entry| entry["label"].as_str())
                .unwrap_or_default()
                .to_owned();
            (kind, label)
        }

        #[test]
        fn a_rate_limited_transfer_spends_within_its_cap_and_refills_with_time() {
            const REQUIREMENT_FAILED: u32 = 6015;
            let caller = Pubkey::new_unique();
            let recipient = Pubkey::new_unique();
            let (mut context, template) = setup(&fixture("rate-limited-transfer"), 20, &[caller, recipient]);
            let entry = entry_address(&template, 0, &caller);
            let pay = |context: &MolluskContext<HashMap<Pubkey, Account>>, amount: u64| {
                // The fixture's account order: caller, recipient, limits, systemProgram.
                let metas = vec![
                    AccountMeta::new(caller, true),
                    AccountMeta::new(recipient, false),
                    AccountMeta::new(entry, false),
                    AccountMeta::new_readonly(system_program::id(), false),
                ];
                context.process_instruction(&run_instruction(template, metas, &amount.to_le_bytes()))
            };
            let rent = context.mollusk.sysvars.rent.minimum_balance(72 + 16);

            // The first run creates the entry; the caller pays its rent and the transfer.
            let result = pay(&context, 600_000);
            assert!(result.program_result.is_ok(), "{result:#?}");
            assert_eq!(lamports(&context, caller), 1_000_000_000 - rent - 600_000);
            assert_eq!(account(&context, entry).owner, ID);
            // Up to the cap exactly.
            assert!(pay(&context, 400_000).program_result.is_ok());
            assert_eq!(lamports(&context, recipient), 1_000_000_000 + 1_000_000);
            // One lamport over.
            let result = pay(&context, 1);
            assert_eq!(labeled(&result, "rate-limited-transfer"), (REQUIREMENT_FAILED, "withinRateLimit".into()));

            // Ten seconds refill 100 lamports: 101 is still over, 100 lands.
            context.mollusk.sysvars.clock.unix_timestamp += 10;
            let result = pay(&context, 101);
            assert_eq!(labeled(&result, "rate-limited-transfer"), (REQUIREMENT_FAILED, "withinRateLimit".into()));
            assert!(pay(&context, 100).program_result.is_ok());
            let data = account(&context, entry).data;
            assert_eq!(data[72..80], 1_000_000u64.to_le_bytes());
            assert_eq!(data[80..88], 1_800_000_010i64.to_le_bytes());

            // A clock that steps back refills nothing, and does not fail the run on its own.
            context.mollusk.sysvars.clock.unix_timestamp -= 5;
            let result = pay(&context, 1);
            assert_eq!(labeled(&result, "rate-limited-transfer"), (REQUIREMENT_FAILED, "withinRateLimit".into()));
        }
    }
}
