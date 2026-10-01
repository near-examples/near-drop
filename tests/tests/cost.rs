// get_*_drop_cost must return exactly what create_*_drop charges, and a claim
// must refund the storage it frees.

use near_sdk::{
    serde_json::{json, Value},
    NearToken,
};
use near_workspaces::{
    types::{KeyType, PublicKey, SecretKey},
    Account,
};

use crate::init::{init, init_nft_contract};
use crate::utils::{get_user_balance, CLAIM_GAS, INITIAL_CONTRACT_BALANCE, ONE_HUNDRED_TGAS};

fn new_key() -> SecretKey {
    SecretKey::from_random(KeyType::ED25519)
}

// Quotes the cost via `view`, then checks create rejects cost-1 and accepts cost.
// `view_args` are `create_args` plus the funder.
async fn assert_quote_is_exact(
    contract: &Account,
    creator: &Account,
    view: &str,
    create: &str,
    create_args: Value,
) -> anyhow::Result<NearToken> {
    let mut view_args = create_args.clone();
    view_args["funder"] = json!(creator.id());
    let cost: NearToken = contract.view(contract.id(), view).args_json(view_args).await?.json()?;

    let call = |deposit: NearToken| {
        creator
            .call(contract.id(), create)
            .args_json(create_args.clone())
            .deposit(deposit)
            .gas(ONE_HUNDRED_TGAS)
            .transact()
    };

    let short = call(cost.saturating_sub(NearToken::from_yoctonear(1))).await?;
    assert!(short.is_failure(), "{create}: 1 yocto below {view} must be rejected");

    let exact = call(cost).await?;
    assert!(exact.is_success(), "{create}: {view} must be enough: {:?}", exact.failures());
    Ok(cost)
}

fn keys(n: usize) -> Vec<PublicKey> {
    (0..n).map(|_| new_key().public_key()).collect()
}

#[tokio::test]
async fn near_drop_cost_view_matches_charge() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account().unwrap();
    let (contract, creator, _alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;

    let args = json!({"public_keys": keys(2), "amount_per_drop": NearToken::from_near(1)});
    let cost =
        assert_quote_is_exact(&contract, &creator, "get_near_drop_cost", "create_near_drop", args)
            .await?;

    // Storage is actually charged, so the cost is above the per-key fees alone.
    let fees = NearToken::from_millinear(2 * 1101);
    assert!(cost > fees, "storage not included: {cost}");
    Ok(())
}

// create_ft_drop / create_nft_drop only store the contract id, so any account works.
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
    assert_quote_is_exact(&contract, &creator, "get_ft_drop_cost", "create_ft_drop", args).await?;
    Ok(())
}

#[tokio::test]
async fn nft_drop_cost_view_matches_charge() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account().unwrap();
    let (contract, creator, alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;

    let args = json!({"public_key": new_key().public_key(), "nft_contract": alice.id()});
    assert_quote_is_exact(&contract, &creator, "get_nft_drop_cost", "create_nft_drop", args)
        .await?;
    Ok(())
}

// Claiming a 1-key drop frees all of its storage, so the funder gets back exactly
// cost - amount - ACCESS_KEY_ALLOWANCE (storage + ACCESS_KEY_STORAGE). Fails if
// the maps aren't flushed before measuring (refund would miss the storage).
#[tokio::test]
async fn claim_refunds_freed_storage_to_funder() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account().unwrap();
    let (contract, creator, alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;

    let secret_key = new_key();
    let amount = NearToken::from_near(1);
    let args = json!({"public_keys": [secret_key.public_key()], "amount_per_drop": amount});
    let cost =
        assert_quote_is_exact(&contract, &creator, "get_near_drop_cost", "create_near_drop", args)
            .await?;

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

    let expected = cost
        .saturating_sub(amount)
        .saturating_sub(NearToken::from_millinear(100)); // ACCESS_KEY_ALLOWANCE
    assert_eq!(refund, expected, "funder must get back all freed storage");
    Ok(())
}

// nft_on_approve stores the token_id after create_nft_drop charged for an empty
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
        .view(contract.id(), "get_nft_drop_cost")
        .args_json(json!({
            "funder": creator.id(),
            "public_key": secret_key.public_key(),
            "nft_contract": nft_contract.id(),
        }))
        .await?
        .json()?;
    let create = creator
        .call(contract.id(), "create_nft_drop")
        .args_json(json!({"public_key": secret_key.public_key(), "nft_contract": nft_contract.id()}))
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
        .args_json(json!({"token_id": token_id, "account_id": contract.id(), "msg": drop_id.to_string()}))
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

    // Funder paid cost = storage + ACCESS_KEY_STORAGE + ACCESS_KEY_ALLOWANCE;
    // only the first two are refundable.
    let refundable = cost.saturating_sub(NearToken::from_millinear(100));
    assert!(
        refund <= refundable,
        "claim refunded {refund} but funder only paid {refundable} of refundable storage"
    );
    Ok(())
}
