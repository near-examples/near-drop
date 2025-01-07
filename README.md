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
near call <deployed-to-account> new '{"top_level_account": "<deployed-to-account>"}' --accountId <deployed-to-account>
```

### Create NEAR drop

To create NEAR drop call 'create_near_drop' method and pass following parameters:

- `public_keys` - array of public keys to be used for claiming drops
- `amount_per_drop` - amount of NEAR tokens to claim per drop

```bash
near call <deployed-to-account> create_near_drop '{"public_keys": ["<public-key-1>", "<public-key-2>"], "amount_per_drop": "100000000000000000000000"}' --accountId <creator-account-id> --deposit 1 --gas 300000000000000
```

### Create FT drop

To create FT drop call 'create_ft_drop' method and pass following parameters:

- `public_keys` - array of public keys to be used for claiming drops
- `ft_contract` - FT contract account
- `amount_per_drop` - amount of NEAR tokens to claim per drop

```bash
near call tight-achiever.testnet create_ft_drop '{"public_keys": ["<public-key-1>", "<public-key-2>"], "amount_per_drop": "1", "ft_contract": "ft.tight-achiever.testnet"}' --accountId tight-achiever.testnet --gas 300000000000000
```

### Create NFT drop

To create NFT drop call 'create_ft_drop' method and pass following parameters:

- `public_key` - a public key to be used for claiming drop
- `nft_contract` - NFT contract account

```bash
near call tight-achiever.testnet create_nft_drop '{"public_key": "<public-key>", "nft_contract": "nft.tight-achiever.testnet"}' --accountId tight-achiever.testnet --gas 300000000000000
```

### Claim drop for an existing account

```bash
near contract call-function as-transaction <deployed-to-account> claim_for json-args '{"account_id": "<existing-claimer-account-id>"}' prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' sign-as <deployed-to-account> network-config testnet sign-with-plaintext-private-key --signer-public-key <public-key> --signer-private-key <private-key> send
```

### Claim drop for a new account

```bash
near contract call-function as-transaction <deployed-to-account> create_account_and_claim json-args '{"account_id": "<new-claimer-account-id>"}' prepaid-gas '300.0 Tgas' attached-deposit '0 NEAR' sign-as <account-id> network-config testnet sign-with-plaintext-private-key --signer-public-key <public-key> --signer-private-key <private-key> send
```

## Useful Links

- [cargo-near](https://github.com/near/cargo-near) - NEAR smart contract
  development toolkit for Rust
- [near CLI-RS](https://near.cli.rs) - Iteract with NEAR blockchain from command
  line
- [NEAR Rust SDK Documentation](https://docs.near.org/sdk/rust/introduction)
- [NEAR Documentation](https://docs.near.org)
- [NEAR StackOverflow](https://stackoverflow.com/questions/tagged/nearprotocol)
- [NEAR Discord](https://near.chat)
- [NEAR Telegram Developers Community Group](https://t.me/neardev)
- NEAR DevHub: [Telegram](https://t.me/neardevhub),
  [Twitter](https://twitter.com/neardevhub)
