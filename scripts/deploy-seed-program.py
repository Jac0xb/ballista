"""Deploys an uploaded program buffer to a vanity address made with CreateAccountWithSeed.

Run: uv run --with solders --with requests python3 scripts/deploy-seed-program.py \
       --authority ~/.config/solana/ballista-devnet/authority.json --seed 6cQ34yIvBjD6EDCg \
       --program BLSTAiW4gdKKfsfnK5JnXQWDke2yrVZxmMwZe9fVNJuD --buffer <BUFFER> [--url https://api.devnet.solana.com]

The address is createWithSeed(authority, seed, BPFLoaderUpgradeable), ground with cavemanloverboy/vanity
(`vanity grind --base <authority> --owner BPFLoaderUpgradeab1e11111111111111111111111 --pattern BLSTA...`),
which searches seeds by SHA-256 instead of ed25519 keypairs. `solana program deploy` cannot use such an
address, because it creates the program account from a keypair, so this script sends the two
instructions itself, in one transaction:

  1. CreateAccountWithSeed: the program account, 36 bytes, owned by the upgradeable loader, with the
     authority as base and payer.
  2. Upgradeable loader DeployWithMaxDataLen: the buffer (uploaded beforehand with `solana program
     write-buffer`, its authority the same key) becomes the program, the authority its upgrade
     authority, and the buffer's lamports return to the payer.
"""
import argparse, base64, json, os, struct, time
import requests
from solders.compute_budget import set_compute_unit_limit
from solders.hash import Hash
from solders.instruction import AccountMeta, Instruction
from solders.keypair import Keypair
from solders.message import Message
from solders.pubkey import Pubkey
from solders.system_program import CreateAccountWithSeedParams, create_account_with_seed
from solders.sysvar import CLOCK, RENT
from solders.transaction import Transaction

LOADER = Pubkey.from_string('BPFLoaderUpgradeab1e11111111111111111111111')
SYSTEM = Pubkey.from_string('11111111111111111111111111111111')
PROGRAM_ACCOUNT_SIZE = 36  # UpgradeableLoaderState::Program { programdata_address }
BUFFER_HEADER = 37  # UpgradeableLoaderState::Buffer { authority_address: Option<Pubkey> }

p = argparse.ArgumentParser()
p.add_argument('--authority', required=True)
p.add_argument('--seed', required=True)
p.add_argument('--program', required=True)
p.add_argument('--buffer', required=True)
p.add_argument('--url', default='https://api.devnet.solana.com')
a = p.parse_args()

authority = Keypair.from_bytes(bytes(json.load(open(os.path.expanduser(a.authority)))))
program = Pubkey.create_with_seed(authority.pubkey(), a.seed, LOADER)
assert str(program) == a.program, f'seed gives {program}, not {a.program}'
buffer = Pubkey.from_string(a.buffer)
programdata, _ = Pubkey.find_program_address([bytes(program)], LOADER)

def rpc(method, *params):
    r = requests.post(a.url, json={'jsonrpc': '2.0', 'id': 1, 'method': method, 'params': list(params)}, timeout=60).json()
    if 'error' in r:
        raise SystemExit(f'{method}: {json.dumps(r["error"], indent=2)}')
    return r['result']

def account(key):
    return rpc('getAccountInfo', str(key), {'encoding': 'base64', 'commitment': 'confirmed'})['value']

assert account(program) is None, f'{program} already exists'
buf = account(buffer)
assert buf is not None and buf['owner'] == str(LOADER), 'buffer missing or not owned by the loader'
max_data_len = len(base64.b64decode(buf['data'][0])) - BUFFER_HEADER
lamports = rpc('getMinimumBalanceForRentExemption', PROGRAM_ACCOUNT_SIZE)

create = create_account_with_seed(CreateAccountWithSeedParams(
    from_pubkey=authority.pubkey(), to_pubkey=program, base=authority.pubkey(), seed=a.seed,
    lamports=lamports, space=PROGRAM_ACCOUNT_SIZE, owner=LOADER))
deploy = Instruction(LOADER, struct.pack('<IQ', 2, max_data_len), [
    AccountMeta(authority.pubkey(), True, True),  # payer
    AccountMeta(programdata, False, True),
    AccountMeta(program, False, True),
    AccountMeta(buffer, False, True),
    AccountMeta(RENT, False, False),
    AccountMeta(CLOCK, False, False),
    AccountMeta(SYSTEM, False, False),
    AccountMeta(authority.pubkey(), True, False),  # upgrade authority
])
blockhash = Hash.from_string(rpc('getLatestBlockhash', {'commitment': 'confirmed'})['value']['blockhash'])
tx = Transaction([authority], Message([set_compute_unit_limit(1_400_000), create, deploy], authority.pubkey()), blockhash)
sig = rpc('sendTransaction', base64.b64encode(bytes(tx)).decode(), {'encoding': 'base64', 'preflightCommitment': 'confirmed'})
for _ in range(60):
    status = rpc('getSignatureStatuses', [sig])['value'][0]
    if status and status.get('confirmationStatus') in ('confirmed', 'finalized'):
        if status.get('err'):
            raise SystemExit(f'transaction {sig} failed: {status["err"]}')
        break
    time.sleep(1)
else:
    raise SystemExit(f'transaction {sig} not confirmed after 60 s')
print(json.dumps({'program': str(program), 'programdata': str(programdata), 'max_data_len': max_data_len, 'signature': str(sig)}))
