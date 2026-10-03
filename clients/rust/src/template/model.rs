//! The template tree: what [`Template`] holds before it compiles. It mirrors the TypeScript SDK's
//! `Template`, `Expression`, `Step` and `DataPart` one to one, so the compiler can lower it to the
//! same bytes.

use solana_program::pubkey::Pubkey;

/// The type of a run input, a row input, or a registry field.
///
/// A registry field takes one of the five fixed-width types: `Bool`, `U64`, `I64`, `U128` or
/// `Pubkey`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Type {
    Bool,
    U64,
    I64,
    U128,
    Pubkey,
    /// A byte string of at most this many bytes, from 1 to 1,024.
    Bytes(u16),
}

impl Type {
    pub(crate) fn value_type(self) -> ValueType {
        match self {
            Type::Bool => ValueType::Bool,
            Type::U64 => ValueType::U64,
            Type::I64 => ValueType::I64,
            Type::U128 => ValueType::U128,
            Type::Pubkey => ValueType::Pubkey,
            Type::Bytes(_) => ValueType::Bytes,
        }
    }

    pub(crate) fn max_length(self) -> usize {
        match self {
            Type::Bytes(length) => length as usize,
            _ => 0,
        }
    }

    /// The type's name as the TypeScript SDK spells it.
    pub fn name(self) -> &'static str {
        self.value_type().name()
    }
}

/// What a value is at run time: every register holds one of these.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ValueType {
    Bool,
    U64,
    I64,
    U128,
    Pubkey,
    Bytes,
}

impl ValueType {
    pub fn name(self) -> &'static str {
        match self {
            ValueType::Bool => "bool",
            ValueType::U64 => "u64",
            ValueType::I64 => "i64",
            ValueType::U128 => "u128",
            ValueType::Pubkey => "pubkey",
            ValueType::Bytes => "bytes",
        }
    }
}

/// A width read from account data, return data, instruction data or a registry field.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ReadType {
    Bool,
    U8,
    U16,
    U32,
    I32,
    U64,
    I64,
    U128,
    Pubkey,
}

impl ReadType {
    /// Bytes the read covers.
    pub fn width(self) -> usize {
        match self {
            ReadType::Bool | ReadType::U8 => 1,
            ReadType::U16 => 2,
            ReadType::U32 | ReadType::I32 => 4,
            ReadType::U64 | ReadType::I64 => 8,
            ReadType::U128 => 16,
            ReadType::Pubkey => 32,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ReadType::Bool => "bool",
            ReadType::U8 => "u8",
            ReadType::U16 => "u16",
            ReadType::U32 => "u32",
            ReadType::I32 => "i32",
            ReadType::U64 => "u64",
            ReadType::I64 => "i64",
            ReadType::U128 => "u128",
            ReadType::Pubkey => "pubkey",
        }
    }
}

/// A constant value.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub(crate) enum Literal {
    Bool(bool),
    U64(u64),
    I64(i64),
    U128(u128),
    Pubkey([u8; 32]),
    Bytes(Vec<u8>),
}

/// A reference to a declared account: a fixed account by name, or an account of the current
/// batch row (valid inside `step::for_each` only).
///
/// A `&str` converts to a fixed account, so most places that take an account take its name.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum AccountRef {
    Fixed(String),
    Iteration(String),
}

impl AccountRef {
    pub fn name(&self) -> &str {
        match self {
            AccountRef::Fixed(name) | AccountRef::Iteration(name) => name,
        }
    }
}

impl From<&str> for AccountRef {
    fn from(name: &str) -> Self {
        AccountRef::Fixed(name.to_string())
    }
}

impl From<String> for AccountRef {
    fn from(name: String) -> Self {
        AccountRef::Fixed(name)
    }
}

impl From<&String> for AccountRef {
    fn from(name: &String) -> Self {
        AccountRef::Fixed(name.clone())
    }
}

impl From<&AccountRef> for AccountRef {
    fn from(reference: &AccountRef) -> Self {
        reference.clone()
    }
}

/// The registry entry a fixed account holds. Build it with [`account::registry`](super::account::registry).
#[derive(Clone, Debug)]
pub(crate) struct RegistrySpec {
    pub name: String,
    pub key: Option<Expr>,
    pub payer: String,
}

/// The constraints a declared account must meet: the TypeScript SDK's account declaration.
///
/// Start from [`account::readonly`](super::account::readonly), [`account::signer`](super::account::signer),
/// [`account::writable`](super::account::writable) or [`account::program`](super::account::program), and
/// chain the rest:
///
/// ```
/// use ballista_sdk::template::account;
/// use ballista_sdk::TOKEN_PROGRAM_ID;
///
/// let vault = account::writable().owner(TOKEN_PROGRAM_ID).min_data_length(165);
/// ```
#[derive(Clone, Debug, Default)]
pub struct Account {
    pub(crate) signer: bool,
    pub(crate) writable: bool,
    pub(crate) executable: bool,
    pub(crate) address: Option<[u8; 32]>,
    pub(crate) owner: Option<[u8; 32]>,
    pub(crate) min_data_length: u32,
    pub(crate) unsafe_unpinned: bool,
    pub(crate) registry: Option<RegistrySpec>,
    /// A builder method used where it does not apply, reported when the template compiles.
    pub(crate) misuse: Option<String>,
}

impl Account {
    /// An account with no constraint: any account, read-only, not a signer.
    pub fn new() -> Self {
        Self::default()
    }

    /// The account must sign the run.
    pub fn signer(mut self) -> Self {
        self.signer = true;
        self
    }

    /// The account must be passed writable.
    pub fn writable(mut self) -> Self {
        self.writable = true;
        self
    }

    /// The account must be an executable program.
    pub fn executable(mut self) -> Self {
        self.executable = true;
        self
    }

    /// The account must be exactly this address.
    pub fn address(mut self, address: impl Into<Pubkey>) -> Self {
        self.address = Some(address.into().to_bytes());
        self
    }

    /// The account must be owned by this program.
    pub fn owner(mut self, owner: impl Into<Pubkey>) -> Self {
        self.owner = Some(owner.into().to_bytes());
        self
    }

    /// The account must hold at least this many bytes of data. The compiler raises it on its own
    /// to cover every fixed-offset read.
    pub fn min_data_length(mut self, length: u32) -> Self {
        self.min_data_length = length;
        self
    }

    /// Opts the account out of the compiler's pin requirements: an invoked program or a PDA's
    /// program need not pin an address, and an account read as data need not pin an owner or an
    /// address. The template then trusts whatever account the caller passes here.
    pub fn unsafe_unpinned(mut self) -> Self {
        self.unsafe_unpinned = true;
        self
    }

    /// For a registry account: the `pubkey` expression that keys its entry, evaluated before the
    /// first step. Without one, the account holds the registry's one template-wide entry.
    pub fn key(mut self, key: impl Into<Expr>) -> Self {
        match &mut self.registry {
            Some(registry) => registry.key = Some(key.into()),
            None => {
                self.misuse = Some(
                    "key() applies only to a registry account: build it with account::registry"
                        .into(),
                )
            }
        }
        self
    }
}

/// A field of an account every account has, whatever its data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccountField {
    Key,
    Owner,
    Lamports,
    DataLength,
    IsEmpty,
}

/// Where a data read starts: a fixed byte offset, which raises the account's minimum data length,
/// or a `u64` expression read at run time.
#[derive(Clone, Debug)]
pub enum Offset {
    Static(i64),
    Dynamic(Expr),
}

impl From<u32> for Offset {
    fn from(offset: u32) -> Self {
        Offset::Static(offset.into())
    }
}

impl From<i32> for Offset {
    fn from(offset: i32) -> Self {
        Offset::Static(offset.into())
    }
}

impl From<u64> for Offset {
    fn from(offset: u64) -> Self {
        Offset::Static(i64::try_from(offset).unwrap_or(i64::MAX))
    }
}

impl From<usize> for Offset {
    fn from(offset: usize) -> Self {
        Offset::Static(i64::try_from(offset).unwrap_or(i64::MAX))
    }
}

impl From<Expr> for Offset {
    fn from(offset: Expr) -> Self {
        Offset::Dynamic(offset)
    }
}

impl From<&Expr> for Offset {
    fn from(offset: &Expr) -> Self {
        Offset::Dynamic(offset.clone())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Min,
    Max,
    Equal,
    NotEqual,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    And,
    Or,
    Remainder,
    ShiftLeft,
    ShiftRight,
    BitAnd,
    BitOr,
    BitXor,
}

impl BinaryOp {
    pub(crate) fn name(self) -> &'static str {
        match self {
            BinaryOp::Add => "add",
            BinaryOp::Subtract => "subtract",
            BinaryOp::Multiply => "multiply",
            BinaryOp::Divide => "divide",
            BinaryOp::Min => "min",
            BinaryOp::Max => "max",
            BinaryOp::Equal => "equal",
            BinaryOp::NotEqual => "notEqual",
            BinaryOp::LessThan => "lessThan",
            BinaryOp::LessThanOrEqual => "lessThanOrEqual",
            BinaryOp::GreaterThan => "greaterThan",
            BinaryOp::GreaterThanOrEqual => "greaterThanOrEqual",
            BinaryOp::And => "and",
            BinaryOp::Or => "or",
            BinaryOp::Remainder => "remainder",
            BinaryOp::ShiftLeft => "shiftLeft",
            BinaryOp::ShiftRight => "shiftRight",
            BinaryOp::BitAnd => "bitAnd",
            BinaryOp::BitOr => "bitOr",
            BinaryOp::BitXor => "bitXor",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InstructionField {
    Program,
    AccountCount,
    DataLength,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CastType {
    U64,
    I64,
    U128,
}

/// The expression tree, as the TypeScript SDK's `Expression` union.
#[derive(Clone, Debug)]
pub(crate) enum Node {
    Input(String),
    RowInput(String),
    Variable(String),
    Literal(Literal),
    AccountField(AccountRef, AccountField),
    AccountData {
        account: AccountRef,
        offset: Box<Offset>,
        ty: ReadType,
    },
    ReturnData {
        offset: i64,
        ty: ReadType,
    },
    ClockSlot,
    ClockUnixTimestamp,
    LoopIndex,
    Pda {
        program: AccountRef,
        seeds: Vec<Expr>,
        bump: Option<Box<Expr>>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    MultiplyDivide {
        left: Box<Expr>,
        right: Box<Expr>,
        divisor: Box<Expr>,
        round_up: bool,
    },
    PowerOfTen(Box<Expr>),
    Not(Box<Expr>),
    Select {
        condition: Box<Expr>,
        if_true: Box<Expr>,
        if_false: Box<Expr>,
    },
    Cast {
        to: CastType,
        value: Box<Expr>,
    },
    InstructionCount(AccountRef),
    CurrentInstructionIndex(AccountRef),
    Instruction {
        sysvar: AccountRef,
        index: Box<Expr>,
        field: InstructionField,
    },
    InstructionAccount {
        sysvar: AccountRef,
        index: Box<Expr>,
        position: Box<Expr>,
        flags: bool,
    },
    InstructionData {
        sysvar: AccountRef,
        index: Box<Expr>,
        offset: Box<Expr>,
        ty: ReadType,
    },
    InstructionDataBytes {
        sysvar: AccountRef,
        index: Box<Expr>,
        offset: Box<Expr>,
        length: usize,
    },
    AccountDataBytes {
        account: AccountRef,
        offset: Box<Expr>,
        length: usize,
    },
    BytesLength(Box<Expr>),
    Registry {
        account: String,
        field: String,
    },
    GroupLength(String),
    /// `groupAny` when `count` is false, `groupCount` when it is true.
    GroupFilter {
        group: String,
        count: bool,
        filter: GroupFilter,
    },
    /// An expression a constructor refused; compiling it fails with this message.
    Invalid(String),
}

/// What [`expr::group_any`](super::expr::group_any) and [`expr::group_count`](super::expr::group_count)
/// test each member of an account group against: the TypeScript SDK's `GroupFilter`. A member
/// matches when all of these hold:
///
/// - its owner is one of the programs, one or two (Token and Token-2022, say);
/// - its data holds at least the minimum length, by default just enough for every match;
/// - for each match, its data at the offset holds the value, encoded as invocation data encodes
///   a value of its type: a `pubkey` as 32 bytes, a `u64` or `i64` as 8 little-endian bytes, a
///   `u128` as 16, a `bool` as one byte;
/// - its address is none of the except keys.
///
/// ```
/// use ballista_sdk::template::prelude::*;
///
/// // A token account owned by `user`, other than `destination`.
/// let filter = GroupFilter::new()
///     .program(TOKEN_PROGRAM_ID)
///     .equals(32, account_key("user"))
///     .except_key(account_key("destination"));
/// ```
#[derive(Clone, Debug, Default)]
pub struct GroupFilter {
    pub(crate) programs: Vec<[u8; 32]>,
    pub(crate) min_data_length: Option<u32>,
    pub(crate) matches: Vec<(u16, Expr)>,
    pub(crate) except_keys: Vec<Expr>,
}

impl GroupFilter {
    pub fn new() -> Self {
        Self::default()
    }

    /// A program the member's owner may be. Name one or two.
    pub fn program(mut self, program: impl Into<Pubkey>) -> Self {
        self.programs.push(program.into().to_bytes());
        self
    }

    /// The fewest data bytes a member may hold. Without it, the filter needs just enough bytes
    /// for every match.
    pub fn min_data_length(mut self, length: u32) -> Self {
        self.min_data_length = Some(length);
        self
    }

    /// The member's data at `offset` must hold `value`, a `bool`, `u64`, `i64`, `u128` or
    /// `pubkey`. One to four per filter.
    pub fn equals(mut self, offset: u16, value: impl Into<Expr>) -> Self {
        self.matches.push((offset, value.into()));
        self
    }

    /// A member at this address, a `pubkey`, never matches. Up to four per filter.
    pub fn except_key(mut self, key: impl Into<Expr>) -> Self {
        self.except_keys.push(key.into());
        self
    }
}

/// A value computed at run time: an input, a constant, an account's field or data, a variable, or
/// arithmetic and logic over other expressions. Build one with the functions in
/// [`expr`](super::expr), and combine them with `+ - * / %`, `& | ^ << >>`, `!` and the comparison
/// methods (`.gt()`, `.lte()`, `.eq()` and the rest).
///
/// A bare integer converts to a `u64` constant wherever an expression is expected.
#[derive(Clone, Debug)]
pub struct Expr(pub(crate) Node);

impl Expr {
    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Expr(Node::Invalid(message.into()))
    }
}

/// One piece of invocation data, an emitted log, or return data.
#[derive(Clone, Debug)]
pub struct DataPart(pub(crate) DataNode);

#[derive(Clone, Debug)]
pub(crate) enum DataNode {
    Literal(Vec<u8>),
    Encoded(Encoding, Expr),
}

/// How an expression's value is written into invocation data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Encoding {
    U8,
    U16,
    U32,
    U64,
    I64,
    U128,
    Pubkey,
    Bool,
    Bytes,
}

impl Encoding {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Encoding::U8 => "u8",
            Encoding::U16 => "u16",
            Encoding::U32 => "u32",
            Encoding::U64 => "u64",
            Encoding::I64 => "i64",
            Encoding::U128 => "u128",
            Encoding::Pubkey => "pubkey",
            Encoding::Bool => "bool",
            Encoding::Bytes => "bytes",
        }
    }
}

/// An account an invocation passes, with the privileges it passes it with.
#[derive(Clone, Debug)]
pub(crate) struct InvokeAccount {
    pub account: AccountRef,
    pub signer: bool,
    pub writable: bool,
}

/// A cross-program invocation. Build one with [`step::invoke`](super::step::invoke) or a helper
/// such as [`system_transfer`](super::system_transfer); it converts into a [`Step`].
#[derive(Clone, Debug)]
pub struct Invoke {
    pub(crate) program: AccountRef,
    pub(crate) accounts: Vec<InvokeAccount>,
    pub(crate) data: Vec<DataPart>,
    pub(crate) when: Option<Expr>,
    pub(crate) account_group: Option<String>,
    pub(crate) program_address: Option<[u8; 32]>,
    pub(crate) label: Option<String>,
}

impl Invoke {
    /// Passes `account` read-only and unsigned.
    pub fn readonly(self, account: impl Into<AccountRef>) -> Self {
        self.account(account, false, false)
    }

    /// Passes `account` writable, unsigned.
    pub fn writable(self, account: impl Into<AccountRef>) -> Self {
        self.account(account, false, true)
    }

    /// Passes `account` as a read-only signer.
    pub fn signer(self, account: impl Into<AccountRef>) -> Self {
        self.account(account, true, false)
    }

    /// Passes `account` as a writable signer.
    pub fn writable_signer(self, account: impl Into<AccountRef>) -> Self {
        self.account(account, true, true)
    }

    /// Passes `account` with the given privileges. Each must also be declared on the account.
    pub fn account(mut self, account: impl Into<AccountRef>, signer: bool, writable: bool) -> Self {
        self.accounts.push(InvokeAccount {
            account: account.into(),
            signer,
            writable,
        });
        self
    }

    /// Appends one part of the instruction data.
    pub fn data(mut self, part: DataPart) -> Self {
        self.data.push(part);
        self
    }

    /// Appends parts of the instruction data, in order.
    pub fn data_parts(mut self, parts: impl IntoIterator<Item = DataPart>) -> Self {
        self.data.extend(parts);
        self
    }

    /// Makes the invocation conditional: it runs only when `condition`, a `bool`, is true. A
    /// second guard is joined to the first with `and`, the first evaluated first.
    pub fn when(mut self, condition: impl Into<Expr>) -> Self {
        let condition = condition.into();
        self.when = Some(match self.when.take() {
            Some(existing) => super::expr::and(existing, condition),
            None => condition,
        });
        self
    }

    /// Forwards the members of a declared account group after the listed accounts.
    pub fn account_group(mut self, group: impl Into<String>) -> Self {
        self.account_group = Some(group.into());
        self
    }

    /// The program this invocation is written for. Compiling fails if the program account pins a
    /// different address.
    pub fn program_address(mut self, address: impl Into<Pubkey>) -> Self {
        self.program_address = Some(address.into().to_bytes());
        self
    }

    /// Names the step in the source map, so a failed run names it.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = non_empty(label.into());
        self
    }
}

/// A loop: [`step::for_each`](super::step::for_each) over the batch rows, or
/// [`step::repeat`](super::step::repeat) a counted number of times. It converts into a [`Step`].
#[derive(Clone, Debug)]
pub struct Loop {
    /// `Some((count, max))` for a `repeat`, `None` for a `forEach`.
    pub(crate) repeat: Option<(Expr, i64)>,
    pub(crate) steps: Vec<Step>,
    pub(crate) carry: Option<Vec<String>>,
    pub(crate) label: Option<String>,
}

impl Loop {
    /// Appends one step to the loop body.
    pub fn step(mut self, step: impl Into<Step>) -> Self {
        self.steps.push(step.into());
        self
    }

    /// Appends steps to the loop body, in order.
    pub fn steps<S: Into<Step>>(mut self, steps: impl IntoIterator<Item = S>) -> Self {
        self.steps.extend(steps.into_iter().map(Into::into));
        self
    }

    /// Carries a variable defined before the loop across passes and out of it. The body may
    /// rewrite it with `step::assign`.
    pub fn carry(mut self, name: impl Into<String>) -> Self {
        self.carry.get_or_insert_with(Vec::new).push(name.into());
        self
    }

    /// Names the step in the source map.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = non_empty(label.into());
        self
    }
}

/// One step of a template, as the TypeScript SDK's `Step` union.
#[derive(Clone, Debug)]
pub struct Step(pub(crate) StepNode);

#[derive(Clone, Debug)]
pub(crate) enum StepNode {
    Require {
        condition: Expr,
        label: Option<String>,
    },
    Let {
        name: String,
        value: Expr,
        label: Option<String>,
    },
    Assign {
        name: String,
        value: Expr,
        label: Option<String>,
    },
    Invoke(Invoke),
    Emit {
        parts: Vec<DataPart>,
        label: Option<String>,
    },
    SetReturnData {
        parts: Vec<DataPart>,
        label: Option<String>,
    },
    SetRegistry {
        account: String,
        field: String,
        value: Expr,
        label: Option<String>,
    },
    Loop(Loop),
    /// A step a helper refused; compiling it fails with this message.
    Invalid(String),
}

impl Step {
    /// Names the step in the source map, so a failed run names it: `RequirementFailed at steps[2]
    /// (withinBudget)`.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        let label = non_empty(label.into());
        match &mut self.0 {
            StepNode::Require { label: slot, .. }
            | StepNode::Let { label: slot, .. }
            | StepNode::Assign { label: slot, .. }
            | StepNode::Emit { label: slot, .. }
            | StepNode::SetReturnData { label: slot, .. }
            | StepNode::SetRegistry { label: slot, .. } => *slot = label,
            StepNode::Invoke(invoke) => invoke.label = label,
            StepNode::Loop(body) => body.label = label,
            StepNode::Invalid(_) => {}
        }
        self
    }

    pub(crate) fn label_text(&self) -> Option<&str> {
        match &self.0 {
            StepNode::Require { label, .. }
            | StepNode::Let { label, .. }
            | StepNode::Assign { label, .. }
            | StepNode::Emit { label, .. }
            | StepNode::SetReturnData { label, .. }
            | StepNode::SetRegistry { label, .. } => label.as_deref(),
            StepNode::Invoke(invoke) => invoke.label.as_deref(),
            StepNode::Loop(body) => body.label.as_deref(),
            StepNode::Invalid(_) => None,
        }
    }
}

impl From<Invoke> for Step {
    fn from(invoke: Invoke) -> Self {
        Step(StepNode::Invoke(invoke))
    }
}

impl From<Loop> for Step {
    fn from(body: Loop) -> Self {
        Step(StepNode::Loop(body))
    }
}

pub(crate) fn non_empty(label: String) -> Option<String> {
    (!label.is_empty()).then_some(label)
}

/// The batch a template runs `step::for_each` over: up to `max_iterations` rows of accounts, and
/// optionally of inputs, supplied by the caller at run time.
#[derive(Clone, Debug)]
pub struct Batch {
    pub(crate) max_iterations: i64,
    pub(crate) min_iterations: i64,
    pub(crate) row: Vec<(String, Account)>,
    pub(crate) row_inputs: Vec<(String, Type)>,
}

impl Batch {
    /// A batch of at most `max_iterations` rows, from 1 to 60.
    pub fn new(max_iterations: u8) -> Self {
        Batch {
            max_iterations: max_iterations.into(),
            min_iterations: 0,
            row: Vec::new(),
            row_inputs: Vec::new(),
        }
    }

    /// Runs with fewer rows than this fail instead of succeeding vacuously.
    pub fn min_iterations(mut self, min_iterations: u8) -> Self {
        self.min_iterations = min_iterations.into();
        self
    }

    /// Declares an account each row supplies. Steps name it with `account::iteration(name)`.
    pub fn account(mut self, name: impl Into<String>, account: Account) -> Self {
        self.row.push((name.into(), account));
        self
    }

    /// Declares an input each row supplies, after the fixed inputs in the run data. Steps read it
    /// with `expr::row_input(name)`.
    pub fn input(mut self, name: impl Into<String>, ty: Type) -> Self {
        self.row_inputs.push((name.into(), ty));
        self
    }
}

/// A template definition, the TypeScript SDK's `defineTemplate` input: inputs, registries,
/// accounts, an optional batch, account groups, and steps. [`Template::compile`] lowers it to the
/// bytes a template account holds.
///
/// ```
/// use ballista_sdk::template::prelude::*;
/// use ballista_sdk::SYSTEM_PROGRAM_ID;
///
/// let sweep = Template::new()
///     .input("reserve", Type::U64)
///     .account("systemProgram", account::program(SYSTEM_PROGRAM_ID))
///     .account("vault", account::signer().writable())
///     .account("destination", account::writable())
///     .step(step::let_("balance", lamports("vault")))
///     .step(step::require(var("balance").gt(input("reserve"))).label("aboveReserve"))
///     .step(system_transfer(
///         "systemProgram",
///         "vault",
///         "destination",
///         var("balance") - input("reserve"),
///     ));
/// let compiled = sweep.compile().unwrap();
/// assert_eq!(compiled.input_order, ["reserve"]);
/// ```
#[derive(Clone, Debug, Default)]
pub struct Template {
    pub(crate) inputs: Vec<(String, Type)>,
    pub(crate) registries: Vec<(String, Vec<(String, Type)>)>,
    pub(crate) accounts: Vec<(String, Account)>,
    pub(crate) batch: Option<Batch>,
    pub(crate) emit_event: bool,
    pub(crate) account_groups: Vec<String>,
    pub(crate) steps: Vec<Step>,
    /// Names declared twice, reported when the template compiles.
    pub(crate) duplicates: Vec<String>,
}

impl Template {
    pub fn new() -> Self {
        Self::default()
    }

    /// Declares a run input. Inputs are encoded in the run data in declaration order.
    pub fn input(mut self, name: impl Into<String>, ty: Type) -> Self {
        let name = name.into();
        if self.inputs.iter().any(|(other, _)| *other == name) {
            self.duplicates.push(format!("input {name}"));
        }
        self.inputs.push((name, ty));
        self
    }

    /// Declares a registry: state that outlives a run, one entry per key. Its fields are packed in
    /// the order given. The registry's index, part of its entries' addresses, is its position
    /// among the registries declared.
    pub fn registry<N: Into<String>>(
        mut self,
        name: impl Into<String>,
        fields: impl IntoIterator<Item = (N, Type)>,
    ) -> Self {
        let name = name.into();
        if self.registries.iter().any(|(other, _)| *other == name) {
            self.duplicates.push(format!("registry {name}"));
        }
        let fields = fields
            .into_iter()
            .map(|(field, ty)| (field.into(), ty))
            .collect();
        self.registries.push((name, fields));
        self
    }

    /// Declares a fixed account. Accounts are passed in declaration order.
    pub fn account(mut self, name: impl Into<String>, account: Account) -> Self {
        let name = name.into();
        if self.accounts.iter().any(|(other, _)| *other == name) {
            self.duplicates.push(format!("account {name}"));
        }
        self.accounts.push((name, account));
        self
    }

    /// Declares the batch rows `step::for_each` iterates.
    pub fn batch(mut self, batch: Batch) -> Self {
        self.batch = Some(batch);
        self
    }

    /// Declares a caller-sized group of accounts, supplied at run time after the batch rows. An
    /// invocation names it with `.account_group(name)` to forward its members.
    pub fn account_group(mut self, name: impl Into<String>) -> Self {
        self.account_groups.push(name.into());
        self
    }

    /// Emits a `BEV1` data log after every successful run.
    pub fn emit_event(mut self) -> Self {
        self.emit_event = true;
        self
    }

    /// Appends one step.
    pub fn step(mut self, step: impl Into<Step>) -> Self {
        self.steps.push(step.into());
        self
    }

    /// Appends steps, in order: the output of a helper such as `rate_limit`, or a `Vec<Step>`.
    pub fn steps<S: Into<Step>>(mut self, steps: impl IntoIterator<Item = S>) -> Self {
        self.steps.extend(steps.into_iter().map(Into::into));
        self
    }
}
