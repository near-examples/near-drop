// Security regression tests: each one reproduces an attack against the drop
// contract and asserts the SECURE behaviour. They fail on the current
// (vulnerable) code and should pass once each finding is fixed.

use near_sdk::{serde_json::json, NearToken};
use near_workspaces::{
    types::{KeyType, SecretKey},
    Account,
};

use crate::init::{init, init_failing_ft_contract, init_ft_contract, init_nft_contract};
use crate::utils::{CLAIM_GAS, INITIAL_CONTRACT_BALANCE, ONE_HUNDRED_TGAS};

// Create a 1-key FT drop and fund it. Returns (drop_id, secret_key).
async fn create_and_fund_ft_drop(
    contract: &Account,
    creator: &Account,
    ft_contract: &near_workspaces::Contract,
) -> anyhow::Result<(u32, SecretKey)> {
    let amount_per_drop = NearToken::from_yoctonear(1);
    let secret_key = SecretKey::from_random(KeyType::ED25519);

    let create = creator
        .call(contract.id(), "create_ft_drop")
        .args_json(json!({
            "public_keys": vec![secret_key.public_key()],
            "ft_contract": ft_contract.id(),
            "amount_per_drop": amount_per_drop,
        }))
        .deposit(NearToken::from_millinear(407))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;
    assert!(create.is_success(), "create_ft_drop failed: {create:#?}");
    let drop_id: u32 = create.json()?;

    // Register the drop contract on the FT so it can hold the tokens.
    let _ = creator
        .call(ft_contract.id(), "storage_deposit")
        .args_json(json!({"account_id": contract.id()}))
        .deposit(NearToken::from_yoctonear(12_500_000_000_000_000_000_000))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;

    let fund = creator
        .call(ft_contract.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": contract.id(),
            "amount": amount_per_drop,
            "msg": drop_id.to_string(),
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;
    assert!(fund.is_success(), "funding failed: {fund:#?}");

    Ok((drop_id, secret_key))
}

// init()'s `alice` is only ever a claim *recipient*, so it is barely funded.
// Tests where the attacker is the *signer* need an account that can pay gas.
async fn funded_attacker(root: &Account) -> anyhow::Result<Account> {
    Ok(root
        .create_subaccount("attacker")
        .initial_balance(NearToken::from_near(10))
        .transact()
        .await?
        .unwrap())
}

// --- Finding 1: resolve_* callbacks are not #[private] -> anyone can drain ---

// An external account calls the NEAR claim callback directly. Since it is not
// #[private], the callback runs with no promise result (treated as failure) and
// refunds `amount` to any `funder` the attacker names -> contract drain.
#[tokio::test]
async fn resolve_near_claim_must_be_private() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account().unwrap();
    let (contract, _creator, _alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;
    let attacker = funded_attacker(&root).await?;

    let attack = attacker
        .call(contract.id(), "resolve_near_claim")
        .args_json(json!({
            "account_created": false,
            "drop_deleted": true,
            "funder": attacker.id(),
            "amount": NearToken::from_near(3),
        }))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;

    assert!(
        attack.is_failure(),
        "resolve_near_claim must reject direct external calls (needs #[private])"
    );
    Ok(())
}

#[tokio::test]
async fn resolve_ft_claim_must_be_private() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account().unwrap();
    let (contract, _creator, _alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;
    let attacker = funded_attacker(&root).await?;

    let attack = attacker
        .call(contract.id(), "resolve_ft_claim")
        .args_json(json!({
            "account_created": false,
            "drop_deleted": true,
            "funder": attacker.id(),
            "amount": NearToken::from_near(1),
            "ft_contract": "ft.near",
        }))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;

    assert!(
        attack.is_failure(),
        "resolve_ft_claim must reject direct external calls (needs #[private])"
    );
    Ok(())
}

#[tokio::test]
async fn resolve_nft_claim_must_be_private() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account().unwrap();
    let (contract, _creator, _alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;
    let attacker = funded_attacker(&root).await?;

    let attack = attacker
        .call(contract.id(), "resolve_nft_claim")
        .args_json(json!({
            "account_created": false,
            "drop_deleted": true,
            "funder": attacker.id(),
            "token_id": "1",
            "nft_contract": "nft.near",
        }))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;

    assert!(
        attack.is_failure(),
        "resolve_nft_claim must reject direct external calls (needs #[private])"
    );
    Ok(())
}

// --- Finding 2: storage charged in yocto (not NEAR) + empty keys accepted ---

// Storing a drop costs ~0.001 NEAR of real storage, but required_storage_drop
// returns the byte count as yoctoNEAR. An attacker creates drops for a few
// hundred thousand yocto and forces the contract to lock its own balance.
#[tokio::test]
async fn create_drop_must_charge_real_storage_cost() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account().unwrap();
    let (contract, _creator, _alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;
    let attacker = funded_attacker(&root).await?;

    // No keys -> per-key cost is 0, so only the (undercharged) storage is checked.
    // 100_000 yocto is far below the real storage cost of persisting a Drop.
    let attack = attacker
        .call(contract.id(), "create_near_drop")
        .args_json(json!({
            "public_keys": Vec::<String>::new(),
            "amount_per_drop": NearToken::from_yoctonear(1),
        }))
        .deposit(NearToken::from_yoctonear(100_000))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;

    assert!(
        attack.is_failure(),
        "create_near_drop must charge real storage cost and reject empty key sets"
    );
    Ok(())
}

// --- Finding 4: duplicate public keys in one Vec strand funds ---

// Passing the same key twice makes counter=2 while only one key can ever claim,
// leaving an unrecoverable drop with locked funds. Creation must reject dupes.
#[tokio::test]
async fn create_drop_must_reject_duplicate_keys() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account().unwrap();
    let (contract, creator, _alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;

    let secret_key = SecretKey::from_random(KeyType::ED25519);
    let pk = secret_key.public_key();

    let attack = creator
        .call(contract.id(), "create_near_drop")
        .args_json(json!({
            "public_keys": vec![pk.clone(), pk.clone()],
            "amount_per_drop": NearToken::from_near(1),
        }))
        .deposit(NearToken::from_millinear(2210))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;

    assert!(
        attack.is_failure(),
        "create_near_drop must reject a key list containing duplicates"
    );
    Ok(())
}

// --- Finding 3: access key is never deleted after the drop is consumed ---

// After a single-use drop is claimed and deleted, the access key it added to the
// contract must be removed too; otherwise its leftover allowance keeps burning
// contract gas and the storage refund is unbacked.
#[tokio::test]
async fn access_key_deleted_after_claim() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account().unwrap();
    let (contract, creator, alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;

    let secret_key = SecretKey::from_random(KeyType::ED25519);
    let pk = secret_key.public_key();

    let create = creator
        .call(contract.id(), "create_near_drop")
        .args_json(json!({
            "public_keys": vec![pk.clone()],
            "amount_per_drop": NearToken::from_near(1),
        }))
        .deposit(NearToken::from_millinear(2810))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;
    assert!(create.is_success());

    // Key exists on the contract right after creation.
    assert!(contract.view_access_key(&pk).await.is_ok());

    let claimer: Account = Account::from_secret_key(contract.id().clone(), secret_key, &worker);
    let claim = claimer
        .call(contract.id(), "claim_for")
        .args_json(json!({"account_id": alice.id()}))
        .gas(CLAIM_GAS)
        .transact()
        .await?;
    assert!(claim.is_success());

    assert!(
        contract.view_access_key(&pk).await.is_err(),
        "access key must be deleted once the drop is fully claimed"
    );
    Ok(())
}

// --- Finding 8: resolve_account_create ignores Ok(false) from create_account ---

// Claiming onto an already-existing account makes create_account return false.
// The contract must not treat that as success and consume the drop.
#[tokio::test]
async fn claim_onto_existing_account_must_not_consume_drop() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account().unwrap();
    let (contract, creator, alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;

    let secret_key = SecretKey::from_random(KeyType::ED25519);
    let pk = secret_key.public_key();

    let create = creator
        .call(contract.id(), "create_near_drop")
        .args_json(json!({
            "public_keys": vec![pk.clone()],
            "amount_per_drop": NearToken::from_near(1),
        }))
        .deposit(NearToken::from_millinear(2810))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;
    let drop_id: u32 = create.json().unwrap();

    // alice already exists, so create_account fails and returns Ok(false).
    let claimer: Account = Account::from_secret_key(contract.id().clone(), secret_key, &worker);
    let _ = claimer
        .call(contract.id(), "create_account_and_claim")
        .args_json(json!({"account_id": alice.id()}))
        .gas(crate::utils::CREATE_ACCOUNT_AND_CLAIM_GAS)
        .transact()
        .await?;

    // The drop must survive a failed account creation.
    let get_drop = creator
        .call(contract.id(), "get_drop_by_id")
        .args_json(json!({"drop_id": drop_id}))
        .transact()
        .await?;
    assert!(
        get_drop.is_success(),
        "drop must not be consumed when create_account returns false"
    );
    Ok(())
}

// --- Finding 6: ft_on_transfer accepts funding an already-funded drop ---

// A second FT transfer to a fully-funded drop must be refunded, not swallowed.
#[tokio::test]
async fn ft_on_transfer_refunds_already_funded_drop() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account().unwrap();
    let (contract, creator, _alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;
    let ft_contract = init_ft_contract(&worker, &creator).await?;

    let amount_per_drop = NearToken::from_yoctonear(1);
    let secret_key = SecretKey::from_random(KeyType::ED25519);

    let create = creator
        .call(contract.id(), "create_ft_drop")
        .args_json(json!({
            "public_keys": vec![secret_key.public_key()],
            "ft_contract": ft_contract.id(),
            "amount_per_drop": amount_per_drop,
        }))
        .deposit(NearToken::from_millinear(407))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;
    let drop_id: u32 = create.json().unwrap();

    // Register the contract on the FT so transfers land.
    let _ = creator
        .call(ft_contract.id(), "storage_deposit")
        .args_json(json!({"account_id": contract.id()}))
        .deposit(NearToken::from_yoctonear(12_500_000_000_000_000_000_000))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;

    let fund = |amount: NearToken| {
        creator
            .call(ft_contract.id(), "ft_transfer_call")
            .args_json(json!({
                "receiver_id": contract.id(),
                "amount": amount,
                "msg": drop_id.to_string(),
            }))
            .deposit(NearToken::from_yoctonear(1))
            .gas(ONE_HUNDRED_TGAS)
    };

    // First (legitimate) funding.
    assert!(fund(amount_per_drop).transact().await?.is_success());

    let balance_before_second = ft_contract
        .call("ft_balance_of")
        .args_json(json!({"account_id": creator.id()}))
        .view()
        .await?
        .json::<NearToken>()?;

    // Second funding of the same, already-funded drop.
    assert!(fund(amount_per_drop).transact().await?.is_success());

    let balance_after_second = ft_contract
        .call("ft_balance_of")
        .args_json(json!({"account_id": creator.id()}))
        .view()
        .await?
        .json::<NearToken>()?;

    assert_eq!(
        balance_before_second, balance_after_second,
        "tokens sent to an already-funded drop must be refunded, not trapped"
    );
    Ok(())
}

// --- Finding 7: nft_on_approve accepts approvals from a non-funder ---

// Anyone owning a token on the same NFT contract can hijack another user's drop
// by approving it with their own token id. Only the funder's approval counts.
#[tokio::test]
async fn nft_on_approve_rejects_non_funder() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account().unwrap();
    let (contract, creator, _alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;
    let (nft_contract, _creator_token) = init_nft_contract(&worker, &creator).await?;

    // Attacker owns a different token on the same NFT contract.
    let mallory = root
        .create_subaccount("mallory")
        .initial_balance(NearToken::from_near(5))
        .transact()
        .await?
        .unwrap();
    let mallory_token = "666";
    let _ = mallory
        .call(nft_contract.id(), "nft_mint")
        .args_json(json!({
            "token_id": mallory_token,
            "token_owner_id": mallory.id(),
            "token_metadata": {"title": "evil", "copies": 1},
        }))
        .deposit(NearToken::from_yoctonear(6_580_000_000_000_000_000_000))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;

    // Creator opens a drop (not yet funded).
    let secret_key = SecretKey::from_random(KeyType::ED25519);
    let create = creator
        .call(contract.id(), "create_nft_drop")
        .args_json(json!({"public_key": secret_key.public_key(), "nft_contract": nft_contract.id()}))
        .deposit(NearToken::from_millinear(407))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;
    let drop_id: u32 = create.json().unwrap();

    // Mallory tries to bind her token to the creator's drop.
    let _ = mallory
        .call(nft_contract.id(), "nft_approve")
        .args_json(json!({
            "token_id": mallory_token,
            "account_id": contract.id(),
            "msg": drop_id.to_string(),
        }))
        .deposit(NearToken::from_yoctonear(450_000_000_000_000_000_000))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;

    let drop = contract
        .view(contract.id(), "get_drop_by_id")
        .args_json(json!({"drop_id": drop_id}))
        .await?
        .json::<serde_json::Value>()?;

    assert_ne!(
        drop["NFT"]["token_id"], mallory_token,
        "a non-funder must not be able to bind their token to someone else's drop"
    );
    Ok(())
}

// --- Registration accounting: funder must not overpay FT registration ---

// On a successful claim only the FT's real registration cost is spent; the
// funder is refunded the pre-paid excess (FT_REGISTER minus the real cost).
#[tokio::test]
async fn ft_claim_refunds_registration_excess_to_funder() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account().unwrap();
    let (contract, creator, alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;
    let ft_contract = init_ft_contract(&worker, &creator).await?;

    let (_drop_id, secret_key) = create_and_fund_ft_drop(&contract, &creator, &ft_contract).await?;

    let funder_before = creator.view_account().await?.balance;

    let claimer = Account::from_secret_key(contract.id().clone(), secret_key, &worker);
    let claim = claimer
        .call(contract.id(), "claim_for")
        .args_json(json!({"account_id": alice.id()}))
        .gas(CLAIM_GAS)
        .transact()
        .await?;
    assert!(claim.is_success(), "claim failed: {claim:#?}");

    let refunded = creator
        .view_account()
        .await?
        .balance
        .saturating_sub(funder_before);

    // Without the excess refund the funder would only get back ACCESS_KEY_STORAGE +
    // freed storage (~0.0025 N). The unused FT_REGISTER excess (~0.011 N) pushes it past.
    assert!(
        refunded > NearToken::from_millinear(5),
        "funder should be refunded the unused FT registration excess, got {refunded}"
    );
    Ok(())
}

// When the claim's ft_transfer fails, the whole pre-paid FT_REGISTER comes back to
// the funder (the storage_deposit + ft_transfer batch reverts atomically).
#[tokio::test]
async fn ft_claim_failure_refunds_registration_to_funder() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account().unwrap();
    let (contract, creator, alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;
    // This FT's ft_transfer always panics, so the claim hits its failure branch.
    let ft_contract = init_failing_ft_contract(&worker, &creator).await?;

    let (_drop_id, secret_key) = create_and_fund_ft_drop(&contract, &creator, &ft_contract).await?;

    let funder_before = creator.view_account().await?.balance;

    let claimer = Account::from_secret_key(contract.id().clone(), secret_key, &worker);
    let claim = claimer
        .call(contract.id(), "claim_for")
        .args_json(json!({"account_id": alice.id()}))
        .gas(CLAIM_GAS)
        .transact()
        .await?;

    // ft_transfer panics -> the claim batch fails, but resolve_ft_claim handles it.
    assert!(
        claim.receipt_failures().len() > 0,
        "ft_transfer should have failed"
    );
    assert_eq!(claim.json::<bool>()?, true);

    let refunded = creator
        .view_account()
        .await?
        .balance
        .saturating_sub(funder_before);

    // The whole pre-paid FT_REGISTER (~0.0125 N) is returned on failure.
    assert!(
        refunded > NearToken::from_millinear(10),
        "funder must be refunded FT_REGISTER when the claim fails, got {refunded}"
    );
    Ok(())
}
