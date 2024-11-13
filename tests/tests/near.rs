use near_sdk::AccountId;
use near_sdk::{serde_json::json, NearToken};
use near_workspaces::types::{KeyType, SecretKey};
use near_workspaces::Account;

use crate::init::init;
use crate::utils::get_user_balance;

#[tokio::test]
async fn drop_on_existing_account() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account().unwrap();

    let (mut contract, creator, alice) = init(&worker, &root).await?;

    // Get balances before creating/claiming a drop
    let alice_balance_before = get_user_balance(&alice).await;
    let contract_balance_before = get_user_balance(contract.as_account()).await;

    // Define the drop amount to be 1 NearToken
    let drop_amount = NearToken::from_near(1);

    // Retrieve the secret key for Alice's account
    let secret_key = alice.secret_key();

    // Creator initiates a call to create a NEAR drop
    let create_result = creator
        .call(contract.id(), "create_near_drop")
        .args_json(json!({"public_key": secret_key.public_key(), "amount": drop_amount}))
        .deposit(NearToken::from_millinear(1500))
        .max_gas()
        .transact()
        .await?;
    assert!(create_result.is_success());

    // Set the secret key for the mutable contract account to Alice's secret key
    contract.as_account_mut().set_secret_key(secret_key.clone());

    // Contract calls the "claim_for" function to claim the drop for Alice's account
    let claim_result = contract
        .call("claim_for")
        .args_json(json!({"account_id": alice.id()}))
        .max_gas()
        .transact()
        .await?;
    assert!(claim_result.is_success());

    // Get balances after claiming the drop
    let alice_balance_after = get_user_balance(&alice).await;
    let contract_balance_after = get_user_balance(contract.as_account()).await;

    // Verify that Alice's balance after the claim is equal to her balance before plus the drop amount
    assert_eq!(
        alice_balance_after,
        alice_balance_before.saturating_add(drop_amount),
        "user did not receive the claim amount"
    );

    // Try to claim the drop again and check it fails
    let claim_result = contract
        .call("claim_for")
        .args_json(json!({"account_id": alice.id()}))
        .max_gas()
        .transact()
        .await?;
    assert!(claim_result.is_failure());

    // Ideally there should be no surplus in the contract
    assert!(contract_balance_after.ge(&contract_balance_before));

    Ok(())
}

#[tokio::test]
async fn drop_on_new_account() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account().unwrap();
    println!("root account: {:?}", root);

    let (mut contract, creator, alice) = init(&worker, &root).await?;

    // Get contract balance before creating/claiming a drop
    let contract_balance_before = get_user_balance(contract.as_account()).await;

    // Define the drop amount to be 1 NearToken
    let drop_amount = NearToken::from_near(1);

    // Generate the secret key
    let secret_key = SecretKey::from_random(KeyType::ED25519);

    // Creator initiates a call to create a NEAR drop
    let create_result = creator
        .call(contract.id(), "create_near_drop")
        .args_json(json!({"public_key": secret_key.public_key(), "amount": drop_amount}))
        .deposit(NearToken::from_millinear(1500))
        .max_gas()
        .transact()
        .await?;
    assert!(create_result.is_success());

    // Set the secret key for the mutable contract account to generated secret key
    contract.as_account_mut().set_secret_key(secret_key.clone());

    let long_account_id: AccountId =
        "a12345678901234567890123456789012345678901234567890123.test.near"
            .parse()
            .unwrap();
    let long_account = Account::from_secret_key(long_account_id.clone(), secret_key, &worker);

    let claim_result = contract
        .call("create_account_and_claim")
        .args_json(json!({"account_id": long_account_id}))
        .max_gas()
        .transact()
        .await?;
    println!("claim_result: {:?}", claim_result);
    assert!(claim_result.is_success());

    // Get balances after claiming the drop
    let long_account_balance = get_user_balance(&long_account).await;
    let contract_balance_after = get_user_balance(contract.as_account()).await;

    // Verify that user's balance after the claim is equal to the drop amount
    assert_eq!(long_account_balance, drop_amount);

    // Try to claim the drop again and check it fails
    let claim_result = contract
        .call("claim_for")
        .args_json(json!({"account_id": alice.id()}))
        .max_gas()
        .transact()
        .await?;
    assert!(claim_result.is_failure());

    // Ideally there should be no surplus in the contract
    assert!(contract_balance_after.ge(&contract_balance_before));

    Ok(())
}
