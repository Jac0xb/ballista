//! The TypeScript SDK's step helpers: transfers, associated token accounts, PDA assertions, the
//! Ed25519 signature binding and the rate limit. Each builds the same steps its TypeScript
//! counterpart builds, so a template compiles to the same bytes in either language.

use solana_program::pubkey::Pubkey;

use super::builders::{data, step};
use super::expr::{self, var};
use super::model::*;

const SYSTEM_PROGRAM: [u8; 32] = [0; 32];
const TOKEN_PROGRAM: Pubkey = crate::TOKEN_PROGRAM_ID;
const ASSOCIATED_TOKEN_PROGRAM: Pubkey = crate::ASSOCIATED_TOKEN_PROGRAM_ID;

/// A System program transfer of `lamports`, a `u64`, from `from` (a writable signer) to `to`. The
/// invocation is written for the System program: compiling fails if `system_program` pins
/// another address.
pub fn system_transfer(
    system_program: impl Into<AccountRef>,
    from: impl Into<AccountRef>,
    to: impl Into<AccountRef>,
    lamports: impl Into<Expr>,
) -> Invoke {
    step::invoke(system_program)
        .program_address(Pubkey::new_from_array(SYSTEM_PROGRAM))
        .writable_signer(from)
        .writable(to)
        .data(data::literal([2, 0, 0, 0]))
        .data(data::u64(lamports))
}

/// An SPL Token transfer of `amount`, a `u64`, from `source` to `destination`, signed by
/// `authority`.
pub fn token_transfer(
    token_program: impl Into<AccountRef>,
    source: impl Into<AccountRef>,
    destination: impl Into<AccountRef>,
    authority: impl Into<AccountRef>,
    amount: impl Into<Expr>,
) -> Invoke {
    step::invoke(token_program)
        .program_address(TOKEN_PROGRAM)
        .writable(source)
        .writable(destination)
        .signer(authority)
        .data(data::literal([3]))
        .data(data::u64(amount))
}

/// The accounts an associated token account's creation names, as the TypeScript SDK's object of
/// the same fields. A plain `&str` names a fixed account:
///
/// ```
/// use ballista_sdk::template::prelude::*;
///
/// let create = ensure_associated_token_account(AtaAccounts {
///     associated_token_program: "associatedTokenProgram".into(),
///     payer: "payer".into(),
///     associated_token_account: "ata".into(),
///     owner: "wallet".into(),
///     mint: "mint".into(),
///     system_program: "systemProgram".into(),
///     token_program: "tokenProgram".into(),
/// });
/// ```
#[derive(Clone, Debug)]
pub struct AtaAccounts {
    pub associated_token_program: AccountRef,
    pub payer: AccountRef,
    pub associated_token_account: AccountRef,
    pub owner: AccountRef,
    pub mint: AccountRef,
    pub system_program: AccountRef,
    pub token_program: AccountRef,
}

/// The Associated Token program's `Create`: fails if the account exists. See
/// [`ensure_associated_token_account`] for the version that skips an existing account.
pub fn create_associated_token_account(accounts: AtaAccounts) -> Invoke {
    step::invoke(accounts.associated_token_program)
        .program_address(ASSOCIATED_TOKEN_PROGRAM)
        .writable_signer(accounts.payer)
        .writable(accounts.associated_token_account)
        .readonly(accounts.owner)
        .readonly(accounts.mint)
        .readonly(accounts.system_program)
        .readonly(accounts.token_program)
}

/// `Create`, guarded on the account being empty: an existing account is left alone. A further
/// `.when(...)` adds to the guard.
pub fn ensure_associated_token_account(accounts: AtaAccounts) -> Invoke {
    let is_missing = expr::is_empty(accounts.associated_token_account.clone());
    create_associated_token_account(accounts).when(is_missing)
}

/// Requires that `account`'s key is the program address of `seeds` under `program`. Supply a bump
/// with `.bump(...)` to derive once instead of searching for the canonical bump.
pub fn assert_pda(
    account: impl Into<AccountRef>,
    program: impl Into<AccountRef>,
    seeds: impl IntoIterator<Item = Expr>,
) -> AssertPda {
    AssertPda {
        account: account.into(),
        program: program.into(),
        seeds: seeds.into_iter().collect(),
        bump: None,
        label: None,
    }
}

/// Requires that `associated_token_account` is the associated token account of `owner` for
/// `mint` under `token_program`.
pub fn assert_ata(
    associated_token_account: impl Into<AccountRef>,
    owner: impl Into<AccountRef>,
    mint: impl Into<AccountRef>,
    token_program: impl Into<AccountRef>,
    associated_token_program: impl Into<AccountRef>,
) -> AssertPda {
    assert_pda(
        associated_token_account,
        associated_token_program,
        [expr::key(owner), expr::key(token_program), expr::key(mint)],
    )
}

/// The TypeScript SDK's `assertAssociatedTokenAccount`, the same as [`assert_ata`].
pub fn assert_associated_token_account(
    associated_token_account: impl Into<AccountRef>,
    owner: impl Into<AccountRef>,
    mint: impl Into<AccountRef>,
    token_program: impl Into<AccountRef>,
    associated_token_program: impl Into<AccountRef>,
) -> AssertPda {
    assert_ata(
        associated_token_account,
        owner,
        mint,
        token_program,
        associated_token_program,
    )
}

/// A PDA assertion from [`assert_pda`] or [`assert_ata`]; converts into a [`Step`].
#[derive(Clone, Debug)]
pub struct AssertPda {
    account: AccountRef,
    program: AccountRef,
    seeds: Vec<Expr>,
    bump: Option<Expr>,
    label: Option<String>,
}

impl AssertPda {
    /// Derives with this `u64` bump, about 1,900 compute units instead of about 4,850 for a
    /// search three bumps deep. The assertion then proves only that the account derives from these
    /// seeds and this bump, which need not be the canonical one.
    pub fn bump(mut self, bump: impl Into<Expr>) -> Self {
        self.bump = Some(bump.into());
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = non_empty(label.into());
        self
    }
}

impl From<AssertPda> for Step {
    fn from(assertion: AssertPda) -> Self {
        let address = match assertion.bump {
            Some(bump) => expr::pda_with_bump(assertion.program, assertion.seeds, bump),
            None => expr::pda(assertion.program, assertion.seeds),
        };
        let step = step::require(expr::key(assertion.account).eq(address));
        match assertion.label {
            Some(label) => step.label(label),
            None => step,
        }
    }
}

// ------------------------------------------------------------------------------------- Ed25519

/// Byte offsets in the first 16 bytes of an Ed25519 precompile instruction with one signature.
const SIGNATURE_COUNT: usize = 0;
const SIGNATURE_INSTRUCTION_INDEX: usize = 4;
const PUBLIC_KEY_OFFSET: u64 = 6;
const PUBLIC_KEY_INSTRUCTION_INDEX: usize = 8;
const MESSAGE_DATA_OFFSET: u64 = 10;
const MESSAGE_DATA_SIZE: usize = 12;
const MESSAGE_INSTRUCTION_INDEX: usize = 14;
/// An instruction-index field of `u16::MAX`: the Ed25519 instruction's own data.
const THIS_INSTRUCTION: u128 = 0xffff;

/// Binds an Ed25519 signature the transaction's precompile instruction verified to this
/// template, as the TypeScript SDK's `ed25519Signature`.
///
/// [`Ed25519Signature::steps`] require that instruction `index` is the Ed25519 program, holds
/// exactly one self-contained signature by `signer` over exactly `message_length` bytes;
/// [`Ed25519Signature::field`] then reads the signed message. `signer` must be a key the
/// transaction's builder cannot choose, such as a pinned key or a signer's key: an input is
/// refused.
pub fn ed25519_signature(
    sysvar: impl Into<AccountRef>,
    index: impl Into<Expr>,
    signer: impl Into<Expr>,
    message_length: u32,
) -> Ed25519Signature {
    Ed25519Signature {
        sysvar: sysvar.into(),
        index: index.into(),
        signer: signer.into(),
        message_length,
        name: "signature".into(),
    }
}

/// See [`ed25519_signature`].
#[derive(Clone, Debug)]
pub struct Ed25519Signature {
    sysvar: AccountRef,
    index: Expr,
    signer: Expr,
    message_length: u32,
    name: String,
}

impl Ed25519Signature {
    /// Prefixes the step labels and the two variables the steps bind, `<name>Instruction` and
    /// `<name>Message`, so one template can check more than one signature. Default `signature`.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    fn refusal(&self) -> Option<String> {
        if self.message_length < 1 || self.message_length > 0xffff {
            return Some("messageLength must be from 1 to 65535 bytes".into());
        }
        match &self.signer.0 {
            Node::Input(name) => Some(format!(
                "signer must be a key the transaction's builder cannot choose, not the input {name}"
            )),
            Node::RowInput(name) => Some(format!(
                "signer must be a key the transaction's builder cannot choose, not the rowInput {name}"
            )),
            _ => None,
        }
    }

    fn offset_field(&self, offset: u64) -> Expr {
        expr::instruction_data(
            self.sysvar.clone(),
            var(format!("{}Instruction", self.name)),
            offset,
            ReadType::U16,
        )
    }

    /// The steps that bind the signature. Put them before any step that reads [`Self::field`].
    pub fn steps(&self) -> Vec<Step> {
        if let Some(message) = self.refusal() {
            return vec![Step(StepNode::Invalid(message))];
        }
        let name = &self.name;
        let instruction = var(format!("{name}Instruction"));
        let mut mask = 0u128;
        let mut expected = 0u128;
        for (offset, width, value) in [
            (SIGNATURE_COUNT, 1, 1),
            (SIGNATURE_INSTRUCTION_INDEX, 2, THIS_INSTRUCTION),
            (PUBLIC_KEY_INSTRUCTION_INDEX, 2, THIS_INSTRUCTION),
            (MESSAGE_DATA_SIZE, 2, u128::from(self.message_length)),
            (MESSAGE_INSTRUCTION_INDEX, 2, THIS_INSTRUCTION),
        ] {
            let shift = offset * 8;
            mask |= ((1u128 << (width * 8)) - 1) << shift;
            expected |= value << shift;
        }
        vec![
            step::let_(format!("{name}Instruction"), self.index.clone())
                .label(format!("{name}InstructionIndex")),
            step::require(
                expr::instruction_program(self.sysvar.clone(), instruction.clone())
                    .eq(expr::pubkey(crate::ED25519_PROGRAM_ID)),
            )
            .label(format!("{name}IsEd25519")),
            step::require(
                (expr::instruction_data(
                    self.sysvar.clone(),
                    instruction.clone(),
                    0,
                    ReadType::U128,
                ) & expr::u128(mask))
                .eq(expr::u128(expected)),
            )
            .label(format!("{name}IsOneSelfContainedSignature")),
            step::require(
                expr::instruction_data(
                    self.sysvar.clone(),
                    instruction,
                    self.offset_field(PUBLIC_KEY_OFFSET),
                    ReadType::Pubkey,
                )
                .eq(self.signer.clone()),
            )
            .label(format!("{name}IsBySigner")),
            step::let_(
                format!("{name}Message"),
                self.offset_field(MESSAGE_DATA_OFFSET),
            )
            .label(format!("{name}MessageOffset")),
        ]
    }

    /// The value at `offset` in the signed message. The whole read must lie inside the message.
    pub fn field(&self, offset: u32, ty: ReadType) -> Expr {
        if let Some(message) = self.refusal() {
            return Expr::invalid(message);
        }
        if offset as usize + ty.width() > self.message_length as usize {
            return Expr::invalid(format!(
                "{} at {offset} does not lie inside the {}-byte signed message",
                ty.name(),
                self.message_length
            ));
        }
        let message = var(format!("{}Message", self.name));
        let at = if offset == 0 {
            message
        } else {
            message + expr::u64(offset.into())
        };
        expr::instruction_data(
            self.sysvar.clone(),
            var(format!("{}Instruction", self.name)),
            at,
            ty,
        )
    }
}

// ----------------------------------------------------------------------------------- rate limit

/// Whether `value` is built only from literals, arithmetic or logic over them, and registry
/// reads: the expressions [`rate_limit`] trusts for `cap` and `refill_per_second`. An allow
/// list, as the TypeScript SDK's.
fn is_template_constant(value: &Expr) -> bool {
    match &value.0 {
        Node::Literal(_) | Node::Registry { .. } => true,
        Node::Binary { left, right, .. } => {
            is_template_constant(left) && is_template_constant(right)
        }
        Node::MultiplyDivide {
            left,
            right,
            divisor,
            ..
        } => {
            is_template_constant(left)
                && is_template_constant(right)
                && is_template_constant(divisor)
        }
        Node::PowerOfTen(value) | Node::Not(value) | Node::BytesLength(value) => {
            is_template_constant(value)
        }
        Node::Cast { value, .. } => is_template_constant(value),
        Node::Select {
            condition,
            if_true,
            if_false,
        } => {
            is_template_constant(condition)
                && is_template_constant(if_true)
                && is_template_constant(if_false)
        }
        _ => false,
    }
}

/// Steps that spend `amount` from a limit that refills over time, kept in the registry entry in
/// fixed account `registry`, as the TypeScript SDK's `rateLimit`.
///
/// The entry's `spent` field (a `u64`) is what has been spent and not yet refilled, and its
/// `lastSpend` field (an `i64`) the Unix time of the last spend. Each run refills `spent` by the
/// seconds elapsed times `refill_per_second`, adds `amount`, requires the total to be at most
/// `cap`, and writes both fields back.
///
/// `cap` and `refill_per_second` must be template constants written inline: literals, arithmetic
/// or logic over literals, or registry fields. An input, a variable, or a read of the transaction
/// or of an account is refused. The registry account's key must not come from the caller either,
/// which this helper cannot check.
pub fn rate_limit(
    registry: impl Into<String>,
    cap: impl Into<Expr>,
    refill_per_second: impl Into<Expr>,
    amount: impl Into<Expr>,
) -> RateLimit {
    RateLimit {
        registry: registry.into(),
        cap: cap.into(),
        refill_per_second: refill_per_second.into(),
        amount: amount.into(),
        spent: "spent".into(),
        last_spend: "lastSpend".into(),
        name: None,
    }
}

/// See [`rate_limit`]. Iterate it, or pass it to `Template::steps`, for its steps.
#[derive(Clone, Debug)]
pub struct RateLimit {
    registry: String,
    cap: Expr,
    refill_per_second: Expr,
    amount: Expr,
    spent: String,
    last_spend: String,
    name: Option<String>,
}

fn capitalized(text: &str) -> String {
    let mut characters = text.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().chain(characters).collect(),
        None => String::new(),
    }
}

impl RateLimit {
    /// The entry's `u64` field of what has been spent. Default `spent`.
    pub fn spent(mut self, field: impl Into<String>) -> Self {
        self.spent = field.into();
        self
    }

    /// The entry's `i64` field of when it was last spent. Default `lastSpend`.
    pub fn last_spend(mut self, field: impl Into<String>) -> Self {
        self.last_spend = field.into();
        self
    }

    /// Prefixes the variables the steps bind and names the requirement `within<Name>`.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// The steps, in order.
    pub fn steps(self) -> Vec<Step> {
        for (field, value) in [
            ("cap", &self.cap),
            ("refillPerSecond", &self.refill_per_second),
        ] {
            if !is_template_constant(value) {
                return vec![Step(StepNode::Invalid(format!(
                    "{field} must be written inline, from literals and arithmetic: a template \
                     constant, not an input, a variable, a read of the caller-built transaction, \
                     or of an account's data or fields. It appears once in the generated steps, \
                     so inlining it costs nothing."
                )))];
            }
        }
        let prefix = match &self.name {
            Some(name) => name.clone(),
            None if self.spent == "spent" => self.registry.clone(),
            None => format!("{}{}", self.registry, capitalized(&self.spent)),
        };
        let u128 = |value: Expr| expr::cast(Type::U128, value);
        let last = var(format!("{prefix}Last"));
        let now = var(format!("{prefix}Now"));
        let spent = var(format!("{prefix}Spent"));
        let refill = var(format!("{prefix}Refill"));
        let total = var(format!("{prefix}Total"));
        let requirement = format!(
            "within{}",
            capitalized(self.name.as_deref().unwrap_or("rateLimit"))
        );
        vec![
            step::let_(
                format!("{prefix}Last"),
                expr::registry(&self.registry, &self.last_spend),
            ),
            step::let_(
                format!("{prefix}Now"),
                expr::max(expr::clock_unix_timestamp(), last.clone()),
            ),
            step::let_(
                format!("{prefix}Spent"),
                u128(expr::registry(&self.registry, &self.spent)),
            ),
            step::let_(
                format!("{prefix}Refill"),
                expr::multiply(
                    u128(expr::subtract(now.clone(), last)),
                    u128(self.refill_per_second),
                ),
            ),
            step::let_(
                format!("{prefix}Total"),
                expr::add(
                    expr::subtract(spent.clone(), expr::min(spent, refill)),
                    u128(self.amount),
                ),
            ),
            step::require(expr::less_than_or_equal(total.clone(), u128(self.cap)))
                .label(requirement),
            step::set_registry(&self.registry, &self.spent, expr::cast(Type::U64, total)),
            step::set_registry(&self.registry, &self.last_spend, now),
        ]
    }
}

impl IntoIterator for RateLimit {
    type Item = Step;
    type IntoIter = std::vec::IntoIter<Step>;

    fn into_iter(self) -> Self::IntoIter {
        self.steps().into_iter()
    }
}
