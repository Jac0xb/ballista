//! The `step`, `data` and `account` modules: the TypeScript SDK's `step.*`, `data.*` and
//! `account.*`.

/// Steps: the TypeScript SDK's `step.*`. Every step takes a label with `.label(...)`.
pub mod step {
    use super::super::model::*;

    /// Fails the run unless `condition`, a `bool`, holds. `require(a.and(b))` checks `a` and `b`
    /// as two requirements, which costs no `and` instruction.
    pub fn require(condition: impl Into<Expr>) -> Step {
        Step(StepNode::Require {
            condition: condition.into(),
            label: None,
        })
    }

    /// Binds `name` to `value`, computed once, here. `let` is a Rust keyword, hence the
    /// underscore.
    pub fn let_(name: impl Into<String>, value: impl Into<Expr>) -> Step {
        Step(StepNode::Let {
            name: name.into(),
            value: value.into(),
            label: None,
        })
    }

    /// Binds `name` to `value` before a later step changes it: the same as [`let_`], named for
    /// reading balances before an invocation and comparing after.
    pub fn snapshot(name: impl Into<String>, value: impl Into<Expr>) -> Step {
        let_(name, value)
    }

    /// Rewrites a variable the enclosing loop carries.
    pub fn assign(name: impl Into<String>, value: impl Into<Expr>) -> Step {
        Step(StepNode::Assign {
            name: name.into(),
            value: value.into(),
            label: None,
        })
    }

    /// Invokes the program in account `program`. Add its accounts and data with the builder:
    ///
    /// ```
    /// use ballista_sdk::template::prelude::*;
    /// use ballista_sdk::TOKEN_PROGRAM_ID;
    ///
    /// let transfer = step::invoke("tokenProgram")
    ///     .program_address(TOKEN_PROGRAM_ID)
    ///     .writable("source")
    ///     .writable("destination")
    ///     .signer("authority")
    ///     .data(data::literal([3]))
    ///     .data(data::u64(input("amount")));
    /// ```
    pub fn invoke(program: impl Into<AccountRef>) -> Invoke {
        Invoke {
            program: program.into(),
            accounts: Vec::new(),
            data: Vec::new(),
            when: None,
            account_group: None,
            program_address: None,
            label: None,
        }
    }

    /// Logs the parts, encoded as invocation data is, as one `Program data:` field. The first part
    /// is a literal tag of at least 4 bytes that does not start with `BEV`, the run event's.
    pub fn emit(parts: impl IntoIterator<Item = DataPart>) -> Step {
        Step(StepNode::Emit {
            parts: parts.into_iter().collect(),
            label: None,
        })
    }

    /// Sets the parts, encoded as invocation data is, as the run's return data: once, outside
    /// every loop, after the last invocation.
    pub fn set_return_data(parts: impl IntoIterator<Item = DataPart>) -> Step {
        Step(StepNode::SetReturnData {
            parts: parts.into_iter().collect(),
            label: None,
        })
    }

    /// Writes `value` into a field of the registry entry in fixed account `account`.
    pub fn set_registry(
        account: impl Into<String>,
        field: impl Into<String>,
        value: impl Into<Expr>,
    ) -> Step {
        Step(StepNode::SetRegistry {
            account: account.into(),
            field: field.into(),
            value: value.into(),
            label: None,
        })
    }

    /// Runs the body once per batch row. Add the body with `.step(...)` and carried variables
    /// with `.carry(...)`.
    pub fn for_each() -> Loop {
        Loop {
            repeat: None,
            steps: Vec::new(),
            carry: None,
            label: None,
        }
    }

    /// Runs the body `count` times, a `u64` read once before the first pass. A run whose count is
    /// above `max` (1 to 255) fails.
    pub fn repeat(count: impl Into<Expr>, max: u8) -> Loop {
        Loop {
            repeat: Some((count.into(), max.into())),
            steps: Vec::new(),
            carry: None,
            label: None,
        }
    }
}

/// Invocation data, logs and return data: the TypeScript SDK's `data.*`.
pub mod data {
    use super::super::model::*;

    /// Literal bytes, such as an instruction discriminator.
    pub fn literal(bytes: impl AsRef<[u8]>) -> DataPart {
        DataPart(DataNode::Literal(bytes.as_ref().to_vec()))
    }

    /// `value` written with `encoding`.
    pub fn encode(encoding: Encoding, value: impl Into<Expr>) -> DataPart {
        DataPart(DataNode::Encoded(encoding, value.into()))
    }

    /// A `u64` or `u128` value as one little-endian byte; a larger value fails the run.
    pub fn u8(value: impl Into<Expr>) -> DataPart {
        encode(Encoding::U8, value)
    }

    pub fn u16(value: impl Into<Expr>) -> DataPart {
        encode(Encoding::U16, value)
    }

    pub fn u32(value: impl Into<Expr>) -> DataPart {
        encode(Encoding::U32, value)
    }

    pub fn u64(value: impl Into<Expr>) -> DataPart {
        encode(Encoding::U64, value)
    }

    pub fn i64(value: impl Into<Expr>) -> DataPart {
        encode(Encoding::I64, value)
    }

    pub fn u128(value: impl Into<Expr>) -> DataPart {
        encode(Encoding::U128, value)
    }

    pub fn pubkey(value: impl Into<Expr>) -> DataPart {
        encode(Encoding::Pubkey, value)
    }

    pub fn bool(value: impl Into<Expr>) -> DataPart {
        encode(Encoding::Bool, value)
    }

    /// A `bytes` value, as many bytes as it holds.
    pub fn bytes(value: impl Into<Expr>) -> DataPart {
        encode(Encoding::Bytes, value)
    }
}

/// Account declarations and references: the TypeScript SDK's `account.*`.
pub mod account {
    use solana_program::pubkey::Pubkey;

    use super::super::model::*;

    /// A reference to fixed account `name`. A plain `&str` converts to the same thing.
    pub fn fixed(name: impl Into<String>) -> AccountRef {
        AccountRef::Fixed(name.into())
    }

    /// A reference to account `name` of the current batch row; valid inside `step::for_each`.
    pub fn iteration(name: impl Into<String>) -> AccountRef {
        AccountRef::Iteration(name.into())
    }

    /// Any account, read-only, not a signer: the TypeScript SDK's `{}`.
    pub fn readonly() -> Account {
        Account::new()
    }

    /// An account that signs: `{ signer: true }`.
    pub fn signer() -> Account {
        Account::new().signer()
    }

    /// A writable account: `{ writable: true }`.
    pub fn writable() -> Account {
        Account::new().writable()
    }

    /// An executable account: `{ executable: true }`. Pin it with `.address(...)`, or opt out with
    /// `.unsafe_unpinned()`.
    pub fn executable() -> Account {
        Account::new().executable()
    }

    /// A program pinned to `address`: `{ executable: true, address }`.
    pub fn program(address: impl Into<Pubkey>) -> Account {
        Account::new().executable().address(address)
    }

    /// The System program, pinned: a template with registry accounts declares it.
    pub fn system_program() -> Account {
        program(Pubkey::new_from_array([0; 32]))
    }

    /// An account holding an entry of `registry`, opened (and created on first use, with
    /// `payer`'s lamports) before the first step. `payer` is a fixed account declared signer and
    /// writable. Key the entry with `.key(expr)`; without a key the account holds the registry's
    /// one template-wide entry.
    pub fn registry(registry: impl Into<String>, payer: impl Into<String>) -> Account {
        let mut account = Account::new().writable();
        account.registry = Some(RegistrySpec {
            name: registry.into(),
            key: None,
            payer: payer.into(),
        });
        account
    }
}
