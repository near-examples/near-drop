use crate::init::TestAccount as Account;
use near_sdk::{serde_json::json, NearToken};

use crate::init::{init, init_ft_contract};
use crate::utils::{CLAIM_GAS, INITIAL_CONTRACT_BALANCE, ONE_HUNDRED_TGAS};

#[tokio::test]
async fn ft_drop_requires_recipient_registration_deposit() -> anyhow::Result<()> {
    let worker = crate::init::sandbox().await?;
    let root = worker.root_account().unwrap();

    let (contract, creator, _) = init(&root, INITIAL_CONTRACT_BALANCE).await?;
    let ft_contract = init_ft_contract(&worker, &creator).await?;
    let secret_key = near_api::signer::generate_secret_key().expect("generate test key");

    let cost: NearToken = near_api::Contract(contract.id().clone())
        .call_function(
            "get_drop_cost",
            json!({"funder": creator.id(), "drop": {"FT": {

                "public_keys": [secret_key.public_key()],
                "ft_contract": ft_contract.id(),
                "amount_per_drop": NearToken::from_yoctonear(1),
            }}}),
        )
        .read_only::<_>()
        .fetch_from(&contract.network)
        .await?
        .data;

    // Cover everything except the fixed 0.00126 NEAR registration budget.
    let create_drop_result = near_api::Contract(contract.id().clone())
        .call_function(
            "create_drop",
            json!({"drop": {"FT": {
                "public_keys": [secret_key.public_key()],
                "ft_contract": ft_contract.id(),
                "amount_per_drop": NearToken::from_yoctonear(1),
            }}}),
        )
        .transaction()
        .deposit(cost.saturating_sub(NearToken::from_yoctonear(1_260_000_000_000_000_000_000)))
        .gas(ONE_HUNDRED_TGAS)
        .with_signer(creator.id().clone(), creator.signer.clone())
        .send_to(&creator.network)
        .await?
        .into_full()
        .expect("final transaction outcome");

    assert!(
        create_drop_result.is_failure(),
        "FT drop creation must reject deposits that omit recipient registration funding"
    );

    Ok(())
}

#[tokio::test]
async fn drop_on_existing_account() -> anyhow::Result<()> {
    let worker = crate::init::sandbox().await?;
    let root = worker.root_account().unwrap();

    let (contract, creator, alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;
    let ft_contract = init_ft_contract(&worker, &creator).await?;

    let amount_per_drop = NearToken::from_yoctonear(1);

    // Generate the secret keys
    let secret_key_1 = near_api::signer::generate_secret_key().expect("generate test key");
    let secret_key_2 = near_api::signer::generate_secret_key().expect("generate test key");
    let public_keys = [secret_key_1.public_key(), secret_key_2.public_key()];

    // Creator initiates a call to create a NEAR drop
    let create_drop_result = near_api::Contract(contract.id().clone()).call_function("create_drop", json!({"drop": {"FT": {"public_keys": public_keys, "ft_contract": ft_contract.id(), "amount_per_drop": amount_per_drop}}})).transaction()
        .deposit(NearToken::from_millinear(506))
        .gas(ONE_HUNDRED_TGAS)
        .with_signer(creator.id().clone(), creator.signer.clone())
        .send_to(&creator.network)
        .await?.into_full().expect("final transaction outcome");
    assert!(create_drop_result.is_success());

    let drop_id: serde_json::Value = create_drop_result.json().unwrap();
    assert_eq!(drop_id, 0);

    let storage_deposit_result = near_api::Contract(ft_contract.id().clone())
        .call_function("storage_deposit", json!({"account_id": contract.id()}))
        .transaction()
        .deposit(NearToken::from_yoctonear(12500000000000000000000))
        .gas(ONE_HUNDRED_TGAS)
        .with_signer(creator.id().clone(), creator.signer.clone())
        .send_to(&creator.network)
        .await?
        .into_full()
        .expect("final transaction outcome");
    assert!(storage_deposit_result.is_success());

    let args = json!({"receiver_id": contract.id(), "amount": amount_per_drop.saturating_mul(public_keys.len().try_into().unwrap()), "msg": drop_id.to_string()});

    let ft_transfer_result = near_api::Contract(ft_contract.id().clone())
        .call_function("ft_transfer_call", args)
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .gas(ONE_HUNDRED_TGAS)
        .with_signer(creator.id().clone(), creator.signer.clone())
        .send_to(&creator.network)
        .await?
        .into_full()
        .expect("final transaction outcome");
    assert!(ft_transfer_result.is_success());

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
    let claimer_1: Account =
        Account::from_secret_key(contract.id().clone(), secret_key_1.clone(), &worker);
    let claimer_2: Account =
        Account::from_secret_key(contract.id().clone(), secret_key_2.clone(), &worker);

    let claim_result_1 = near_api::Contract(contract.id().clone())
        .call_function("claim_for", json!({"account_id": alice.id()}))
        .transaction()
        .gas(CLAIM_GAS)
        .with_signer(claimer_1.id().clone(), claimer_1.signer.clone())
        .send_to(&claimer_1.network)
        .await?
        .into_full()
        .expect("final transaction outcome");
    assert!(claim_result_1.is_success());
    assert!(claim_result_1.json::<bool>()?);

    let alice_ft_balance_1 = near_api::Contract(ft_contract.id().clone())
        .call_function("ft_balance_of", (alice.id(),))
        .read_only::<NearToken>()
        .fetch_from(&ft_contract.network)
        .await?
        .data;
    assert!(alice_ft_balance_1.eq(&amount_per_drop));

    // Shouldn't be able to claim again with the same key
    let failed_claim_result = near_api::Contract(contract.id().clone())
        .call_function("claim_for", json!({"account_id": alice.id()}))
        .transaction()
        .gas(CLAIM_GAS)
        .with_signer(claimer_1.id().clone(), claimer_1.signer.clone())
        .send_to(&claimer_1.network)
        .await;
    // The key was deleted on claim, so re-signing with it is rejected at broadcast.
    assert!(failed_claim_result.is_err());

    let claim_result_2 = near_api::Contract(contract.id().clone())
        .call_function("claim_for", json!({"account_id": alice.id()}))
        .transaction()
        .gas(CLAIM_GAS)
        .with_signer(claimer_2.id().clone(), claimer_2.signer.clone())
        .send_to(&claimer_2.network)
        .await?
        .into_full()
        .expect("final transaction outcome");
    assert!(claim_result_2.is_success());
    assert!(claim_result_2.json::<bool>()?);

    let alice_ft_balance_2 = near_api::Contract(ft_contract.id().clone())
        .call_function("ft_balance_of", (alice.id(),))
        .read_only::<NearToken>()
        .fetch_from(&ft_contract.network)
        .await?
        .data;
    assert!(alice_ft_balance_2 == alice_ft_balance_1.saturating_add(amount_per_drop));

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
