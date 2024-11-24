```
~~~~Testing~~~~
```

Usecases
-----> Batching
----->-----> Token Transfer
----->-----> NFT Minting
-----> Tasking (???)
----->-----> TODO: Pay to have someone crack something with a PDA?
-----> Instruction Compression
----->-----> cNFT Minting
-----> Defi
----->-----> Jup swap into marginfi vault with exact amount
-----> Masquerading
----->-----> TODO: need to add PDA take over
----->-----> This would involve PayerDerivedAccount(0,..., 255)
----->-----> Bypass signature fee on minting token (smaller transaction / no signature fee).
-----> Constraints
----->-----> Budgeting
----->----->-----> Only spend X amount of token
----->-----> Desirable State Check
----->----->-----> Only execute if slot === some slot
----->----->-----> Only execute if currtimestamp === some timestamp
-----> Dynamic Instruction Building
----->-----> Just in time instruction building
----->----->-----> Close solana account with correct amount to avoid rent minimum error.
----->-----> Conditional Execution
----->----->-----> If Token Account Doesn't Exist, Create One
----->----->-----> skip ascoiated token call by using pda token account bruh

- [ ] Spending Limit Constraints
- [ ] Batching
  - [ ] Send 30 SOL transfers
  - [ ] Mint 15 cNFTs
  - [ ] Transfer 21 tokens.
- Conditional Execution
  - If token account does not exist, create it
  - [ ]
