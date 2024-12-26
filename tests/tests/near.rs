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

    let (contract, creator, alice) = init(&worker, &root).await?;

    // Get balances before creating/claiming a drop
    let alice_balance_before = get_user_balance(&alice).await;
    let contract_balance_before = get_user_balance(contract.as_account()).await;

    let drop_id = "1";

    // Define the drop amount to be 1 NearToken
    let amount_per_drop = NearToken::from_near(1);

    // Create a random secret keys
    let secret_key_1 = SecretKey::from_random(KeyType::ED25519);
    let secret_key_2 = SecretKey::from_random(KeyType::ED25519);

    println!("secret_key_1: {}", secret_key_1);
    println!("public_key_1: {}", secret_key_1.public_key());
    println!("secret_key_2: {}", secret_key_2);
    println!("public_key_2: {}", secret_key_2.public_key());

    // Creator initiates a call to create a NEAR drop
    println!(
        "{:?}",
        json!({"drop_id": drop_id, "public_keys": vec![secret_key_1.public_key(), secret_key_2.public_key()], "amount_per_drop": amount_per_drop})
    );
    let create_drop_result = creator
        .call(contract.id(), "create_near_drop")
        .args_json(
            json!({"drop_id": drop_id, "public_keys": vec![secret_key_1.public_key(), secret_key_2.public_key()], "amount_per_drop": amount_per_drop}),
        )
        .deposit(NearToken::from_millinear(3410))
        .max_gas()
        .transact()
        .await?;
    assert!(create_drop_result.is_success());

    let get_drop_result_1 = creator
        .call(contract.id(), "get_drop_by_id")
        .args_json(json!({"drop_id": drop_id}))
        .transact()
        .await?;
    assert!(get_drop_result_1.is_success());

    // instantiate a new version of the contract, using the secret key
    let claimer_1: Account =
        Account::from_secret_key(contract.id().clone(), secret_key_1.clone(), &worker);

    // Contract calls the "claim_for" function to claim the drop for Alice's account
    let claim_result_1 = claimer_1
        .call(contract.id(), "claim_for")
        .args_json(json!({"account_id": alice.id()}))
        .max_gas()
        .transact()
        .await?;
    assert!(claim_result_1.is_success());

    // Get balances after claiming the drop
    let alice_balance_after = get_user_balance(&alice).await;
    let contract_balance_after = get_user_balance(contract.as_account()).await;

    // Verify that Alice's balance after the claim is equal to her balance before plus the drop amount
    assert_eq!(
        alice_balance_after,
        alice_balance_before.saturating_add(amount_per_drop),
        "user did not receive the claim amount"
    );

    let claimer_2: Account =
        Account::from_secret_key(contract.id().clone(), secret_key_2.clone(), &worker);
    // Claim the drop again
    let claim_result_2 = claimer_2
        .call(contract.id(), "claim_for")
        .args_json(json!({"account_id": alice.id()}))
        .max_gas()
        .transact()
        .await?;
    assert!(claim_result_2.is_success());

    // Ideally there should be no surplus in the contract
    assert!(contract_balance_after.ge(&contract_balance_before));

    // Try to claim the drop again with the same key and check it fails
    let claim_result_3 = claimer_1
        .call(contract.id(), "claim_for")
        .args_json(json!({"account_id": alice.id()}))
        .max_gas()
        .transact()
        .await?;
    assert!(claim_result_3.is_failure());

    let get_drop_result_2 = creator
        .call(contract.id(), "get_drop_by_id")
        .args_json(json!({"drop_id": drop_id}))
        .transact()
        .await?;
    assert!(get_drop_result_2.is_failure());

    Ok(())
}

#[tokio::test]
async fn drop_on_new_account() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account().unwrap();

    let (contract, creator, alice) = init(&worker, &root).await?;

    // Get contract balance before creating/claiming a drop
    let contract_balance_before = get_user_balance(contract.as_account()).await;

    let drop_id = "1";
    let amount_per_drop = NearToken::from_near(1);

    // Generate the secret key
    let secret_key = SecretKey::from_random(KeyType::ED25519);
    let public_keys = vec![secret_key.public_key()];

    // Creator initiates a call to create a NEAR drop
    let create_drop_result = creator
        .call(contract.id(), "create_near_drop")
        .args_json(
            json!({"drop_id": drop_id, "public_keys": public_keys, "amount_per_drop": amount_per_drop}),
        )
        .deposit(NearToken::from_millinear(1500))
        .max_gas()
        .transact()
        .await?;
    assert!(create_drop_result.is_success());

    let get_drop_result_1 = creator
        .call(contract.id(), "get_drop_by_id")
        .args_json(json!({"drop_id": drop_id}))
        .transact()
        .await?;
    assert!(get_drop_result_1.is_success());

    // instantiate a new version of the contract, using the secret key
    let claimer: Account =
        Account::from_secret_key(contract.id().clone(), secret_key.clone(), &worker);

    let long_account_id: AccountId =
        "a12345678901234567890123456789012345678901234567890123.test.near"
            .parse()
            .unwrap();
    let long_account = Account::from_secret_key(long_account_id.clone(), secret_key, &worker);

    let claim_result_1 = claimer
        .call(contract.id(), "create_account_and_claim")
        .args_json(json!({"account_id": long_account_id}))
        .max_gas()
        .transact()
        .await?;
    println!("claim_result_1: {:?}", claim_result_1);
    assert!(claim_result_1.is_success());

    // Get balances after claiming the drop
    let long_account_balance = get_user_balance(&long_account).await;
    let contract_balance_after = get_user_balance(contract.as_account()).await;

    // Verify that user's balance after the claim is equal to the drop amount
    assert_eq!(long_account_balance, amount_per_drop);

    // Try to claim the drop again and check it fails
    let claim_result_2 = claimer
        .call(contract.id(), "claim_for")
        .args_json(json!({"account_id": alice.id()}))
        .max_gas()
        .transact()
        .await?;
    assert!(claim_result_2.is_failure());

    // Ideally there should be no surplus in the contract
    assert!(contract_balance_after.ge(&contract_balance_before));

    let get_drop_result_2 = creator
        .call(contract.id(), "get_drop_by_id")
        .args_json(json!({"drop_id": drop_id}))
        .transact()
        .await?;
    assert!(get_drop_result_2.is_failure());

    Ok(())
}
