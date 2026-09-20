#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use ballista_common::instruction::{
        IX_BEGIN_TEMPLATE, IX_CANCEL_TEMPLATE, IX_CREATE_TEMPLATE, IX_FINALIZE_TEMPLATE, IX_RUN,
        IX_WRITE_TEMPLATE_CHUNK,
    };
    use ballista_common::template::{
        AccountConstraint, CpiAccountRecord, CpiDescriptor, DataSegment, InputDescriptor,
        InstructionRecord, ProgramBuilder, ProgramHeader, ProgramView, PubkeyRecord, Segment,
        TemplateAccount, ACCOUNT_EXECUTABLE, ACCOUNT_SIGNER, ACCOUNT_WRITABLE, DATA_LITERAL,
        DATA_REG_PUBKEY, DATA_REG_U64, ITERATION_ACCOUNT_BIT, MAX_PDA_SEEDS, NO_INDEX,
        OP_ACCOUNT_IS_EMPTY, OP_ACCOUNT_KEY, OP_ACCOUNT_LAMPORTS, OP_ADD, OP_DERIVE_PDA, OP_EQ,
        OP_FOREACH, OP_INVOKE, OP_LOAD_INPUT, OP_LTE, OP_NE, OP_READ_U64, OP_REQUIRE, OP_SUB,
        VALUE_BOOL, VALUE_U64,
    };
    use mollusk_svm::{program::loader_keys::LOADER_V3, Mollusk, MolluskContext};
    use mollusk_svm_programs_memo::memo;
    use mollusk_svm_programs_token::{associated_token, token};
    use solana_account::{Account, ReadableAccount};
    use solana_instruction::{AccountMeta, Instruction};
    use solana_program_option::COption;
    use solana_pubkey::{pubkey, Pubkey};
    use solana_sdk_ids::system_program;
    use spl_token_interface::state::{Account as TokenAccount, AccountState, Mint};
    use zerocopy::{Immutable, IntoBytes};

    const BALLISTA_ELF: &[u8] = include_bytes!("../../../target/deploy/ballista.so");
    const ID: Pubkey = pubkey!("BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD");
    const TEMPLATE_SEED: &[u8] = b"template-v2";

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
            let decoded = TemplateAccount::parse(template_account.data()).expect("v2 template");
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
        assert!(context
            .process_instruction(&wrong_owner_run)
            .program_result
            .is_err());
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
    /// does not expose program logs, so the layout is covered by a host unit test.
    #[test]
    fn event_flag_does_not_change_run_semantics() {
        let creator = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let recipient = Pubkey::new_unique();
        let context = context(funded_accounts([creator, payer, recipient], 10_000_000_000));

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
            reserved0: 0,
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
        let header = ProgramHeader::new(3, 1, batch_max, 0, 1, 1, 3, 1, 3, 2, 1, 0, 1);
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
            reserved0: 0,
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
        let header = ProgramHeader::new(7, 2, batch_max, 0, 1, 8, 12, 2, 9, 5, 3, 0, 1);
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
                reserved0: 0,
                account_start_le: [0; 2],
                account_len: 6,
                segment_len: 0,
                segment_start_le: [0; 2],
                max_data_len_le: [0; 2],
                reserved1: [0; 2],
            },
            CpiDescriptor {
                program_account: 1,
                reserved0: 0,
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
}
