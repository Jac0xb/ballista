# Ballista orchestration use cases

Ballista is reusable glue between existing SVM programs. A stored template can validate accounts,
read state, calculate typed values, guard effects, build CPI data, and repeat a bounded body over a
tail account range. It does not add custody, scheduling, or protocol behavior that the invoked
programs do not already expose.

The table records what the Ballista 0.3 language can express. "External capability"
means the workflow also depends on a downstream protocol instruction, transaction signer, oracle,
delegate, multisig, or other account state.

| # | Use case | Ballista 0.3 capability | External capability required |
| -: | --- | --- | --- |
| 1 | Batch SOL payroll | Bounded account range and System Program CPI | Funding-authority signer |
| 2 | Batch SPL token payouts | Bounded account range and Token Program CPI | Token-authority signer |
| 3 | Create missing ATAs and pay | Account emptiness guard, stride-two range, ATA and token CPIs | Token-authority signer |
| 4 | Revenue splitting | Checked arithmetic, constants, and guarded transfers | Funding-authority signer |
| 5 | Refund batches | Account reads, requirements, and bounded transfers | Refund-authority signer and eligibility state |
| 6 | DAO contributor payments | Generic CPI and bounded payouts | Multisig or governance execution authority |
| 7 | Multi-asset treasury distribution | Ordered generic CPIs and bounded ranges | Treasury signers or delegates |
| 8 | Swap then deposit | Dynamic CPI bytes and ordered effects | Swap route and deposit-program instructions |
| 9 | Withdraw then swap | Account reads between ordered effects | Withdrawal authority and swap route |
| 10 | Atomic portfolio rebalance | Checked calculations and ordered protocol CPIs | Supported withdrawal, swap, and deposit programs |
| 11 | Limit-order execution | Oracle read, comparison, requirement, and guarded swap | Fresh oracle account and swap authority |
| 12 | Stop-loss execution | Oracle read, comparison, requirement, and guarded swap | Fresh oracle account and swap authority |
| 13 | Slippage-aware routing | Checked arithmetic and generated CPI arguments | Client-selected route and price source |
| 14 | Claim and compound rewards | Ordered claim, swap, and deposit CPIs | Reward and vault protocol instructions |
| 15 | LP position migration | Ordered remove, swap, and add-liquidity CPIs | AMM position authority |
| 16 | Liquidity repositioning | State reads, calculations, and ordered AMM CPIs | Concentrated-liquidity protocol instructions |
| 17 | Debt refinancing | Atomic borrow, repay, withdraw, and deposit CPIs | Compatible lending-market instructions |
| 18 | Atomic deleveraging | Account reads, checked calculations, swap, and repay | Lending authority and route |
| 19 | Atomic leverage increase | Borrow, swap, and redeposit CPIs | Lending authority and route |
| 20 | Collateral health maintenance | Oracle/account reads, health calculation, and guarded effect | Lending protocol state and signer/delegate |
| 21 | Wallet cleanup | Empty-account guard and bounded close-account CPI | Account close authority |
| 22 | Token migration | Bounded or ordered generic CPIs | Migration program and required authority |
| 23 | Batch NFT or cNFT minting | Bounded account range and reusable CPI data template | Mint/compression program authority and accounts |
| 24 | Bulk NFT distribution | Bounded account range and transfer CPI | Asset authority |
| 25 | Conditional escrow settlement | Escrow state read, requirements, and guarded settlement CPI | Existing escrow program and settlement state |

## Capability boundary

- Stored template bytes are not sent again on every run; only input values and transaction account
  references are supplied.
- Every account touched by a CPI must still be present in the outer transaction.
- A finalized template is public and repeatable. Authorization comes from transaction signers and
  downstream programs, not from ownership of the template.
- The single bounded account range is for compression, not unbounded computation. Its iteration
  count is inferred from the supplied tail accounts and constrained at template finalization.
- Automation without an end-user signature requires an authority or delegate model provided by a
  downstream program. Ballista 0.3 does not custody assets or sign through a per-user PDA.
