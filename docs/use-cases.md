# Twenty-five orchestration use cases

Ballista is reusable glue between existing Solana programs. “External capability” means the
workflow also depends on a downstream instruction, transaction signer, oracle, delegate, multisig,
or protocol account state.

| # | Use case | Ballista capability | External capability required |
| -: | --- | --- | --- |
| 1 | Batch SOL payroll | Bounded account range and System CPI | Funding-authority signer |
| 2 | Batch SPL token payouts | Bounded account range and Token CPI | Token-authority signer |
| 3 | Create missing ATAs and pay | ATA assertion, emptiness guard, stride-two loop | Token-authority signer |
| 4 | Revenue splitting | Checked arithmetic and ordered transfers | Funding-authority signer |
| 5 | Refund batches | State requirements and bounded transfers | Eligibility state and refund signer |
| 6 | DAO contributor payments | Generic CPI and bounded payouts | Governance execution authority |
| 7 | Multi-asset treasury distribution | Ordered generic CPIs and account rows | Treasury signers or delegates |
| 8 | Swap then deposit | Client CPI bytes and post-swap delta guard | Route and deposit instructions |
| 9 | Withdraw then swap | Ordered effects and intermediate state reads | Withdrawal authority and route |
| 10 | Atomic portfolio rebalance | Checked calculations and ordered CPIs | Compatible asset protocols |
| 11 | Limit-order execution | Oracle read, requirement, guarded swap | Fresh oracle and swap authority |
| 12 | Stop-loss execution | Price comparison and guarded swap | Fresh oracle and swap authority |
| 13 | Slippage-aware routing | Snapshots, checked math, minimum output | Client-selected route |
| 14 | Claim and compound rewards | Claim, swap, and deposit CPIs | Reward and vault instructions |
| 15 | LP position migration | Remove, swap, and add-liquidity CPIs | AMM position authority |
| 16 | Liquidity repositioning | State reads and ordered AMM CPIs | Concentrated-liquidity instructions |
| 17 | Debt refinancing | Borrow, repay, withdraw, and deposit | Compatible lending instructions |
| 18 | Atomic deleveraging | Health guard, swap, and repay | Lending authority and route |
| 19 | Atomic leverage increase | Borrow, swap, and redeposit | Lending authority and route |
| 20 | Collateral maintenance | Oracle reads and guarded effect | Lending state and signer/delegate |
| 21 | Wallet cleanup | Zero-balance guard and bounded close CPI | Account close authority |
| 22 | Token migration | Bounded or ordered generic CPIs | Migration program and authority |
| 23 | Batch NFT or cNFT minting | Account rows and reusable CPI data | Mint/compression authority |
| 24 | Bulk NFT distribution | Bounded transfer CPI | Asset authority |
| 25 | Conditional escrow settlement | State reads and guarded settlement CPI | Existing escrow program |

## Capability boundary

- Stored template bytes are not sent on every run; only inputs and runtime accounts are supplied.
- Every CPI account must still be present in the outer transaction.
- Finalized templates are public and repeatable. Authorization comes from outer signers and
  downstream state.
- The one account range compresses repeated execution but never creates unbounded computation.
- Unattended automation needs a delegate or authority model supplied by another program. Ballista
  does not custody assets, schedule itself, or sign through a per-user PDA.

For concrete implementations, browse the [cookbook](/examples/).
