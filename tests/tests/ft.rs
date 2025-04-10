use near_sdk::{serde_json::json, AccountId, NearToken};
use near_workspaces::{
    types::{KeyType, SecretKey},
    Account,
};

use crate::init::{init, init_ft_contract};
use crate::utils::{INITIAL_CONTRACT_BALANCE, ONE_HUNDRED_TGAS};

#[tokio::test]
async fn drop_on_existing_account() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account().unwrap();

    let (contract, creator, alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;
    let ft_contract = init_ft_contract(&worker, &creator).await?;

    let amount_per_drop = NearToken::from_yoctonear(1);

    // Generate the secret keys
    let secret_key_1 = SecretKey::from_random(KeyType::ED25519);
    let secret_key_2 = SecretKey::from_random(KeyType::ED25519);
    let public_keys = [secret_key_1.public_key(), secret_key_2.public_key()];

    // Creator initiates a call to create a NEAR drop
    let create_drop_result = creator
        .call(contract.id(), "create_ft_drop")
        .args_json(json!({"public_keys": public_keys, "ft_contract": ft_contract.id(), "amount_per_drop": amount_per_drop}))
        .deposit(NearToken::from_millinear(506))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;
    assert!(create_drop_result.is_success());

    let drop_id: serde_json::Value = create_drop_result.json().unwrap();
    assert_eq!(drop_id, 0);

    let storage_deposit_result = creator
        .call(ft_contract.id(), "storage_deposit")
        .args_json(json!({"account_id": contract.id()}))
        .deposit(NearToken::from_yoctonear(12500000000000000000000))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;
    assert!(storage_deposit_result.is_success());

    let args = json!({"receiver_id": contract.id(), "amount": amount_per_drop.saturating_mul(public_keys.len().try_into().unwrap()), "msg": drop_id.to_string()});

    let ft_transfer_result = creator
        .call(ft_contract.id(), "ft_transfer_call")
        .args_json(args)
        .deposit(NearToken::from_yoctonear(1))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;
    assert!(ft_transfer_result.is_success());

    let get_drop_result_1 = creator
        .call(contract.id(), "get_drop_by_id")
        .args_json(json!({"drop_id": drop_id}))
        .transact()
        .await?;
    assert!(get_drop_result_1.is_success());

    // instantiate a new version of the contract, using the secret key
    let claimer_1: Account =
        Account::from_secret_key(contract.id().clone(), secret_key_1.clone(), &worker);
    let claimer_2: Account =
        Account::from_secret_key(contract.id().clone(), secret_key_2.clone(), &worker);

    let claim_result_1 = claimer_1
        .call(contract.id(), "claim_for")
        .args_json(json!({"account_id": alice.id()}))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;
    assert!(claim_result_1.is_success());

    let alice_ft_balance_1 = ft_contract
        .call("ft_balance_of")
        .args_json((alice.id(),))
        .view()
        .await?
        .json::<NearToken>()?;
    assert!(alice_ft_balance_1.eq(&amount_per_drop));

    // Shouldn't be able to claim again with the same key
    let failed_claim_result = claimer_1
        .call(contract.id(), "claim_for")
        .args_json(json!({"account_id": alice.id()}))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;
    assert!(failed_claim_result.is_failure());

    let claim_result_2 = claimer_2
        .call(contract.id(), "claim_for")
        .args_json(json!({"account_id": alice.id()}))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;
    assert!(claim_result_2.is_success());

    let alice_ft_balance_2 = ft_contract
        .call("ft_balance_of")
        .args_json((alice.id(),))
        .view()
        .await?
        .json::<NearToken>()?;
    assert!(alice_ft_balance_2 == alice_ft_balance_1.saturating_add(amount_per_drop));

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

    let (contract, creator, alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;
    let ft_contract = init_ft_contract(&worker, &creator).await?;

    let amount_per_drop = NearToken::from_yoctonear(1);

    // Generate the secret key
    let secret_key = SecretKey::from_random(KeyType::ED25519);
    let public_keys = vec![secret_key.public_key()];

    // Creator initiates a call to create a NEAR drop
    let create_drop_result_1 = creator
        .call(contract.id(), "create_ft_drop")
        .args_json(json!({"public_keys": public_keys, "ft_contract": ft_contract.id(), "amount_per_drop": amount_per_drop}))
        .deposit(NearToken::from_millinear(407))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;
    assert!(create_drop_result_1.is_success());

    let drop_id: serde_json::Value = create_drop_result_1.json().unwrap();
    assert_eq!(drop_id, 0);

    // Shouldn't create a drop with the same keys
    let create_near_drop_result_2 = creator
        .call(contract.id(), "create_ft_drop")
        .args_json(json!({"public_keys": public_keys, "ft_contract": ft_contract.id(), "amount_per_drop": amount_per_drop}))
        .deposit(NearToken::from_millinear(407))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;
    assert!(create_near_drop_result_2.is_failure());

    let storage_deposit_result = creator
        .call(ft_contract.id(), "storage_deposit")
        .args_json(json!({"account_id": contract.id()}))
        .deposit(NearToken::from_yoctonear(12500000000000000000000))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;
    assert!(storage_deposit_result.is_success());

    let args = json!({"receiver_id": contract.id(), "amount": amount_per_drop, "msg": drop_id.to_string()});

    let ft_transfer_result = creator
        .call(ft_contract.id(), "ft_transfer_call")
        .args_json(args)
        .deposit(NearToken::from_yoctonear(1))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;
    assert!(ft_transfer_result.is_success());

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
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;
    assert!(claim_result_1.is_success());

    let long_account_ft_balance = ft_contract
        .call("ft_balance_of")
        .args_json((long_account.id(),))
        .view()
        .await?
        .json::<NearToken>()?;
    assert!(long_account_ft_balance.eq(&amount_per_drop));

    // Try to claim the drop again and check it fails
    let claim_result_2 = claimer
        .call(contract.id(), "claim_for")
        .args_json(json!({"account_id": alice.id()}))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;
    assert!(claim_result_2.is_failure());

    let get_drop_result_2 = creator
        .call(contract.id(), "get_drop_by_id")
        .args_json(json!({"drop_id": drop_id}))
        .transact()
        .await?;
    assert!(get_drop_result_2.is_failure());

    Ok(())
}
