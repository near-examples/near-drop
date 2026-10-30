// get_drop_cost must return exactly what create_drop charges, and a claim
// must refund the storage it frees.

use crate::init::TestAccount as Account;
use near_api::types::{PublicKey, SecretKey};
use near_sdk::{
    serde_json::{json, Value},
    NearToken,
};

use crate::init::{init, init_ft_contract, init_nft_contract};
use crate::utils::{
    get_user_balance, CLAIM_GAS, CLAIM_GAS_BUDGET, INITIAL_CONTRACT_BALANCE, ONE_HUNDRED_TGAS,
};

fn new_key() -> SecretKey {
    near_api::signer::generate_secret_key().expect("generate test key")
}

// Quotes the cost via `view`, then checks create rejects cost-1 and accepts cost.
// The quote uses the same drop inputs as creation, plus the funder.
async fn assert_quote_is_exact(
    contract: &Account,
    creator: &Account,
    kind: &str,
    create_args: Value,
) -> anyhow::Result<NearToken> {
    let view_args = json!({"funder": creator.id(), "drop": {(kind): create_args.clone()}});
    let cost: NearToken = near_api::Contract(contract.id().clone())
        .call_function("get_drop_cost", view_args)
        .read_only::<_>()
        .fetch_from(&contract.network)
        .await?
        .data;

    let call = |deposit: NearToken| {
        near_api::Contract(contract.id().clone())
            .call_function(
                "create_drop",
                json!({"drop": {(kind): create_args.clone()}}),
            )
            .transaction()
            .deposit(deposit)
            .gas(ONE_HUNDRED_TGAS)
            .with_signer(creator.id().clone(), creator.signer.clone())
            .send_to(&creator.network)
    };

    let short = call(cost.saturating_sub(NearToken::from_yoctonear(1))).await?;
    assert!(
        short.is_failure(),
        "{kind}: 1 yocto below get_drop_cost must be rejected"
    );

    let exact = call(cost).await?;
    assert!(
        exact.is_success(),
        "{kind}: get_drop_cost must be enough: {:?}",
        exact
            .into_full()
            .expect("final transaction outcome")
            .failures()
    );
    Ok(cost)
}

fn keys(n: usize) -> Vec<PublicKey> {
    (0..n).map(|_| new_key().public_key()).collect()
}

// Exact quoted deposits must cover asset delivery, gas, and all storage refunds
// without consuming the contract's existing reserve across a large mixed batch.
#[tokio::test]
async fn hundred_mixed_claims_preserve_small_contract_reserve() -> anyhow::Result<()> {
    use near_contract_standards::non_fungible_token::Token;
    use near_sdk::Gas;

    let worker = crate::init::sandbox().await?;
    let root = worker.root_account()?;
    let (contract, creator, alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;
    let ft = init_ft_contract(&worker, &creator).await?;
    let (nft, _) = init_nft_contract(&worker, &creator).await?;
    near_api::Contract(ft.id().clone())
        .call_function("storage_deposit", json!({"account_id": contract.id()}))
        .transaction()
        .deposit(NearToken::from_millinear(20))
        .gas(ONE_HUNDRED_TGAS)
        .with_signer(creator.id().clone(), creator.signer.clone())
        .send_to(&creator.network)
        .await?
        .into_full()
        .expect("final transaction outcome")
        .into_result()?;
    worker.fast_forward(5).await?;

    // The sandbox charges 10^19 yoctoNEAR per byte. WASM/state storage backing
    // is separate from the 1 NEAR liquid reserve whose preservation we test.
    let storage_byte_cost = NearToken::from_yoctonear(10_000_000_000_000_000_000);
    let initial = near_api::Account(contract.id().clone())
        .view()
        .fetch_from(&contract.network)
        .await?
        .data;
    let target = storage_byte_cost
        .saturating_mul(initial.storage_usage as u128)
        .saturating_add(NearToken::from_near(1));
    near_api::Tokens::account(contract.id().clone())
        .send_to(root.id().clone())
        .near(initial.amount.saturating_sub(target))
        .with_signer(contract.signer.clone())
        .send_to(&contract.network)
        .await?
        .into_full()
        .expect("final transaction outcome")
        .into_result()?;
    worker.fast_forward(5).await?;
    // Restore the gas spent normalizing the balance, before measuring anything.
    let normalized = near_api::Account(contract.id().clone())
        .view()
        .fetch_from(&contract.network)
        .await?
        .data
        .amount;
    assert!(normalized <= target);
    near_api::Tokens::account(root.id().clone())
        .send_to(contract.id().clone())
        .near(target.saturating_sub(normalized))
        .with_signer(root.signer.clone())
        .send_to(&root.network)
        .await?
        .into_full()
        .expect("final transaction outcome")
        .into_result()?;
    worker.fast_forward(5).await?;
    let before = near_api::Account(contract.id().clone())
        .view()
        .fetch_from(&contract.network)
        .await?
        .data;
    assert_eq!(before.amount, target);
    let recipient_before = get_user_balance(&alice).await;

    let near_amount = NearToken::from_millinear(1);
    let near_keys: Vec<_> = (0..40).map(|_| new_key()).collect();
    let ft_keys: Vec<_> = (0..40).map(|_| new_key()).collect();
    let nft_keys: Vec<_> = (0..20).map(|_| new_key()).collect();
    let mut drop_ids = Vec::new();
    let mut nft_ids = Vec::new();
    let mut drops = vec![
        json!({"NEAR": {
            "public_keys": near_keys.iter().map(|key| key.public_key()).collect::<Vec<_>>(),
            "amount_per_drop": near_amount,
        }}),
        json!({"FT": {
            "public_keys": ft_keys.iter().map(|key| key.public_key()).collect::<Vec<_>>(),
            "ft_contract": ft.id(), "amount_per_drop": "1",
        }}),
    ];
    drops.extend(nft_keys.iter().map(|key| {
        json!({"NFT": {
            "public_key": key.public_key(), "nft_contract": nft.id(),
        }})
    }));

    for (index, drop) in drops.into_iter().enumerate() {
        let cost: NearToken = near_api::Contract(contract.id().clone())
            .call_function(
                "get_drop_cost",
                json!({"funder": creator.id(), "drop": drop}),
            )
            .read_only::<_>()
            .fetch_from(&contract.network)
            .await?
            .data;
        let created = near_api::Contract(contract.id().clone())
            .call_function("create_drop", json!({"drop": drop}))
            .transaction()
            .deposit(cost)
            .gas(Gas::from_tgas(300))
            .with_signer(creator.id().clone(), creator.signer.clone())
            .send_to(&creator.network)
            .await?
            .into_full()
            .expect("final transaction outcome");
        assert!(
            created.is_success() && created.receipt_failures().is_empty(),
            "{created:?}"
        );
        let drop_id: u32 = created.json()?;
        drop_ids.push(drop_id);
        if index == 1 {
            let funded = near_api::Contract(ft.id().clone()).call_function("ft_transfer_call", json!({"receiver_id": contract.id(), "amount": "40", "msg": drop_id.to_string()})).transaction()
                .deposit(NearToken::from_yoctonear(1))
                .gas(ONE_HUNDRED_TGAS)
                .with_signer(creator.id().clone(), creator.signer.clone())
        .send_to(&creator.network)
                .await?.into_full().expect("final transaction outcome");
            assert!(funded.receipt_failures().is_empty(), "{funded:?}");
            assert_eq!(funded.json::<String>()?, "40");
        } else if index >= 2 {
            let token_id = format!("balance-test-{index}");
            near_api::Contract(nft.id().clone()).call_function("nft_mint", json!({"token_id": token_id, "token_owner_id": creator.id(), "token_metadata": {"copies": 1}})).transaction()
                .deposit(NearToken::from_millinear(10))
                .gas(ONE_HUNDRED_TGAS)
                .with_signer(creator.id().clone(), creator.signer.clone())
        .send_to(&creator.network)
                .await?.into_full().expect("final transaction outcome")
                .into_result()?;
            let approved = near_api::Contract(nft.id().clone()).call_function("nft_approve", json!({"token_id": token_id, "account_id": contract.id(), "msg": drop_id.to_string()})).transaction()
                .deposit(NearToken::from_millinear(10))
                .gas(ONE_HUNDRED_TGAS)
                .with_signer(creator.id().clone(), creator.signer.clone())
        .send_to(&creator.network)
                .await?.into_full().expect("final transaction outcome");
            assert!(approved.receipt_failures().is_empty(), "{approved:?}");
            nft_ids.push(token_id);
        }
    }

    // Sign every claim with its actual restricted key, so the contract pays gas.
    for (index, key) in near_keys
        .into_iter()
        .chain(ft_keys)
        .chain(nft_keys)
        .enumerate()
    {
        let claimer = Account::from_secret_key(contract.id().clone(), key, &worker);
        let claimed = near_api::Contract(contract.id().clone())
            .call_function("claim_for", json!({"account_id": alice.id()}))
            .transaction()
            .gas(CLAIM_GAS)
            .with_signer(claimer.id().clone(), claimer.signer.clone())
            .send_to(&claimer.network)
            .await?
            .into_full()
            .expect("final transaction outcome");
        assert!(
            claimed.is_success() && claimed.receipt_failures().is_empty(),
            "claim {index}: {claimed:?}"
        );
        assert!(claimed.json::<bool>()?, "claim {index} returned false");
    }
    // Include detached funder refunds, key deletions, and protocol gas refunds.
    worker.fast_forward(10).await?;
    let after = near_api::Account(contract.id().clone())
        .view()
        .fetch_from(&contract.network)
        .await?
        .data;
    assert_eq!(
        after.storage_usage, before.storage_usage,
        "all drop/key storage must be freed"
    );
    assert!(
        after.amount >= before.amount,
        "100 claims consumed the reserve: before {}, after {}",
        before.amount,
        after.amount
    );
    assert_eq!(
        get_user_balance(&alice).await,
        recipient_before.saturating_add(near_amount.saturating_mul(40))
    );
    let ft_received: String = near_api::Contract(ft.id().clone())
        .call_function("ft_balance_of", json!({"account_id": alice.id()}))
        .read_only::<_>()
        .fetch_from(&ft.network)
        .await?
        .data;
    assert_eq!(ft_received, "40");
    for token_id in nft_ids {
        let token: Token = near_api::Contract(nft.id().clone())
            .call_function("nft_token", json!({"token_id": token_id}))
            .read_only::<_>()
            .fetch_from(&nft.network)
            .await?
            .data;
        assert_eq!(&token.owner_id, alice.id());
    }
    for drop_id in drop_ids {
        assert!(near_api::Contract(contract.id().clone())
            .call_function("get_drop_by_id", json!({"drop_id": drop_id}))
            .read_only::<serde_json::Value>()
            .fetch_from(&contract.network)
            .await
            .is_err());
    }
    println!(
        "100 claims (40 NEAR, 40 FT, 20 NFT): contract balance before {}, after {}, gain {}",
        before.amount,
        after.amount,
        after.amount.saturating_sub(before.amount)
    );
    Ok(())
}

#[tokio::test]
async fn near_drop_cost_view_matches_charge() -> anyhow::Result<()> {
    let worker = crate::init::sandbox().await?;
    let root = worker.root_account().unwrap();
    let (contract, creator, _alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;

    let args = json!({"public_keys": keys(2), "amount_per_drop": NearToken::from_near(1)});
    let cost = assert_quote_is_exact(&contract, &creator, "NEAR", args).await?;

    // Storage is actually charged, so the cost is above the per-key fees alone.
    let fees = NearToken::from_near(1)
        .saturating_add(CLAIM_GAS_BUDGET)
        .saturating_add(NearToken::from_millinear(1))
        .saturating_mul(2);
    assert!(cost > fees, "storage not included: {cost}");
    Ok(())
}

// FT and NFT creation only store the token contract id, so any account works.
#[tokio::test]
async fn ft_drop_cost_view_matches_charge() -> anyhow::Result<()> {
    let worker = crate::init::sandbox().await?;
    let root = worker.root_account().unwrap();
    let (contract, creator, alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;

    let args = json!({
        "public_keys": keys(2),
        "ft_contract": alice.id(),
        "amount_per_drop": NearToken::from_yoctonear(1),
    });
    assert_quote_is_exact(&contract, &creator, "FT", args).await?;
    Ok(())
}

#[tokio::test]
async fn nft_drop_cost_view_matches_charge() -> anyhow::Result<()> {
    let worker = crate::init::sandbox().await?;
    let root = worker.root_account().unwrap();
    let (contract, creator, alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;

    let args = json!({"public_key": new_key().public_key(), "nft_contract": alice.id()});
    assert_quote_is_exact(&contract, &creator, "NFT", args).await?;
    Ok(())
}

// Claiming a 1-key drop frees all of its storage, so the funder gets back exactly
// cost - amount - CLAIM_GAS_BUDGET (storage + ACCESS_KEY_STORAGE). Fails if
// the maps aren't flushed before measuring (refund would miss the storage).
#[tokio::test]
async fn claim_refunds_freed_storage_to_funder() -> anyhow::Result<()> {
    let worker = crate::init::sandbox().await?;
    let root = worker.root_account().unwrap();
    let (contract, creator, alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;

    let secret_key = new_key();
    let amount = NearToken::from_near(1);
    let args = json!({"public_keys": [secret_key.public_key()], "amount_per_drop": amount});
    let cost = assert_quote_is_exact(&contract, &creator, "NEAR", args).await?;

    // Let the create's gas refund land first, so only the claim refund is measured.
    // The claim is signed by the drop key, so the creator pays no gas here.
    worker.fast_forward(5).await?;
    let before = get_user_balance(&creator).await;
    let claimer = Account::from_secret_key(contract.id().clone(), secret_key, &worker);
    let claim = near_api::Contract(contract.id().clone())
        .call_function("claim_for", json!({"account_id": alice.id()}))
        .transaction()
        .gas(CLAIM_GAS)
        .with_signer(claimer.id().clone(), claimer.signer.clone())
        .send_to(&claimer.network)
        .await?
        .into_full()
        .expect("final transaction outcome");
    assert!(claim.is_success(), "{:?}", claim.failures());
    let refund = get_user_balance(&creator).await.saturating_sub(before);

    let expected = cost.saturating_sub(amount).saturating_sub(CLAIM_GAS_BUDGET); // CLAIM_GAS_BUDGET
    assert_eq!(refund, expected, "funder must get back all freed storage");
    Ok(())
}

// nft_on_approve stores the token_id after create_drop charged for an empty
// one. If the claim then refunds storage the funder never paid, a funder with a
// long token_id drains the contract on every claim.
#[tokio::test]
async fn nft_claim_refunds_no_more_storage_than_charged() -> anyhow::Result<()> {
    let worker = crate::init::sandbox().await?;
    let root = worker.root_account().unwrap();
    let (contract, creator, alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;
    let (nft_contract, _) = init_nft_contract(&worker, &creator).await?;

    let secret_key = new_key();
    let cost: NearToken = near_api::Contract(contract.id().clone())
        .call_function(
            "get_drop_cost",
            json!({"funder": creator.id(), "drop": {"NFT": {

                "public_key": secret_key.public_key(),
                "nft_contract": nft_contract.id(),
            }}}),
        )
        .read_only::<_>()
        .fetch_from(&contract.network)
        .await?
        .data;
    let create = near_api::Contract(contract.id().clone()).call_function("create_drop", json!({"drop": {"NFT": {"public_key": secret_key.public_key(), "nft_contract": nft_contract.id()}}})).transaction()
        .deposit(cost)
        .gas(ONE_HUNDRED_TGAS)
        .with_signer(creator.id().clone(), creator.signer.clone())
        .send_to(&creator.network)
        .await?.into_full().expect("final transaction outcome");
    assert!(create.is_success(), "{:?}", create.failures());
    let drop_id: u32 = create.json()?;

    // Longest token_id the contract accepts (MAX_TOKEN_ID_LEN).
    let token_id = "x".repeat(128);
    let mint = near_api::Contract(nft_contract.id().clone())
        .call_function(
            "nft_mint",
            json!({
                "token_id": token_id,
                "token_owner_id": creator.id(),
                "token_metadata": {"title": "long", "copies": 1},
            }),
        )
        .transaction()
        .deposit(NearToken::from_millinear(100))
        .gas(ONE_HUNDRED_TGAS)
        .with_signer(creator.id().clone(), creator.signer.clone())
        .send_to(&creator.network)
        .await?
        .into_full()
        .expect("final transaction outcome");
    assert!(mint.is_success(), "{:?}", mint.failures());

    let approve = near_api::Contract(nft_contract.id().clone())
        .call_function(
            "nft_approve",
            json!({"token_id": token_id, "account_id": contract.id(), "msg": drop_id.to_string()}),
        )
        .transaction()
        .deposit(NearToken::from_millinear(10))
        .gas(ONE_HUNDRED_TGAS)
        .with_signer(creator.id().clone(), creator.signer.clone())
        .send_to(&creator.network)
        .await?
        .into_full()
        .expect("final transaction outcome");
    assert!(approve.is_success(), "{:?}", approve.failures());

    worker.fast_forward(5).await?;
    let before = get_user_balance(&creator).await;
    let claimer = Account::from_secret_key(contract.id().clone(), secret_key, &worker);
    let claim = near_api::Contract(contract.id().clone())
        .call_function("claim_for", json!({"account_id": alice.id()}))
        .transaction()
        .gas(CLAIM_GAS)
        .with_signer(claimer.id().clone(), claimer.signer.clone())
        .send_to(&claimer.network)
        .await?
        .into_full()
        .expect("final transaction outcome");
    assert!(claim.is_success(), "{:?}", claim.failures());
    let refund = get_user_balance(&creator).await.saturating_sub(before);

    // Funder paid cost = storage + ACCESS_KEY_STORAGE + CLAIM_GAS_BUDGET;
    // only the first two are refundable.
    let refundable = cost.saturating_sub(CLAIM_GAS_BUDGET);
    assert!(
        refund <= refundable,
        "claim refunded {refund} but funder only paid {refundable} of refundable storage"
    );
    Ok(())
}

#[tokio::test]
async fn hundred_key_allowances_do_not_reserve_account_balance() -> anyhow::Result<()> {
    use anyhow::Context;
    use near_api::types::AccessKeyPermission;
    use near_sdk::Gas;

    let worker = crate::init::sandbox().await?;
    let root = worker.root_account()?;
    // Enough to back the deployed WASM, but less than the keys' combined allowance.
    let (contract, creator, alice) = init(&root, NearToken::from_millinear(2600)).await?;
    let keys: Vec<_> = (0..100).map(|_| new_key()).collect();
    let public_keys: Vec<_> = keys.iter().map(|key| key.public_key()).collect();
    let allowance = NearToken::from_millinear(33);
    let combined_allowance = allowance.saturating_mul(100);
    assert_eq!(combined_allowance, NearToken::from_millinear(3300));
    let drop = json!({"NEAR": {"public_keys": public_keys, "amount_per_drop": "1"}});
    let cost: NearToken = near_api::Contract(contract.id().clone())
        .call_function(
            "get_drop_cost",
            json!({"funder": creator.id(), "drop": drop}),
        )
        .read_only::<_>()
        .fetch_from(&contract.network)
        .await?
        .data;
    worker.fast_forward(5).await?;
    let before = near_api::Account(contract.id().clone())
        .view()
        .fetch_from(&contract.network)
        .await?
        .data
        .amount;
    assert!(before.saturating_add(cost) < combined_allowance);
    let created = near_api::Contract(contract.id().clone())
        .call_function("create_drop", json!({"drop": drop}))
        .transaction()
        .deposit(cost)
        .gas(Gas::from_tgas(300))
        .with_signer(creator.id().clone(), creator.signer.clone())
        .send_to(&creator.network)
        .await
        .context("create 100-key drop")?
        .into_full()
        .expect("final transaction outcome");
    assert!(created.is_success(), "100-key creation failed: {created:?}");
    assert!(
        created.receipt_failures().is_empty(),
        "an AddKey receipt failed"
    );
    worker.fast_forward(5).await?;
    let after = near_api::Account(contract.id().clone())
        .view()
        .fetch_from(&contract.network)
        .await?
        .data
        .amount;
    assert!(
        after < combined_allowance,
        "account must hold less than 3.3 NEAR"
    );
    assert!(
        after >= before.saturating_add(cost),
        "adding allowances must not deduct their value"
    );
    for public_key in &public_keys {
        let key = near_api::Account(contract.id().clone())
            .access_key(public_key.clone())
            .fetch_from(&contract.network)
            .await?;
        let AccessKeyPermission::FunctionCall(permission) = key.data.permission else {
            anyhow::bail!("drop key must be a function-call key");
        };
        assert_eq!(permission.allowance, Some(allowance));
        assert_eq!(permission.receiver_id, contract.id().as_str());
        assert_eq!(permission.method_names, ["claim_for"]);
    }
    // The balance is usable even while all 100 keys and their allowances exist.
    near_api::Tokens::account(contract.id().clone())
        .send_to(creator.id().clone())
        .near(NearToken::from_millinear(50))
        .with_signer(contract.signer.clone())
        .send_to(&contract.network)
        .await
        .context("transfer while allowances exist")?
        .into_full()
        .expect("final transaction outcome")
        .into_result()?;
    let claimer = Account::from_secret_key(contract.id().clone(), keys[0].clone(), &worker);
    let claimed = near_api::Contract(contract.id().clone())
        .call_function("claim_for", json!({"account_id": alice.id()}))
        .transaction()
        .gas(CLAIM_GAS)
        .with_signer(claimer.id().clone(), claimer.signer.clone())
        .send_to(&claimer.network)
        .await
        .context("claim with drop key")?
        .into_full()
        .expect("final transaction outcome");
    assert!(claimed.receipt_failures().is_empty());
    assert!(
        claimed.json::<bool>()?,
        "key must remain usable without 3.3 NEAR reserved"
    );
    Ok(())
}
