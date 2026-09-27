//! Upload templates with Ballista's own instructions, and build runs by the account and input
//! names the TypeScript compiler recorded in `fixtures/protocol-examples.json`.
//!
//! Binding by name is the point: hand-copied positions are how a runner once came to pass an
//! extra account.

use {
    crate::{decode_hex, tx},
    ballista_sdk::{
        ballista_common::template::{
            ProgramView, ACCOUNT_SIGNER, ACCOUNT_WRITABLE, NO_INDEX, VALUE_BOOL, VALUE_BYTES,
            VALUE_I64, VALUE_PUBKEY, VALUE_U128, VALUE_U64,
        },
        begin_template_instruction, create_template_instruction, finalize_template_instruction,
        find_template_pda, run_instruction, template_hash, write_template_chunk_instruction,
        RunInputs,
    },
    litesvm::LiteSVM,
    serde::{Deserialize, Deserializer},
    solana_address::Address,
    solana_instruction::{AccountMeta, Instruction},
    solana_keypair::Keypair,
    solana_signer::Signer,
    std::{collections::BTreeMap, fs, ops::Index},
};

/// The live-protocol examples' payloads and name orders, written by
/// `clients/js/src/protocol-examples.test.ts` (`pnpm fixtures`).
pub const EXAMPLES_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/protocol-examples.json"
);

/// Payload bytes per `write_template_chunk`. Its transaction is 209 bytes plus the chunk: one
/// signature, three keys, the blockhash, and the instruction's 5-byte header.
const WRITE_CHUNK_LEN: usize = 1_000;

/// One example: its compiled payload and the order of each kind of name, as the compiler lays
/// them out in a run.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Example {
    #[serde(deserialize_with = "hex_payload")]
    pub payload: Vec<u8>,
    pub fixed_accounts: Vec<String>,
    pub inputs: Vec<String>,
    pub row_inputs: Vec<String>,
    pub batch_accounts: Vec<String>,
    pub account_groups: Vec<String>,
}

/// Every example in the fixture, by name: `&examples()["tokenSweepIntoSwap"]`.
pub struct Examples(BTreeMap<String, Example>);

/// Reads `fixtures/protocol-examples.json`.
pub fn examples() -> Examples {
    let text = fs::read(EXAMPLES_PATH)
        .unwrap_or_else(|error| panic!("reading {EXAMPLES_PATH} failed: {error}"));
    Examples(
        serde_json::from_slice(&text)
            .unwrap_or_else(|error| panic!("parsing {EXAMPLES_PATH} failed: {error}")),
    )
}

impl Examples {
    /// Every example, by name.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Example)> {
        self.0
            .iter()
            .map(|(name, example)| (name.as_str(), example))
    }
}

impl Index<&str> for Examples {
    type Output = Example;

    fn index(&self, name: &str) -> &Example {
        self.0.get(name).unwrap_or_else(|| {
            panic!(
                "no example {name:?}; the fixture has {:?}",
                self.0.keys().collect::<Vec<_>>()
            )
        })
    }
}

fn hex_payload<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
    Ok(decode_hex(&String::deserialize(deserializer)?))
}

/// Uploads `payload` as `creator`'s template `id` and returns the template's address.
///
/// It uses Ballista's own instructions: one `create_template` when that fits in a transaction,
/// otherwise `begin_template`, a `write_template_chunk` per 1,000 bytes, and
/// `finalize_template`. The creator pays.
///
/// # Panics
///
/// If any of those transactions fails.
pub fn upload(svm: &mut LiteSVM, creator: &Keypair, id: u16, payload: &[u8]) -> Address {
    let creator_address = creator.pubkey();
    let (template, _) = find_template_pda(&creator_address, id);
    let create = create_template_instruction(creator_address, id, payload);
    let one_shot = tx::transaction(svm, creator, &[], std::slice::from_ref(&create), &[]);
    let instructions = if tx::wire_size(&one_shot) <= tx::PACKET_DATA_SIZE {
        vec![create]
    } else {
        let len = u32::try_from(payload.len()).expect("a payload is at most 10,240 bytes");
        let mut chunked = vec![begin_template_instruction(
            creator_address,
            id,
            len,
            template_hash(payload),
        )];
        for (index, chunk) in payload.chunks(WRITE_CHUNK_LEN).enumerate() {
            let offset = u32::try_from(index * WRITE_CHUNK_LEN).unwrap();
            chunked.push(write_template_chunk_instruction(
                creator_address,
                template,
                offset,
                chunk,
            ));
        }
        chunked.push(finalize_template_instruction(creator_address, template));
        chunked
    };
    for instruction in instructions {
        tx::send(svm, creator, &[], &[instruction], &[])
            .unwrap_or_else(|failure| panic!("uploading template {id} failed: {failure:?}"));
    }
    template
}

/// A run input's value. Its type must be the one the template declares.
#[derive(Clone, Copy, Debug)]
pub enum InputValue<'v> {
    Bool(bool),
    U64(u64),
    I64(i64),
    U128(u128),
    Pubkey(Address),
    /// Encoded with a little-endian `u16` length first.
    Bytes(&'v [u8]),
}

/// A `run` instruction, bound by name.
///
/// The accounts are the template, the fixed accounts in `fixedAccounts` order, each batch row's
/// accounts in `batchAccounts` order, then each account group's members in `accountGroups`
/// order. The data is one length byte per group in `accountGroups` order, the inputs in `inputs`
/// order, then each row's inputs in `rowInputs` order.
///
/// Every binding is checked against the template: a name it does not declare, a name bound
/// twice, flags other than the declared ones, an address other than a pinned one, or a value of
/// another type panics, and so does [`Run::build`] with a name left unbound.
///
/// ```text
/// let run = Run::new(template, &examples["tokenSweepIntoSwap"])
///     .account("seller", seller, true, true)
///     .input_u64("dustFloor", 1_000)
///     .group("routeAccounts", leg.instructions.swap.accounts[4..].to_vec())
///     // ... every other name ...
///     .build();
/// ```
pub struct Run<'a> {
    template: Address,
    view: ProgramView<'a>,
    accounts: Slots<'a, AccountMeta>,
    inputs: Slots<'a, Vec<u8>>,
    groups: Slots<'a, Vec<AccountMeta>>,
    rows: Vec<(Vec<AccountMeta>, Vec<Vec<u8>>)>,
    example: &'a Example,
}

/// One batch row's accounts and inputs, bound by name; see [`Run::row`].
pub struct Row<'a> {
    view: ProgramView<'a>,
    accounts: Slots<'a, AccountMeta>,
    inputs: Slots<'a, Vec<u8>>,
}

impl<'a> Run<'a> {
    /// Starts a run of the template at `template`, which holds `example`'s payload.
    ///
    /// # Panics
    ///
    /// If the payload does not parse, or its header counts differ from the fixture's name lists.
    pub fn new(template: Address, example: &'a Example) -> Self {
        let view = ProgramView::parse(&example.payload)
            .unwrap_or_else(|error| panic!("the example's payload does not parse: {error:?}"));
        let header = view.header;
        assert!(
            header.fixed_account_count() == example.fixed_accounts.len()
                && header.batch_stride() == example.batch_accounts.len()
                && header.input_count() == example.inputs.len()
                && header.row_input_count() == example.row_inputs.len()
                && header.account_group_count() == example.account_groups.len(),
            "the fixture's names do not match its payload's header; regenerate it with `pnpm fixtures`"
        );
        Run {
            template,
            view,
            accounts: Slots::new("accounts", &example.fixed_accounts),
            inputs: Slots::new("inputs", &example.inputs),
            groups: Slots::new("account groups", &example.account_groups),
            rows: Vec::new(),
            example,
        }
    }

    /// Binds a fixed account. `writable` and `signer` must be the template's declared flags; they
    /// are spelled out so a test reads plainly, and checked so they cannot drift.
    pub fn account(mut self, name: &str, address: Address, writable: bool, signer: bool) -> Self {
        let position = self.accounts.position(name);
        let meta = bind_account(&self.view, position, name, address, writable, signer);
        self.accounts.set(name, position, meta);
        self
    }

    /// Binds an input.
    pub fn input(mut self, name: &str, value: InputValue) -> Self {
        let position = self.inputs.position(name);
        let bytes = encode_input(&self.view, position, name, value);
        self.inputs.set(name, position, bytes);
        self
    }

    /// Binds an account group's members, in order. Members never sign: Ballista forwards them
    /// with the transaction's writable flag and without a signature, so each is passed as a
    /// non-signer and keeps its writable flag.
    pub fn group(mut self, name: &str, members: impl IntoIterator<Item = AccountMeta>) -> Self {
        let position = self.groups.position(name);
        let members: Vec<AccountMeta> = members
            .into_iter()
            .map(|member| AccountMeta {
                is_signer: false,
                ..member
            })
            .collect();
        assert!(
            members.len() <= usize::from(u8::MAX),
            "account group {name} has {} members; at most 255 fit its length byte",
            members.len()
        );
        self.groups.set(name, position, members);
        self
    }

    /// Adds a batch row, bound by the example's `batchAccounts` and `rowInputs` names.
    ///
    /// # Panics
    ///
    /// If the row leaves a name unbound.
    pub fn row(mut self, fill: impl FnOnce(Row<'a>) -> Row<'a>) -> Self {
        let row = fill(Row {
            view: self.view,
            accounts: Slots::new("batch accounts", &self.example.batch_accounts),
            inputs: Slots::new("row inputs", &self.example.row_inputs),
        });
        check_bound(
            &format!("in batch row {}", self.rows.len()),
            [row.accounts.unbound(), row.inputs.unbound()],
        );
        self.rows.push((row.accounts.finish(), row.inputs.finish()));
        self
    }

    /// The `run` instruction.
    ///
    /// # Panics
    ///
    /// If a name is unbound, or the row count is outside the template's batch bounds.
    pub fn build(self) -> Instruction {
        check_bound(
            "in the run",
            [
                self.accounts.unbound(),
                self.inputs.unbound(),
                self.groups.unbound(),
            ],
        );
        let header = self.view.header;
        assert!(
            (header.batch_min_iterations()..=header.batch_max_iterations())
                .contains(&self.rows.len()),
            "{} batch rows; the template takes {} to {}",
            self.rows.len(),
            header.batch_min_iterations(),
            header.batch_max_iterations()
        );
        let groups = self.groups.finish();
        let lengths: Vec<u8> = groups
            .iter()
            .map(|members| u8::try_from(members.len()).unwrap())
            .collect();

        let mut accounts = self.accounts.finish();
        let mut data = RunInputs::new().groups(&lengths).finish();
        data.extend(self.inputs.finish().concat());
        for (row_accounts, row_inputs) in self.rows {
            accounts.extend(row_accounts);
            data.extend(row_inputs.concat());
        }
        accounts.extend(groups.into_iter().flatten());
        run_instruction(self.template, accounts, &data)
    }
}

impl<'a> Row<'a> {
    /// Binds one of the row's accounts; see [`Run::account`].
    pub fn account(mut self, name: &str, address: Address, writable: bool, signer: bool) -> Self {
        let position = self.accounts.position(name);
        let constraint = self.view.header.fixed_account_count() + position;
        let meta = bind_account(&self.view, constraint, name, address, writable, signer);
        self.accounts.set(name, position, meta);
        self
    }

    /// Binds one of the row's inputs.
    pub fn input(mut self, name: &str, value: InputValue) -> Self {
        let position = self.inputs.position(name);
        let descriptor = self.view.header.input_count() + position;
        let bytes = encode_input(&self.view, descriptor, name, value);
        self.inputs.set(name, position, bytes);
        self
    }
}

/// `input_bool`, `input_u64` and the rest: [`Run::input`] and [`Row::input`] with the type named.
macro_rules! typed_inputs {
    ($builder:ident) => {
        impl $builder<'_> {
            pub fn input_bool(self, name: &str, value: bool) -> Self {
                self.input(name, InputValue::Bool(value))
            }
            pub fn input_u64(self, name: &str, value: u64) -> Self {
                self.input(name, InputValue::U64(value))
            }
            pub fn input_i64(self, name: &str, value: i64) -> Self {
                self.input(name, InputValue::I64(value))
            }
            pub fn input_u128(self, name: &str, value: u128) -> Self {
                self.input(name, InputValue::U128(value))
            }
            pub fn input_pubkey(self, name: &str, value: Address) -> Self {
                self.input(name, InputValue::Pubkey(value))
            }
            pub fn input_bytes(self, name: &str, value: &[u8]) -> Self {
                self.input(name, InputValue::Bytes(value))
            }
        }
    };
}
typed_inputs!(Run);
typed_inputs!(Row);

/// Values bound by name to the positions a fixture's name list gives them.
struct Slots<'a, T> {
    /// What the names are, in the plural: "accounts", "row inputs".
    kind: &'static str,
    names: &'a [String],
    values: Vec<Option<T>>,
}

impl<'a, T> Slots<'a, T> {
    fn new(kind: &'static str, names: &'a [String]) -> Self {
        Slots {
            kind,
            names,
            values: names.iter().map(|_| None).collect(),
        }
    }

    fn position(&self, name: &str) -> usize {
        self.names
            .iter()
            .position(|declared| declared == name)
            .unwrap_or_else(|| {
                panic!(
                    "the template has no {} named {name:?}; it declares {:?}",
                    self.kind, self.names
                )
            })
    }

    fn set(&mut self, name: &str, position: usize, value: T) {
        assert!(
            self.values[position].is_none(),
            "{name:?} is bound twice ({})",
            self.kind
        );
        self.values[position] = Some(value);
    }

    /// `accounts ["kamino", "owner"]`: the names nothing was bound to, if any.
    fn unbound(&self) -> Option<String> {
        let unbound: Vec<&String> = self
            .names
            .iter()
            .zip(&self.values)
            .filter(|(_, value)| value.is_none())
            .map(|(name, _)| name)
            .collect();
        (!unbound.is_empty()).then(|| format!("{} {unbound:?}", self.kind))
    }

    /// The values in the fixture's order, once [`check_bound`] has passed.
    fn finish(self) -> Vec<T> {
        self.values
            .into_iter()
            .map(|value| value.expect("every name is bound"))
            .collect()
    }
}

fn check_bound<const N: usize>(place: &str, unbound: [Option<String>; N]) {
    let unbound: Vec<String> = unbound.into_iter().flatten().collect();
    assert!(
        unbound.is_empty(),
        "unbound {place}: {}",
        unbound.join(", ")
    );
}

fn bind_account(
    view: &ProgramView,
    constraint: usize,
    name: &str,
    address: Address,
    writable: bool,
    signer: bool,
) -> AccountMeta {
    let constraint = &view.accounts[constraint];
    let declared = (
        constraint.flags & ACCOUNT_WRITABLE != 0,
        constraint.flags & ACCOUNT_SIGNER != 0,
    );
    assert_eq!(
        (writable, signer),
        declared,
        "account {name:?}: (writable, signer) must be the template's declared flags"
    );
    if constraint.address_index != NO_INDEX {
        let pinned =
            Address::new_from_array(view.pubkeys[usize::from(constraint.address_index)].bytes);
        assert_eq!(address, pinned, "account {name:?} is pinned to {pinned}");
    }
    AccountMeta {
        pubkey: address,
        is_signer: signer,
        is_writable: writable,
    }
}

fn encode_input(view: &ProgramView, descriptor: usize, name: &str, value: InputValue) -> Vec<u8> {
    let descriptor = &view.inputs[descriptor];
    let (value_type, encoded) = match value {
        InputValue::Bool(value) => (VALUE_BOOL, RunInputs::new().bool(value)),
        InputValue::U64(value) => (VALUE_U64, RunInputs::new().u64(value)),
        InputValue::I64(value) => (VALUE_I64, RunInputs::new().i64(value)),
        InputValue::U128(value) => (VALUE_U128, RunInputs::new().u128(value)),
        InputValue::Pubkey(value) => (VALUE_PUBKEY, RunInputs::new().pubkey(&value)),
        InputValue::Bytes(value) => {
            assert!(
                value.len() <= descriptor.max_len(),
                "input {name:?} takes at most {} bytes; given {}",
                descriptor.max_len(),
                value.len()
            );
            (VALUE_BYTES, RunInputs::new().bytes(value))
        }
    };
    assert_eq!(
        descriptor.value_type,
        value_type,
        "input {name:?} is a {}, not a {}",
        type_name(descriptor.value_type),
        type_name(value_type)
    );
    encoded.finish()
}

fn type_name(value_type: u8) -> &'static str {
    match value_type {
        VALUE_BOOL => "bool",
        VALUE_U64 => "u64",
        VALUE_I64 => "i64",
        VALUE_U128 => "u128",
        VALUE_PUBKEY => "pubkey",
        VALUE_BYTES => "bytes",
        _ => "value of an unknown type",
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::{
            snapshot,
            wallet::{fund, keypair, SOL},
        },
        ballista_sdk::{
            ballista_common::template::{
                TemplateAccount, ACCOUNT_EXECUTABLE, DATA_REG_U64, MAX_TEMPLATE_PAYLOAD_LEN,
            },
            ProgramBuilder, Segment, SYSTEM_PROGRAM_ID, TOKEN_PROGRAM_ID,
        },
    };

    const JUPITER: Address = Address::from_str_const("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4");
    const KAMINO: Address = Address::from_str_const("KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD");

    fn key(byte: u8) -> Address {
        Address::new_from_array([byte; 32])
    }

    #[test]
    fn every_example_lines_up_with_its_payload() {
        let examples = examples();
        assert_eq!(examples.iter().count(), 12);
        for (_, example) in examples.iter() {
            // `Run::new` checks the header's counts against the name lists.
            let _ = Run::new(key(1), example);
        }
    }

    /// Bound in scrambled order, the run comes out in the fixture's order; compare with the same
    /// instruction assembled by hand from `jupiterDepositExactOutput`'s current order.
    #[test]
    fn a_run_built_by_name_matches_one_built_by_hand() {
        let examples = examples();
        let example = &examples["jupiterDepositExactOutput"];
        let template = key(99);
        let owner = key(4);
        let route_args = [0xe5, 0x17, 0xcb, 0x97, 1, 2, 3];
        let route_accounts = vec![
            AccountMeta::new_readonly(JUPITER, false),
            AccountMeta::new(key(21), false),
            // Signer flags are dropped: group members never sign.
            AccountMeta::new(owner, true),
        ];

        let built = Run::new(template, example)
            .group("routeAccounts", route_accounts)
            .input_u64("minimumOut", 1_234_567)
            .account("reserveDestinationDepositCollateral", key(13), true, false)
            .account("reserveCollateralMint", key(12), true, false)
            .account("reserveLiquiditySupply", key(11), true, false)
            .account("reserve", key(10), true, false)
            .account("lendingMarketAuthority", key(9), false, false)
            .account("lendingMarket", key(8), false, false)
            .account("obligation", key(7), true, false)
            .account("destinationAta", key(6), true, false)
            .account("sourceAta", key(5), true, false)
            .account("owner", owner, true, true)
            .input_bytes("routeArgs", &route_args)
            .account("tokenProgram", TOKEN_PROGRAM_ID, false, false)
            .account("kamino", KAMINO, false, false)
            .account("jupiter", JUPITER, false, false)
            .build();

        let mut data = vec![5, 3]; // `run`, then routeAccounts' length
        data.extend_from_slice(&7u16.to_le_bytes());
        data.extend_from_slice(&route_args);
        data.extend_from_slice(&1_234_567u64.to_le_bytes());
        let by_hand = Instruction {
            program_id: ballista_sdk::ID,
            accounts: vec![
                AccountMeta::new_readonly(template, false),
                AccountMeta::new_readonly(JUPITER, false),
                AccountMeta::new_readonly(KAMINO, false),
                AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
                AccountMeta::new(owner, true),
                AccountMeta::new(key(5), false),
                AccountMeta::new(key(6), false),
                AccountMeta::new(key(7), false),
                AccountMeta::new_readonly(key(8), false),
                AccountMeta::new_readonly(key(9), false),
                AccountMeta::new(key(10), false),
                AccountMeta::new(key(11), false),
                AccountMeta::new(key(12), false),
                AccountMeta::new(key(13), false),
                AccountMeta::new_readonly(JUPITER, false),
                AccountMeta::new(key(21), false),
                AccountMeta::new(owner, false),
            ],
            data,
        };
        assert_eq!(built, by_hand);
    }

    fn deposit_run(example: &Example) -> Run<'_> {
        Run::new(key(99), example)
    }

    #[test]
    #[should_panic(expected = "the template has no accounts named \"payer\"")]
    fn an_unknown_name_panics() {
        let examples = examples();
        let _ = deposit_run(&examples["jupiterDepositExactOutput"]).account(
            "payer",
            key(1),
            true,
            true,
        );
    }

    #[test]
    #[should_panic(
        expected = "unbound in the run: accounts [\"kamino\", \"tokenProgram\", \"owner\", \"sourceAta\", \"destinationAta\", \"obligation\", \"lendingMarket\", \"lendingMarketAuthority\", \"reserve\", \"reserveLiquiditySupply\", \"reserveCollateralMint\", \"reserveDestinationDepositCollateral\"], inputs [\"routeArgs\", \"minimumOut\"], account groups [\"routeAccounts\"]"
    )]
    fn a_missing_name_panics() {
        let examples = examples();
        let _ = deposit_run(&examples["jupiterDepositExactOutput"])
            .account("jupiter", JUPITER, false, false)
            .build();
    }

    #[test]
    #[should_panic(expected = "must be the template's declared flags")]
    fn flags_other_than_the_declared_ones_panic() {
        let examples = examples();
        let _ = deposit_run(&examples["jupiterDepositExactOutput"]).account(
            "owner",
            key(4),
            true,
            false,
        );
    }

    #[test]
    #[should_panic(expected = "is pinned to JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4")]
    fn another_address_for_a_pinned_account_panics() {
        let examples = examples();
        let _ = deposit_run(&examples["jupiterDepositExactOutput"]).account(
            "jupiter",
            key(1),
            false,
            false,
        );
    }

    #[test]
    #[should_panic(expected = "input \"minimumOut\" is a u64, not a i64")]
    fn a_value_of_another_type_panics() {
        let examples = examples();
        let _ = deposit_run(&examples["jupiterDepositExactOutput"]).input_i64("minimumOut", 1);
    }

    #[test]
    #[should_panic(expected = "\"minimumOut\" is bound twice (inputs)")]
    fn a_name_bound_twice_panics() {
        let examples = examples();
        let _ = deposit_run(&examples["jupiterDepositExactOutput"])
            .input_u64("minimumOut", 1)
            .input_u64("minimumOut", 2);
    }

    /// The meta the template declares for account record `constraint`: its flags, and its pinned
    /// address or else `address`.
    fn declared(view: &ProgramView, constraint: usize, address: Address) -> AccountMeta {
        let constraint = &view.accounts[constraint];
        let pubkey = match constraint.address_index {
            NO_INDEX => address,
            index => Address::new_from_array(view.pubkeys[usize::from(index)].bytes),
        };
        AccountMeta {
            pubkey,
            is_signer: constraint.flags & ACCOUNT_SIGNER != 0,
            is_writable: constraint.flags & ACCOUNT_WRITABLE != 0,
        }
    }

    /// Batch rows follow the fixed accounts, and row accounts go in `batchAccounts` order.
    #[test]
    fn batch_rows_follow_the_fixed_accounts() {
        let examples = examples();
        let example = &examples["orcaHarvestManyPositions"];
        let view = ProgramView::parse(&example.payload).unwrap();
        let fixed = example.fixed_accounts.len();
        let mut run = Run::new(key(99), example);
        let mut expected = vec![AccountMeta::new_readonly(key(99), false)];
        for (position, name) in example.fixed_accounts.iter().enumerate() {
            let meta = declared(&view, position, key(position as u8));
            run = run.account(name, meta.pubkey, meta.is_writable, meta.is_signer);
            expected.push(meta);
        }
        for row in 0..2 {
            let metas: Vec<AccountMeta> = (0..example.batch_accounts.len())
                .map(|at| declared(&view, fixed + at, key(50 + 10 * row + at as u8)))
                .collect();
            run = run.row(|mut fill| {
                for (name, meta) in example.batch_accounts.iter().zip(&metas).rev() {
                    fill = fill.account(name, meta.pubkey, meta.is_writable, meta.is_signer);
                }
                fill
            });
            expected.extend(metas);
        }
        let built = run.input_u64("dustFloor", 9).build();
        assert_eq!(built.accounts, expected);
        assert_eq!(built.data, [&[5][..], &9u64.to_le_bytes()].concat());
    }

    /// A template too large for one transaction goes up in chunks and runs.
    #[test]
    fn a_large_template_uploads_in_chunks() {
        let mut builder = ProgramBuilder::new();
        let system = builder.account(
            ACCOUNT_EXECUTABLE,
            Some(SYSTEM_PROGRAM_ID.to_bytes()),
            None,
            0,
        );
        let from = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        let to = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        let lamports = builder.input(VALUE_U64, 0);
        let amount = builder.load_input(lamports);
        let transfer = builder.blob(&[2, 0, 0, 0]);
        // Unused bytes, there only to make the payload too large for one transaction.
        builder.blob(&[0xab; 2_500]);
        let cpi = builder.cpi(
            system,
            &[
                (from, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
                (to, ACCOUNT_WRITABLE),
            ],
            &[
                Segment::Literal(transfer),
                Segment::Register(DATA_REG_U64, amount),
            ],
        );
        builder.invoke(cpi, None);
        let payload = builder.build().unwrap();
        assert!(payload.len() > 2_500 && payload.len() <= MAX_TEMPLATE_PAYLOAD_LEN);

        let mut svm = LiteSVM::new();
        snapshot::add_ballista(&mut svm);
        let creator = keypair(b"ballista-protocol-tests-creator2");
        let destination = keypair(b"ballista-protocol-tests-receiver").pubkey();
        fund(&mut svm, &creator.pubkey(), 10 * SOL);
        fund(&mut svm, &destination, SOL);
        let template = upload(&mut svm, &creator, 7, &payload);

        let stored = svm.get_account(&template).unwrap();
        let account = TemplateAccount::parse(&stored.data).unwrap();
        assert_eq!(account.payload(), payload.as_slice());
        assert!(account.finalized_program().is_ok());

        let run = run_instruction(
            template,
            vec![
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
                AccountMeta::new(creator.pubkey(), true),
                AccountMeta::new(destination, false),
            ],
            &RunInputs::new().u64(1_000).finish(),
        );
        tx::send(&mut svm, &creator, &[], &[run], &[]).unwrap();
        assert_eq!(svm.get_balance(&destination), Some(SOL + 1_000));
    }
}
