//! The committed one-slot mainnet snapshot: load it, check every hash, and build a LiteSVM from
//! it with Ballista built from source.
//!
//! `snapshot/` holds `manifest.json` (slot, clock, and each program's and account's hash),
//! `accounts.json` (every account's data), `routes.json` (Jupiter's routes, by name) and
//! `programs/*.so` (program binaries, in Git LFS). `scripts/snapshot/snapshot.mjs` writes it.

use {
    crate::{decode_hex, tx, wallet},
    base64::{engine::general_purpose::STANDARD as BASE64, Engine},
    litesvm::LiteSVM,
    serde::{de::DeserializeOwned, Deserialize},
    sha2::{Digest, Sha256},
    solana_account::Account,
    solana_address::Address,
    solana_clock::Clock,
    solana_epoch_schedule::EpochSchedule,
    solana_instruction::{AccountMeta, Instruction},
    solana_message::AddressLookupTableAccount,
    solana_sdk_ids::{address_lookup_table, native_loader, sysvar},
    solana_signer::Signer,
    std::{
        collections::{BTreeMap, HashMap},
        fs,
        path::Path,
        str::FromStr,
    },
};

/// The committed snapshot, `tests/protocols/snapshot`.
pub const SNAPSHOT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/snapshot");

/// Ballista built from source, by `cargo build-sbf --manifest-path programs/ballista/Cargo.toml`.
pub const BALLISTA_SO: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/deploy/ballista.so"
);

/// The snapshot format this loader reads.
const FORMAT: u32 = 1;
/// A lookup table's header before its addresses (`LOOKUP_TABLE_META_SIZE`).
const LOOKUP_TABLE_META_LEN: usize = 56;

/// Mainnet state at one slot, checked against its manifest.
#[derive(Clone)]
pub struct Snapshot {
    /// The slot every account was read at.
    pub slot: u64,
    /// The Clock sysvar as it was at that slot.
    pub clock: Clock,
    accounts: Vec<(Address, Account)>,
    /// The manifest's names for accounts and programs: `pythSolUsd`, `jupiter`.
    names: BTreeMap<String, Address>,
    lookup_tables: BTreeMap<Address, AddressLookupTableAccount>,
    routes: BTreeMap<String, Route>,
}

/// A swap Jupiter's API quoted at snapshot time: one leg per `route` instruction.
#[derive(Clone, Debug)]
pub struct Route {
    pub description: String,
    pub legs: Vec<Leg>,
}

/// One quote and the instructions the Swap API returned for it, built for the test wallet
/// ([`wallet::wallet`]).
#[derive(Clone, Debug)]
pub struct Leg {
    pub input_mint: Address,
    pub output_mint: Address,
    /// The quote's input.
    pub in_amount: u64,
    /// The quote's expected output. Assert against [`Leg::other_amount_threshold`] or the replayed
    /// fill instead: the snapshot is a slot or two after the quote.
    pub out_amount: u64,
    /// The least `route` accepts after slippage.
    pub other_amount_threshold: u64,
    pub slippage_bps: u16,
    pub instructions: LegInstructions,
    /// `route`'s arguments, split the way templates take them.
    pub route: RouteArgs,
    /// The tables Jupiter's transaction looks keys up in, decoded from the snapshot.
    pub lookup_tables: Vec<AddressLookupTableAccount>,
    /// The wallet's token account the route sells from.
    pub source_token_account: Address,
    /// The wallet's token account the route pays into.
    pub destination_token_account: Address,
    /// The wire size of Jupiter's own transaction (every instruction, the wallet signing, these
    /// tables), as the snapshot tool computed it.
    pub transaction_size: usize,
}

/// A leg's instructions, grouped as the Swap API returns them.
#[derive(Clone, Debug)]
pub struct LegInstructions {
    pub compute_budget: Vec<Instruction>,
    /// Creates the token accounts and, when selling SOL, wraps it.
    pub setup: Vec<Instruction>,
    pub token_ledger: Option<Instruction>,
    /// Jupiter's `route`.
    pub swap: Instruction,
    /// Unwraps SOL, when the route bought or sold it.
    pub cleanup: Option<Instruction>,
    pub other: Vec<Instruction>,
}

/// `route`'s instruction data after the discriminator, whole and in parts.
#[derive(Clone, Debug)]
pub struct RouteArgs {
    /// Everything after the discriminator: the plan, then a 19-byte tail of the four fields below.
    pub args: Vec<u8>,
    /// The Borsh `route_plan`: a `u32` step count, then the steps. Opaque: its `Swap` enum grows.
    pub route_plan: Vec<u8>,
    pub in_amount: u64,
    pub quoted_out_amount: u64,
    pub slippage_bps: u16,
    pub platform_fee_bps: u8,
}

impl Snapshot {
    /// Reads a snapshot directory and checks it: every program file against its `elfSha256`, and
    /// every account, rebuilt from its data, its ELF and its zero padding, against its `sha256`.
    ///
    /// # Panics
    ///
    /// If a file is missing or malformed, a hash differs, an account is missing or repeated, a
    /// program file is still a Git LFS pointer, the snapshot holds a builtin or a sysvar, or a
    /// lookup table is not fully usable at the snapshot's slot.
    pub fn load(dir: impl AsRef<Path>) -> Snapshot {
        let dir = dir.as_ref();
        let manifest: ManifestFile = read_json(&dir.join("manifest.json"));
        let stored: Vec<StoredAccount> = read_json(&dir.join("accounts.json"));
        let routes: RoutesFile = read_json(&dir.join("routes.json"));
        assert!(
            manifest.format == FORMAT && routes.format == FORMAT,
            "this loader reads snapshot format {FORMAT}"
        );

        let clock = manifest.clock.to_clock();
        assert!(
            clock.slot == manifest.slot
                && routes.slot == manifest.slot
                && clock.unix_timestamp == manifest.unix_timestamp,
            "the manifest's clock, its slot and the routes' slot disagree"
        );
        let test_wallet = wallet::wallet().pubkey();
        for recorded in [&manifest.wallet.address, &routes.wallet.address] {
            assert_eq!(
                parse_address(recorded),
                test_wallet,
                "the routes were built for another wallet than wallet::WALLET_SEED's"
            );
        }

        let programs: HashMap<&str, Vec<u8>> = manifest
            .programs
            .iter()
            .map(|program| (program.file.as_str(), read_program(dir, program)))
            .collect();

        // Each account in accounts.json is checked off the manifest's list, so one listed twice
        // cannot stand in for one that is missing.
        let mut expected: HashMap<&str, &ManifestAccount> = manifest
            .accounts
            .iter()
            .map(|account| (account.address.as_str(), account))
            .collect();
        assert_eq!(
            expected.len(),
            manifest.accounts.len(),
            "the manifest lists an account twice"
        );
        let mut accounts = Vec::with_capacity(stored.len());
        for entry in &stored {
            let recorded = expected.remove(entry.address.as_str()).unwrap_or_else(|| {
                panic!(
                    "{} is in accounts.json twice, or is not in the manifest",
                    entry.address
                )
            });
            let account = entry.rebuild(&programs);
            check_account(entry, recorded, &account);
            accounts.push((parse_address(&entry.address), account));
        }
        assert!(
            expected.is_empty(),
            "accounts.json lacks {:?}",
            expected.keys().collect::<Vec<_>>()
        );

        let mut names = BTreeMap::new();
        for account in &manifest.accounts {
            // `account pythSolUsd` and `program jupiter` name an account. The other roles
            // (`route solToUsdc`, `programData jupiter`, `lookup table, …`) say why it is here.
            for role in &account.roles {
                let name = role
                    .strip_prefix("account ")
                    .or_else(|| role.strip_prefix("program "));
                if let Some(name) = name {
                    let previous = names.insert(name.to_string(), parse_address(&account.address));
                    assert!(previous.is_none(), "two accounts are named {name:?}");
                }
            }
        }

        let lookup_tables: BTreeMap<Address, AddressLookupTableAccount> = accounts
            .iter()
            .filter(|(_, account)| account.owner == address_lookup_table::ID)
            .map(|(address, account)| {
                (
                    *address,
                    decode_lookup_table(*address, account, manifest.slot),
                )
            })
            .collect();
        assert_eq!(
            lookup_tables.len(),
            manifest.lookup_tables.len(),
            "the manifest lists another set of lookup tables"
        );
        for table in &manifest.lookup_tables {
            let decoded = lookup_tables
                .get(&parse_address(&table.address))
                .unwrap_or_else(|| {
                    panic!("lookup table {} is not in accounts.json", table.address)
                });
            assert_eq!(
                decoded.addresses.len(),
                table.entries,
                "lookup table {} has another entry count than the manifest's",
                table.address
            );
        }

        let routes = routes
            .routes
            .into_iter()
            .map(|(name, route)| {
                let route = route.decode(&name, &lookup_tables);
                (name, route)
            })
            .collect();

        Snapshot {
            slot: manifest.slot,
            clock,
            accounts,
            names,
            lookup_tables,
            routes,
        }
    }

    /// A LiteSVM holding the snapshot, with Ballista built from source.
    ///
    /// - The clock is the snapshot's own, so oracles read as fresh and pools accept the time. The
    ///   epoch schedule is mainnet's, which has no warmup, so it agrees with the clock's epoch.
    /// - Accounts are written with ProgramData before Program: writing a Program account compiles
    ///   its ELF from its ProgramData, and without one it is cached as "not deployed". Mainnet's
    ///   Token program (p-token) replaces the build LiteSVM bundles.
    /// - Logs are not truncated, so a failure deep in a route is still in them.
    ///
    /// # Panics
    ///
    /// If a program fails to load or is left cached as not deployed, or `ballista.so` has not been
    /// built.
    pub fn into_svm(self) -> LiteSVM {
        let mut svm = LiteSVM::new().with_log_bytes_limit(None);
        // Before the programs: each is deployed as of the current slot.
        svm.set_sysvar(&self.clock);
        svm.set_sysvar(&EpochSchedule::without_warmup());
        let mut accounts = self.accounts;
        accounts.sort_by_key(|(_, account)| account.executable);
        let mut programs = vec![ballista_sdk::ID];
        for (address, account) in accounts {
            if account.executable {
                programs.push(address);
            }
            svm.set_account(address, account)
                .unwrap_or_else(|error| panic!("loading {address} failed: {error:?}"));
        }
        add_ballista(&mut svm);
        // LiteSVM reports a tombstone only when the program is called: "Program is not deployed".
        for program in programs {
            let deployed = svm
                .accounts_db()
                .programs_cache
                .find(&program)
                .is_some_and(|entry| !entry.is_tombstone());
            assert!(
                deployed,
                "program {program} is cached as not deployed; its ProgramData must be written first"
            );
        }
        svm
    }

    /// [`Snapshot::into_svm`] for a snapshot that is used again, say for a fresh SVM per case.
    pub fn svm(&self) -> LiteSVM {
        self.clone().into_svm()
    }

    /// The account or program the manifest names `name`: `pythSolUsd`, `jitoTip`, `usdcMint`,
    /// `wsolMint`, or a program such as `jupiter` or `token`.
    ///
    /// # Panics
    ///
    /// If the manifest names nothing so.
    pub fn named(&self, name: &str) -> Address {
        *self.names.get(name).unwrap_or_else(|| {
            panic!(
                "no account named {name:?}; the manifest names {:?}",
                self.names.keys().collect::<Vec<_>>()
            )
        })
    }

    /// Every address lookup table in the snapshot, decoded.
    pub fn lookup_tables(&self) -> Vec<AddressLookupTableAccount> {
        self.lookup_tables.values().cloned().collect()
    }

    /// The route recorded under `name` in `routes.json`.
    ///
    /// # Panics
    ///
    /// If there is none.
    pub fn route(&self, name: &str) -> &Route {
        self.routes.get(name).unwrap_or_else(|| {
            panic!(
                "no route {name:?}; the snapshot has {:?}",
                self.routes.keys().collect::<Vec<_>>()
            )
        })
    }

    /// Every route, by name.
    pub fn routes(&self) -> impl Iterator<Item = (&str, &Route)> {
        self.routes
            .iter()
            .map(|(name, route)| (name.as_str(), route))
    }

    /// An account as the snapshot holds it.
    pub fn account(&self, address: &Address) -> Option<&Account> {
        self.accounts
            .iter()
            .find(|(stored, _)| stored == address)
            .map(|(_, account)| account)
    }
}

impl Route {
    /// Every leg's lookup tables, each once, in leg order.
    pub fn lookup_tables(&self) -> Vec<AddressLookupTableAccount> {
        let mut tables: Vec<AddressLookupTableAccount> = Vec::new();
        for table in self.legs.iter().flat_map(|leg| &leg.lookup_tables) {
            if tables.iter().all(|kept| kept.key != table.key) {
                tables.push(table.clone());
            }
        }
        tables
    }
}

impl LegInstructions {
    /// Jupiter's own transaction with `route` replaced by `run`, a Ballista run that carries it:
    /// the compute budget, the setup that creates the token accounts and wraps SOL, `run`, then
    /// the cleanup that unwraps it. A token-ledger or other instruction, which the snapshot's
    /// routes have none of, would keep its place.
    pub fn with_swap(&self, run: Instruction) -> Vec<Instruction> {
        let mut instructions = self.compute_budget.clone();
        instructions.extend(self.setup.iter().cloned());
        instructions.extend(self.token_ledger.clone());
        instructions.push(run);
        instructions.extend(self.cleanup.clone());
        instructions.extend(self.other.iter().cloned());
        instructions
    }

    /// Every instruction, in the order of Jupiter's own transaction.
    pub fn all(&self) -> Vec<Instruction> {
        self.with_swap(self.swap.clone())
    }
}

/// The accounts at the head of a route's own instruction that a template passes itself: the token
/// program, the wallet, and the two token accounts it measures. The rest arrive as the run's
/// `routeAccounts` group.
pub const ROUTE_HEAD: usize = 4;

/// Where a route sells from and pays to.
pub struct Routing {
    /// At `sourceAta`: the account the template reads as sold from.
    pub source: Address,
    /// At `destinationAta`: the account the template measures the proceeds or fill in.
    pub destination: Address,
    /// The route's accounts after [`ROUTE_HEAD`], forwarded as `routeAccounts`. They name the
    /// accounts each step moves, which Jupiter does not tie to the two above.
    pub steps: Vec<AccountMeta>,
}

impl Routing {
    /// The route as the Swap API built it: the wallet's own accounts throughout.
    pub fn of(leg: &Leg) -> Routing {
        Routing {
            source: leg.source_token_account,
            destination: leg.destination_token_account,
            steps: leg.instructions.swap.accounts[ROUTE_HEAD..].to_vec(),
        }
    }

    /// The route with its step paying `account` in place of the wallet's destination account, the
    /// one place the steps name it.
    ///
    /// # Panics
    ///
    /// If the route's steps do not name the destination account exactly once.
    pub fn paying(leg: &Leg, account: Address) -> Routing {
        let mut routing = Routing::of(leg);
        let outputs: Vec<&mut AccountMeta> = routing
            .steps
            .iter_mut()
            .filter(|meta| meta.pubkey == leg.destination_token_account)
            .collect();
        assert_eq!(
            outputs.len(),
            1,
            "a route's steps should name its destination account exactly once, as its output"
        );
        for output in outputs {
            output.pubkey = account;
        }
        routing
    }
}

/// Whether `jupiter` was invoked at all in a failed run: a requirement that fires before the route
/// runs stops the run before this is true.
pub fn jupiter_ran(jupiter: &Address, failure: &tx::Failure) -> bool {
    let invoked = format!("Program {jupiter} invoke");
    failure.logs.iter().any(|line| line.starts_with(&invoked))
}

/// Loads Ballista, built from source, at its declared ID.
///
/// # Panics
///
/// If `ballista.so` has not been built.
pub fn add_ballista(svm: &mut LiteSVM) {
    svm.add_program_from_file(ballista_sdk::ID, BALLISTA_SO)
        .unwrap_or_else(|error| {
            panic!(
                "loading {BALLISTA_SO} failed ({error:?}); build it with \
                 `cargo build-sbf --manifest-path programs/ballista/Cargo.toml`"
            )
        });
}

/// Moves the clock forward by `slots` and `seconds`, never back: Orca, for one, stamps its pools
/// with the time and rejects an earlier one. The epoch fields stay as they are.
pub fn warp(svm: &mut LiteSVM, slots: u64, seconds: u64) {
    let mut clock = svm.get_sysvar::<Clock>();
    clock.slot = clock.slot.checked_add(slots).expect("the slot overflows");
    clock.unix_timestamp = i64::try_from(seconds)
        .ok()
        .and_then(|seconds| clock.unix_timestamp.checked_add(seconds))
        .expect("the time overflows");
    svm.set_sysvar(&clock);
}

fn read_json<T: DeserializeOwned>(path: &Path) -> T {
    let text = fs::read(path).unwrap_or_else(|error| panic!("reading {path:?} failed: {error}"));
    serde_json::from_slice(&text).unwrap_or_else(|error| panic!("parsing {path:?} failed: {error}"))
}

/// A program's ELF, checked against the manifest.
fn read_program(dir: &Path, program: &ManifestProgram) -> Vec<u8> {
    let path = dir.join(&program.file);
    let elf = fs::read(&path).unwrap_or_else(|error| panic!("reading {path:?} failed: {error}"));
    assert!(
        !elf.starts_with(b"version https://git-lfs"),
        "{path:?} is a Git LFS pointer, not the program; run `git lfs pull`"
    );
    assert!(
        elf.len() == program.elf_length && sha256_matches(&elf, &program.elf_sha256),
        "{path:?} ({}) does not match the manifest's elfSha256",
        program.name
    );
    elf
}

fn check_account(entry: &StoredAccount, recorded: &ManifestAccount, account: &Account) {
    let address = &entry.address;
    // LiteSVM supplies these itself, and builds the instructions sysvar per transaction.
    assert!(
        account.owner != native_loader::ID && account.owner != sysvar::ID,
        "{address} is a builtin or a sysvar, which a snapshot must not hold"
    );
    assert!(
        entry.owner == recorded.owner
            && entry.lamports == recorded.lamports
            && entry.executable == recorded.executable
            && entry.rent_epoch == recorded.rent_epoch
            && entry.data_length == recorded.data_length,
        "{address}: accounts.json and the manifest disagree on the account's fields"
    );
    assert!(
        sha256_matches(&account.data, &recorded.sha256),
        "{address}: the rebuilt data does not match the manifest's sha256"
    );
}

fn sha256_matches(bytes: &[u8], expected_hex: &str) -> bool {
    Sha256::digest(bytes).as_slice() == decode_hex(expected_hex)
}

/// A lookup table's addresses, all of them usable at `slot`.
fn decode_lookup_table(key: Address, account: &Account, slot: u64) -> AddressLookupTableAccount {
    let data = &account.data;
    assert!(
        data.len() >= LOOKUP_TABLE_META_LEN && data[..4] == 1u32.to_le_bytes(),
        "{key} is not a lookup table"
    );
    // A deactivating table resolves through SlotHashes, and LiteSVM's single entry treats it as
    // closed.
    assert_eq!(
        data[4..12],
        u64::MAX.to_le_bytes(),
        "lookup table {key} is deactivating"
    );
    // Entries added in the current slot are not usable until the next one: only those before
    // `last_extended_slot_start_index` are. The snapshot tool refuses such a table, and so does
    // this, so every address decoded here resolves.
    let last_extended_slot = u64::from_le_bytes(data[12..20].try_into().unwrap());
    assert!(
        last_extended_slot < slot,
        "lookup table {key} was extended at slot {last_extended_slot}, not before the snapshot's {slot}"
    );
    let (entries, rest) = data[LOOKUP_TABLE_META_LEN..].as_chunks::<32>();
    assert!(
        rest.is_empty(),
        "lookup table {key} ends in a partial address"
    );
    let addresses = entries
        .iter()
        .map(|entry| Address::new_from_array(*entry))
        .collect();
    AddressLookupTableAccount { key, addresses }
}

fn parse_address(text: &str) -> Address {
    Address::from_str(text).unwrap_or_else(|error| panic!("{text:?} is not an address: {error:?}"))
}

fn decode_base64(text: &str) -> Vec<u8> {
    BASE64
        .decode(text)
        .unwrap_or_else(|error| panic!("invalid base64 in the snapshot: {error}"))
}

// ------------------------------------------------------------------------------ the JSON files

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestFile {
    format: u32,
    slot: u64,
    unix_timestamp: i64,
    clock: ClockEntry,
    wallet: WalletEntry,
    programs: Vec<ManifestProgram>,
    accounts: Vec<ManifestAccount>,
    lookup_tables: Vec<ManifestLookupTable>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClockEntry {
    slot: u64,
    epoch_start_timestamp: i64,
    epoch: u64,
    leader_schedule_epoch: u64,
    unix_timestamp: i64,
}

impl ClockEntry {
    fn to_clock(&self) -> Clock {
        Clock {
            slot: self.slot,
            epoch_start_timestamp: self.epoch_start_timestamp,
            epoch: self.epoch,
            leader_schedule_epoch: self.leader_schedule_epoch,
            unix_timestamp: self.unix_timestamp,
        }
    }
}

#[derive(Deserialize)]
struct WalletEntry {
    address: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestProgram {
    name: String,
    file: String,
    elf_length: usize,
    elf_sha256: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestAccount {
    address: String,
    /// Why the account is in the snapshot: `account pythSolUsd`, `program jupiter`,
    /// `route solToUsdc`.
    roles: Vec<String>,
    owner: String,
    lamports: u64,
    executable: bool,
    rent_epoch: u64,
    data_length: usize,
    sha256: String,
}

#[derive(Deserialize)]
struct ManifestLookupTable {
    address: String,
    entries: usize,
}

/// An `accounts.json` entry. The account's data is `data`, then the `elf` file if there is one,
/// then zeros up to `dataLength`: only ProgramData accounts and loader-v2 programs have an ELF.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredAccount {
    address: String,
    owner: String,
    lamports: u64,
    executable: bool,
    rent_epoch: u64,
    data_length: usize,
    data: String,
    elf: Option<String>,
}

impl StoredAccount {
    fn rebuild(&self, programs: &HashMap<&str, Vec<u8>>) -> Account {
        let mut data = decode_base64(&self.data);
        if let Some(elf) = &self.elf {
            let elf = programs.get(elf.as_str()).unwrap_or_else(|| {
                panic!(
                    "{}'s ELF {elf} is not a program in the manifest",
                    self.address
                )
            });
            data.extend_from_slice(elf);
        }
        assert!(
            data.len() <= self.data_length,
            "{}'s data is longer than its dataLength",
            self.address
        );
        data.resize(self.data_length, 0);
        Account {
            lamports: self.lamports,
            data,
            owner: parse_address(&self.owner),
            executable: self.executable,
            rent_epoch: self.rent_epoch,
        }
    }
}

#[derive(Deserialize)]
struct RoutesFile {
    format: u32,
    slot: u64,
    wallet: WalletEntry,
    routes: BTreeMap<String, RouteEntry>,
}

#[derive(Deserialize)]
struct RouteEntry {
    description: String,
    legs: Vec<LegEntry>,
}

impl RouteEntry {
    fn decode(self, name: &str, tables: &BTreeMap<Address, AddressLookupTableAccount>) -> Route {
        let legs = self
            .legs
            .iter()
            .map(|leg| leg.decode(name, tables))
            .collect();
        Route {
            description: self.description,
            legs,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegEntry {
    input_mint: String,
    output_mint: String,
    in_amount: u64,
    out_amount: u64,
    other_amount_threshold: u64,
    slippage_bps: u16,
    address_lookup_table_addresses: Vec<String>,
    instructions: InstructionsEntry,
    route: RouteArgsEntry,
    transaction: TransactionEntry,
    wallet_accounts: WalletAccountsEntry,
}

impl LegEntry {
    fn decode(&self, name: &str, tables: &BTreeMap<Address, AddressLookupTableAccount>) -> Leg {
        let instructions = self.instructions.decode();
        let route = self.route.decode();
        assert_eq!(
            instructions.swap.data.get(8..),
            Some(route.args.as_slice()),
            "route {name}: argsHex is not the swap instruction's data after its discriminator"
        );
        let lookup_tables =
            self.address_lookup_table_addresses
                .iter()
                .map(|address| {
                    tables.get(&parse_address(address)).cloned().unwrap_or_else(|| {
                    panic!("route {name} uses lookup table {address}, which the snapshot lacks")
                })
                })
                .collect();
        Leg {
            input_mint: parse_address(&self.input_mint),
            output_mint: parse_address(&self.output_mint),
            in_amount: self.in_amount,
            out_amount: self.out_amount,
            other_amount_threshold: self.other_amount_threshold,
            slippage_bps: self.slippage_bps,
            instructions,
            route,
            lookup_tables,
            source_token_account: parse_address(&self.wallet_accounts.source_token_account),
            destination_token_account: parse_address(
                &self.wallet_accounts.destination_token_account,
            ),
            transaction_size: self.transaction.bytes,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstructionsEntry {
    compute_budget: Vec<InstructionEntry>,
    setup: Vec<InstructionEntry>,
    token_ledger: Option<InstructionEntry>,
    swap: InstructionEntry,
    cleanup: Option<InstructionEntry>,
    other: Vec<InstructionEntry>,
}

impl InstructionsEntry {
    fn decode(&self) -> LegInstructions {
        let all =
            |entries: &[InstructionEntry]| entries.iter().map(InstructionEntry::decode).collect();
        LegInstructions {
            compute_budget: all(&self.compute_budget),
            setup: all(&self.setup),
            token_ledger: self.token_ledger.as_ref().map(InstructionEntry::decode),
            swap: self.swap.decode(),
            cleanup: self.cleanup.as_ref().map(InstructionEntry::decode),
            other: all(&self.other),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstructionEntry {
    program_id: String,
    accounts: Vec<AccountMetaEntry>,
    data: String,
}

impl InstructionEntry {
    fn decode(&self) -> Instruction {
        Instruction {
            program_id: parse_address(&self.program_id),
            accounts: self
                .accounts
                .iter()
                .map(|meta| AccountMeta {
                    pubkey: parse_address(&meta.pubkey),
                    is_signer: meta.is_signer,
                    is_writable: meta.is_writable,
                })
                .collect(),
            data: decode_base64(&self.data),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountMetaEntry {
    pubkey: String,
    is_signer: bool,
    is_writable: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RouteArgsEntry {
    args_hex: String,
    route_plan_hex: String,
    in_amount: u64,
    quoted_out_amount: u64,
    slippage_bps: u16,
    platform_fee_bps: u8,
}

impl RouteArgsEntry {
    fn decode(&self) -> RouteArgs {
        RouteArgs {
            args: decode_hex(&self.args_hex),
            route_plan: decode_hex(&self.route_plan_hex),
            in_amount: self.in_amount,
            quoted_out_amount: self.quoted_out_amount,
            slippage_bps: self.slippage_bps,
            platform_fee_bps: self.platform_fee_bps,
        }
    }
}

#[derive(Deserialize)]
struct TransactionEntry {
    bytes: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WalletAccountsEntry {
    source_token_account: String,
    destination_token_account: String,
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::{
            tx,
            wallet::{fund, token_account, token_balance, SOL, WSOL_MINT},
        },
        ballista_sdk::{ASSOCIATED_TOKEN_PROGRAM_ID, SYSTEM_PROGRAM_ID, TOKEN_PROGRAM_ID},
        solana_compute_budget_interface::ComputeBudgetInstruction,
        solana_sdk_ids::bpf_loader_upgradeable,
    };

    fn program_data_address(program: &Address) -> Address {
        Address::find_program_address(&[program.as_ref()], &bpf_loader_upgradeable::ID).0
    }

    #[test]
    fn the_svm_holds_the_snapshot_at_its_slot() {
        let snapshot = Snapshot::load(SNAPSHOT_DIR);
        let expected = snapshot.accounts.clone();
        let clock = snapshot.clock.clone();
        assert_eq!(clock.slot, snapshot.slot);
        let svm = snapshot.into_svm();

        assert_eq!(svm.get_sysvar::<Clock>(), clock);
        let schedule = svm.get_sysvar::<EpochSchedule>();
        assert_eq!(schedule.get_epoch(clock.slot), clock.epoch);
        assert_eq!(
            schedule.get_leader_schedule_epoch(clock.slot),
            clock.leader_schedule_epoch
        );
        for (address, account) in &expected {
            assert_eq!(
                svm.get_account(address).as_ref(),
                Some(account),
                "{address}"
            );
        }
        // Ballista is deployed as of the snapshot's slot too: its ProgramData records the slot.
        let ballista = svm
            .get_account(&program_data_address(&ballista_sdk::ID))
            .unwrap();
        assert_eq!(ballista.data[4..12], clock.slot.to_le_bytes());
    }

    /// LiteSVM bundles a p-token build of its own at the Token program's address, upgradeable too
    /// and with its ProgramData at the same derived address, so the owner and the ProgramData
    /// address cannot tell the two apart. The bytes can.
    #[test]
    fn mainnet_token_replaces_the_build_litesvm_bundles() {
        let snapshot = Snapshot::load(SNAPSHOT_DIR);
        let program_data = program_data_address(&TOKEN_PROGRAM_ID);
        let mainnet = [TOKEN_PROGRAM_ID, program_data]
            .map(|address| snapshot.account(&address).unwrap().clone());
        let bundled = LiteSVM::new().get_account(&program_data).unwrap();
        assert_ne!(
            bundled.data, mainnet[1].data,
            "LiteSVM now bundles mainnet's build, so this test shows nothing"
        );

        let svm = snapshot.into_svm();
        assert_eq!(
            svm.get_account(&TOKEN_PROGRAM_ID).as_ref(),
            Some(&mainnet[0])
        );
        assert_eq!(svm.get_account(&program_data).as_ref(), Some(&mainnet[1]));
    }

    /// Jupiter's own instructions for every leg of every route, each in a fresh SVM, signed by the
    /// test wallet and sent with the leg's lookup tables. Between them they call every AMM the
    /// snapshot routes through, so a program the loader left undeployed fails here.
    #[test]
    fn every_route_leg_replays_against_the_snapshot() {
        let snapshot = Snapshot::load(SNAPSHOT_DIR);
        let owner = wallet::wallet();
        let mut replayed = 0;
        for (name, route) in snapshot.routes() {
            for (index, leg) in route.legs.iter().enumerate() {
                let mut svm = snapshot.svm();
                fund(&mut svm, &owner.pubkey(), 10 * SOL);
                // A leg that sells SOL wraps it in its setup; any other input the wallet holds.
                if leg.input_mint != WSOL_MINT {
                    token_account(&mut svm, &owner.pubkey(), &leg.input_mint, leg.in_amount);
                }
                let before = svm.get_balance(&owner.pubkey()).unwrap();
                let outcome = tx::send(
                    &mut svm,
                    &owner,
                    &[],
                    &leg.instructions.all(),
                    &leg.lookup_tables,
                )
                .unwrap_or_else(|failure| panic!("{name} leg {index}: {failure:?}"));

                assert_eq!(outcome.size, leg.transaction_size, "{name} leg {index}");
                if let Some(cleanup) = &leg.instructions.cleanup {
                    // It closes the wrapped SOL account.
                    assert_eq!(svm.get_account(&cleanup.accounts[0].pubkey), None);
                }
                let received = if leg.output_mint == WSOL_MINT {
                    // Unwrapped by the cleanup, which also returned the account's rent.
                    svm.get_balance(&owner.pubkey()).unwrap() + outcome.fee - before
                } else {
                    token_balance(&svm, &leg.destination_token_account)
                };
                assert!(
                    received >= leg.other_amount_threshold,
                    "{name} leg {index}: received {received}, below the threshold {}\n{outcome:?}",
                    leg.other_amount_threshold
                );
                replayed += 1;
            }
        }
        assert!(replayed > 1, "only {replayed} legs to replay");
    }

    /// LiteSVM keeps 10 KB of logs by default; a failure deep in a route can come after that.
    #[test]
    fn long_logs_are_kept_whole() {
        let snapshot = Snapshot::load(SNAPSHOT_DIR);
        let usdc = snapshot.named("usdcMint");
        let mut svm = snapshot.into_svm();
        let owner = wallet::wallet();
        fund(&mut svm, &owner.pubkey(), SOL);
        let account = token_account(&mut svm, &owner.pubkey(), &usdc, 0);
        // CreateIdempotent for an account that exists: four log lines for ten transaction bytes.
        // A transaction runs at most 64 instructions, CPIs included.
        let create = Instruction {
            program_id: ASSOCIATED_TOKEN_PROGRAM_ID,
            accounts: vec![
                AccountMeta::new(owner.pubkey(), true),
                AccountMeta::new(account, false),
                AccountMeta::new_readonly(owner.pubkey(), false),
                AccountMeta::new_readonly(usdc, false),
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
                AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
            ],
            data: vec![1],
        };
        let mut instructions = vec![ComputeBudgetInstruction::set_compute_unit_limit(1_400_000)];
        instructions.extend(std::iter::repeat_n(create, 60));

        let outcome = tx::send(&mut svm, &owner, &[], &instructions, &[]).unwrap();
        let logged: usize = outcome.logs.iter().map(String::len).sum();
        assert!(logged > 10_000, "only {logged} bytes of logs");
        assert!(!outcome.logs.iter().any(|line| line == "Log truncated"));
    }

    #[test]
    fn accounts_and_programs_are_found_by_name() {
        let snapshot = Snapshot::load(SNAPSHOT_DIR);
        assert_eq!(
            snapshot.named("pythSolUsd"),
            Address::from_str_const("7UVimffxr9ow1uXYxsr4LHAcV58mLzhmwaeKvJ1pjLiE")
        );
        assert_eq!(
            snapshot.named("usdcMint"),
            Address::from_str_const("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v")
        );
        assert_eq!(snapshot.named("wsolMint"), WSOL_MINT);
        assert_eq!(snapshot.named("token"), TOKEN_PROGRAM_ID);
        assert_eq!(
            snapshot.named("jupiter"),
            Address::from_str_const("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4")
        );
        let tip = snapshot.account(&snapshot.named("jitoTip")).unwrap();
        assert_eq!(
            tip.owner,
            Address::from_str_const("T1pyyaTNZsKv2WcRAB8oVnk93mLJw2XzjtVYqCsaHqt")
        );
    }

    #[test]
    #[should_panic(expected = "no account named \"pythEthUsd\"")]
    fn an_unknown_name_panics() {
        Snapshot::load(SNAPSHOT_DIR).named("pythEthUsd");
    }

    #[test]
    fn lookup_tables_decode_from_the_snapshot() {
        let snapshot = Snapshot::load(SNAPSHOT_DIR);
        let tables = snapshot.lookup_tables();
        assert!(!tables.is_empty());
        for table in &tables {
            let data = &snapshot.account(&table.key).unwrap().data;
            assert_eq!(
                data.len(),
                LOOKUP_TABLE_META_LEN + 32 * table.addresses.len()
            );
            assert_eq!(
                table.addresses.last().unwrap().as_ref(),
                &data[data.len() - 32..]
            );
        }

        // A route's tables are its legs', each once, in leg order.
        let round_trip = snapshot.route("solToUsdcToSol");
        let mut expected: Vec<Address> = Vec::new();
        for leg in &round_trip.legs {
            for table in &leg.lookup_tables {
                if !expected.contains(&table.key) {
                    expected.push(table.key);
                }
            }
        }
        let keys: Vec<Address> = round_trip
            .lookup_tables()
            .iter()
            .map(|table| table.key)
            .collect();
        assert_eq!(keys, expected);
    }

    /// A table's account data: the header, then two addresses.
    fn lookup_table(deactivation_slot: u64, last_extended_slot: u64) -> Account {
        let mut data = vec![0; LOOKUP_TABLE_META_LEN];
        data[..4].copy_from_slice(&1u32.to_le_bytes());
        data[4..12].copy_from_slice(&deactivation_slot.to_le_bytes());
        data[12..20].copy_from_slice(&last_extended_slot.to_le_bytes());
        data.extend_from_slice(&[7; 64]);
        Account {
            lamports: 1,
            data,
            owner: address_lookup_table::ID,
            executable: false,
            rent_epoch: u64::MAX,
        }
    }

    #[test]
    fn an_active_table_decodes_to_its_addresses() {
        let key = Address::new_from_array([1; 32]);
        let decoded = decode_lookup_table(key, &lookup_table(u64::MAX, 99), 100);
        assert_eq!(decoded.key, key);
        assert_eq!(decoded.addresses, vec![Address::new_from_array([7; 32]); 2]);
    }

    #[test]
    #[should_panic(expected = "is deactivating")]
    fn a_deactivating_table_is_refused() {
        decode_lookup_table(Address::new_from_array([1; 32]), &lookup_table(50, 10), 100);
    }

    #[test]
    #[should_panic(expected = "was extended at slot 100, not before the snapshot's 100")]
    fn a_table_extended_in_the_snapshot_slot_is_refused() {
        decode_lookup_table(
            Address::new_from_array([1; 32]),
            &lookup_table(u64::MAX, 100),
            100,
        );
    }

    #[test]
    fn warping_moves_the_clock_forward() {
        let mut svm = LiteSVM::new();
        let before = svm.get_sysvar::<Clock>();
        warp(&mut svm, 150, 60);
        let after = svm.get_sysvar::<Clock>();
        assert_eq!(after.slot, before.slot + 150);
        assert_eq!(after.unix_timestamp, before.unix_timestamp + 60);
        assert_eq!(after.epoch, before.epoch);
    }

    /// A scratch directory for one test, removed after `test` runs, whose panic propagates.
    fn in_scratch_dir(name: &str, test: impl FnOnce(&Path) + std::panic::UnwindSafe) {
        let dir =
            std::env::temp_dir().join(format!("ballista-protocol-{name}-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let result = std::panic::catch_unwind(|| test(&dir));
        fs::remove_dir_all(&dir).unwrap();
        if let Err(panic) = result {
            std::panic::resume_unwind(panic);
        }
    }

    /// Loads a copy of the snapshot whose `accounts.json` `edit` has changed.
    fn load_edited(name: &str, edit: impl FnOnce(&mut Vec<serde_json::Value>)) {
        let source = Path::new(SNAPSHOT_DIR);
        let mut accounts: Vec<serde_json::Value> = read_json(&source.join("accounts.json"));
        edit(&mut accounts);
        in_scratch_dir(name, |copy| {
            fs::create_dir(copy.join("programs")).unwrap();
            for entry in fs::read_dir(source.join("programs")).unwrap() {
                let path = entry.unwrap().path();
                fs::copy(&path, copy.join("programs").join(path.file_name().unwrap())).unwrap();
            }
            for file in ["manifest.json", "routes.json"] {
                fs::copy(source.join(file), copy.join(file)).unwrap();
            }
            fs::write(
                copy.join("accounts.json"),
                serde_json::to_vec(&accounts).unwrap(),
            )
            .unwrap();
            Snapshot::load(copy);
        });
    }

    #[test]
    #[should_panic(expected = "does not match the manifest's sha256")]
    fn a_changed_account_is_caught() {
        load_edited("changed", |accounts| {
            let feed = accounts
                .iter_mut()
                .find(|account| {
                    account["address"] == "7UVimffxr9ow1uXYxsr4LHAcV58mLzhmwaeKvJ1pjLiE"
                })
                .unwrap();
            let mut data = decode_base64(feed["data"].as_str().unwrap());
            data[73] ^= 1;
            feed["data"] = BASE64.encode(data).into();
        });
    }

    /// The list keeps its length, so only checking each entry off the manifest notices.
    #[test]
    #[should_panic(expected = "is in accounts.json twice, or is not in the manifest")]
    fn an_account_repeated_in_place_of_another_is_caught() {
        load_edited("repeated", |accounts| accounts[1] = accounts[0].clone());
    }

    #[test]
    #[should_panic(expected = "is a Git LFS pointer, not the program; run `git lfs pull`")]
    fn a_git_lfs_pointer_is_refused() {
        in_scratch_dir("lfs", |dir| {
            let pointer = "version https://git-lfs.github.com/spec/v1\n\
                           oid sha256:6804554e69fd3a58caa191dc4a58f4c67223d30ca28ab8987f39fc18d2f7374d\n\
                           size 105032\n";
            fs::write(dir.join("program.so"), pointer).unwrap();
            let program = ManifestProgram {
                name: "associatedToken".to_string(),
                file: "program.so".to_string(),
                elf_length: 105_032,
                elf_sha256: "6804554e69fd3a58caa191dc4a58f4c67223d30ca28ab8987f39fc18d2f7374d"
                    .to_string(),
            };
            read_program(dir, &program);
        });
    }
}
