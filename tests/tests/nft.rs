use near_contract_standards::non_fungible_token::Token;
use near_sdk::{serde_json::json, NearToken};
use near_workspaces::{
    types::{KeyType, SecretKey},
    Account,
};

use crate::init::{init, init_nft_contract};

#[tokio::test]
async fn drop_on_existing_account() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account().unwrap();

    let (contract, creator, alice) = init(&worker, &root).await?;
    let (nft_contract, token_id) = init_nft_contract(&worker, &creator).await?;

    // Generate the secret key
    let secret_key = SecretKey::from_random(KeyType::ED25519);

    // Creator initiates a call to create a NEAR drop
    let create_result = creator
        .call(contract.id(), "create_nft_drop")
        .args_json(
            json!({"public_key": secret_key.public_key(), "nft_contract": nft_contract.id()}),
        )
        .deposit(NearToken::from_millinear(407))
        .max_gas()
        .transact()
        .await?;
    assert!(create_result.is_success());

    let approve_res = creator
        .call(nft_contract.id(), "nft_approve")
        .args_json(json!({"token_id": token_id, "account_id": contract.id(), "msg": secret_key.public_key()}))
        .deposit(NearToken::from_yoctonear(450000000000000000000))
        .max_gas()
        .transact()
        .await?;
    assert!(approve_res.is_success());

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

    let alice_nfts = nft_contract
        .call("nft_tokens_for_owner")
        .args_json(json!({"account_id": alice.id()}))
        .view()
        .await?
        .json::<Vec<Token>>()?;

    assert_eq!(alice_nfts[0].token_id, token_id);

    Ok(())
}
