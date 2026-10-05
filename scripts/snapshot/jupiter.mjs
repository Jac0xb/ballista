/**
 * Jupiter Swap API (v1) for the protocol-test snapshot: a quote, then its instructions, forced to
 * the `route` instruction the live-protocol templates invoke. Plain `fetch`, no dependencies.
 *
 * Three things decide which instruction Jupiter returns, and all three are pinned here:
 * - `useSharedAccounts: false` in the swap-instructions body. Left out, the router sometimes
 *   picks `shared_accounts_route`, whose accounts are ordered differently.
 * - `instructionVersion=V1` in the quote (`V2` gives `route_v2`).
 * - `swapMode=ExactIn` (`ExactOut` gives `exact_out_route`).
 * The instruction's first eight bytes are checked against `route`'s discriminator anyway.
 */
import { createHash } from 'node:crypto';
import { fetchWithRetry, parseJson, stringifyJson } from './rpc.mjs';

export const JUPITER_V6 = 'JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4';
export const ASSOCIATED_TOKEN_PROGRAM = 'ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL';
export const BALLISTA_PROGRAM = 'BLSTAmUBA29tcRUvoq5DBYxRhGptrnWPtfQW65RszRWR';

const API_KEY = process.env.JUPITER_API_KEY || '';
/** Keyless `lite-api.jup.ag` by default; `api.jup.ag` when a key is set. */
export const JUPITER_API = API_KEY
  ? 'https://api.jup.ag/swap/v1'
  : 'https://lite-api.jup.ag/swap/v1';

/** An Anchor instruction discriminator: `sha256("global:<name>")[..8]`, as hex. */
const discriminator = (name) =>
  createHash('sha256').update(`global:${name}`).digest().subarray(0, 8).toString('hex');

export const ROUTE_DISCRIMINATOR = discriminator('route');
const JUPITER_INSTRUCTIONS = Object.fromEntries(
  [
    'route',
    'route_with_token_ledger',
    'shared_accounts_route',
    'shared_accounts_route_with_token_ledger',
    'exact_out_route',
    'shared_accounts_exact_out_route',
    'route_v2',
    'shared_accounts_route_v2',
    'exact_out_route_v2',
    'shared_accounts_exact_out_route_v2',
  ].map((name) => [discriminator(name), name]),
);

/** `route`'s data ends `in_amount u64, quoted_out_amount u64, slippage_bps u16, platform_fee_bps u8`. */
const ROUTE_TAIL_LENGTH = 19;
/** The accounts `route` lists before the swaps' own, per the jupiter-cpi IDL. */
const ROUTE_FIXED_ACCOUNTS = 9;

// ------------------------------------------------------------------------------------- API

function jupiter(path, init = {}) {
  const url = `${JUPITER_API}${path}`;
  const headers = { 'content-type': 'application/json', ...(API_KEY && { 'x-api-key': API_KEY }) };
  return fetchWithRetry(`Jupiter ${path.split('?')[0]}`, url, { ...init, headers }, (text, status) => {
    if (status !== 200) throw new Error(`Jupiter ${path.split('?')[0]}: HTTP ${status} ${text.slice(0, 400)}`);
    return { value: parseJson(text) };
  });
}

/** Jupiter's dex labels, `label → program id`, from `/program-id-to-label`. */
export async function dexPrograms() {
  const byProgram = await jupiter('/program-id-to-label');
  return new Map(Object.entries(byProgram).map(([program, label]) => [label, program]));
}

/**
 * Quote one ExactIn swap and fetch its instructions for `wallet`.
 *
 * `wrapAndUnwrapSol` stays on, so for SOL legs the setup wraps (create the wSOL account, transfer,
 * `SyncNative`) and the cleanup unwraps. `skipUserAccountsRpcCalls` keeps Jupiter from looking
 * the test wallet up on mainnet. Returns the quote verbatim, every instruction verbatim, the
 * lookup tables, `route`'s decoded arguments, and every account any instruction references.
 */
export async function fetchRouteLeg({ wallet, inputMint, outputMint, amount, dexes, maxAccounts, slippageBps }) {
  const request = {
    inputMint,
    outputMint,
    amount: String(amount),
    swapMode: 'ExactIn',
    slippageBps: String(slippageBps),
    maxAccounts: String(maxAccounts),
    instructionVersion: 'V1',
    dexes: dexes.join(','),
  };
  const quote = await jupiter(`/quote?${new URLSearchParams(request)}`);
  if (!Array.isArray(quote.routePlan) || quote.routePlan.length === 0) {
    throw new Error(`Jupiter quote has no route plan: ${stringifyJson(quote).slice(0, 400)}`);
  }
  const response = await jupiter('/swap-instructions', {
    method: 'POST',
    body: stringifyJson({
      userPublicKey: wallet,
      quoteResponse: quote,
      useSharedAccounts: false,
      wrapAndUnwrapSol: true,
      dynamicComputeUnitLimit: false,
      skipUserAccountsRpcCalls: true,
    }),
  });
  if (response.error || !response.swapInstruction) {
    throw new Error(`Jupiter swap-instructions: ${stringifyJson(response).slice(0, 400)}`);
  }

  const instructions = {
    computeBudget: response.computeBudgetInstructions ?? [],
    setup: response.setupInstructions ?? [],
    tokenLedger: response.tokenLedgerInstruction ?? null,
    swap: response.swapInstruction,
    cleanup: response.cleanupInstruction ?? null,
    other: response.otherInstructions ?? [],
  };
  const swap = instructions.swap;
  const data = Buffer.from(swap.data, 'base64');
  const found = data.subarray(0, 8).toString('hex');
  if (swap.programId !== JUPITER_V6 || found !== ROUTE_DISCRIMINATOR) {
    const name = JUPITER_INSTRUCTIONS[found] ?? `discriminator ${found}`;
    throw new Error(`Jupiter returned ${name} on ${swap.programId}, not Jupiter v6 \`route\``);
  }
  const route = splitRouteData(data);
  if (route.inAmount !== BigInt(request.amount)) {
    throw new Error(`route in_amount ${route.inAmount} is not the ${request.amount} requested`);
  }

  // `route` fixes nine accounts before the swaps' own: token_program, user_transfer_authority (the
  // one signer), user_source_token_account, user_destination_token_account,
  // destination_token_account, destination_mint, platform_fee_account, event_authority, program.
  const count = Array.isArray(swap.accounts) ? swap.accounts.length : 0;
  if (count <= ROUTE_FIXED_ACCOUNTS) {
    throw new Error(
      `Jupiter's route for ${inputMint} → ${outputMint} has ${count} accounts, but route fixes ` +
        `${ROUTE_FIXED_ACCOUNTS} before any swap's own, so the Swap API's account layout has changed. ` +
        'Compare a fresh swap-instructions response with the jupiter-cpi IDL (github.com/jup-ag/jupiter-cpi), ' +
        'and update this check and the templates that pass route accounts ' +
        '(JUPITER_ROUTE_FIXED_ACCOUNTS in clients/js/examples/protocols/shared.ts) before snapshotting.',
    );
  }
  const [, authority, source, destination] = swap.accounts;
  if (authority?.pubkey !== wallet || !authority.isSigner) {
    throw new Error(`route's authority is ${authority?.pubkey}, not the wallet ${wallet}`);
  }
  const created = instructions.setup
    .filter((ix) => ix.programId === ASSOCIATED_TOKEN_PROGRAM && ix.accounts[2]?.pubkey === wallet)
    .map((ix) => ix.accounts[1].pubkey);

  const accounts = new Map();
  const reference = (address, kind) =>
    accounts.set(address, (accounts.get(address) ?? new Set()).add(kind));
  for (const [kind, list] of Object.entries(instructions)) {
    for (const ix of [list].flat().filter(Boolean)) {
      reference(ix.programId, kind);
      for (const meta of ix.accounts) reference(meta.pubkey, kind);
    }
  }

  return {
    request,
    quote,
    instructions,
    addressLookupTableAddresses: response.addressLookupTableAddresses ?? [],
    route,
    hops: quote.routePlan.map(({ swapInfo, percent, bps }) => ({
      label: swapInfo.label,
      ammKey: swapInfo.ammKey,
      inputMint: swapInfo.inputMint,
      outputMint: swapInfo.outputMint,
      inAmount: swapInfo.inAmount,
      outAmount: swapInfo.outAmount,
      percent,
      ...(bps != null && { bps }),
    })),
    walletAccounts: {
      authority: wallet,
      sourceTokenAccount: source.pubkey,
      destinationTokenAccount: destination.pubkey,
      createdTokenAccounts: created,
    },
    accounts,
  };
}

/**
 * `route`'s instruction data: the discriminator, the Borsh `route_plan` (a `u32` count, then the
 * steps), and a fixed 19-byte tail. Only the tail's layout is stable across Jupiter releases; the
 * plan's `Swap` enum grows, so the plan is kept as opaque bytes.
 */
export function splitRouteData(data) {
  const tail = data.length - ROUTE_TAIL_LENGTH;
  if (tail < 12) throw new Error(`route data is ${data.length} bytes, too short`);
  return {
    discriminator: data.subarray(0, 8).toString('hex'),
    argsHex: data.subarray(8).toString('hex'),
    routePlanHex: data.subarray(8, tail).toString('hex'),
    routePlanSteps: data.readUInt32LE(8),
    inAmount: data.readBigUInt64LE(tail),
    quotedOutAmount: data.readBigUInt64LE(tail + 8),
    slippageBps: data.readUInt16LE(tail + 16),
    platformFeeBps: data[tail + 18],
  };
}

// ---------------------------------------------------------------------- transaction size

export const PACKET_DATA_SIZE = 1232;

const compactU16Length = (value) => (value < 0x80 ? 1 : value < 0x4000 ? 2 : 3);
const dataLength = (ix) => ix.dataLength ?? Buffer.from(ix.data, 'base64').length;

/**
 * The wire size of a signed v0 transaction, with the keys compiled the way
 * `solana_message::v0::Message::try_compile` does: the payer, every signer and every invoked
 * program stay static; any other key found in a table is looked up, table by table in order.
 * `tables` holds `{ address, addresses }`.
 */
export function v0TransactionSize({ payer, instructions, tables }) {
  const keys = new Map();
  const key = (address) => {
    if (!keys.has(address)) keys.set(address, { signer: false, writable: false, invoked: false });
    return keys.get(address);
  };
  for (const ix of instructions) {
    key(ix.programId).invoked = true;
    for (const meta of ix.accounts) {
      const entry = key(meta.pubkey);
      entry.signer ||= meta.isSigner;
      entry.writable ||= meta.isWritable;
    }
  }
  Object.assign(key(payer), { signer: true, writable: true });

  let lookups = 0;
  let lookupBytes = 0;
  let lookedUp = 0;
  for (const table of tables) {
    const entries = new Set(table.addresses);
    let writable = 0;
    let readonly = 0;
    for (const [address, entry] of keys) {
      if (entry.signer || entry.invoked || !entries.has(address)) continue;
      if (entry.writable) writable++;
      else readonly++;
      keys.delete(address);
    }
    if (writable + readonly === 0) continue;
    lookups++;
    lookedUp += writable + readonly;
    lookupBytes += 32 + compactU16Length(writable) + writable + compactU16Length(readonly) + readonly;
  }

  const signatures = [...keys.values()].filter((entry) => entry.signer).length;
  let bytes = compactU16Length(signatures) + 64 * signatures;
  bytes += 1 + 3; // version prefix, header
  bytes += compactU16Length(keys.size) + 32 * keys.size + 32; // static keys, blockhash
  bytes += compactU16Length(instructions.length);
  for (const ix of instructions) {
    const length = dataLength(ix);
    bytes += 1 + compactU16Length(ix.accounts.length) + ix.accounts.length;
    bytes += compactU16Length(length) + length;
  }
  bytes += compactU16Length(lookups) + lookupBytes;
  return { bytes, staticKeys: keys.size, lookedUpKeys: lookedUp, tablesUsed: lookups };
}

/**
 * What a live-protocol template adds around the route it carries, beyond the route's own
 * accounts: Ballista's program and the template account, plus room for two more accounts (a price
 * feed, a tip account) and 48 bytes of other inputs.
 */
export const TEMPLATE_ALLOWANCE = { extraAccounts: 2, inputBytes: 48 };

/**
 * Estimates the transaction that runs a route inside a Ballista template: Jupiter's own
 * transaction with each leg's `route` replaced by one Ballista `run` that passes every leg's
 * accounts and carries every leg's arguments as inputs. Setup and cleanup stay in, so this is the
 * larger of the shapes a test builds.
 */
export function templateRunSize({ payer, legs, tables }) {
  const unique = (instructions) => [
    ...new Map(instructions.map((ix) => [stringifyJson(ix), ix])).values(),
  ];
  const routeAccounts = legs.flatMap((leg) => leg.instructions.swap.accounts);
  const routeArgs = legs.reduce((sum, leg) => sum + dataLength(leg.instructions.swap) - 8, 0);
  const run = {
    programId: BALLISTA_PROGRAM,
    accounts: [
      { pubkey: '(template)', isSigner: false, isWritable: false },
      ...Array.from({ length: TEMPLATE_ALLOWANCE.extraAccounts }, (_, index) => ({
        pubkey: `(template account ${index})`,
        isSigner: false,
        isWritable: true,
      })),
      ...routeAccounts,
    ],
    // The `run` tag, one group-length byte, a u16 length per `bytes` input, the inputs.
    dataLength: 1 + 1 + 2 * legs.length + routeArgs + TEMPLATE_ALLOWANCE.inputBytes,
  };
  const instructions = [
    ...legs[0].instructions.computeBudget,
    ...unique(legs.flatMap((leg) => leg.instructions.setup)),
    run,
    ...unique(legs.flatMap((leg) => [leg.instructions.cleanup].filter(Boolean))),
  ];
  return v0TransactionSize({ payer, instructions, tables });
}
