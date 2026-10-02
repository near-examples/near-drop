use near_sdk::{
    serde_json::{json, Value},
    NearToken,
};
use near_workspaces::{
    types::{KeyType, SecretKey},
    Account,
};

use crate::init::{init, init_ft_contract, init_nft_contract};
use crate::utils::{CLAIM_GAS, CLAIM_GAS_BUDGET, INITIAL_CONTRACT_BALANCE, ONE_HUNDRED_TGAS};

fn key() -> SecretKey {
    SecretKey::from_random(KeyType::ED25519)
}

async fn create(
    contract: &Account,
    creator: &Account,
    kind: &str,
    args: Value,
) -> anyhow::Result<u32> {
    let quote_args = json!({"funder": creator.id(), "drop": {(kind.to_uppercase()): args.clone()}});
    let cost: NearToken = contract
        .view(contract.id(), "get_drop_cost")
        .args_json(quote_args)
        .await?
        .json()?;
    let result = creator
        .call(contract.id(), "create_drop")
        .args_json(json!({"drop": {(kind.to_uppercase()): args}}))
        .deposit(cost)
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;
    assert!(result.is_success(), "creation failed: {result:#?}");
    Ok(result.json()?)
}

#[tokio::test]
async fn delete_drop_cancels_only_remaining_keys_and_checks_owner() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
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
    assert!(claimer
        .call(contract.id(), "claim_for")
        .args_json(json!({"account_id": alice.id()}))
        .gas(CLAIM_GAS)
        .transact()
        .await?
        .is_success());

    let denied = alice
        .call(contract.id(), "delete_drop")
        .args_json(json!({"drop_id": id}))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;
    assert!(denied.is_failure());
    let drop: Value = contract
        .view(contract.id(), "get_drop_by_id")
        .args_json(json!({"drop_id": id}))
        .await?
        .json()?;
    assert_eq!(drop["NEAR"]["counter"], 2);

    worker.fast_forward(10).await?;
    let before = creator.view_account().await?.balance;
    let deleted = creator
        .call(contract.id(), "delete_drop")
        .args_json(json!({"drop_id": id}))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;
    assert!(deleted.is_success());
    worker.fast_forward(10).await?;
    // Two untouched keys refund 0.2 N of assets + 0.0064 N of gas budgets + storage,
    // less the gas paid by the creator to cancel the drop.
    assert!(
        creator.view_account().await?.balance.saturating_sub(before)
            > NearToken::from_millinear(200)
                .saturating_add(CLAIM_GAS_BUDGET.saturating_mul(2))
                .saturating_sub(NearToken::from_millinear(10))
    );
    for key in &keys {
        assert!(contract.view_access_key(&key.public_key()).await.is_err());
        assert!(contract
            .view(contract.id(), "get_drop_id_by_key")
            .args_json(json!({"public_key": key.public_key()}))
            .await
            .is_err());
    }
    assert!(contract
        .view(contract.id(), "get_drop_by_id")
        .args_json(json!({"drop_id": id}))
        .await
        .is_err());
    assert!(creator
        .call(contract.id(), "delete_drop")
        .args_json(json!({"drop_id": id}))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?
        .is_failure());
    Ok(())
}

#[tokio::test]
async fn ft_drop_requires_own_funding_and_returns_tokens_on_deletion() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account().unwrap();
    let (contract, creator, alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;
    assert!(root
        .transfer_near(alice.id(), NearToken::from_near(1))
        .await?
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
        assert!(creator
            .call(ft.id(), "storage_deposit")
            .args_json(json!({"account_id": account}))
            .deposit(NearToken::from_millinear(2))
            .gas(ONE_HUNDRED_TGAS)
            .transact()
            .await?
            .is_success());
    }
    assert!(creator
        .call(ft.id(), "ft_transfer")
        .args_json(json!({"receiver_id": alice.id(), "amount": "2"}))
        .deposit(NearToken::from_yoctonear(1))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?
        .is_success());
    let rejected = alice
        .call(ft.id(), "ft_transfer_call")
        .args_json(json!({"receiver_id": contract.id(), "amount": "2", "msg": id.to_string()}))
        .deposit(NearToken::from_yoctonear(1))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;
    assert!(!rejected.receipt_failures().is_empty());
    let alice_balance: String = ft
        .view("ft_balance_of")
        .args_json(json!({"account_id": alice.id()}))
        .await?
        .json()?;
    assert_eq!(alice_balance, "2");
    let drop: Value = contract
        .view(contract.id(), "get_drop_by_id")
        .args_json(json!({"drop_id": id}))
        .await?
        .json()?;
    assert_eq!(drop["FT"]["funded"], false);

    assert!(creator
        .call(ft.id(), "ft_transfer_call")
        .args_json(json!({"receiver_id": contract.id(), "amount": "2", "msg": id.to_string()}))
        .deposit(NearToken::from_yoctonear(1))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?
        .is_success());
    let before: NearToken = ft
        .view("ft_balance_of")
        .args_json(json!({"account_id": creator.id()}))
        .await?
        .json()?;
    assert!(creator
        .call(contract.id(), "delete_drop")
        .args_json(json!({"drop_id": id}))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?
        .is_success());
    let after: NearToken = ft
        .view("ft_balance_of")
        .args_json(json!({"account_id": creator.id()}))
        .await?
        .json()?;
    assert_eq!(after.saturating_sub(before), NearToken::from_yoctonear(2));
    let held: String = ft
        .view("ft_balance_of")
        .args_json(json!({"account_id": contract.id()}))
        .await?
        .json()?;
    assert_eq!(held, "0");
    for key in &keys {
        assert!(contract.view_access_key(&key.public_key()).await.is_err());
    }
    Ok(())
}

#[tokio::test]
async fn delete_nft_drop_keeps_nft_with_funder() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
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
    assert!(creator
        .call(nft.id(), "nft_approve")
        .args_json(
            json!({"token_id": token_id, "account_id": contract.id(), "msg": id.to_string()})
        )
        .deposit(NearToken::from_millinear(10))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?
        .is_success());
    assert!(creator
        .call(contract.id(), "delete_drop")
        .args_json(json!({"drop_id": id}))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?
        .is_success());
    let token: Value = nft
        .view("nft_token")
        .args_json(json!({"token_id": token_id}))
        .await?
        .json()?;
    assert_eq!(token["owner_id"], creator.id().as_str());
    assert!(contract.view_access_key(&key.public_key()).await.is_err());
    Ok(())
}

#[tokio::test]
async fn unfunded_claim_attempts_consume_keys_and_refund_deposits() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
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
        let cost: NearToken = contract
            .view(contract.id(), "get_drop_cost")
            .args_json(json!({"funder": creator.id(), "drop": {(kind.to_uppercase()): args}}))
            .await?
            .json()?;
        worker.fast_forward(10).await?;
        let before = creator.view_account().await?.balance;
        let count = if kind == "ft" { 2 } else { 1 };
        for key in keys.iter().take(count) {
            let claimer = Account::from_secret_key(contract.id().clone(), key.clone(), &worker);
            let claim = claimer
                .call(contract.id(), "claim_for")
                .args_json(json!({"account_id": alice.id()}))
                .gas(CLAIM_GAS)
                .transact()
                .await?;
            assert!(claim.is_success(), "unfunded claim should be handled");
            assert!(!claim.json::<bool>()?);
            assert!(contract.view_access_key(&key.public_key()).await.is_err());
        }
        assert_eq!(
            creator.view_account().await?.balance.saturating_sub(before),
            cost.saturating_sub(CLAIM_GAS_BUDGET.saturating_mul(count as u128))
        );
        assert!(contract
            .view(contract.id(), "get_drop_by_id")
            .args_json(json!({"drop_id": id}))
            .await
            .is_err());
    }
    Ok(())
}
