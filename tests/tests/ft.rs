use near_sdk::{json_types::U128, serde_json::json, AccountId, NearToken};
use near_workspaces::{
    types::{KeyType, SecretKey},
    Account,
};

use crate::init::{init, init_ft_contract};

#[tokio::test]
async fn drop_on_existing_account() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account().unwrap();

    let (contract, creator, alice) = init(&worker, &root).await?;
    let ft_contract = init_ft_contract(&worker, &creator).await?;

    // Generate the secret key
    let secret_key = SecretKey::from_random(KeyType::ED25519);

    // Creator initiates a call to create a NEAR drop
    let create_result = creator
        .call(contract.id(), "create_ft_drop")
        .args_json(json!({"public_key": secret_key.public_key(), "ft_contract": ft_contract.id(), "counter": "1"}))
        .deposit(NearToken::from_millinear(407))
        .max_gas()
        .transact()
        .await?;
    assert!(create_result.is_success());

    let storage_deposit_res = creator
        .call(ft_contract.id(), "storage_deposit")
        .args_json(json!({"account_id": contract.id()}))
        .deposit(NearToken::from_yoctonear(12500000000000000000000))
        .max_gas()
        .transact()
        .await?;
    assert!(storage_deposit_res.is_success());

    let args = json!({"receiver_id": contract.id(), "amount": "2", "msg": secret_key.public_key()});
    let ft_drop_amount = NearToken::from_yoctonear(1);

    let ft_transfer_res = creator
        .call(ft_contract.id(), "ft_transfer_call")
        .args_json(args)
        .deposit(ft_drop_amount)
        .max_gas()
        .transact()
        .await?;
    assert!(ft_transfer_res.is_success());

    // instantiate a new version of the contract, using the secret key
    let claimer: Account =
        Account::from_secret_key(contract.id().clone(), secret_key.clone(), &worker);

    let claim_result = claimer
        .call(contract.id(), "claim_for")
        .args_json(json!({"account_id": alice.id()}))
        .max_gas()
        .transact()
        .await?;
    assert!(claim_result.is_success());

    let alice_ft_balance = ft_contract
        .call("ft_balance_of")
        .args_json((alice.id(),))
        .view()
        .await?
        .json::<U128>()?;
    assert!(alice_ft_balance == U128(ft_drop_amount.as_yoctonear()));

    let second_claim_result = claimer
        .call(contract.id(), "claim_for")
        .args_json(json!({"account_id": alice.id()}))
        .max_gas()
        .transact()
        .await?;
    assert!(second_claim_result.is_failure());

    Ok(())
}

#[tokio::test]
async fn drop_on_new_account() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account().unwrap();

    let (contract, creator, alice) = init(&worker, &root).await?;
    let ft_contract = init_ft_contract(&worker, &creator).await?;

    // Generate the secret key
    let secret_key = SecretKey::from_random(KeyType::ED25519);

    // Creator initiates a call to create a NEAR drop
    let create_result = creator
        .call(contract.id(), "create_ft_drop")
        .args_json(json!({"public_key": secret_key.public_key(), "ft_contract": ft_contract.id(), "counter": "2"}))
        .deposit(NearToken::from_millinear(407))
        .max_gas()
        .transact()
        .await?;
    assert!(create_result.is_success());

    let storage_deposit_res = creator
        .call(ft_contract.id(), "storage_deposit")
        .args_json(json!({"account_id": contract.id()}))
        .deposit(NearToken::from_yoctonear(12500000000000000000000))
        .max_gas()
        .transact()
        .await?;
    assert!(storage_deposit_res.is_success());

    let args = json!({"receiver_id": contract.id(), "amount": "1", "msg": secret_key.public_key()});
    let ft_drop_amount = NearToken::from_yoctonear(1);

    let ft_transfer_res = creator
        .call(ft_contract.id(), "ft_transfer_call")
        .args_json(args)
        .deposit(ft_drop_amount)
        .max_gas()
        .transact()
        .await?;
    assert!(ft_transfer_res.is_success());

    // instantiate a new version of the contract, using the secret key
    let claimer: Account =
        Account::from_secret_key(contract.id().clone(), secret_key.clone(), &worker);

    let long_account_id: AccountId =
        "a12345678901234567890123456789012345678901234567890123.test.near"
            .parse()
            .unwrap();
    let long_account = Account::from_secret_key(long_account_id.clone(), secret_key, &worker);

    let first_claim_result = claimer
        .call(contract.id(), "create_account_and_claim")
        .args_json(json!({"account_id": long_account_id}))
        .max_gas()
        .transact()
        .await?;
    println!("first_claim_result: {:#?}", first_claim_result);

    assert!(first_claim_result.is_success());

    let long_account_ft_balance = ft_contract
        .call("ft_balance_of")
        .args_json((long_account.id(),))
        .view()
        .await?
        .json::<U128>()?;
    assert!(long_account_ft_balance == U128(ft_drop_amount.as_yoctonear()));

    // Try to claim the drop again and check it fails
    let second_claim_result = claimer
        .call(contract.id(), "claim_for")
        .args_json(json!({"account_id": alice.id()}))
        .max_gas()
        .transact()
        .await?;
    println!("second_claim_result: {:#?}", second_claim_result);
    assert!(second_claim_result.is_success());

    let third_claim_result = claimer
        .call(contract.id(), "claim_for")
        .args_json(json!({"account_id": alice.id()}))
        .max_gas()
        .transact()
        .await?;
    assert!(third_claim_result.is_failure());

    Ok(())
}
