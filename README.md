# Near Drop Contract

The smart contract exposes multiple methods to handle creating NEAR/FT/NFT drops and claiming created drops by another user using a PublicKey.

## How to Build Locally?

Install [`cargo-near`](https://github.com/near/cargo-near) and run:

```bash
cargo near build
```

## How to Test Locally?

```bash
cargo test
```

## How to Interact?

_In this example we will be using [NEAR CLI](https://github.com/near/near-cli-rs)
to interact with the NEAR blockchain and the smart contract_

### Initialize

To initialize the contract do:

```bash
near call <deployed-to-account> new '{}' --accountId <deployed-to-account>
```

### Create a drop

Call `create_drop` with a `drop` object containing one variant: `NEAR`, `FT`, or
`NFT`. The contract assigns the funder to the caller. Attach enough NEAR to
cover the quoted cost from `get_drop_cost`, passing the same `drop` object and
the creator's account as `funder`.

For NEAR, provide `public_keys` and `amount_per_drop` in yoctoNEAR:

```bash
near call <deployed-to-account> create_drop '{"drop":{"NEAR":{"public_keys":["<public-key-1>","<public-key-2>"],"amount_per_drop":"100000000000000000000000"}}}' --accountId <creator-account-id> --deposit 1 --gas 300000000000000
```

For FT, provide `public_keys`, `ft_contract`, and `amount_per_drop` in the token's
smallest units. Fund the created drop separately through `ft_transfer_call`:

```bash
near call <deployed-to-account> create_drop '{"drop":{"FT":{"public_keys":["<public-key>"],"ft_contract":"<ft-contract>","amount_per_drop":"1"}}}' --accountId <creator-account-id> --deposit 1 --gas 300000000000000
```

For NFT, provide one `public_key` and `nft_contract`. Approve the NFT for the
created drop separately through `nft_approve`:

```bash
near call <deployed-to-account> create_drop '{"drop":{"NFT":{"public_key":"<public-key>","nft_contract":"<nft-contract>"}}}' --accountId <creator-account-id> --deposit 1 --gas 300000000000000
```

Only the account that created an FT drop can fund it through `ft_transfer_call`.

### Delete a drop

Call `delete_drop` from the funder's account to cancel all remaining keys:

```bash
near call <deployed-to-account> delete_drop '{"drop_id": 0}' --accountId <creator-account-id> --gas 300000000000000
```

Deletion refunds unused NEAR or FT assets, storage, and the remaining keys' gas
and registration budgets. NFTs already belong to the funder because drops use
approval rather than custody. The NFT approval can be revoked separately by its
owner. A drop with some keys already claimed refunds only its remaining keys.

### Claim drop for an existing account

```bash
near contract call-function as-transaction <deployed-to-account> claim_for json-args '{"account_id": "<existing-claimer-account-id>"}' prepaid-gas '32.0 Tgas' attached-deposit '0 NEAR' sign-as <deployed-to-account> network-config testnet sign-with-plaintext-private-key --signer-public-key <public-key> --signer-private-key <private-key> send
```

`claim_for` requires an existing account. It returns `true` when the asset was
delivered and `false` for a handled claim failure, including an unfunded drop.
Refund outcomes do not affect that boolean. Read the returned boolean rather
than treating transaction success as delivery. Rejected transactions or runtime
failures can still fail without returning a boolean.

Accepted claim attempts consume their key before transferring assets. Handled
failures refund unused assets and deposits to the funder;
the attempted key's gas budget is treated as spent. Transaction rejection,
invalid arguments, or runtime traps such as running out of gas can occur before
the key deletion commits.

Claims attach 32 TGas. Each key has a 0.033 NEAR spending allowance, while the
funder contributes only 0.0032 NEAR per key for gas. Allowances do not reserve
account balance. The contract uses a shared liquid reserve for upfront gas
purchases and receives protocol refunds; maintain that reserve separately from
storage backing and assets owed to users. Deletion refunds the unused 0.0032
NEAR contribution per key. Higher gas prices can consume the shared reserve.

Earlier gas measurements, before the FT registration check, found minimum passing
limits of 9 TGas for NEAR, 30 TGas for FT, and 19 TGas for NFT. Current FT claim
tests pass at 32 TGas in the sandbox; the updated FT minimum has not been measured. See
[the gas report](reports/claim-gas.md) for scenarios and reproduction steps.

FT claims check whether the recipient is registered and refund the 0.00126 NEAR
registration budget to the funder when registration is skipped. New registrations
use that fixed deposit. Any excess the FT refunds for a new registration stays
in the drop contract. Refunds use direct transfers without retry state.

This version adds a stored list of keys per drop and removes the top-level
account configuration. Existing deployments require state migration or a fresh
deployment; initialization now takes no arguments.

## Useful Links

- [cargo-near](https://github.com/near/cargo-near) - NEAR smart contract
  development toolkit for Rust
- [near CLI-RS](https://near.cli.rs) - Iteract with NEAR blockchain from command
  line
- [NEAR Rust SDK Documentation](https://docs.near.org/sdk/rust/introduction)
- [NEAR Documentation](https://docs.near.org)
- [NEAR StackOverflow](https://stackoverflow.com/questions/tagged/nearprotocol)
- [NEAR Discord](https://discord.com/invite/GZ7735Xjce)
- [NEAR Telegram Developers Community Group](https://t.me/neardev)
- NEAR DevHub: [Telegram](https://t.me/neardevhub),
  [Twitter](https://twitter.com/neardevhub)
