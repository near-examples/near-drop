// use near_sdk::AccountId;
use near_sdk::{json_types::U128, serde_json::json, NearToken};
// use near_workspaces::types::{KeyType, SecretKey};
// use near_workspaces::Account;

use crate::init::{init, init_ft_contract};
// use crate::utils::get_user_balance;

#[tokio::test]
async fn drop_on_existing_account() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account().unwrap();

    let (mut contract, creator, alice) = init(&worker, &root).await?;
    let ft_contract = init_ft_contract(&worker, &creator).await?;

    // Retrieve the secret key for Alice's account
    let secret_key = alice.secret_key();

    // Creator initiates a call to create a NEAR drop
    let create_result = creator
        .call(contract.id(), "create_ft_drop")
        .args_json(json!({"public_key": secret_key.public_key(), "ft_contract": ft_contract.id()}))
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
    println!("args: {:?}\n", args);

    let ft_transfer_res = creator
        .call(ft_contract.id(), "ft_transfer_call")
        .args_json(
            json!({"receiver_id": contract.id(), "amount": "1", "msg": secret_key.public_key()}),
        )
        .deposit(NearToken::from_yoctonear(1))
        .max_gas()
        .transact()
        .await?;
    println!("ft_transfer_res: {:#?}\n", ft_transfer_res);
    assert!(ft_transfer_res.is_success());

    let creator_ft_balance = ft_contract
        .call("ft_balance_of")
        .args_json((creator.id(),))
        .view()
        .await?
        .json::<U128>()?;
    println!("creator_ft_balance: {:?}\n", creator_ft_balance);

    let contract_ft_balance = ft_contract
        .call("ft_balance_of")
        .args_json((contract.id(),))
        .view()
        .await?
        .json::<U128>()?;
    println!("contract_ft_balance: {:?}\n", contract_ft_balance);

    contract.as_account_mut().set_secret_key(secret_key.clone());

    let claim_result = contract
        .call("claim_for")
        .args_json(json!({"account_id": alice.id()}))
        .max_gas()
        .transact()
        .await?;
    println!("claim_result: {:?}", claim_result);
    assert!(claim_result.is_success());
    Ok(())
}
