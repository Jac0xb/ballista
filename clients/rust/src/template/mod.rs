//! Declarative template authoring: the TypeScript SDK's `defineTemplate` and `compileTemplate`,
//! in Rust.
//!
//! A [`Template`] names its inputs, registries, accounts, batch rows and account groups, and lists
//! its steps. [`Template::compile`] checks it as the TypeScript compiler does (names, types, pinned
//! programs and pinned data reads, the wire format's limits) and lowers it to the same bytes,
//! byte for byte: a template written in either language uploads to the same account.
//!
//! The API mirrors the TypeScript one:
//!
//! | TypeScript | Rust |
//! | --- | --- |
//! | `defineTemplate({ inputs, registries, accounts, batch, accountGroups, emitEvent, steps })` | [`Template::new()`] with `.input()`, `.registry()`, `.account()`, `.batch()`, `.account_group()`, `.emit_event()`, `.step()` |
//! | `{ signer: true, writable: true }` | [`account::signer()`]`.writable()` |
//! | `{ executable: true, address }` | [`account::program(address)`](account::program) |
//! | `account.fixed('vault')`, `account.iteration('recipient')` | `"vault"`, [`account::iteration("recipient")`](account::iteration) |
//! | `expression.*` | [`expr`]`::*`, plus operators: `a - b`, `a.gt(b)`, `a.not()` |
//! | `step.*` | [`step`]`::*`; a label is `.label("name")` |
//! | `data.literal(bytes)`, `data.encode('u64', value)` | [`data::literal`], [`data::u64`] |
//! | `systemTransfer`, `tokenTransfer`, `assertAta`, `rateLimit`, ... | [`system_transfer`], [`token_transfer`], [`assert_ata`], [`rate_limit`], ... |
//! | `compileTemplate(template)` | [`Template::compile`] |
//! | `buildRunInstruction({ compiled, inputs, accounts, batchRows, accountGroups })` | [`CompiledTemplate::run`] with `.input()`, `.account()`, `.row()`, `.group()`, then `.instruction()` |
//! | `encodeRunInputs(compiled, inputs)` | [`Run::encode_inputs`] |
//!
//! Names of inputs, accounts, variables and registries are the same camelCase strings in both.
//!
//! ```
//! use ballista_sdk::template::prelude::*;
//!
//! let sweep = Template::new()
//!     .input("reserve", Type::U64)
//!     .account("systemProgram", account::program(SYSTEM_PROGRAM_ID))
//!     .account("vault", account::signer().writable())
//!     .account("destination", account::writable())
//!     .step(step::let_("balance", lamports("vault")))
//!     .step(step::require(var("balance").gt(input("reserve"))).label("aboveReserve"))
//!     .step(system_transfer(
//!         "systemProgram",
//!         "vault",
//!         "destination",
//!         var("balance") - input("reserve"),
//!     ));
//! let compiled = sweep.compile()?;
//! # Ok::<(), ballista_sdk::template::CompileError>(())
//! ```

mod builders;
mod compile;
pub mod expr;
mod helpers;
mod model;
mod reuse;
mod run;
mod validate;

use core::fmt;

pub use builders::{account, data, step};
pub use compile::{CompileStats, CompiledTemplate, SourceMapEntry};
pub use helpers::{
    assert_associated_token_account, assert_ata, assert_pda, create_associated_token_account,
    ed25519_signature, ensure_associated_token_account, rate_limit, system_transfer,
    token_transfer, AssertPda, AtaAccounts, Ed25519Signature, RateLimit,
};
pub use model::{
    Account, AccountField, AccountRef, Batch, DataPart, Encoding, Expr, GroupFilter, Invoke, Loop,
    Offset, ReadType, Step, Template, Type, ValueType,
};
pub use run::{Integer, Row, Run, RunError, RunValue};

/// Everything a template definition and its run use: the types, the `account`, `step` and
/// `data` modules, the helpers, every expression function (`input`, `var`, `lamports`, `u64`,
/// ...), [`Row`] for a run's batch rows, `Pubkey` and `pubkey!`, and the addresses templates pin.
pub mod prelude {
    pub use super::expr::*;
    pub use super::{
        account, assert_associated_token_account, assert_ata, assert_pda,
        create_associated_token_account, data, ed25519_signature, ensure_associated_token_account,
        expr, rate_limit, step, system_transfer, token_transfer, Account, AccountField, AccountRef,
        AtaAccounts, Batch, CompileError, CompiledTemplate, DataPart, Encoding, Expr, GroupFilter,
        Invoke, Loop, Offset, ReadType, Row, RunError, Step, Template, Type,
    };
    pub use crate::ballista_common::instruction::IX_RUN;
    pub use crate::{
        ASSOCIATED_TOKEN_PROGRAM_ID, ED25519_PROGRAM_ID, INSTRUCTIONS_SYSVAR_ID, SYSTEM_PROGRAM_ID,
        TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID,
    };
    pub use solana_program::pubkey;
    pub use solana_program::pubkey::Pubkey;
}

/// Why a template did not compile. The message is the TypeScript compiler's for the same
/// template, except where it names an API: it names the Rust one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompileError {
    message: String,
}

impl CompileError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        CompileError {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for CompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CompileError {}
