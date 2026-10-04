//! Expressions: the TypeScript SDK's `expression.*`, one function each.
//!
//! Names follow the TypeScript functions in snake case: `expression.accountField(account.fixed(
//! 'vault'), 'lamports')` is `account_field("vault", AccountField::Lamports)`, or its shorthand
//! `lamports("vault")`. Inputs, variables, accounts and registries are named by the same
//! camelCase strings the TypeScript SDK uses, so a template keeps one set of names in both.
//!
//! Operators build the binary expressions: `a + b` is `add(a, b)`, and likewise `-` (`subtract`),
//! `*` (`multiply`), `/` (`divide`), `%` (`remainder`), `&` (`bit_and`), `|` (`bit_or`), `^`
//! (`bit_xor`), `<<` (`shift_left`), `>>` (`shift_right`) and `!` (`not`). Comparisons and logic
//! are methods: `.eq()`, `.ne()`, `.lt()`, `.lte()`, `.gt()`, `.gte()`, `.and()`, `.or()`.

use core::ops;

use solana_program::pubkey::Pubkey;

use super::model::*;

fn boxed(value: impl Into<Expr>) -> Box<Expr> {
    Box::new(value.into())
}

fn binary(op: BinaryOp, left: impl Into<Expr>, right: impl Into<Expr>) -> Expr {
    Expr(Node::Binary {
        op,
        left: boxed(left),
        right: boxed(right),
    })
}

// ------------------------------------------------------------------------------------- values

/// A run input, by its declared name.
pub fn input(name: impl Into<String>) -> Expr {
    Expr(Node::Input(name.into()))
}

/// An input of the current batch row; valid inside `step::for_each` only.
pub fn row_input(name: impl Into<String>) -> Expr {
    Expr(Node::RowInput(name.into()))
}

/// A variable bound by `step::let_`.
pub fn variable(name: impl Into<String>) -> Expr {
    Expr(Node::Variable(name.into()))
}

/// Shorthand for [`variable`].
pub fn var(name: impl Into<String>) -> Expr {
    variable(name)
}

/// A value bound by `step::snapshot`; the same as [`variable`].
pub fn snapshot(name: impl Into<String>) -> Expr {
    variable(name)
}

pub fn bool(value: bool) -> Expr {
    Expr(Node::Literal(Literal::Bool(value)))
}

pub fn u64(value: u64) -> Expr {
    Expr(Node::Literal(Literal::U64(value)))
}

pub fn i64(value: i64) -> Expr {
    Expr(Node::Literal(Literal::I64(value)))
}

pub fn u128(value: u128) -> Expr {
    Expr(Node::Literal(Literal::U128(value)))
}

pub fn pubkey(value: impl Into<Pubkey>) -> Expr {
    Expr(Node::Literal(Literal::Pubkey(value.into().to_bytes())))
}

/// A constant byte string of at most 1,024 bytes.
pub fn bytes(value: impl AsRef<[u8]>) -> Expr {
    Expr(Node::Literal(Literal::Bytes(value.as_ref().to_vec())))
}

// ----------------------------------------------------------------------------------- accounts

/// A field of an account every account has.
pub fn account_field(account: impl Into<AccountRef>, field: AccountField) -> Expr {
    Expr(Node::AccountField(account.into(), field))
}

/// The account's address, a `pubkey`.
pub fn key(account: impl Into<AccountRef>) -> Expr {
    account_field(account, AccountField::Key)
}

/// The key of fixed account `name`; the TypeScript SDK's `expression.accountKey`.
pub fn account_key(name: impl Into<String>) -> Expr {
    key(AccountRef::Fixed(name.into()))
}

/// The program that owns the account, a `pubkey`.
pub fn owner(account: impl Into<AccountRef>) -> Expr {
    account_field(account, AccountField::Owner)
}

/// The account's balance, a `u64`.
pub fn lamports(account: impl Into<AccountRef>) -> Expr {
    account_field(account, AccountField::Lamports)
}

/// The length of the account's data, a `u64`.
pub fn data_length(account: impl Into<AccountRef>) -> Expr {
    account_field(account, AccountField::DataLength)
}

/// Whether the account holds no lamports and no data, a `bool`.
pub fn is_empty(account: impl Into<AccountRef>) -> Expr {
    account_field(account, AccountField::IsEmpty)
}

/// A typed read of the account's data. A fixed offset (an integer) raises the account's minimum
/// data length to cover the read; an expression offset is a `u64` read at run time. The account
/// must pin its owner or its address.
pub fn account_data(
    account: impl Into<AccountRef>,
    offset: impl Into<Offset>,
    ty: ReadType,
) -> Expr {
    Expr(Node::AccountData {
        account: account.into(),
        offset: Box::new(offset.into()),
        ty,
    })
}

/// Exactly `length` bytes (1 to 1,024) of a read-only, pinned account's data from a `u64` offset.
pub fn account_data_bytes(
    account: impl Into<AccountRef>,
    offset: impl Into<Expr>,
    length: usize,
) -> Expr {
    Expr(Node::AccountDataBytes {
        account: account.into(),
        offset: boxed(offset),
        length,
    })
}

/// A typed read of the start of the return data the invocation just before this step set. Valid
/// only as the value of a `step::let_` directly after an unconditional invocation.
pub fn return_data(ty: ReadType) -> Expr {
    return_data_at(ty, 0)
}

/// [`return_data`] read from byte `offset` of the return data.
pub fn return_data_at(ty: ReadType, offset: u32) -> Expr {
    Expr(Node::ReturnData {
        offset: offset.into(),
        ty,
    })
}

// ------------------------------------------------------------------------------- clock, loops

pub fn clock_slot() -> Expr {
    Expr(Node::ClockSlot)
}

pub fn clock_unix_timestamp() -> Expr {
    Expr(Node::ClockUnixTimestamp)
}

/// The current pass of the enclosing loop, from 0.
pub fn loop_index() -> Expr {
    Expr(Node::LoopIndex)
}

// ----------------------------------------------------------------------------------------- PDAs

/// The canonical program address of `seeds` (1 to 15, each at most 32 bytes) under `program`,
/// which must pin its address.
pub fn pda(program: impl Into<AccountRef>, seeds: impl IntoIterator<Item = Expr>) -> Expr {
    Expr(Node::Pda {
        program: program.into(),
        seeds: seeds.into_iter().collect(),
        bump: None,
    })
}

/// The program address of `seeds` and the `u64` `bump` under `program`: one derivation instead of
/// the canonical-bump search.
pub fn pda_with_bump(
    program: impl Into<AccountRef>,
    seeds: impl IntoIterator<Item = Expr>,
    bump: impl Into<Expr>,
) -> Expr {
    Expr(Node::Pda {
        program: program.into(),
        seeds: seeds.into_iter().collect(),
        bump: Some(boxed(bump)),
    })
}

// ----------------------------------------------------------------------------------- arithmetic

pub fn add(left: impl Into<Expr>, right: impl Into<Expr>) -> Expr {
    binary(BinaryOp::Add, left, right)
}

pub fn subtract(left: impl Into<Expr>, right: impl Into<Expr>) -> Expr {
    binary(BinaryOp::Subtract, left, right)
}

pub fn multiply(left: impl Into<Expr>, right: impl Into<Expr>) -> Expr {
    binary(BinaryOp::Multiply, left, right)
}

pub fn divide(left: impl Into<Expr>, right: impl Into<Expr>) -> Expr {
    binary(BinaryOp::Divide, left, right)
}

pub fn min(left: impl Into<Expr>, right: impl Into<Expr>) -> Expr {
    binary(BinaryOp::Min, left, right)
}

pub fn max(left: impl Into<Expr>, right: impl Into<Expr>) -> Expr {
    binary(BinaryOp::Max, left, right)
}

pub fn remainder(left: impl Into<Expr>, right: impl Into<Expr>) -> Expr {
    binary(BinaryOp::Remainder, left, right)
}

pub fn shift_left(value: impl Into<Expr>, amount: impl Into<Expr>) -> Expr {
    binary(BinaryOp::ShiftLeft, value, amount)
}

pub fn shift_right(value: impl Into<Expr>, amount: impl Into<Expr>) -> Expr {
    binary(BinaryOp::ShiftRight, value, amount)
}

pub fn bit_and(left: impl Into<Expr>, right: impl Into<Expr>) -> Expr {
    binary(BinaryOp::BitAnd, left, right)
}

pub fn bit_or(left: impl Into<Expr>, right: impl Into<Expr>) -> Expr {
    binary(BinaryOp::BitOr, left, right)
}

pub fn bit_xor(left: impl Into<Expr>, right: impl Into<Expr>) -> Expr {
    binary(BinaryOp::BitXor, left, right)
}

/// `left × right ÷ divisor` with the product computed exactly, rounded down.
pub fn multiply_divide(
    left: impl Into<Expr>,
    right: impl Into<Expr>,
    divisor: impl Into<Expr>,
) -> Expr {
    Expr(Node::MultiplyDivide {
        left: boxed(left),
        right: boxed(right),
        divisor: boxed(divisor),
        round_up: false,
    })
}

/// `left × right ÷ divisor` with the product computed exactly, rounded up.
pub fn multiply_divide_up(
    left: impl Into<Expr>,
    right: impl Into<Expr>,
    divisor: impl Into<Expr>,
) -> Expr {
    Expr(Node::MultiplyDivide {
        left: boxed(left),
        right: boxed(right),
        divisor: boxed(divisor),
        round_up: true,
    })
}

/// `10^exponent` as a `u128`, from a `u64` exponent.
pub fn power_of_ten(exponent: impl Into<Expr>) -> Expr {
    Expr(Node::PowerOfTen(boxed(exponent)))
}

/// Converts a numeric value to `to`, one of `Type::U64`, `Type::I64` or `Type::U128`. A value that
/// does not fit fails the run.
pub fn cast(to: Type, value: impl Into<Expr>) -> Expr {
    let to = match to {
        Type::U64 => CastType::U64,
        Type::I64 => CastType::I64,
        Type::U128 => CastType::U128,
        other => {
            return Expr::invalid(format!(
                "cast converts to u64, i64 or u128, not {}",
                other.name()
            ))
        }
    };
    Expr(Node::Cast {
        to,
        value: boxed(value),
    })
}

// ---------------------------------------------------------------------------------------- logic

pub fn equal(left: impl Into<Expr>, right: impl Into<Expr>) -> Expr {
    binary(BinaryOp::Equal, left, right)
}

pub fn not_equal(left: impl Into<Expr>, right: impl Into<Expr>) -> Expr {
    binary(BinaryOp::NotEqual, left, right)
}

pub fn less_than(left: impl Into<Expr>, right: impl Into<Expr>) -> Expr {
    binary(BinaryOp::LessThan, left, right)
}

pub fn less_than_or_equal(left: impl Into<Expr>, right: impl Into<Expr>) -> Expr {
    binary(BinaryOp::LessThanOrEqual, left, right)
}

pub fn greater_than(left: impl Into<Expr>, right: impl Into<Expr>) -> Expr {
    binary(BinaryOp::GreaterThan, left, right)
}

pub fn greater_than_or_equal(left: impl Into<Expr>, right: impl Into<Expr>) -> Expr {
    binary(BinaryOp::GreaterThanOrEqual, left, right)
}

pub fn and(left: impl Into<Expr>, right: impl Into<Expr>) -> Expr {
    binary(BinaryOp::And, left, right)
}

pub fn or(left: impl Into<Expr>, right: impl Into<Expr>) -> Expr {
    binary(BinaryOp::Or, left, right)
}

pub fn not(value: impl Into<Expr>) -> Expr {
    Expr(Node::Not(boxed(value)))
}

/// `if_true` when `condition` holds, else `if_false`. Both branches are evaluated.
pub fn select(
    condition: impl Into<Expr>,
    if_true: impl Into<Expr>,
    if_false: impl Into<Expr>,
) -> Expr {
    Expr(Node::Select {
        condition: boxed(condition),
        if_true: boxed(if_true),
        if_false: boxed(if_false),
    })
}

// ---------------------------------------------------------------------------- introspection

/// How many instructions the transaction holds. `sysvar` is a fixed account pinned to the
/// Instructions sysvar.
pub fn instruction_count(sysvar: impl Into<AccountRef>) -> Expr {
    Expr(Node::InstructionCount(sysvar.into()))
}

/// The index of the instruction running this template.
pub fn current_instruction_index(sysvar: impl Into<AccountRef>) -> Expr {
    Expr(Node::CurrentInstructionIndex(sysvar.into()))
}

fn instruction_field(
    sysvar: impl Into<AccountRef>,
    index: impl Into<Expr>,
    field: InstructionField,
) -> Expr {
    Expr(Node::Instruction {
        sysvar: sysvar.into(),
        index: boxed(index),
        field,
    })
}

/// The program instruction `index` invokes.
pub fn instruction_program(sysvar: impl Into<AccountRef>, index: impl Into<Expr>) -> Expr {
    instruction_field(sysvar, index, InstructionField::Program)
}

pub fn instruction_account_count(sysvar: impl Into<AccountRef>, index: impl Into<Expr>) -> Expr {
    instruction_field(sysvar, index, InstructionField::AccountCount)
}

pub fn instruction_data_length(sysvar: impl Into<AccountRef>, index: impl Into<Expr>) -> Expr {
    instruction_field(sysvar, index, InstructionField::DataLength)
}

/// The key of account `position` of instruction `index`.
pub fn instruction_account(
    sysvar: impl Into<AccountRef>,
    index: impl Into<Expr>,
    position: impl Into<Expr>,
) -> Expr {
    Expr(Node::InstructionAccount {
        sysvar: sysvar.into(),
        index: boxed(index),
        position: boxed(position),
        flags: false,
    })
}

/// The flags of account `position` of instruction `index`: bit 0 signer, bit 1 writable.
pub fn instruction_account_flags(
    sysvar: impl Into<AccountRef>,
    index: impl Into<Expr>,
    position: impl Into<Expr>,
) -> Expr {
    Expr(Node::InstructionAccount {
        sysvar: sysvar.into(),
        index: boxed(index),
        position: boxed(position),
        flags: true,
    })
}

fn instruction_account_flag(
    sysvar: impl Into<AccountRef>,
    index: impl Into<Expr>,
    position: impl Into<Expr>,
    bit: u64,
) -> Expr {
    not_equal(
        bit_and(instruction_account_flags(sysvar, index, position), u64(bit)),
        u64(0),
    )
}

/// Whether account `position` of instruction `index` signs it.
pub fn instruction_account_is_signer(
    sysvar: impl Into<AccountRef>,
    index: impl Into<Expr>,
    position: impl Into<Expr>,
) -> Expr {
    instruction_account_flag(sysvar, index, position, 1)
}

/// Whether account `position` of instruction `index` is writable in it.
pub fn instruction_account_is_writable(
    sysvar: impl Into<AccountRef>,
    index: impl Into<Expr>,
    position: impl Into<Expr>,
) -> Expr {
    instruction_account_flag(sysvar, index, position, 2)
}

/// A typed read of instruction `index`'s data at a `u64` offset.
pub fn instruction_data(
    sysvar: impl Into<AccountRef>,
    index: impl Into<Expr>,
    offset: impl Into<Expr>,
    ty: ReadType,
) -> Expr {
    Expr(Node::InstructionData {
        sysvar: sysvar.into(),
        index: boxed(index),
        offset: boxed(offset),
        ty,
    })
}

/// Exactly `length` bytes (1 to 1,024) of instruction `index`'s data from a `u64` offset.
pub fn instruction_data_bytes(
    sysvar: impl Into<AccountRef>,
    index: impl Into<Expr>,
    offset: impl Into<Expr>,
    length: usize,
) -> Expr {
    Expr(Node::InstructionDataBytes {
        sysvar: sysvar.into(),
        index: boxed(index),
        offset: boxed(offset),
        length,
    })
}

/// The length of a `bytes` value, a `u64`.
pub fn bytes_length(value: impl Into<Expr>) -> Expr {
    Expr(Node::BytesLength(boxed(value)))
}

// ---------------------------------------------------------------------------------- registries

/// A field of the registry entry in fixed account `account`, which is declared with
/// `account::registry`. Typed as the field.
pub fn registry(account: impl Into<String>, field: impl Into<String>) -> Expr {
    Expr(Node::Registry {
        account: account.into(),
        field: field.into(),
    })
}

// ------------------------------------------------------------------------------ account groups

/// How many members the caller supplied in account group `group`, a `u64`.
pub fn group_length(group: impl Into<String>) -> Expr {
    Expr(Node::GroupLength(group.into()))
}

/// Whether any member of account group `group` matches `filter`, a `bool`.
pub fn group_any(group: impl Into<String>, filter: GroupFilter) -> Expr {
    Expr(Node::GroupFilter {
        group: group.into(),
        count: false,
        filter,
    })
}

/// How many members of account group `group` match `filter`, a `u64`.
pub fn group_count(group: impl Into<String>, filter: GroupFilter) -> Expr {
    Expr(Node::GroupFilter {
        group: group.into(),
        count: true,
        filter,
    })
}

// ------------------------------------------------------------------------------ conversions

/// A bare integer is a `u64` constant, as the TypeScript SDK reads a number given for an index,
/// a position or an offset.
impl From<u64> for Expr {
    fn from(value: u64) -> Self {
        u64(value)
    }
}

impl From<u32> for Expr {
    fn from(value: u32) -> Self {
        u64(value.into())
    }
}

impl From<usize> for Expr {
    fn from(value: usize) -> Self {
        u64(value as u64)
    }
}

/// An unsuffixed integer literal is an `i32` to Rust; it still means a `u64` constant here.
impl From<i32> for Expr {
    fn from(value: i32) -> Self {
        match core::primitive::u64::try_from(value) {
            Ok(value) => u64(value),
            Err(_) => Expr::invalid(format!(
                "{value} is not a u64: write expr::i64({value}) for a signed constant"
            )),
        }
    }
}

impl From<&Expr> for Expr {
    fn from(value: &Expr) -> Self {
        value.clone()
    }
}

impl Expr {
    // The methods take `&self`, so a value used twice needs no `.clone()`.

    /// The `bool` is false. Prefer this to `!` before a method chain: `!a.or(b)` negates the
    /// whole `a.or(b)`.
    #[allow(clippy::should_implement_trait)]
    pub fn not(&self) -> Expr {
        not(self.clone())
    }

    /// `self == other`, a `bool`. Pubkeys, booleans and bytes compare too.
    #[allow(clippy::should_implement_trait)]
    pub fn eq(&self, other: impl Into<Expr>) -> Expr {
        equal(self.clone(), other)
    }

    /// `self != other`.
    #[allow(clippy::should_implement_trait)]
    pub fn ne(&self, other: impl Into<Expr>) -> Expr {
        not_equal(self.clone(), other)
    }

    /// `self < other`.
    pub fn lt(&self, other: impl Into<Expr>) -> Expr {
        less_than(self.clone(), other)
    }

    /// `self <= other`.
    pub fn lte(&self, other: impl Into<Expr>) -> Expr {
        less_than_or_equal(self.clone(), other)
    }

    /// `self > other`.
    pub fn gt(&self, other: impl Into<Expr>) -> Expr {
        greater_than(self.clone(), other)
    }

    /// `self >= other`.
    pub fn gte(&self, other: impl Into<Expr>) -> Expr {
        greater_than_or_equal(self.clone(), other)
    }

    /// Both `bool`s hold.
    pub fn and(&self, other: impl Into<Expr>) -> Expr {
        and(self.clone(), other)
    }

    /// Either `bool` holds.
    pub fn or(&self, other: impl Into<Expr>) -> Expr {
        or(self.clone(), other)
    }

    /// The smaller of the two.
    pub fn min(&self, other: impl Into<Expr>) -> Expr {
        min(self.clone(), other)
    }

    /// The larger of the two.
    pub fn max(&self, other: impl Into<Expr>) -> Expr {
        max(self.clone(), other)
    }

    /// `self × numerator ÷ denominator`, exact, rounded down.
    pub fn mul_div(&self, numerator: impl Into<Expr>, denominator: impl Into<Expr>) -> Expr {
        multiply_divide(self.clone(), numerator, denominator)
    }

    /// `self × numerator ÷ denominator`, exact, rounded up.
    pub fn mul_div_up(&self, numerator: impl Into<Expr>, denominator: impl Into<Expr>) -> Expr {
        multiply_divide_up(self.clone(), numerator, denominator)
    }

    /// Converts the value to `to`: see [`cast`].
    pub fn cast(&self, to: Type) -> Expr {
        cast(to, self.clone())
    }
}

macro_rules! binary_operator {
    ($trait:ident, $method:ident, $function:ident) => {
        impl<R: Into<Expr>> ops::$trait<R> for Expr {
            type Output = Expr;

            fn $method(self, right: R) -> Expr {
                $function(self, right)
            }
        }

        impl<R: Into<Expr>> ops::$trait<R> for &Expr {
            type Output = Expr;

            fn $method(self, right: R) -> Expr {
                $function(self.clone(), right)
            }
        }
    };
}

binary_operator!(Add, add, add);
binary_operator!(Sub, sub, subtract);
binary_operator!(Mul, mul, multiply);
binary_operator!(Div, div, divide);
binary_operator!(Rem, rem, remainder);
binary_operator!(BitAnd, bitand, bit_and);
binary_operator!(BitOr, bitor, bit_or);
binary_operator!(BitXor, bitxor, bit_xor);
binary_operator!(Shl, shl, shift_left);
binary_operator!(Shr, shr, shift_right);

impl ops::Not for Expr {
    type Output = Expr;

    fn not(self) -> Expr {
        not(self)
    }
}

impl ops::Not for &Expr {
    type Output = Expr;

    fn not(self) -> Expr {
        not(self.clone())
    }
}
