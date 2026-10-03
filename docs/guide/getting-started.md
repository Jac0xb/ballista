# Getting started

In this walkthrough you upload a template to a local Solana validator and run it, in TypeScript or
Rust. The template is the one on the [home page](/): it sends everything in a vault above a minimum
balance, the reserve, to another account. The caller picks the reserve; the template reads the
balance while the transaction runs and works out the amount.

Each step has a **Template** tab and a **Run** tab. Follow the Template tab through steps 1 to 4 to
write and upload the template, then follow the Run tab through them to run it. All the code goes in
one file, in that order: in TypeScript a file such as `sweep.mts`, and in Rust the body of
`fn main() -> Result<(), Box<dyn std::error::Error>>` in `src/main.rs`, ending with `Ok(())`.

## Install {#install-the-workspace}

```bash
# TypeScript (Node.js 22 or later). pnpm, yarn, and bun work too.
npm install @jac0xb/ballista @solana/kit

# Rust. ballista-sdk uses solana-program 4.1.0, so the client crates must match it.
cargo new sweep && cd sweep
cargo add ballista-sdk solana-program@=4.1.0 solana-rpc-client@4 solana-keypair@3 \
  solana-signer@3 solana-transaction@4 solana-transaction-error@3 solana-commitment-config@3
```

Then start a local validator with the Ballista program loaded. This needs the
[Solana CLI](https://solana.com/docs/intro/installation), and builds the program from source:

```bash
git clone https://github.com/Jac0xb/ballista.git
cargo build-sbf --manifest-path ballista/programs/ballista/Cargo.toml
solana-test-validator --reset \
  --bpf-program BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD ballista/target/deploy/ballista.so
```

Leave it running. When the file is complete, run it with `npx tsx sweep.mts` or `cargo run`.

## 1. Connect

::: code-group

```ts [TypeScript · Template]
import {
  airdropFactory,
  appendTransactionMessageInstructions,
  assertIsTransactionWithBlockhashLifetime,
  createSolanaRpc,
  createSolanaRpcSubscriptions,
  createTransactionMessage,
  generateKeyPairSigner,
  lamports,
  pipe,
  sendAndConfirmTransactionFactory,
  setTransactionMessageFeePayerSigner,
  setTransactionMessageLifetimeUsingBlockhash,
  signTransactionMessageWithSigners,
  type Instruction,
  type TransactionSigner,
} from '@solana/kit';

// The local validator's default RPC and WebSocket addresses.
const rpc = createSolanaRpc('http://127.0.0.1:8899');
const rpcSubscriptions = createSolanaRpcSubscriptions('ws://127.0.0.1:8900');
const airdrop = airdropFactory({ rpc, rpcSubscriptions });
const sendAndConfirm = sendAndConfirmTransactionFactory({ rpc, rpcSubscriptions });

// A new wallet with 1 SOL from the validator's faucet.
async function fundedSigner(): Promise<TransactionSigner> {
  const signer = await generateKeyPairSigner();
  await airdrop({
    recipientAddress: signer.address,
    lamports: lamports(1_000_000_000n),
    commitment: 'confirmed',
  });
  return signer;
}

// Send instructions in one transaction that `feePayer` signs and pays for.
async function send(feePayer: TransactionSigner, instructions: Instruction[]): Promise<void> {
  const { value: blockhash } = await rpc.getLatestBlockhash().send();
  const message = pipe(
    createTransactionMessage({ version: 0 }),
    (m) => setTransactionMessageFeePayerSigner(feePayer, m),
    (m) => setTransactionMessageLifetimeUsingBlockhash(blockhash, m),
    (m) => appendTransactionMessageInstructions(instructions, m),
  );
  const transaction = await signTransactionMessageWithSigners(message);
  assertIsTransactionWithBlockhashLifetime(transaction);
  await sendAndConfirm(transaction, { commitment: 'confirmed' });
}

// The creator uploads the template and pays the rent for its account.
const creator = await fundedSigner();
```

```ts [TypeScript · Run]
// The vault is the account the template sweeps. It signs the run and pays the fee.
const vault = await fundedSigner();
// Any account can receive the lamports; here, another new wallet.
const destination = (await fundedSigner()).address;
```

```rust [Rust · Template]
use solana_commitment_config::CommitmentConfig;
use solana_keypair::Keypair;
use solana_program::instruction::Instruction;
use solana_rpc_client::rpc_client::RpcClient;
use solana_signer::Signer;
use solana_transaction::Transaction;

// The local validator's default RPC address.
let rpc = RpcClient::new_with_commitment(
    "http://127.0.0.1:8899".to_string(),
    CommitmentConfig::confirmed(),
);

// A new wallet with 1 SOL from the validator's faucet.
let funded_signer = || -> Result<Keypair, Box<dyn std::error::Error>> {
    let signer = Keypair::new();
    let signature = rpc.request_airdrop(&signer.pubkey(), 1_000_000_000)?;
    rpc.poll_for_signature(&signature)?;
    Ok(signer)
};

// Send instructions in one transaction that `fee_payer` signs and pays for.
let send = |fee_payer: &Keypair, instructions: &[Instruction]| {
    let blockhash = rpc.get_latest_blockhash()?;
    let transaction = Transaction::new_signed_with_payer(
        instructions,
        Some(&fee_payer.pubkey()),
        &[fee_payer],
        blockhash,
    );
    rpc.send_and_confirm_transaction(&transaction)
};

// The creator uploads the template and pays the rent for its account.
let creator = funded_signer()?;
```

```rust [Rust · Run]
// The vault is the account the template sweeps. It signs the run and pays the fee.
let vault_keypair = funded_signer()?;
// Any account can receive the lamports; here, another new wallet.
let destination_pubkey = funded_signer()?.pubkey();
```

:::

The Template tab sets up two helpers that the rest of the file uses: `fundedSigner` creates a wallet
and asks the validator's faucet for 1 SOL, which works only on a local validator or devnet, and
`send` signs and sends a transaction. The Run tab uses them to create the vault and the destination.
The creator and the vault are different wallets to show that anyone can run a finalized template;
the creator does not sign runs.

## 2. Define the template

::: code-group

```ts [TypeScript · Template]
import {
  account,
  compileTemplate,
  defineTemplate,
  expression,
  step,
  systemTransfer,
  SYSTEM_PROGRAM_ADDRESS_BYTES,
} from '@jac0xb/ballista';

const sweep = defineTemplate({
  inputs: { reserve: { type: 'u64' } },
  accounts: {
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
    vault: { signer: true, writable: true },
    destination: { writable: true },
  },
  steps: [
    step.let('balance', expression.accountField(account.fixed('vault'), 'lamports')),
    step.require(
      expression.greaterThan(expression.variable('balance'), expression.input('reserve')),
      'aboveReserve',
    ),
    systemTransfer({
      systemProgram: account.fixed('systemProgram'),
      from: account.fixed('vault'),
      to: account.fixed('destination'),
      lamports: expression.subtract(expression.variable('balance'), expression.input('reserve')),
    }),
  ],
});

const compiled = compileTemplate(sweep);
```

```ts [TypeScript · Run]
import { getTemplateAddress } from '@jac0xb/ballista/kit';

// A template's address comes from its creator's address and its template ID.
const [templateAddress] = await getTemplateAddress(creator.address, 7);
```

```rust [Rust · Template]
use ballista_sdk::{
    ballista_common::template::{
        ACCOUNT_EXECUTABLE, ACCOUNT_SIGNER, ACCOUNT_WRITABLE, DATA_REG_U64, OP_GT, OP_SUB,
        VALUE_U64,
    },
    ProgramBuilder, Segment, SYSTEM_PROGRAM_ID,
};

let mut builder = ProgramBuilder::new();
let system = builder.account(ACCOUNT_EXECUTABLE, Some(SYSTEM_PROGRAM_ID.to_bytes()), None, 0);
let vault = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
let destination = builder.account(ACCOUNT_WRITABLE, None, None, 0);
let reserve_input = builder.input(VALUE_U64, 0);

let reserve = builder.load_input(reserve_input);
let balance = builder.account_lamports(vault);
let above_reserve = builder.binary(OP_GT, balance, reserve);
builder.require(above_reserve);
let amount = builder.binary(OP_SUB, balance, reserve);
let discriminator = builder.blob(&[2, 0, 0, 0]); // SystemInstruction::Transfer
let transfer = builder.cpi(
    system,
    &[
        (vault, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
        (destination, ACCOUNT_WRITABLE),
    ],
    &[
        Segment::Literal(discriminator),
        Segment::Register(DATA_REG_U64, amount),
    ],
);
builder.invoke(transfer, None);

let payload = builder.build()?;
```

```rust [Rust · Run]
// A template's address comes from its creator's address and its template ID.
let (template, _) = ballista_sdk::find_template_pda(&creator.pubkey(), 7);
```

:::

The template takes one input, `reserve`, and three accounts. Each account entry says what the
caller's account must be: `signer` means it must sign the transaction, `writable` that the
transaction must let it change, `executable` that it must be a program, and `address` fixes it to
one exact address. The steps read the vault's balance in lamports (one SOL is 1,000,000,000
lamports), stop the run unless it is above the reserve, and transfer the difference. The label
`'aboveReserve'` names the check in error messages.

Compiling is deterministic, and the Rust builder produces the same bytes; [Author it in
Rust](#author-it-in-rust) explains the difference. The Run tab derives the template's address, a
[PDA](/reference/glossary#pda) (program-derived address) of the Ballista program. `7` is the template ID, which the creator
picks: any number from 0 to 65,535 that the creator has not used yet.

## 3. Upload and run

::: code-group

```ts [TypeScript · Template]
import { buildKitTemplateUploadPlan } from '@jac0xb/ballista/kit';

const upload = await buildKitTemplateUploadPlan({
  compiled,
  creator: creator.address,
  templateId: 7,
});
for (const { instruction } of upload.instructions) {
  await send(creator, [instruction]);
}
console.log('uploaded to', upload.templateAddress);
```

```ts [TypeScript · Run]
import {
  BALLISTA_ADDRESS,
  SYSTEM_PROGRAM_ADDRESS,
  buildKitRunInstruction,
} from '@jac0xb/ballista/kit';

// The caller picks the reserve; the template works out the amount.
function sweepInstruction(reserve: bigint) {
  return buildKitRunInstruction({
    compiled,
    programAddress: BALLISTA_ADDRESS,
    templateAddress,
    inputs: { reserve },
    accounts: {
      systemProgram: { address: SYSTEM_PROGRAM_ADDRESS },
      vault: { address: vault.address },
      destination: { address: destination },
    },
  });
}

await send(vault, [sweepInstruction(2_000_000n)]);
const { value: left } = await rpc.getBalance(vault.address).send();
console.log('vault keeps', left); // 2000000n
```

```rust [Rust · Template]
use ballista_sdk::create_template_instruction;

let create = create_template_instruction(creator.pubkey(), 7, &payload);
send(&creator, &[create])?;
```

```rust [Rust · Run]
use ballista_sdk::{run_instruction, RunInputs};
use solana_program::instruction::AccountMeta;

// The caller picks the reserve; the template works out the amount.
let sweep_instruction = |reserve: u64| {
    run_instruction(
        template,
        vec![
            AccountMeta::new_readonly(ballista_sdk::SYSTEM_PROGRAM_ID, false),
            AccountMeta::new(vault_keypair.pubkey(), true),
            AccountMeta::new(destination_pubkey, false),
        ],
        &RunInputs::new().u64(reserve).finish(),
    )
};

send(&vault_keypair, &[sweep_instruction(2_000_000)])?;
println!("vault keeps {}", rpc.get_balance(&vault_keypair.pubkey())?); // 2000000
```

:::

This template is small, so its upload is a single `CreateTemplate` instruction that stores, checks,
and finalizes it. A larger template is uploaded in several transactions; the TypeScript plan then
holds all of them, and [Template lifecycle](/guide/template-lifecycle) covers the Rust calls.

The run instruction lists the template account first (both SDKs add it), then the template's
accounts in the order it declares them, and carries the inputs in declaration order. The
TypeScript builder takes them by name and checks them against `compiled`; in Rust you pass them in
order yourself. The vault signs because the template declares it as a signer. The run leaves the
vault with exactly the 2,000,000-lamport reserve.

## 4. Check the result

::: code-group

```ts [TypeScript · Template]
import { fetchEncodedAccount } from '@solana/kit';
import { decodeTemplateAccount } from '@jac0xb/ballista';

const stored = await fetchEncodedAccount(rpc, upload.templateAddress);
if (!stored.exists) throw new Error('No template account');
const decoded = decodeTemplateAccount(stored.data);
console.log(decoded.state === 1 ? 'finalized' : 'still uploading', decoded.payloadLength, 'bytes');
```

```ts [TypeScript · Run]
import { isSolanaError, SOLANA_ERROR__INSTRUCTION_ERROR__CUSTOM } from '@solana/kit';
import { explainRunError } from '@jac0xb/ballista';

// The vault now holds less than 5,000,000 lamports, so the check fails.
try {
  await send(vault, [sweepInstruction(5_000_000n)]);
} catch (error) {
  const cause = error instanceof Error ? error.cause : undefined;
  if (!isSolanaError(cause, SOLANA_ERROR__INSTRUCTION_ERROR__CUSTOM)) throw error;
  console.log(cause.context.code, explainRunError(cause.context.code, compiled)?.message);
  // 202623 RequirementFailed at steps[1] (aboveReserve)
}
```

```rust [Rust · Template]
use ballista_sdk::ballista_common::template::TemplateAccount;

let (template_address, _) = ballista_sdk::find_template_pda(&creator.pubkey(), 7);
let data = rpc.get_account_data(&template_address)?;
let stats = TemplateAccount::parse(&data)?.finalized_program()?.verify()?;
println!("finalized, {} bytecode instructions", stats.instructions);
```

```rust [Rust · Run]
use ballista_sdk::decode_ballista_error;
use solana_program::instruction::InstructionError;
use solana_transaction_error::TransactionError;

// The vault now holds less than 5,000,000 lamports, so the check fails.
let error = send(&vault_keypair, &[sweep_instruction(5_000_000)]).unwrap_err();
if let Some(TransactionError::InstructionError(_, InstructionError::Custom(code))) =
    error.get_transaction_error()
{
    if let Some(decoded) = decode_ballista_error(code) {
        println!("{code}: {} (context {})", decoded.name, decoded.context);
        // 202623: RequirementFailed (context 3)
    }
}
```

:::

The Template tab reads the template account back and confirms it is finalized: checked and locked,
so it can never change.

The Run tab asks for a reserve larger than the vault's balance, so the `require` step stops the
run and nothing moves. The transaction fails with custom error 202623, which is `0x0003177F`. Its
low 16 bits, 6015, are the error kind `RequirementFailed`, and its high 16 bits, 3, are the context,
which says where the failure happened. For `RequirementFailed` the context is the program counter:
the index of the failing bytecode instruction. For other kinds it can be an account index or an
input index instead. `explainRunError` uses the compiled template's source map to name the step,
`steps[1]`, and its label. In Rust, `decode_ballista_error` splits the code the same way; without
the source map you read the context yourself. [Errors and events](/guide/errors-and-events) lists
which context each kind carries.

## Author it in Rust

The Rust Template tabs build the template with `ProgramBuilder`. It works at a lower level than
the TypeScript compiler: you declare accounts and inputs in order, and each value lives in a
[register](/reference/glossary#register), a numbered slot that holds a value during a run. For
this template the builder's output is byte-identical to the compiler's.

The TypeScript compiler also checks that every program the template calls has a fixed address and
that every account whose data it reads has a fixed owner or address. It works out each account's
minimum data length, and it records which step produced each instruction so that errors can name
the step. With the Rust builder, those decisions are yours; see [Pins](/guide/trust-model#pins). A
template built either way can be uploaded and run from either language.

## Run the shipped examples

```bash
pnpm --dir clients/js exec tsx examples/transfer.ts        # compile and print the payload
pnpm --dir clients/js exec tsx examples/run-transfer.ts    # build a Solana Kit run instruction offline
cargo run -p ballista-sdk --example author_template        # build two templates in Rust
cargo run -p ballista-sdk --example run_template           # encode inputs and decode errors in Rust
```

Run these from the repository you cloned. The Rust `author_template` example also builds a payroll
template that keeps a running total across rows and enforces a budget. [Batch
execution](/guide/batching#carry-a-total-across-rows) shows the TypeScript version.

## Next

- See [what templates can do](/guide/runtime-values) that a transaction cannot.
- Learn the [template lifecycle](/guide/template-lifecycle).
- Add [snapshots and assertions](/guide/assertions).
- Build a [30-recipient payroll](/examples/payments#bounded-sol-payroll).
- Send large runs in a [version 1 transaction](/guide/transaction-v1), which allows up to 4,096
  bytes.
