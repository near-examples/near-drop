use near_sdk::{serde_json::json, NearToken};
use near_workspaces::types::{KeyType, SecretKey};
use near_workspaces::Account;

use crate::init::init;
use crate::utils::{get_user_balance, CLAIM_GAS, INITIAL_CONTRACT_BALANCE, ONE_HUNDRED_TGAS};

#[tokio::test]
async fn drop_on_existing_account() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account().unwrap();

    let (contract, creator, alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;

    // Get balances before creating/claiming a drop
    let alice_balance_before = get_user_balance(&alice).await;
    let contract_balance_before = get_user_balance(&contract).await;

    // Define the drop amount to be 1 NearToken
    let amount_per_drop = NearToken::from_near(1);

    // Create a random secret keys
    let secret_key_1 = SecretKey::from_random(KeyType::ED25519);
    let secret_key_2 = SecretKey::from_random(KeyType::ED25519);

    // Creator initiates a call to create a NEAR drop
    let create_drop_result_1 = creator
        .call(contract.id(), "create_drop")
        .args_json(json!({"drop": {"NEAR": {"public_keys": vec![secret_key_1.public_key(), secret_key_2.public_key()], "amount_per_drop": amount_per_drop}}}))
        .deposit(NearToken::from_millinear(2210))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;
    let drop_id: serde_json::Value = create_drop_result_1.json().unwrap();
    assert_eq!(drop_id, 0);

    // Shouldn't create a drop with the same keys
    let create_drop_result_2 = creator
        .call(contract.id(), "create_drop")
        .args_json(json!({"drop": {"NEAR": {"public_keys": vec![secret_key_1.public_key(), secret_key_2.public_key()], "amount_per_drop": amount_per_drop}}}))
        .deposit(NearToken::from_millinear(2210))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;
    assert!(create_drop_result_2.is_failure());

    let get_drop_result_1: near_workspaces::result::ExecutionFinalResult = creator
        .call(contract.id(), "get_drop_by_id")
        .args_json(json!({"drop_id": drop_id}))
        .transact()
        .await?;
    assert!(get_drop_result_1.is_success());

    // instantiate a new version of the contract, using the secret key
    let claimer_1: Account =
        Account::from_secret_key(contract.id().clone(), secret_key_1.clone(), &worker);

    // Contract calls the "claim_for" function to claim the drop for Alice's account
    let claim_result_1: near_workspaces::result::ExecutionFinalResult = claimer_1
        .call(contract.id(), "claim_for")
        .args_json(json!({"account_id": alice.id()}))
        .gas(CLAIM_GAS)
        .transact()
        .await?;
    assert!(claim_result_1.is_success());
    assert!(claim_result_1.json::<bool>()?);

    // Get balances after claiming the drop
    let alice_balance_after_claiming_drop_1 = get_user_balance(&alice).await;

    // Verify that Alice's balance after the claim is equal to her balance before plus the drop amount
    assert_eq!(
        alice_balance_after_claiming_drop_1,
        alice_balance_before.saturating_add(amount_per_drop),
        "user did not receive the claim amount"
    );

    let claimer_2: Account =
        Account::from_secret_key(contract.id().clone(), secret_key_2.clone(), &worker);

    // Claim the drop again
    let claim_result_2 = claimer_2
        .call(contract.id(), "claim_for")
        .args_json(json!({"account_id": alice.id()}))
        .gas(CLAIM_GAS)
        .transact()
        .await?;
    assert!(claim_result_2.is_success());
    assert!(claim_result_2.json::<bool>()?);

    // Get balances after claiming the drop
    let alice_balance_after_claiming_drop_2 = get_user_balance(&alice).await;
    let contract_balance_after = get_user_balance(&contract).await;

    // Verify that Alice's balance after the claim is equal to her balance before plus the drop amount
    assert_eq!(
        alice_balance_after_claiming_drop_2,
        alice_balance_after_claiming_drop_1.saturating_add(amount_per_drop),
        "user did not receive the claim amount"
    );

    // Ideally there should be no surplus in the contract
    assert!(contract_balance_after.ge(&contract_balance_before));
    // assert_eq!(contract_balance_after, contract_balance_before, "hmmmmmm");

    // Try to claim the drop again with the same key and check it fails
    let claim_result_3 = claimer_1
        .call(contract.id(), "claim_for")
        .args_json(json!({"account_id": alice.id()}))
        .gas(CLAIM_GAS)
        .transact()
        .await;
    // The key was deleted on claim, so re-signing with it is rejected at broadcast.
    assert!(claim_result_3.is_err());

    let get_drop_result_2 = creator
        .call(contract.id(), "get_drop_by_id")
        .args_json(json!({"drop_id": drop_id}))
        .transact()
        .await?;
    assert!(get_drop_result_2.is_failure());

    Ok(())
}

#[tokio::test]
async fn failed_near_transfer_returns_false_and_consumes_key() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account().unwrap();
    let (contract, creator, _) = init(&root, INITIAL_CONTRACT_BALANCE).await?;
    let secret_key = SecretKey::from_random(KeyType::ED25519);
    let drop = json!({"NEAR": {
        "public_keys": [secret_key.public_key()],
        "amount_per_drop": NearToken::from_millinear(100),
    }});
    let cost: NearToken = contract
        .view(contract.id(), "get_drop_cost")
        .args_json(json!({"funder": creator.id(), "drop": drop}))
        .await?
        .json()?;
    let created = creator
        .call(contract.id(), "create_drop")
        .args_json(json!({"drop": drop}))
        .deposit(cost)
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;
    assert!(created.is_success());
    let drop_id: u32 = created.json()?;
    let claimer = Account::from_secret_key(contract.id().clone(), secret_key.clone(), &worker);
    let claim = claimer
        .call(contract.id(), "claim_for")
        .args_json(json!({"account_id": format!("missing.{}", root.id())}))
        .gas(CLAIM_GAS)
        .transact()
        .await?;
    assert!(!claim.receipt_failures().is_empty());
    assert!(!claim.json::<bool>()?);
    assert!(contract
        .view_access_key(&secret_key.public_key())
        .await
        .is_err());
    assert!(contract
        .view(contract.id(), "get_drop_by_id")
        .args_json(json!({"drop_id": drop_id}))
        .await
        .is_err());
    Ok(())
}
