// get_drop_cost must return exactly what create_drop charges, and a claim
// must refund the storage it frees.

use near_sdk::{
    serde_json::{json, Value},
    NearToken,
};
use near_workspaces::{
    types::{KeyType, PublicKey, SecretKey},
    Account,
};

use crate::init::{init, init_ft_contract, init_nft_contract};
use crate::utils::{
    get_user_balance, CLAIM_GAS, CLAIM_GAS_BUDGET, INITIAL_CONTRACT_BALANCE, ONE_HUNDRED_TGAS,
};

fn new_key() -> SecretKey {
    SecretKey::from_random(KeyType::ED25519)
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
    let cost: NearToken = contract
        .view(contract.id(), "get_drop_cost")
        .args_json(view_args)
        .await?
        .json()?;

    let call = |deposit: NearToken| {
        creator
            .call(contract.id(), "create_drop")
            .args_json(json!({"drop": {(kind): create_args.clone()}}))
            .deposit(deposit)
            .gas(ONE_HUNDRED_TGAS)
            .transact()
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
        exact.failures()
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

    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;
    let (contract, creator, alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;
    let ft = init_ft_contract(&worker, &creator).await?;
    let (nft, _) = init_nft_contract(&worker, &creator).await?;
    creator
        .call(ft.id(), "storage_deposit")
        .args_json(json!({"account_id": contract.id()}))
        .deposit(NearToken::from_millinear(20))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?
        .into_result()?;
    worker.fast_forward(5).await?;

    // The sandbox charges 10^19 yoctoNEAR per byte. WASM/state storage backing
    // is separate from the 1 NEAR liquid reserve whose preservation we test.
    let storage_byte_cost = NearToken::from_yoctonear(10_000_000_000_000_000_000);
    let initial = contract.view_account().await?;
    let target = storage_byte_cost
        .saturating_mul(initial.storage_usage as u128)
        .saturating_add(NearToken::from_near(1));
    contract
        .transfer_near(root.id(), initial.balance.saturating_sub(target))
        .await?
        .into_result()?;
    worker.fast_forward(5).await?;
    // Restore the gas spent normalizing the balance, before measuring anything.
    let normalized = contract.view_account().await?.balance;
    assert!(normalized <= target);
    root.transfer_near(contract.id(), target.saturating_sub(normalized))
        .await?
        .into_result()?;
    worker.fast_forward(5).await?;
    let before = contract.view_account().await?;
    assert_eq!(before.balance, target);
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
        let cost: NearToken = contract
            .view(contract.id(), "get_drop_cost")
            .args_json(json!({"funder": creator.id(), "drop": drop}))
            .await?
            .json()?;
        let created = creator
            .call(contract.id(), "create_drop")
            .args_json(json!({"drop": drop}))
            .deposit(cost)
            .gas(Gas::from_tgas(300))
            .transact()
            .await?;
        assert!(
            created.is_success() && created.receipt_failures().is_empty(),
            "{created:?}"
        );
        let drop_id: u32 = created.json()?;
        drop_ids.push(drop_id);
        if index == 1 {
            let funded = creator
                .call(ft.id(), "ft_transfer_call")
                .args_json(json!({"receiver_id": contract.id(), "amount": "40", "msg": drop_id.to_string()}))
                .deposit(NearToken::from_yoctonear(1))
                .gas(ONE_HUNDRED_TGAS)
                .transact()
                .await?;
            assert!(funded.receipt_failures().is_empty(), "{funded:?}");
            assert_eq!(funded.json::<String>()?, "40");
        } else if index >= 2 {
            let token_id = format!("balance-test-{index}");
            creator
                .call(nft.id(), "nft_mint")
                .args_json(json!({"token_id": token_id, "token_owner_id": creator.id(), "token_metadata": {"copies": 1}}))
                .deposit(NearToken::from_millinear(10))
                .gas(ONE_HUNDRED_TGAS)
                .transact()
                .await?
                .into_result()?;
            let approved = creator
                .call(nft.id(), "nft_approve")
                .args_json(json!({"token_id": token_id, "account_id": contract.id(), "msg": drop_id.to_string()}))
                .deposit(NearToken::from_millinear(10))
                .gas(ONE_HUNDRED_TGAS)
                .transact()
                .await?;
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
        let claimed = claimer
            .call(contract.id(), "claim_for")
            .args_json(json!({"account_id": alice.id()}))
            .gas(CLAIM_GAS)
            .transact()
            .await?;
        assert!(
            claimed.is_success() && claimed.receipt_failures().is_empty(),
            "claim {index}: {claimed:?}"
        );
        assert!(claimed.json::<bool>()?, "claim {index} returned false");
    }
    // Include detached funder refunds, key deletions, and protocol gas refunds.
    worker.fast_forward(10).await?;
    let after = contract.view_account().await?;
    assert_eq!(
        after.storage_usage, before.storage_usage,
        "all drop/key storage must be freed"
    );
    assert!(
        after.balance >= before.balance,
        "100 claims consumed the reserve: before {}, after {}",
        before.balance,
        after.balance
    );
    assert_eq!(
        get_user_balance(&alice).await,
        recipient_before.saturating_add(near_amount.saturating_mul(40))
    );
    let ft_received: String = ft
        .view("ft_balance_of")
        .args_json(json!({"account_id": alice.id()}))
        .await?
        .json()?;
    assert_eq!(ft_received, "40");
    for token_id in nft_ids {
        let token: Token = nft
            .view("nft_token")
            .args_json(json!({"token_id": token_id}))
            .await?
            .json()?;
        assert_eq!(&token.owner_id, alice.id());
    }
    for drop_id in drop_ids {
        assert!(contract
            .view(contract.id(), "get_drop_by_id")
            .args_json(json!({"drop_id": drop_id}))
            .await
            .is_err());
    }
    println!(
        "100 claims (40 NEAR, 40 FT, 20 NFT): contract balance before {}, after {}, gain {}",
        before.balance,
        after.balance,
        after.balance.saturating_sub(before.balance)
    );
    Ok(())
}

#[tokio::test]
async fn near_drop_cost_view_matches_charge() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
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
    let worker = near_workspaces::sandbox().await?;
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
    let worker = near_workspaces::sandbox().await?;
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
    let worker = near_workspaces::sandbox().await?;
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
    let claim = claimer
        .call(contract.id(), "claim_for")
        .args_json(json!({"account_id": alice.id()}))
        .gas(CLAIM_GAS)
        .transact()
        .await?;
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
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account().unwrap();
    let (contract, creator, alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;
    let (nft_contract, _) = init_nft_contract(&worker, &creator).await?;

    let secret_key = new_key();
    let cost: NearToken = contract
        .view(contract.id(), "get_drop_cost")
        .args_json(json!({"funder": creator.id(), "drop": {"NFT": {

            "public_key": secret_key.public_key(),
            "nft_contract": nft_contract.id(),
        }}}))
        .await?
        .json()?;
    let create = creator
        .call(contract.id(), "create_drop")
        .args_json(json!({"drop": {"NFT": {"public_key": secret_key.public_key(), "nft_contract": nft_contract.id()}}}))
        .deposit(cost)
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;
    assert!(create.is_success(), "{:?}", create.failures());
    let drop_id: u32 = create.json()?;

    // Longest token_id the contract accepts (MAX_TOKEN_ID_LEN).
    let token_id = "x".repeat(128);
    let mint = creator
        .call(nft_contract.id(), "nft_mint")
        .args_json(json!({
            "token_id": token_id,
            "token_owner_id": creator.id(),
            "token_metadata": {"title": "long", "copies": 1},
        }))
        .deposit(NearToken::from_millinear(100))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;
    assert!(mint.is_success(), "{:?}", mint.failures());

    let approve = creator
        .call(nft_contract.id(), "nft_approve")
        .args_json(
            json!({"token_id": token_id, "account_id": contract.id(), "msg": drop_id.to_string()}),
        )
        .deposit(NearToken::from_millinear(10))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;
    assert!(approve.is_success(), "{:?}", approve.failures());

    worker.fast_forward(5).await?;
    let before = get_user_balance(&creator).await;
    let claimer = Account::from_secret_key(contract.id().clone(), secret_key, &worker);
    let claim = claimer
        .call(contract.id(), "claim_for")
        .args_json(json!({"account_id": alice.id()}))
        .gas(CLAIM_GAS)
        .transact()
        .await?;
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
    use near_sdk::Gas;
    use near_workspaces::types::AccessKeyPermission;

    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;
    // Enough to back the deployed WASM, but less than the keys' combined allowance.
    let (contract, creator, alice) = init(&root, NearToken::from_millinear(2600)).await?;
    let keys: Vec<_> = (0..100).map(|_| new_key()).collect();
    let public_keys: Vec<_> = keys.iter().map(|key| key.public_key()).collect();
    let allowance = NearToken::from_millinear(33);
    let combined_allowance = allowance.saturating_mul(100);
    assert_eq!(combined_allowance, NearToken::from_millinear(3300));
    let drop = json!({"NEAR": {"public_keys": public_keys, "amount_per_drop": "1"}});
    let cost: NearToken = contract
        .view(contract.id(), "get_drop_cost")
        .args_json(json!({"funder": creator.id(), "drop": drop}))
        .await?
        .json()?;
    worker.fast_forward(5).await?;
    let before = contract.view_account().await?.balance;
    assert!(before.saturating_add(cost) < combined_allowance);
    let created = creator
        .call(contract.id(), "create_drop")
        .args_json(json!({"drop": drop}))
        .deposit(cost)
        .gas(Gas::from_tgas(300))
        .transact()
        .await?;
    assert!(created.is_success(), "100-key creation failed: {created:?}");
    assert!(
        created.receipt_failures().is_empty(),
        "an AddKey receipt failed"
    );
    worker.fast_forward(5).await?;
    let after = contract.view_account().await?.balance;
    assert!(
        after < combined_allowance,
        "account must hold less than 3.3 NEAR"
    );
    assert!(
        after >= before.saturating_add(cost),
        "adding allowances must not deduct their value"
    );
    for public_key in &public_keys {
        let key = contract.view_access_key(public_key).await?;
        let AccessKeyPermission::FunctionCall(permission) = key.permission else {
            anyhow::bail!("drop key must be a function-call key");
        };
        assert_eq!(permission.allowance, Some(allowance));
        assert_eq!(permission.receiver_id, contract.id().as_str());
        assert_eq!(permission.method_names, ["claim_for"]);
    }
    // The balance is usable even while all 100 keys and their allowances exist.
    contract
        .transfer_near(creator.id(), NearToken::from_millinear(50))
        .await?
        .into_result()?;
    let claimer = Account::from_secret_key(contract.id().clone(), keys[0].clone(), &worker);
    let claimed = claimer
        .call(contract.id(), "claim_for")
        .args_json(json!({"account_id": alice.id()}))
        .gas(CLAIM_GAS)
        .transact()
        .await?;
    assert!(claimed.receipt_failures().is_empty());
    assert!(
        claimed.json::<bool>()?,
        "key must remain usable without 3.3 NEAR reserved"
    );
    Ok(())
}
