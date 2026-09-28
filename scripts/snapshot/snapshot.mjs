/**
 * Snapshots mainnet for the real-protocol tests in `tests/protocols`: the programs, accounts and
 * Jupiter routes that the manifests name, read at one slot, for LiteSVM to load offline.
 *
 * Usage (Node 22 or later, no dependencies):
 *
 *   node scripts/snapshot/snapshot.mjs <manifest.json>... <snapshot-dir>
 *   node scripts/snapshot/snapshot.mjs scripts/snapshot/manifests/milestone-1.json tests/protocols/snapshot
 *
 * Environment:
 *   SOLANA_RPC_URL   a mainnet RPC endpoint (default: the public https://api.mainnet.solana.com)
 *   JUPITER_API_KEY  query api.jup.ag with this key (default: the keyless lite-api.jup.ag)
 *
 * Steps:
 *   1. Quote each route and fetch its instructions from Jupiter, for the manifests' test wallet.
 *      Every route must be Jupiter v6 `route` over the manifest's `dexes`.
 *   2. Read the manifests' programs and accounts, every account the routes' instructions
 *      reference, and their lookup tables, at one slot no older than any quote. The Clock sysvar
 *      comes back in the same calls and gives the snapshot's slot and time.
 *   3. For each executable read, fetch its ELF: loader-v3 programs from their ProgramData (at a
 *      later slot, which is safe while the deploy slot precedes the snapshot's), loader-v2
 *      programs from the account itself. The zero padding after the ELF is cut.
 *   4. Check that each lookup table is active and was last extended before the snapshot slot
 *      (otherwise its newer entries do not resolve), and estimate the size of a transaction
 *      running each route inside a template.
 *   5. Write the snapshot into a new directory and swap it in for <snapshot-dir>, so an interrupted
 *      run never leaves half of each. Then print a summary and what changed since the previous one.
 *
 * <snapshot-dir> belongs to the tool: a run replaces it whole, and refuses to start if it holds
 * anything a snapshot does not. The output depends only on chain state and the APIs' answers, so
 * identical inputs give byte-identical files.
 *
 * Left out, because LiteSVM provides them or the tests write them: builtin programs, sysvars
 * (the Clock is recorded in manifest.json instead), the instructions sysvar, and the test wallet
 * with its token accounts.
 *
 * Output, in <snapshot-dir>:
 *   manifest.json  slot, clock and wallet; each program's ELF and ProgramData header; each
 *                  account's owner, lamports, executable flag, rent epoch, data length and sha256
 *   accounts.json  every account with its data in base64, one per line. For a ProgramData or
 *                  loader-v2 program account, `data` holds only the bytes before the ELF and `elf`
 *                  names the file with the rest: the account is `data ‖ elf`, zero-padded to
 *                  `dataLength`
 *   routes.json    each route's instructions verbatim and its quote (less Jupiter's `timeTaken`),
 *                  `route`'s decoded arguments, the wallet's token accounts, and the size estimates
 *   programs/<program id>.so  each program's ELF (stored with Git LFS)
 * u64 values are exact JSON numbers; read them as BigInt in JavaScript (`parseJson` in rpc.mjs).
 */
import { createPrivateKey, createPublicKey } from 'node:crypto';
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  renameSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { basename, dirname, isAbsolute, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  JUPITER_API,
  PACKET_DATA_SIZE,
  dexPrograms,
  fetchRouteLeg,
  templateRunSize,
  v0TransactionSize,
} from './jupiter.mjs';
import {
  LOADER_V1,
  LOADER_V2,
  LOADER_V3,
  LOOKUP_TABLE_PROGRAM,
  NATIVE_LOADER,
  RPC_HOST,
  SYSVAR_OWNER,
  U64_MAX,
  base58Encode,
  decodeLookupTable,
  getAccountsAtOneSlot,
  isAddress,
  parseJson,
  programDataAddress,
  sha256Hex,
  splitProgramData,
  stringifyJson,
  trimElf,
  u64,
} from './rpc.mjs';

const FORMAT = 1;
/** Fewer spare bytes than this, for a template carrying a route, is worth a warning. */
const TIGHT_BYTES = 100;

/** Accounts LiteSVM provides itself, so they are never fetched. */
const PROVIDED = new Map([
  ['11111111111111111111111111111111', 'System program'],
  ['ComputeBudget111111111111111111111111111111', 'Compute Budget program'],
  ['AddressLookupTab1e1111111111111111111111111', 'Address Lookup Table program'],
  ['Sysvar1nstructions1111111111111111111111111', 'instructions sysvar'],
  ['SysvarC1ock11111111111111111111111111111111', 'Clock sysvar (in manifest.json)'],
  ['SysvarRent111111111111111111111111111111111', 'Rent sysvar'],
  ['SysvarS1otHashes111111111111111111111111111', 'SlotHashes sysvar'],
  ['SysvarEpochSchedu1e111111111111111111111111', 'EpochSchedule sysvar'],
  ['SysvarStakeHistory1111111111111111111111111', 'StakeHistory sysvar'],
  ['SysvarRecentB1ockHashes11111111111111111111', 'RecentBlockhashes sysvar'],
]);

/** Names for programs a route may reference that no manifest names. */
const KNOWN_PROGRAMS = new Map([
  ['TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb', 'token2022'],
  ['MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr', 'memo'],
]);

const LOADER_NAMES = new Map([
  [LOADER_V3, 'loader v3'],
  [LOADER_V2, 'loader v2'],
  [LOADER_V1, 'loader v1'],
]);

const warnings = [];
const warn = (message) => {
  warnings.push(message);
  console.error(`warning: ${message}`);
};
const REPOSITORY = fileURLToPath(new URL('../../', import.meta.url));
/** A path relative to the repository root, or absolute when it lies outside. */
const display = (path) => {
  const inside = relative(REPOSITORY, path);
  return inside.startsWith('..') || isAbsolute(inside) ? path : inside || '.';
};
const group = (value) => value.toLocaleString('en-US');
const short = (address) => `${address.slice(0, 8)}…`;
const pad = (text, width) => String(text).padEnd(width);

// --------------------------------------------------------------------------------- inputs

/**
 * The Ed25519 public key for a 32-byte seed, which is what Rust's `Keypair::new_from_array(seed)`
 * signs with.
 */
function walletAddress(seed) {
  const secret = Buffer.from(seed, 'utf8');
  if (secret.length !== 32) throw new Error(`the wallet seed must be 32 bytes, not ${secret.length}`);
  const pkcs8 = Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), secret]);
  const key = createPublicKey(createPrivateKey({ key: pkcs8, format: 'der', type: 'pkcs8' }));
  return base58Encode(key.export({ format: 'der', type: 'spki' }).subarray(-32));
}

/** Merge the manifests: one wallet, uniquely named programs, accounts and routes. */
function loadManifests(paths) {
  const plan = { seed: null, programs: new Map(), accounts: new Map(), routes: new Map(), sources: [] };
  for (const path of paths) {
    const where = display(path);
    const fail = (message) => {
      throw new Error(`${where}: ${message}`);
    };
    let manifest;
    try {
      manifest = parseJson(readFileSync(path, 'utf8'));
    } catch (error) {
      fail(error.message);
    }

    const seed = manifest.wallet?.seed;
    if (typeof seed !== 'string') fail('wallet.seed is missing');
    if (plan.seed !== null && plan.seed !== seed) fail('wallet.seed differs from another manifest');
    plan.seed = seed;

    for (const kind of ['programs', 'accounts']) {
      for (const [name, address] of Object.entries(manifest[kind] ?? {})) {
        if (!isAddress(address)) fail(`${kind}.${name} is not an address: ${address}`);
        const known = plan[kind].get(name);
        if (known !== undefined && known !== address) fail(`${kind}.${name} is named twice`);
        plan[kind].set(name, address);
      }
    }

    for (const [name, route] of Object.entries(manifest.routes ?? {})) {
      if (plan.routes.has(name)) fail(`route ${name} is defined twice`);
      if (!Array.isArray(route.legs) || route.legs.length === 0) fail(`route ${name} has no legs`);
      const legs = route.legs.map((leg, index) => {
        const where = `routes.${name}.legs[${index}]`;
        const settings = { ...manifest.jupiter, ...route.jupiter, ...leg };
        const { inputMint, outputMint, amount, amountFromPreviousLeg, dexes, maxAccounts, slippageBps } = settings;
        if (!isAddress(inputMint) || !isAddress(outputMint)) fail(`${where}: mints must be addresses`);
        if (amountFromPreviousLeg !== undefined) {
          if (index === 0) fail(`${where}: the first leg has no previous leg`);
          if (!['outAmount', 'otherAmountThreshold'].includes(amountFromPreviousLeg)) {
            fail(`${where}: amountFromPreviousLeg is "outAmount" or "otherAmountThreshold"`);
          }
          if (route.legs[index - 1].outputMint !== inputMint) fail(`${where}: sells a different mint than the previous leg bought`);
        } else if (!/^[1-9]\d*$/.test(String(amount))) {
          fail(`${where}: amount must be a positive integer in base units`);
        }
        if (!Array.isArray(dexes) || dexes.length === 0) fail(`${where}: dexes must be a list of Jupiter labels`);
        if (!Number.isInteger(maxAccounts) || !Number.isInteger(slippageBps)) {
          fail(`${where}: maxAccounts and slippageBps must be integers`);
        }
        return { inputMint, outputMint, amount, amountFromPreviousLeg, dexes, maxAccounts, slippageBps };
      });
      plan.routes.set(name, { description: route.description ?? '', legs });
    }
    plan.sources.push(where);
  }
  if (plan.seed === null) throw new Error('no manifest given');
  return plan;
}

// ---------------------------------------------------------------------------------- output

/** Everything a snapshot directory holds. Finder's `.DS_Store` is tolerated and discarded. */
const SNAPSHOT_ENTRIES = new Set(['manifest.json', 'accounts.json', 'routes.json', 'programs']);

/** A run replaces `outDir` whole, so it must hold nothing but an earlier snapshot. */
function assertReplaceable(outDir) {
  if (!existsSync(outDir)) return;
  const foreign = readdirSync(outDir).filter((entry) => !SNAPSHOT_ENTRIES.has(entry) && entry !== '.DS_Store');
  const programsDir = join(outDir, 'programs');
  if (existsSync(programsDir)) {
    for (const entry of readdirSync(programsDir)) {
      if (!entry.endsWith('.so') && entry !== '.DS_Store') foreign.push(`programs/${entry}`);
    }
  }
  if (foreign.length > 0) {
    throw new Error(
      `${display(outDir)} holds ${foreign.join(', ')}, which no snapshot writes. A run replaces its ` +
        'directory whole, so give it a new directory or an earlier snapshot.',
    );
  }
}

/**
 * Writes `files` (`[relative path, contents]`) as the new `outDir`, all or nothing.
 *
 * Everything goes into a fresh directory beside `outDir`, on the same filesystem, and is swapped in
 * by rename. rename(2) cannot replace a directory that has entries, so the old snapshot first moves
 * aside and is deleted only after the new one is in place. A run killed between those two renames
 * leaves no `outDir`, but both snapshots whole beside it as `.<name>.tmp-*` directories.
 */
function writeSnapshot(outDir, files) {
  const parent = dirname(outDir);
  mkdirSync(parent, { recursive: true });
  const staging = mkdtempSync(join(parent, `.${basename(outDir)}.tmp-`));
  try {
    // mkdtemp creates 0700; keep the permissions of the directory being replaced.
    chmodSync(staging, statSync(existsSync(outDir) ? outDir : parent).mode & 0o777);
    for (const [path, contents] of files) {
      mkdirSync(dirname(join(staging, path)), { recursive: true });
      writeFileSync(join(staging, path), contents);
    }
  } catch (error) {
    rmSync(staging, { recursive: true, force: true });
    throw error;
  }
  if (!existsSync(outDir)) {
    renameSync(staging, outDir);
    return;
  }
  const retired = `${staging}.old`;
  renameSync(outDir, retired);
  try {
    renameSync(staging, outDir);
  } catch (error) {
    renameSync(retired, outDir);
    rmSync(staging, { recursive: true, force: true });
    throw error;
  }
  rmSync(retired, { recursive: true, force: true });
}

/** Orders by a string key, by code unit, so the output does not depend on the machine's locale. */
const byKey = (key) => (a, b) => (key(a) < key(b) ? -1 : key(a) > key(b) ? 1 : 0);

// ------------------------------------------------------------------------------------ main

async function main() {
  const args = process.argv.slice(2);
  if (args.length < 2 || args.includes('--help')) {
    console.error('usage: node scripts/snapshot/snapshot.mjs <manifest.json>... <snapshot-dir>');
    process.exit(args.includes('--help') ? 0 : 2);
  }
  const outDir = resolve(args.at(-1));
  assertReplaceable(outDir);
  const plan = loadManifests(args.slice(0, -1).map((path) => resolve(path)));
  const wallet = walletAddress(plan.seed);
  console.error(`wallet ${wallet} (seed "${plan.seed}"); RPC ${RPC_HOST}; Jupiter ${JUPITER_API}`);

  // 1. Routes, from Jupiter. A snapshot without routes asks Jupiter nothing.
  const labels = plan.routes.size > 0 ? await dexPrograms() : new Map();
  const labelOf = new Map([...labels].map(([label, program]) => [program, label]));
  const dexesUsed = new Map();
  for (const route of plan.routes.values()) {
    for (const leg of route.legs) {
      for (const label of leg.dexes) {
        if (!labels.has(label)) throw new Error(`Jupiter has no dex labelled "${label}"; see ${JUPITER_API}/program-id-to-label`);
        dexesUsed.set(label, labels.get(label));
      }
    }
  }
  const routes = new Map();
  for (const [name, route] of plan.routes) {
    const legs = [];
    for (const [index, leg] of route.legs.entries()) {
      const amount = leg.amountFromPreviousLeg ? legs[index - 1].quote[leg.amountFromPreviousLeg] : leg.amount;
      const fetched = await fetchRouteLeg({ wallet, ...leg, amount });
      for (const hop of fetched.hops) {
        if (!leg.dexes.includes(hop.label)) throw new Error(`route ${name} went through ${hop.label}, outside ${leg.dexes.join(', ')}`);
      }
      console.error(
        `route ${name}[${index}]: ${group(Number(amount))} → ${group(Number(fetched.quote.outAmount))} via ` +
          `${fetched.hops.map((hop) => `${hop.label} ${short(hop.ammKey)} ${hop.percent}%`).join(', ')} ` +
          `(quoted at slot ${group(Number(fetched.quote.contextSlot))})`,
      );
      legs.push(fetched);
    }
    routes.set(name, { description: route.description, legs });
  }
  const allLegs = [...routes.values()].flatMap((route) => route.legs);

  // 2. Every account, at one slot.
  const roles = new Map();
  const addRole = (address, role) => roles.set(address, (roles.get(address) ?? new Set()).add(role));
  for (const [name, address] of plan.programs) addRole(address, `program ${name}`);
  for (const [name, address] of plan.accounts) addRole(address, `account ${name}`);
  const walletOwned = new Set();
  for (const [name, route] of routes) {
    for (const leg of route.legs) {
      for (const address of leg.accounts.keys()) addRole(address, `route ${name}`);
      for (const table of leg.addressLookupTableAddresses) addRole(table, `lookup table, route ${name}`);
      const { authority, sourceTokenAccount, destinationTokenAccount, createdTokenAccounts } = leg.walletAccounts;
      for (const address of [authority, sourceTokenAccount, destinationTokenAccount, ...createdTokenAccounts]) {
        walletOwned.add(address);
      }
    }
  }
  const skipped = new Map();
  for (const address of [...roles.keys()]) {
    const reason = walletOwned.has(address) ? 'the test wallet or its token account' : PROVIDED.get(address);
    if (!reason) continue;
    if ([...roles.get(address)].some((role) => !role.startsWith('route '))) {
      throw new Error(`${address} is declared in a manifest but is ${reason}`);
    }
    skipped.set(address, reason);
    roles.delete(address);
  }
  // No older than any quote; without quotes, any slot.
  const minContextSlot =
    allLegs.length > 0 ? Math.max(...allLegs.map((leg) => Number(leg.quote.contextSlot))) : undefined;
  const { slot, clock, accounts } = await getAccountsAtOneSlot([...roles.keys()], { minContextSlot });
  console.error(`read ${accounts.size} accounts at slot ${group(slot)}`);

  const stored = new Map();
  const absent = [];
  for (const [address, account] of accounts) {
    const role = [...roles.get(address)].sort();
    if (!account) absent.push({ address, roles: role });
    else if (account.owner === NATIVE_LOADER) skipped.set(address, 'builtin program');
    else if (account.owner === SYSVAR_OWNER) skipped.set(address, 'sysvar');
    else stored.set(address, { ...account, roles: role });
  }
  for (const [kind, map] of [['program', plan.programs], ['account', plan.accounts]]) {
    for (const [name, address] of map) {
      const account = stored.get(address);
      const problem = !account ? (skipped.get(address) ?? 'absent') : kind === 'program' && !account.executable ? 'not executable' : null;
      if (problem) throw new Error(`${kind} ${name} (${address}) is ${problem} at slot ${slot}`);
    }
  }

  // 3. Programs and their ELFs.
  const programName = (address) =>
    [...plan.programs].find(([, id]) => id === address)?.[0] ?? KNOWN_PROGRAMS.get(address) ?? labelOf.get(address) ?? address;
  const programs = [];
  for (const [address, account] of [...stored].filter(([, account]) => account.executable)) {
    const name = programName(address);
    const file = `programs/${address}.so`;
    let program;
    if (account.owner === LOADER_V3) {
      const dataAddress = programDataAddress(account.data);
      const fetched = await getAccountsAtOneSlot([dataAddress], { minContextSlot: slot });
      const programData = fetched.accounts.get(dataAddress);
      if (!programData) throw new Error(`${name}'s ProgramData ${dataAddress} is missing`);
      const { deploySlot, upgradeAuthority, header, elf } = splitProgramData(programData.data);
      if (deploySlot > slot) {
        throw new Error(`${name} was redeployed at slot ${deploySlot}, after the snapshot's ${slot}; run again`);
      }
      const trimmed = trimElf(elf);
      if (trimmed.warning) warn(`${name}: ${trimmed.warning}`);
      stored.set(dataAddress, {
        ...programData,
        roles: [...(stored.get(dataAddress)?.roles ?? []), `programData ${name}`].sort(),
        prefix: header,
        elf: trimmed.elf,
        file,
      });
      program = { programData: dataAddress, deploySlot, upgradeAuthority, ...trimmed };
    } else if (account.owner === LOADER_V2 || account.owner === LOADER_V1) {
      const trimmed = trimElf(account.data);
      if (trimmed.warning) warn(`${name}: ${trimmed.warning}`);
      Object.assign(account, { prefix: Buffer.alloc(0), elf: trimmed.elf, file });
      program = { ...trimmed };
    } else {
      throw new Error(`${name} (${address}) is executable under ${account.owner}, a loader this tool does not read`);
    }
    programs.push({
      name,
      programId: address,
      loader: account.owner,
      file,
      elfLength: program.elf.length,
      elfSha256: sha256Hex(program.elf),
      paddingTrimmed: program.padding,
      ...(program.programData && {
        programData: program.programData,
        deploySlot: program.deploySlot,
        upgradeAuthority: program.upgradeAuthority,
      }),
      elfBytes: program.elf,
    });
  }
  programs.sort(byKey((program) => `${program.name} ${program.programId}`));

  // 4. Lookup tables and transaction sizes.
  const tables = new Map();
  for (const [address, account] of stored) {
    if (account.owner !== LOOKUP_TABLE_PROGRAM) continue;
    const table = decodeLookupTable(account.data);
    if (table.deactivationSlot !== U64_MAX) {
      throw new Error(`lookup table ${address} is deactivating (slot ${table.deactivationSlot}); LiteSVM would treat it as closed`);
    }
    if (table.lastExtendedSlot >= BigInt(slot)) {
      throw new Error(`lookup table ${address} was extended at slot ${table.lastExtendedSlot}, not before the snapshot's ${slot}; run again`);
    }
    tables.set(address, table);
  }
  for (const [name, route] of routes) {
    const tablesOf = (legs) =>
      [...new Set(legs.flatMap((leg) => leg.addressLookupTableAddresses))].map((address) => {
        const table = tables.get(address);
        if (!table) throw new Error(`route ${name} uses lookup table ${address}, which is not an active table at slot ${slot}`);
        return { address, addresses: table.addresses };
      });
    for (const leg of route.legs) {
      const { computeBudget, setup, tokenLedger, swap, cleanup, other } = leg.instructions;
      const instructions = [...computeBudget, ...setup, tokenLedger, swap, cleanup, ...other].filter(Boolean);
      leg.transaction = v0TransactionSize({ payer: wallet, instructions, tables: tablesOf([leg]) });
    }
    const run = templateRunSize({ payer: wallet, legs: route.legs, tables: tablesOf(route.legs) });
    route.templateRun = { ...run, spare: PACKET_DATA_SIZE - run.bytes };
    if (route.templateRun.spare < 0) {
      warn(`route ${name}: a template carrying it needs about ${group(run.bytes)} bytes, over the ${group(PACKET_DATA_SIZE)}-byte limit`);
    } else if (route.templateRun.spare < TIGHT_BYTES) {
      warn(`route ${name}: a template carrying it leaves only about ${route.templateRun.spare} of ${group(PACKET_DATA_SIZE)} bytes spare`);
    }
  }

  // 5. Write. Every array is sorted and nothing records when the tool ran, so identical chain
  // state and API answers give identical files; the Clock's `time` is the provenance.
  const previous = readPrevious(outDir);
  const sortedAccounts = [...stored].sort(byKey(([address]) => address));
  const manifest = {
    format: FORMAT,
    manifests: plan.sources,
    rpc: RPC_HOST,
    jupiter: JUPITER_API,
    slot,
    unixTimestamp: clock.unixTimestamp,
    time: new Date(clock.unixTimestamp * 1000).toISOString(),
    clock,
    wallet: { seed: plan.seed, address: wallet },
    programs: programs.map(({ elfBytes, ...program }) => program),
    accounts: sortedAccounts.map(([address, account]) => ({
      address,
      roles: account.roles,
      owner: account.owner,
      lamports: account.lamports,
      executable: account.executable,
      rentEpoch: account.rentEpoch,
      dataLength: account.data.length,
      sha256: sha256Hex(account.data),
    })),
    lookupTables: [...tables]
      .sort(byKey(([address]) => address))
      .map(([address, table]) => ({
        address,
        deactivationSlot: table.deactivationSlot,
        lastExtendedSlot: u64(table.lastExtendedSlot),
        lastExtendedSlotStartIndex: table.lastExtendedSlotStartIndex,
        authority: table.authority,
        entries: table.addresses.length,
      })),
    absent: absent.sort(byKey(({ address }) => address)),
    skipped: [...skipped]
      .sort(byKey(([address]) => address))
      .map(([address, reason]) => ({ address, reason })),
  };
  const routesOut = {
    format: FORMAT,
    slot,
    wallet: { seed: plan.seed, address: wallet },
    jupiter: { api: JUPITER_API, dexes: Object.fromEntries(dexesUsed) },
    routes: Object.fromEntries(
      [...routes].map(([name, route]) => [
        name,
        {
          description: route.description,
          templateRun: route.templateRun,
          legs: route.legs.map((leg) => ({
            inputMint: leg.quote.inputMint,
            outputMint: leg.quote.outputMint,
            inAmount: u64(BigInt(leg.quote.inAmount)),
            outAmount: u64(BigInt(leg.quote.outAmount)),
            otherAmountThreshold: u64(BigInt(leg.quote.otherAmountThreshold)),
            slippageBps: leg.quote.slippageBps,
            contextSlot: leg.quote.contextSlot,
            hops: leg.hops,
            route: { ...leg.route, inAmount: u64(leg.route.inAmount), quotedOutAmount: u64(leg.route.quotedOutAmount) },
            walletAccounts: leg.walletAccounts,
            addressLookupTableAddresses: leg.addressLookupTableAddresses,
            transaction: leg.transaction,
            instructions: leg.instructions,
            request: leg.request,
            // As Jupiter returned it, less `timeTaken`: its server's timing, new on every call.
            quote: { ...leg.quote, timeTaken: undefined },
          })),
        },
      ]),
    ),
  };

  const accountLines = sortedAccounts.map(([address, account]) =>
    stringifyJson({
      address,
      owner: account.owner,
      lamports: account.lamports,
      executable: account.executable,
      rentEpoch: account.rentEpoch,
      dataLength: account.data.length,
      data: (account.elf ? account.prefix : account.data).toString('base64'),
      ...(account.elf && { elf: account.file }),
    }),
  );
  writeSnapshot(outDir, [
    ...programs.map((program) => [program.file, program.elfBytes]),
    ['accounts.json', `[\n${accountLines.join(',\n')}\n]\n`],
    ['routes.json', `${stringifyJson(routesOut, 2)}\n`],
    ['manifest.json', `${stringifyJson(manifest, 2)}\n`],
  ]);

  printSummary({ manifest, routes: routesOut, stored, outDir });
  if (previous) printChanges(previous, { manifest, routes: routesOut });
  if (warnings.length > 0) console.log(`\n${warnings.length} warning(s):\n${warnings.map((w) => `  ${w}`).join('\n')}`);
}

// --------------------------------------------------------------------------------- reports

function readPrevious(outDir) {
  const manifestPath = join(outDir, 'manifest.json');
  const routesPath = join(outDir, 'routes.json');
  if (!existsSync(manifestPath)) return null;
  return {
    manifest: parseJson(readFileSync(manifestPath, 'utf8')),
    routes: existsSync(routesPath) ? parseJson(readFileSync(routesPath, 'utf8')) : { routes: {} },
  };
}

const describeLeg = (leg) =>
  `${group(leg.inAmount)} → ${group(leg.outAmount)} (at least ${group(leg.otherAmountThreshold)}) via ` +
  leg.hops.map((hop) => `${hop.label} ${short(hop.ammKey)}${hop.percent === 100 ? '' : ` ${hop.percent}%`}`).join(' + ');

function printSummary({ manifest, routes, stored, outDir }) {
  const out = [];
  out.push(`\nSnapshot of slot ${group(manifest.slot)}, ${manifest.time} (unix ${manifest.unixTimestamp})`);
  out.push(`  written to ${display(outDir)}; RPC ${manifest.rpc}; Jupiter ${manifest.jupiter}`);
  out.push(`  wallet ${manifest.wallet.address} (seed "${manifest.wallet.seed}")`);

  out.push(`\nPrograms (${manifest.programs.length})`);
  for (const program of manifest.programs) {
    const padding = program.paddingTrimmed ? `, ${group(program.paddingTrimmed)} B padding cut` : '';
    const deployed = program.deploySlot === undefined ? '' : `, deployed at slot ${group(program.deploySlot)}`;
    out.push(
      `  ${pad(program.name, 16)} ${program.programId}  ${LOADER_NAMES.get(program.loader)}  ` +
        `ELF ${group(program.elfLength)} B${padding}${deployed}`,
    );
  }

  const dataBytes = [...stored.values()].reduce((sum, account) => sum + (account.elf ? 0 : account.data.length), 0);
  out.push(
    `\nAccounts: ${manifest.accounts.length} stored (${group(dataBytes)} bytes of non-program data), ` +
      `${manifest.absent.length} referenced but absent, ${manifest.skipped.length} left to LiteSVM or the tests`,
  );
  for (const { address, roles } of manifest.absent) out.push(`  absent: ${address} (${roles.join(', ')})`);
  for (const table of manifest.lookupTables) {
    out.push(`  lookup table ${table.address}: ${table.entries} entries, active, last extended at slot ${group(table.lastExtendedSlot)}`);
  }

  out.push('\nRoutes');
  for (const [name, route] of Object.entries(routes.routes)) {
    route.legs.forEach((leg, index) => {
      const accountsOf = leg.instructions.swap.accounts;
      out.push(`  ${pad(route.legs.length > 1 ? `${name}[${index}]` : name, 18)}${describeLeg(leg)}`);
      out.push(
        `  ${pad('', 18)}route: ${accountsOf.length} accounts (${new Set(accountsOf.map((meta) => meta.pubkey)).size} unique), ` +
          `${leg.addressLookupTableAddresses.length} table(s); Jupiter's transaction ${group(leg.transaction.bytes)} B ` +
          `(${leg.transaction.staticKeys} static + ${leg.transaction.lookedUpKeys} looked-up keys)`,
      );
    });
    out.push(
      `  ${pad('', 18)}in a template: about ${group(route.templateRun.bytes)} of ${group(PACKET_DATA_SIZE)} B ` +
        `(${route.templateRun.spare} spare)`,
    );
  }
  console.log(out.join('\n'));
}

function printChanges(previous, next) {
  const out = [];
  const before = previous.manifest;
  const after = next.manifest;
  out.push(
    `\nChanges since slot ${group(before.slot)} (${before.time}): ` +
      `${group(after.slot - before.slot)} slots, ${group(after.unixTimestamp - before.unixTimestamp)} s`,
  );

  const oldPrograms = new Map(before.programs.map((program) => [program.programId, program]));
  const newPrograms = new Map(after.programs.map((program) => [program.programId, program]));
  for (const [id, program] of newPrograms) {
    const old = oldPrograms.get(id);
    if (!old) out.push(`  + program ${program.name} ${id}`);
    else if (old.elfSha256 !== program.elfSha256) {
      out.push(
        `  ~ program ${program.name}: new ELF, ${group(old.elfLength)} → ${group(program.elfLength)} B, ` +
          `deployed at slot ${old.deploySlot ?? '?'} → ${program.deploySlot ?? '?'}`,
      );
    }
  }
  for (const [id, program] of oldPrograms) if (!newPrograms.has(id)) out.push(`  - program ${program.name} ${id}`);

  const oldAccounts = new Map(before.accounts.map((account) => [account.address, account]));
  const newAccounts = new Map(after.accounts.map((account) => [account.address, account]));
  let dataChanged = 0;
  for (const [address, account] of newAccounts) {
    const old = oldAccounts.get(address);
    if (!old) {
      out.push(`  + account ${address} (${account.roles.join(', ')}; ${group(account.dataLength)} B)`);
      continue;
    }
    if (old.owner !== account.owner) out.push(`  ~ account ${address}: owner ${old.owner} → ${account.owner}`);
    if (old.dataLength !== account.dataLength) {
      out.push(`  ~ account ${address} (${account.roles.join(', ')}): ${group(old.dataLength)} → ${group(account.dataLength)} B`);
    }
    if (old.sha256 !== account.sha256) dataChanged++;
  }
  for (const [address, account] of oldAccounts) {
    if (!newAccounts.has(address)) out.push(`  - account ${address} (${account.roles.join(', ')})`);
  }
  out.push(`  ${dataChanged} of ${newAccounts.size} accounts hold different data`);

  const oldRoutes = previous.routes.routes ?? {};
  const names = new Set([...Object.keys(oldRoutes), ...Object.keys(next.routes.routes)]);
  for (const name of names) {
    const old = oldRoutes[name];
    const route = next.routes.routes[name];
    if (!old) out.push(`  + route ${name}`);
    else if (!route) out.push(`  - route ${name}`);
    else {
      route.legs.forEach((leg, index) => {
        const was = old.legs[index];
        const label = route.legs.length > 1 ? `${name}[${index}]` : name;
        if (!was) return out.push(`  + route ${label}: ${describeLeg(leg)}`);
        const venues = (l) => l.hops.map((hop) => `${hop.label} ${hop.ammKey}`).join(' + ');
        if (venues(was) !== venues(leg)) out.push(`  ~ route ${label}: ${venues(was)} → ${venues(leg)}`);
        out.push(`  ~ route ${label}: out ${group(was.outAmount)} → ${group(leg.outAmount)}; accounts ${was.instructions.swap.accounts.length} → ${leg.instructions.swap.accounts.length}`);
      });
      for (let index = route.legs.length; index < old.legs.length; index++) out.push(`  - route ${name}[${index}]`);
    }
  }
  console.log(out.join('\n'));
}

main().catch((error) => {
  // The tool's own errors say what went wrong; a programming error needs its stack.
  const bug = [TypeError, ReferenceError, RangeError].some((kind) => error instanceof kind);
  console.error(`\nsnapshot failed: ${bug ? error.stack : error.message}`);
  process.exit(1);
});
