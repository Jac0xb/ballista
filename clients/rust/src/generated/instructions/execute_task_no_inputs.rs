//! This code was AUTOGENERATED using the codama library.
//! Please DO NOT EDIT THIS FILE, instead use visitors
//! to add features, then rerun codama to update it.
//!
//! <https://github.com/codama-idl/codama>
//!

use borsh::BorshDeserialize;
use borsh::BorshSerialize;

/// Accounts.
pub struct ExecuteTaskNoInputs {
    /// Payer account
    pub payer: solana_program::pubkey::Pubkey,
    /// Task definition account
    pub task_definition: solana_program::pubkey::Pubkey,
}

impl ExecuteTaskNoInputs {
    pub fn instruction(&self) -> solana_program::instruction::Instruction {
        self.instruction_with_remaining_accounts(&[])
    }
    #[allow(clippy::vec_init_then_push)]
    pub fn instruction_with_remaining_accounts(
        &self,
        remaining_accounts: &[solana_program::instruction::AccountMeta],
    ) -> solana_program::instruction::Instruction {
        let mut accounts = Vec::with_capacity(2 + remaining_accounts.len());
        accounts.push(solana_program::instruction::AccountMeta::new(
            self.payer, true,
        ));
        accounts.push(solana_program::instruction::AccountMeta::new_readonly(
            self.task_definition,
            false,
        ));
        accounts.extend_from_slice(remaining_accounts);
        let data = ExecuteTaskNoInputsInstructionData::new()
            .try_to_vec()
            .unwrap();

        solana_program::instruction::Instruction {
            program_id: crate::BALLISTA_ID,
            accounts,
            data,
        }
    }
}

#[derive(BorshDeserialize, BorshSerialize)]
pub struct ExecuteTaskNoInputsInstructionData {
    discriminator: u8,
}

impl ExecuteTaskNoInputsInstructionData {
    pub fn new() -> Self {
        Self { discriminator: 2 }
    }
}

impl Default for ExecuteTaskNoInputsInstructionData {
    fn default() -> Self {
        Self::new()
    }
}

/// Instruction builder for `ExecuteTaskNoInputs`.
///
/// ### Accounts:
///
///   0. `[writable, signer]` payer
///   1. `[]` task_definition
#[derive(Clone, Debug, Default)]
pub struct ExecuteTaskNoInputsBuilder {
    payer: Option<solana_program::pubkey::Pubkey>,
    task_definition: Option<solana_program::pubkey::Pubkey>,
    __remaining_accounts: Vec<solana_program::instruction::AccountMeta>,
}

impl ExecuteTaskNoInputsBuilder {
    pub fn new() -> Self {
        Self::default()
    }
    /// Payer account
    #[inline(always)]
    pub fn payer(&mut self, payer: solana_program::pubkey::Pubkey) -> &mut Self {
        self.payer = Some(payer);
        self
    }
    /// Task definition account
    #[inline(always)]
    pub fn task_definition(
        &mut self,
        task_definition: solana_program::pubkey::Pubkey,
    ) -> &mut Self {
        self.task_definition = Some(task_definition);
        self
    }
    /// Add an additional account to the instruction.
    #[inline(always)]
    pub fn add_remaining_account(
        &mut self,
        account: solana_program::instruction::AccountMeta,
    ) -> &mut Self {
        self.__remaining_accounts.push(account);
        self
    }
    /// Add additional accounts to the instruction.
    #[inline(always)]
    pub fn add_remaining_accounts(
        &mut self,
        accounts: &[solana_program::instruction::AccountMeta],
    ) -> &mut Self {
        self.__remaining_accounts.extend_from_slice(accounts);
        self
    }
    #[allow(clippy::clone_on_copy)]
    pub fn instruction(&self) -> solana_program::instruction::Instruction {
        let accounts = ExecuteTaskNoInputs {
            payer: self.payer.expect("payer is not set"),
            task_definition: self.task_definition.expect("task_definition is not set"),
        };

        accounts.instruction_with_remaining_accounts(&self.__remaining_accounts)
    }
}

/// `execute_task_no_inputs` CPI accounts.
pub struct ExecuteTaskNoInputsCpiAccounts<'a, 'b> {
    /// Payer account
    pub payer: &'b solana_program::account_info::AccountInfo<'a>,
    /// Task definition account
    pub task_definition: &'b solana_program::account_info::AccountInfo<'a>,
}

/// `execute_task_no_inputs` CPI instruction.
pub struct ExecuteTaskNoInputsCpi<'a, 'b> {
    /// The program to invoke.
    pub __program: &'b solana_program::account_info::AccountInfo<'a>,
    /// Payer account
    pub payer: &'b solana_program::account_info::AccountInfo<'a>,
    /// Task definition account
    pub task_definition: &'b solana_program::account_info::AccountInfo<'a>,
}

impl<'a, 'b> ExecuteTaskNoInputsCpi<'a, 'b> {
    pub fn new(
        program: &'b solana_program::account_info::AccountInfo<'a>,
        accounts: ExecuteTaskNoInputsCpiAccounts<'a, 'b>,
    ) -> Self {
        Self {
            __program: program,
            payer: accounts.payer,
            task_definition: accounts.task_definition,
        }
    }
    #[inline(always)]
    pub fn invoke(&self) -> solana_program::entrypoint::ProgramResult {
        self.invoke_signed_with_remaining_accounts(&[], &[])
    }
    #[inline(always)]
    pub fn invoke_with_remaining_accounts(
        &self,
        remaining_accounts: &[(
            &'b solana_program::account_info::AccountInfo<'a>,
            bool,
            bool,
        )],
    ) -> solana_program::entrypoint::ProgramResult {
        self.invoke_signed_with_remaining_accounts(&[], remaining_accounts)
    }
    #[inline(always)]
    pub fn invoke_signed(
        &self,
        signers_seeds: &[&[&[u8]]],
    ) -> solana_program::entrypoint::ProgramResult {
        self.invoke_signed_with_remaining_accounts(signers_seeds, &[])
    }
    #[allow(clippy::clone_on_copy)]
    #[allow(clippy::vec_init_then_push)]
    pub fn invoke_signed_with_remaining_accounts(
        &self,
        signers_seeds: &[&[&[u8]]],
        remaining_accounts: &[(
            &'b solana_program::account_info::AccountInfo<'a>,
            bool,
            bool,
        )],
    ) -> solana_program::entrypoint::ProgramResult {
        let mut accounts = Vec::with_capacity(2 + remaining_accounts.len());
        accounts.push(solana_program::instruction::AccountMeta::new(
            *self.payer.key,
            true,
        ));
        accounts.push(solana_program::instruction::AccountMeta::new_readonly(
            *self.task_definition.key,
            false,
        ));
        remaining_accounts.iter().for_each(|remaining_account| {
            accounts.push(solana_program::instruction::AccountMeta {
                pubkey: *remaining_account.0.key,
                is_signer: remaining_account.1,
                is_writable: remaining_account.2,
            })
        });
        let data = ExecuteTaskNoInputsInstructionData::new()
            .try_to_vec()
            .unwrap();

        let instruction = solana_program::instruction::Instruction {
            program_id: crate::BALLISTA_ID,
            accounts,
            data,
        };
        let mut account_infos = Vec::with_capacity(2 + 1 + remaining_accounts.len());
        account_infos.push(self.__program.clone());
        account_infos.push(self.payer.clone());
        account_infos.push(self.task_definition.clone());
        remaining_accounts
            .iter()
            .for_each(|remaining_account| account_infos.push(remaining_account.0.clone()));

        if signers_seeds.is_empty() {
            solana_program::program::invoke(&instruction, &account_infos)
        } else {
            solana_program::program::invoke_signed(&instruction, &account_infos, signers_seeds)
        }
    }
}

/// Instruction builder for `ExecuteTaskNoInputs` via CPI.
///
/// ### Accounts:
///
///   0. `[writable, signer]` payer
///   1. `[]` task_definition
#[derive(Clone, Debug)]
pub struct ExecuteTaskNoInputsCpiBuilder<'a, 'b> {
    instruction: Box<ExecuteTaskNoInputsCpiBuilderInstruction<'a, 'b>>,
}

impl<'a, 'b> ExecuteTaskNoInputsCpiBuilder<'a, 'b> {
    pub fn new(program: &'b solana_program::account_info::AccountInfo<'a>) -> Self {
        let instruction = Box::new(ExecuteTaskNoInputsCpiBuilderInstruction {
            __program: program,
            payer: None,
            task_definition: None,
            __remaining_accounts: Vec::new(),
        });
        Self { instruction }
    }
    /// Payer account
    #[inline(always)]
    pub fn payer(&mut self, payer: &'b solana_program::account_info::AccountInfo<'a>) -> &mut Self {
        self.instruction.payer = Some(payer);
        self
    }
    /// Task definition account
    #[inline(always)]
    pub fn task_definition(
        &mut self,
        task_definition: &'b solana_program::account_info::AccountInfo<'a>,
    ) -> &mut Self {
        self.instruction.task_definition = Some(task_definition);
        self
    }
    /// Add an additional account to the instruction.
    #[inline(always)]
    pub fn add_remaining_account(
        &mut self,
        account: &'b solana_program::account_info::AccountInfo<'a>,
        is_writable: bool,
        is_signer: bool,
    ) -> &mut Self {
        self.instruction
            .__remaining_accounts
            .push((account, is_writable, is_signer));
        self
    }
    /// Add additional accounts to the instruction.
    ///
    /// Each account is represented by a tuple of the `AccountInfo`, a `bool` indicating whether the account is writable or not,
    /// and a `bool` indicating whether the account is a signer or not.
    #[inline(always)]
    pub fn add_remaining_accounts(
        &mut self,
        accounts: &[(
            &'b solana_program::account_info::AccountInfo<'a>,
            bool,
            bool,
        )],
    ) -> &mut Self {
        self.instruction
            .__remaining_accounts
            .extend_from_slice(accounts);
        self
    }
    #[inline(always)]
    pub fn invoke(&self) -> solana_program::entrypoint::ProgramResult {
        self.invoke_signed(&[])
    }
    #[allow(clippy::clone_on_copy)]
    #[allow(clippy::vec_init_then_push)]
    pub fn invoke_signed(
        &self,
        signers_seeds: &[&[&[u8]]],
    ) -> solana_program::entrypoint::ProgramResult {
        let instruction = ExecuteTaskNoInputsCpi {
            __program: self.instruction.__program,

            payer: self.instruction.payer.expect("payer is not set"),

            task_definition: self
                .instruction
                .task_definition
                .expect("task_definition is not set"),
        };
        instruction.invoke_signed_with_remaining_accounts(
            signers_seeds,
            &self.instruction.__remaining_accounts,
        )
    }
}

#[derive(Clone, Debug)]
struct ExecuteTaskNoInputsCpiBuilderInstruction<'a, 'b> {
    __program: &'b solana_program::account_info::AccountInfo<'a>,
    payer: Option<&'b solana_program::account_info::AccountInfo<'a>>,
    task_definition: Option<&'b solana_program::account_info::AccountInfo<'a>>,
    /// Additional instruction accounts `(AccountInfo, is_writable, is_signer)`.
    __remaining_accounts: Vec<(
        &'b solana_program::account_info::AccountInfo<'a>,
        bool,
        bool,
    )>,
}
