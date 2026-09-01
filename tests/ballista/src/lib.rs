#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use ballista_common::instruction::{
        IX_BEGIN_TEMPLATE, IX_CANCEL_TEMPLATE, IX_CREATE_TEMPLATE, IX_FINALIZE_TEMPLATE, IX_RUN,
        IX_WRITE_TEMPLATE_CHUNK,
    };
    use ballista_common::template::{
        AccountConstraint, CpiAccountRecord, CpiDescriptor, DataSegment, InputDescriptor,
        InstructionRecord, ProgramHeader, ProgramView, PubkeyRecord, TemplateAccount,
        ACCOUNT_EXECUTABLE, ACCOUNT_SIGNER, ACCOUNT_WRITABLE, DATA_LITERAL, DATA_REG_U64,
        ITERATION_ACCOUNT_BIT, NO_INDEX, OP_FOREACH, OP_INVOKE, OP_LOAD_INPUT, OP_REQUIRE,
        VALUE_BOOL, VALUE_U64,
    };
    use mollusk_svm::{program::loader_keys::LOADER_V3, Mollusk, MolluskContext};
    use mollusk_svm_programs_token::{associated_token, token};
    use solana_account::{Account, ReadableAccount};
    use solana_instruction::{AccountMeta, Instruction};
    use solana_program_option::COption;
    use solana_pubkey::{pubkey, Pubkey};
    use solana_sdk_ids::system_program;
    use spl_token_interface::state::{Account as TokenAccount, AccountState, Mint};
    use zerocopy::{Immutable, IntoBytes};

    const BALLISTA_ELF: &[u8] = include_bytes!("../../../target/deploy/ballista.so");
    const ID: Pubkey = pubkey!("BLSTAxxzuLZzFQpwDGMMXERLCGw36u3Au3XeZNyRHpe2");
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
            token::create_account_for_token_account(token_account(mint, authority, 1_000_000)),
        );
        for destination in &destinations {
            accounts.insert(
                *destination,
                token::create_account_for_token_account(token_account(mint, *destination, 0)),
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
    fn stride_two_conditional_ata_creation_then_transfer() {
        let creator = Pubkey::new_unique();
        let authority = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        let source = Pubkey::new_unique();
        let owner_missing = Pubkey::new_unique();
        let owner_existing = Pubkey::new_unique();
        let (missing_ata, _) = associated_token::create_account_for_associated_token_account(
            token_account(mint, owner_missing, 0),
        );
        let (existing_ata, existing_account) =
            associated_token::create_account_for_associated_token_account(token_account(
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
            token::create_account_for_token_account(token_account(mint, authority, 1_000_000)),
        );
        accounts.insert(existing_ata, existing_account);
        let context = context(accounts);
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
    }

    fn context(accounts: HashMap<Pubkey, Account>) -> MolluskContext<HashMap<Pubkey, Account>> {
        let mut mollusk = Mollusk::default();
        mollusk.add_program_with_loader_and_elf(&ID, &LOADER_V3, BALLISTA_ELF);
        token::add_program(&mut mollusk);
        associated_token::add_program(&mut mollusk);
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

    fn token_account(mint: Pubkey, owner: Pubkey, amount: u64) -> TokenAccount {
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
        let fixed_accounts = if batch { 2 } else { 3 };
        let input_count = if require_guard { 2 } else { 1 };
        let register_count = input_count;
        let mut instructions = Vec::new();
        if require_guard {
            instructions.push(record(OP_LOAD_INPUT, 0, 0, 0, 0, 0));
            instructions.push(record(OP_REQUIRE, NO_INDEX, 0, 0, 0, 0));
            instructions.push(record(OP_LOAD_INPUT, 1, 1, 0, 0, 0));
        } else {
            instructions.push(record(OP_LOAD_INPUT, 0, 0, 0, 0, 0));
        }
        let amount_register = if require_guard { 1 } else { 0 };
        if batch {
            instructions.push(record(OP_FOREACH, NO_INDEX, 1, 0, 0, 0));
        }
        instructions.push(record(OP_INVOKE, NO_INDEX, 0, NO_INDEX, 0, 0));

        let header = ProgramHeader::new(
            fixed_accounts,
            u8::from(batch),
            batch_max.unwrap_or(0),
            input_count,
            register_count,
            instructions.len() as u8,
            1,
            2,
            2,
            1,
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
        let header = ProgramHeader::new(3, 1, batch_max, 1, 1, 3, 1, 3, 2, 1, 1);
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
        let header = ProgramHeader::new(7, 2, batch_max, 1, 2, 5, 2, 9, 2, 3, 1);
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
            record(OP_FOREACH, NO_INDEX, 3, 0, 0, 0),
            record(
                ballista_common::template::OP_ACCOUNT_IS_EMPTY,
                1,
                0x81,
                0,
                0,
                0,
            ),
            record(OP_INVOKE, NO_INDEX, 0, 1, 0, 0),
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
                segment_start_le: [0; 2],
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
