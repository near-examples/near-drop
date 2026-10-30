use crate::init::TestAccount as Account;
use near_api::types::SecretKey;
use near_sdk::{
    serde_json::{json, Value},
    NearToken,
};

use crate::init::{init, init_ft_contract, init_nft_contract};
use crate::utils::{CLAIM_GAS, CLAIM_GAS_BUDGET, INITIAL_CONTRACT_BALANCE, ONE_HUNDRED_TGAS};

fn key() -> SecretKey {
    near_api::signer::generate_secret_key().expect("generate test key")
}

async fn create(
    contract: &Account,
    creator: &Account,
    kind: &str,
    args: Value,
) -> anyhow::Result<u32> {
    let quote_args = json!({"funder": creator.id(), "drop": {(kind.to_uppercase()): args.clone()}});
    let cost: NearToken = near_api::Contract(contract.id().clone())
        .call_function("get_drop_cost", quote_args)
        .read_only::<_>()
        .fetch_from(&contract.network)
        .await?
        .data;
    let result = near_api::Contract(contract.id().clone())
        .call_function(
            "create_drop",
            json!({"drop": {(kind.to_uppercase()): args}}),
        )
        .transaction()
        .deposit(cost)
        .gas(ONE_HUNDRED_TGAS)
        .with_signer(creator.id().clone(), creator.signer.clone())
        .send_to(&creator.network)
        .await?
        .into_full()
        .expect("final transaction outcome");
    assert!(result.is_success(), "creation failed: {result:#?}");
    Ok(result.json()?)
}

#[tokio::test]
async fn delete_drop_cancels_only_remaining_keys_and_checks_owner() -> anyhow::Result<()> {
    let worker = crate::init::sandbox().await?;
    let root = worker.root_account().unwrap();
    let (contract, creator, alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;
    let keys = [key(), key(), key()];
    let id = create(
        &contract,
        &creator,
        "near",
        json!({
            "public_keys": keys.iter().map(|key| key.public_key()).collect::<Vec<_>>(),
            "amount_per_drop": NearToken::from_millinear(100),
        }),
    )
    .await?;
    let claimer = Account::from_secret_key(contract.id().clone(), keys[0].clone(), &worker);
    assert!(near_api::Contract(contract.id().clone())
        .call_function("claim_for", json!({"account_id": alice.id()}))
        .transaction()
        .gas(CLAIM_GAS)
        .with_signer(claimer.id().clone(), claimer.signer.clone())
        .send_to(&claimer.network)
        .await?
        .into_full()
        .expect("final transaction outcome")
        .is_success());

    let denied = near_api::Contract(contract.id().clone())
        .call_function("delete_drop", json!({"drop_id": id}))
        .transaction()
        .gas(ONE_HUNDRED_TGAS)
        .with_signer(alice.id().clone(), alice.signer.clone())
        .send_to(&alice.network)
        .await?
        .into_full()
        .expect("final transaction outcome");
    assert!(denied.is_failure());
    let drop: Value = near_api::Contract(contract.id().clone())
        .call_function("get_drop_by_id", json!({"drop_id": id}))
        .read_only::<_>()
        .fetch_from(&contract.network)
        .await?
        .data;
    assert_eq!(drop["NEAR"]["counter"], 2);

    worker.fast_forward(10).await?;
    let before = near_api::Account(creator.id().clone())
        .view()
        .fetch_from(&creator.network)
        .await?
        .data
        .amount;
    let deleted = near_api::Contract(contract.id().clone())
        .call_function("delete_drop", json!({"drop_id": id}))
        .transaction()
        .gas(ONE_HUNDRED_TGAS)
        .with_signer(creator.id().clone(), creator.signer.clone())
        .send_to(&creator.network)
        .await?
        .into_full()
        .expect("final transaction outcome");
    assert!(deleted.is_success());
    worker.fast_forward(10).await?;
    // Two untouched keys refund 0.2 N of assets + 0.0064 N of gas budgets + storage,
    // less the gas paid by the creator to cancel the drop.
    assert!(
        near_api::Account(creator.id().clone())
            .view()
            .fetch_from(&creator.network)
            .await?
            .data
            .amount
            .saturating_sub(before)
            > NearToken::from_millinear(200)
                .saturating_add(CLAIM_GAS_BUDGET.saturating_mul(2))
                .saturating_sub(NearToken::from_millinear(10))
    );
    for key in &keys {
        assert!(near_api::Account(contract.id().clone())
            .access_key(key.public_key().clone())
            .fetch_from(&contract.network)
            .await
            .is_err());
        assert!(near_api::Contract(contract.id().clone())
            .call_function(
                "get_drop_id_by_key",
                json!({"public_key": key.public_key()})
            )
            .read_only::<serde_json::Value>()
            .fetch_from(&contract.network)
            .await
            .is_err());
    }
    assert!(near_api::Contract(contract.id().clone())
        .call_function("get_drop_by_id", json!({"drop_id": id}))
        .read_only::<serde_json::Value>()
        .fetch_from(&contract.network)
        .await
        .is_err());
    assert!(near_api::Contract(contract.id().clone())
        .call_function("delete_drop", json!({"drop_id": id}))
        .transaction()
        .gas(ONE_HUNDRED_TGAS)
        .with_signer(creator.id().clone(), creator.signer.clone())
        .send_to(&creator.network)
        .await?
        .into_full()
        .expect("final transaction outcome")
        .is_failure());
    Ok(())
}

#[tokio::test]
async fn ft_drop_requires_own_funding_and_returns_tokens_on_deletion() -> anyhow::Result<()> {
    let worker = crate::init::sandbox().await?;
    let root = worker.root_account().unwrap();
    let (contract, creator, alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;
    assert!(near_api::Tokens::account(root.id().clone())
        .send_to(alice.id().clone())
        .near(NearToken::from_near(1))
        .with_signer(root.signer.clone())
        .send_to(&root.network)
        .await?
        .into_full()
        .expect("final transaction outcome")
        .is_success());
    let ft = init_ft_contract(&worker, &creator).await?;
    let keys = [key(), key()];
    let id = create(
        &contract,
        &creator,
        "ft",
        json!({"public_keys": [keys[0].public_key(), keys[1].public_key()],
        "ft_contract": ft.id(), "amount_per_drop": NearToken::from_yoctonear(1)}),
    )
    .await?;
    for account in [contract.id(), alice.id()] {
        assert!(near_api::Contract(ft.id().clone())
            .call_function("storage_deposit", json!({"account_id": account}))
            .transaction()
            .deposit(NearToken::from_millinear(2))
            .gas(ONE_HUNDRED_TGAS)
            .with_signer(creator.id().clone(), creator.signer.clone())
            .send_to(&creator.network)
            .await?
            .into_full()
            .expect("final transaction outcome")
            .is_success());
    }
    assert!(near_api::Contract(ft.id().clone())
        .call_function(
            "ft_transfer",
            json!({"receiver_id": alice.id(), "amount": "2"})
        )
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .gas(ONE_HUNDRED_TGAS)
        .with_signer(creator.id().clone(), creator.signer.clone())
        .send_to(&creator.network)
        .await?
        .into_full()
        .expect("final transaction outcome")
        .is_success());
    let rejected = near_api::Contract(ft.id().clone())
        .call_function(
            "ft_transfer_call",
            json!({"receiver_id": contract.id(), "amount": "2", "msg": id.to_string()}),
        )
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .gas(ONE_HUNDRED_TGAS)
        .with_signer(alice.id().clone(), alice.signer.clone())
        .send_to(&alice.network)
        .await?
        .into_full()
        .expect("final transaction outcome");
    assert!(!rejected.receipt_failures().is_empty());
    let alice_balance: String = near_api::Contract(ft.id().clone())
        .call_function("ft_balance_of", json!({"account_id": alice.id()}))
        .read_only::<_>()
        .fetch_from(&ft.network)
        .await?
        .data;
    assert_eq!(alice_balance, "2");
    let drop: Value = near_api::Contract(contract.id().clone())
        .call_function("get_drop_by_id", json!({"drop_id": id}))
        .read_only::<_>()
        .fetch_from(&contract.network)
        .await?
        .data;
    assert_eq!(drop["FT"]["funded"], false);

    assert!(near_api::Contract(ft.id().clone())
        .call_function(
            "ft_transfer_call",
            json!({"receiver_id": contract.id(), "amount": "2", "msg": id.to_string()})
        )
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .gas(ONE_HUNDRED_TGAS)
        .with_signer(creator.id().clone(), creator.signer.clone())
        .send_to(&creator.network)
        .await?
        .into_full()
        .expect("final transaction outcome")
        .is_success());
    let before: NearToken = near_api::Contract(ft.id().clone())
        .call_function("ft_balance_of", json!({"account_id": creator.id()}))
        .read_only::<_>()
        .fetch_from(&ft.network)
        .await?
        .data;
    assert!(near_api::Contract(contract.id().clone())
        .call_function("delete_drop", json!({"drop_id": id}))
        .transaction()
        .gas(ONE_HUNDRED_TGAS)
        .with_signer(creator.id().clone(), creator.signer.clone())
        .send_to(&creator.network)
        .await?
        .into_full()
        .expect("final transaction outcome")
        .is_success());
    let after: NearToken = near_api::Contract(ft.id().clone())
        .call_function("ft_balance_of", json!({"account_id": creator.id()}))
        .read_only::<_>()
        .fetch_from(&ft.network)
        .await?
        .data;
    assert_eq!(after.saturating_sub(before), NearToken::from_yoctonear(2));
    let held: String = near_api::Contract(ft.id().clone())
        .call_function("ft_balance_of", json!({"account_id": contract.id()}))
        .read_only::<_>()
        .fetch_from(&ft.network)
        .await?
        .data;
    assert_eq!(held, "0");
    for key in &keys {
        assert!(near_api::Account(contract.id().clone())
            .access_key(key.public_key().clone())
            .fetch_from(&contract.network)
            .await
            .is_err());
    }
    Ok(())
}

#[tokio::test]
async fn delete_nft_drop_keeps_nft_with_funder() -> anyhow::Result<()> {
    let worker = crate::init::sandbox().await?;
    let root = worker.root_account().unwrap();
    let (contract, creator, _) = init(&root, INITIAL_CONTRACT_BALANCE).await?;
    let (nft, token_id) = init_nft_contract(&worker, &creator).await?;
    let key = key();
    let id = create(
        &contract,
        &creator,
        "nft",
        json!({"public_key": key.public_key(), "nft_contract": nft.id()}),
    )
    .await?;
    assert!(near_api::Contract(nft.id().clone())
        .call_function(
            "nft_approve",
            json!({"token_id": token_id, "account_id": contract.id(), "msg": id.to_string()})
        )
        .transaction()
        .deposit(NearToken::from_millinear(10))
        .gas(ONE_HUNDRED_TGAS)
        .with_signer(creator.id().clone(), creator.signer.clone())
        .send_to(&creator.network)
        .await?
        .into_full()
        .expect("final transaction outcome")
        .is_success());
    assert!(near_api::Contract(contract.id().clone())
        .call_function("delete_drop", json!({"drop_id": id}))
        .transaction()
        .gas(ONE_HUNDRED_TGAS)
        .with_signer(creator.id().clone(), creator.signer.clone())
        .send_to(&creator.network)
        .await?
        .into_full()
        .expect("final transaction outcome")
        .is_success());
    let token: Value = near_api::Contract(nft.id().clone())
        .call_function("nft_token", json!({"token_id": token_id}))
        .read_only::<_>()
        .fetch_from(&nft.network)
        .await?
        .data;
    assert_eq!(token["owner_id"], creator.id().as_str());
    assert!(near_api::Account(contract.id().clone())
        .access_key(key.public_key().clone())
        .fetch_from(&contract.network)
        .await
        .is_err());
    Ok(())
}

#[tokio::test]
async fn unfunded_claim_attempts_consume_keys_and_refund_deposits() -> anyhow::Result<()> {
    let worker = crate::init::sandbox().await?;
    let root = worker.root_account().unwrap();
    let (contract, creator, alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;
    for kind in ["ft", "nft"] {
        let keys = [key(), key()];
        let args = if kind == "ft" {
            json!({"public_keys": [keys[0].public_key(), keys[1].public_key()],
            "ft_contract": alice.id(), "amount_per_drop": NearToken::from_yoctonear(1)})
        } else {
            json!({"public_key": keys[0].public_key(), "nft_contract": alice.id()})
        };
        let id = create(&contract, &creator, kind, args.clone()).await?;
        let cost: NearToken = near_api::Contract(contract.id().clone())
            .call_function(
                "get_drop_cost",
                json!({"funder": creator.id(), "drop": {(kind.to_uppercase()): args}}),
            )
            .read_only::<_>()
            .fetch_from(&contract.network)
            .await?
            .data;
        worker.fast_forward(10).await?;
        let before = near_api::Account(creator.id().clone())
            .view()
            .fetch_from(&creator.network)
            .await?
            .data
            .amount;
        let count = if kind == "ft" { 2 } else { 1 };
        for key in keys.iter().take(count) {
            let claimer = Account::from_secret_key(contract.id().clone(), key.clone(), &worker);
            let claim = near_api::Contract(contract.id().clone())
                .call_function("claim_for", json!({"account_id": alice.id()}))
                .transaction()
                .gas(CLAIM_GAS)
                .with_signer(claimer.id().clone(), claimer.signer.clone())
                .send_to(&claimer.network)
                .await?
                .into_full()
                .expect("final transaction outcome");
            assert!(claim.is_success(), "unfunded claim should be handled");
            assert!(!claim.json::<bool>()?);
            assert!(near_api::Account(contract.id().clone())
                .access_key(key.public_key().clone())
                .fetch_from(&contract.network)
                .await
                .is_err());
        }
        assert_eq!(
            near_api::Account(creator.id().clone())
                .view()
                .fetch_from(&creator.network)
                .await?
                .data
                .amount
                .saturating_sub(before),
            cost.saturating_sub(CLAIM_GAS_BUDGET.saturating_mul(count as u128))
        );
        assert!(near_api::Contract(contract.id().clone())
            .call_function("get_drop_by_id", json!({"drop_id": id}))
            .read_only::<serde_json::Value>()
            .fetch_from(&contract.network)
            .await
            .is_err());
    }
    Ok(())
}
