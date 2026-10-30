use crate::init::TestAccount as Account;
use near_contract_standards::non_fungible_token::Token;
use near_sdk::{serde_json::json, NearToken};

use crate::{
    init::{init, init_nft_contract},
    utils::{CLAIM_GAS, INITIAL_CONTRACT_BALANCE, ONE_HUNDRED_TGAS},
};

#[tokio::test]
async fn drop_on_existing_account() -> anyhow::Result<()> {
    let worker = crate::init::sandbox().await?;
    let root = worker.root_account().unwrap();

    let (contract, creator, alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;
    let (nft_contract, token_id) = init_nft_contract(&worker, &creator).await?;

    // Generate the secret key
    let secret_key = near_api::signer::generate_secret_key().expect("generate test key");

    // Creator initiates a call to create a NEAR drop
    let create_drop_result_1 = near_api::Contract(contract.id().clone()).call_function("create_drop", json!({"drop": {"NFT": {"public_key": secret_key.public_key(), "nft_contract": nft_contract.id()}}})).transaction()
        .deposit(NearToken::from_millinear(407))
        .gas(ONE_HUNDRED_TGAS)
        .with_signer(creator.id().clone(), creator.signer.clone())
        .send_to(&creator.network)
        .await?.into_full().expect("final transaction outcome");
    assert!(create_drop_result_1.is_success());

    let drop_id: u32 = create_drop_result_1.json().unwrap();
    assert_eq!(drop_id, 0);

    // Shouldn't create a drop with the same keys
    let create_drop_result_2 = near_api::Contract(contract.id().clone()).call_function("create_drop", json!({"drop": {"NFT": {"public_key": secret_key.public_key(), "nft_contract": nft_contract.id()}}})).transaction()
        .deposit(NearToken::from_millinear(407))
        .gas(ONE_HUNDRED_TGAS)
        .with_signer(creator.id().clone(), creator.signer.clone())
        .send_to(&creator.network)
        .await?.into_full().expect("final transaction outcome");
    assert!(create_drop_result_2.is_failure());

    let approve_result = near_api::Contract(nft_contract.id().clone())
        .call_function(
            "nft_approve",
            json!({"token_id": token_id, "account_id": contract.id(), "msg": drop_id.to_string()}),
        )
        .transaction()
        .deposit(NearToken::from_yoctonear(450000000000000000000))
        .gas(ONE_HUNDRED_TGAS)
        .with_signer(creator.id().clone(), creator.signer.clone())
        .send_to(&creator.network)
        .await?
        .into_full()
        .expect("final transaction outcome");
    assert!(approve_result.is_success());

    let get_drop_result_1 = near_api::Contract(contract.id().clone())
        .call_function("get_drop_by_id", json!({"drop_id": drop_id}))
        .transaction()
        .with_signer(creator.id().clone(), creator.signer.clone())
        .send_to(&creator.network)
        .await?
        .into_full()
        .expect("final transaction outcome");
    assert!(get_drop_result_1.is_success());

    // instantiate a new version of the contract, using the secret key
    let claimer: Account =
        Account::from_secret_key(contract.id().clone(), secret_key.clone(), &worker);

    let claim_result = near_api::Contract(contract.id().clone())
        .call_function("claim_for", json!({"account_id": alice.id()}))
        .transaction()
        .gas(CLAIM_GAS)
        .with_signer(claimer.id().clone(), claimer.signer.clone())
        .send_to(&claimer.network)
        .await?
        .into_full()
        .expect("final transaction outcome");
    assert!(claim_result.is_success());
    assert!(claim_result.json::<bool>()?);

    let alice_nfts = near_api::Contract(nft_contract.id().clone())
        .call_function("nft_tokens_for_owner", json!({"account_id": alice.id()}))
        .read_only::<Vec<Token>>()
        .fetch_from(&nft_contract.network)
        .await?
        .data;

    assert_eq!(alice_nfts[0].token_id, token_id);

    let get_drop_result_2 = near_api::Contract(contract.id().clone())
        .call_function("get_drop_by_id", json!({"drop_id": drop_id}))
        .transaction()
        .with_signer(creator.id().clone(), creator.signer.clone())
        .send_to(&creator.network)
        .await?
        .into_full()
        .expect("final transaction outcome");
    assert!(get_drop_result_2.is_failure());

    Ok(())
}

#[tokio::test]
async fn claim_fails_if_approval_revoked() -> anyhow::Result<()> {
    let worker = crate::init::sandbox().await?;
    let root = worker.root_account().unwrap();

    let (contract, creator, alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;
    let (nft_contract, token_id) = init_nft_contract(&worker, &creator).await?;

    let secret_key = near_api::signer::generate_secret_key().expect("generate test key");

    let create_drop_result = near_api::Contract(contract.id().clone()).call_function("create_drop", json!({"drop": {"NFT": {"public_key": secret_key.public_key(), "nft_contract": nft_contract.id()}}})).transaction()
        .deposit(NearToken::from_millinear(407))
        .gas(ONE_HUNDRED_TGAS)
        .with_signer(creator.id().clone(), creator.signer.clone())
        .send_to(&creator.network)
        .await?.into_full().expect("final transaction outcome");
    assert!(create_drop_result.is_success());
    let drop_id: u32 = create_drop_result.json().unwrap();

    let approve_result = near_api::Contract(nft_contract.id().clone())
        .call_function(
            "nft_approve",
            json!({"token_id": token_id, "account_id": contract.id(), "msg": drop_id.to_string()}),
        )
        .transaction()
        .deposit(NearToken::from_yoctonear(450000000000000000000))
        .gas(ONE_HUNDRED_TGAS)
        .with_signer(creator.id().clone(), creator.signer.clone())
        .send_to(&creator.network)
        .await?
        .into_full()
        .expect("final transaction outcome");
    assert!(approve_result.is_success());

    // The funder revokes the approval, so the drop contract can no longer transfer the NFT
    let revoke_result = near_api::Contract(nft_contract.id().clone())
        .call_function(
            "nft_revoke",
            json!({"token_id": token_id, "account_id": contract.id()}),
        )
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .gas(ONE_HUNDRED_TGAS)
        .with_signer(creator.id().clone(), creator.signer.clone())
        .send_to(&creator.network)
        .await?
        .into_full()
        .expect("final transaction outcome");
    assert!(revoke_result.is_success());

    let creator_balance_before = near_api::Account(creator.id().clone())
        .view()
        .fetch_from(&creator.network)
        .await?
        .data
        .amount;

    let claimer: Account =
        Account::from_secret_key(contract.id().clone(), secret_key.clone(), &worker);
    let claim_result = near_api::Contract(contract.id().clone())
        .call_function("claim_for", json!({"account_id": alice.id()}))
        .transaction()
        .gas(CLAIM_GAS)
        .with_signer(claimer.id().clone(), claimer.signer.clone())
        .send_to(&claimer.network)
        .await?
        .into_full()
        .expect("final transaction outcome");

    // The funder gets the storage refund back (read before the funder signs anything else)
    let creator_balance_after = near_api::Account(creator.id().clone())
        .view()
        .fetch_from(&creator.network)
        .await?
        .data
        .amount;
    assert!(creator_balance_after > creator_balance_before);

    // nft_transfer failed and resolve_nft_claim handled it
    assert!(claim_result.receipt_failures().len() > 0);
    assert!(!claim_result.json::<bool>()?);

    // The NFT never left the funder
    let token = near_api::Contract(nft_contract.id().clone())
        .call_function("nft_token", json!({"token_id": token_id}))
        .read_only::<Token>()
        .fetch_from(&nft_contract.network)
        .await?
        .data;
    assert_eq!(&token.owner_id, creator.id());

    let alice_nfts = near_api::Contract(nft_contract.id().clone())
        .call_function("nft_tokens_for_owner", json!({"account_id": alice.id()}))
        .read_only::<Vec<Token>>()
        .fetch_from(&nft_contract.network)
        .await?
        .data;
    assert!(alice_nfts.is_empty());

    // The drop is consumed
    let get_drop_result = near_api::Contract(contract.id().clone())
        .call_function("get_drop_by_id", json!({"drop_id": drop_id}))
        .transaction()
        .with_signer(creator.id().clone(), creator.signer.clone())
        .send_to(&creator.network)
        .await?
        .into_full()
        .expect("final transaction outcome");
    assert!(get_drop_result.is_failure());

    Ok(())
}
