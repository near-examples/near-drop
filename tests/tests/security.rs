// Security regression tests: each one reproduces an attack against the drop
// contract and asserts the SECURE behaviour. They fail on the current
// (vulnerable) code and should pass once each finding is fixed.

use near_sdk::{serde_json::json, NearToken};
use near_workspaces::{
    types::{KeyType, SecretKey},
    Account,
};

use crate::init::{init, init_failing_ft_contract, init_ft_contract, init_nft_contract};
use crate::utils::{CLAIM_GAS, CLAIM_GAS_BUDGET, INITIAL_CONTRACT_BALANCE, ONE_HUNDRED_TGAS};

// Create a 1-key FT drop and fund it. Returns (drop_id, secret_key).
async fn create_and_fund_ft_drop(
    contract: &Account,
    creator: &Account,
    ft_contract: &near_workspaces::Contract,
) -> anyhow::Result<(u32, SecretKey)> {
    let amount_per_drop = NearToken::from_yoctonear(1);
    let secret_key = SecretKey::from_random(KeyType::ED25519);

    let create = creator
        .call(contract.id(), "create_drop")
        .args_json(json!({"drop": {"FT": {
            "public_keys": vec![secret_key.public_key()],
            "ft_contract": ft_contract.id(),
            "amount_per_drop": amount_per_drop,
        }}}))
        .deposit(NearToken::from_millinear(407))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;
    assert!(create.is_success(), "create_drop failed: {create:#?}");
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

    for (method, args) in [
        (
            "resolve_ft_claim",
            json!({
                "storage_refund": "0", "funder": attacker.id(), "amount": "1",
                "ft_contract": "ft.near", "refund_registration": true,
            }),
        ),
        (
            "resolve_ft_registration",
            json!({
                "account_id": attacker.id(), "storage_refund": "0",
                "drop": {"funder": attacker.id(), "amount": "1", "ft_contract": "ft.near", "counter": 1, "funded": true},
            }),
        ),
    ] {
        let attack = attacker
            .call(contract.id(), method)
            .args_json(args)
            .gas(ONE_HUNDRED_TGAS)
            .transact()
            .await?;
        assert!(attack.is_failure(), "{method} must reject external calls");
        assert!(
            format!("{:?}", attack.failures()).contains("private"),
            "{attack:?}"
        );
    }
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

// Storing a drop costs real storage (bytes × storage_byte_cost). Paying only
// the per-key fees must not be enough, or the contract locks its own balance.
#[tokio::test]
async fn create_drop_must_charge_real_storage_cost() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account().unwrap();
    let (contract, _creator, _alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;
    let attacker = funded_attacker(&root).await?;

    let create = |public_keys: Vec<near_workspaces::types::PublicKey>, deposit: NearToken| {
        attacker
            .call(contract.id(), "create_drop")
            .args_json(json!({"drop": {"NEAR": {"public_keys": public_keys, "amount_per_drop": NearToken::from_yoctonear(1)}}}))
            .deposit(deposit)
            .gas(ONE_HUNDRED_TGAS)
            .transact()
    };

    // Empty key sets have no per-key cost at all, so they must be rejected outright.
    let empty = create(vec![], NearToken::from_near(1)).await?;
    assert!(empty.is_failure(), "create_drop must reject empty key sets");

    // 1 yocto + CLAIM_GAS_BUDGET + ACCESS_KEY_STORAGE: per-key fees, zero storage.
    let fees_only = CLAIM_GAS_BUDGET
        .saturating_add(NearToken::from_millinear(1))
        .saturating_add(NearToken::from_yoctonear(1));
    let pk = SecretKey::from_random(KeyType::ED25519).public_key();
    let attack = create(vec![pk], fees_only).await?;
    assert!(
        attack.is_failure(),
        "create_drop must charge the drop's storage on top of per-key fees"
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
        .call(contract.id(), "create_drop")
        .args_json(json!({"drop": {"NEAR": {
            "public_keys": vec![pk.clone(), pk.clone()],
            "amount_per_drop": NearToken::from_near(1),
        }}}))
        .deposit(NearToken::from_millinear(2210))
        .gas(ONE_HUNDRED_TGAS)
        .transact()
        .await?;

    assert!(
        attack.is_failure(),
        "create_drop must reject a key list containing duplicates"
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
        .call(contract.id(), "create_drop")
        .args_json(json!({"drop": {"NEAR": {
            "public_keys": vec![pk.clone()],
            "amount_per_drop": NearToken::from_near(1),
        }}}))
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

#[tokio::test]
async fn ft_on_transfer_refunds_already_funded_drop() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account().unwrap();
    let (contract, creator, _alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;
    let ft_contract = init_ft_contract(&worker, &creator).await?;

    let amount_per_drop = NearToken::from_yoctonear(1);
    let secret_key = SecretKey::from_random(KeyType::ED25519);

    let create = creator
        .call(contract.id(), "create_drop")
        .args_json(json!({"drop": {"FT": {
            "public_keys": vec![secret_key.public_key()],
            "ft_contract": ft_contract.id(),
            "amount_per_drop": amount_per_drop,
        }}}))
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
    // Only the NFT contract owner (creator) can mint, so mint it for mallory.
    let mint = creator
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
    assert!(mint.is_success(), "mint failed: {:?}", mint.failures());

    // Creator opens a drop (not yet funded).
    let secret_key = SecretKey::from_random(KeyType::ED25519);
    let create = creator
        .call(contract.id(), "create_drop")
        .args_json(json!({"drop": {"NFT": {"public_key": secret_key.public_key(), "nft_contract": nft_contract.id()}}}))
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

// --- Fixed registration budget and claim refunds ---

// Successful claims refund freed storage; FT registration uses a fixed budget.
#[tokio::test]
async fn ft_claim_with_fixed_registration_budget_refunds_storage() -> anyhow::Result<()> {
    for registered in [false, true] {
        let worker = near_workspaces::sandbox().await?;
        let root = worker.root_account().unwrap();
        let (contract, creator, alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;
        let ft_contract = init_ft_contract(&worker, &creator).await?;

        let (_drop_id, secret_key) =
            create_and_fund_ft_drop(&contract, &creator, &ft_contract).await?;

        let cost: NearToken = contract
            .view(contract.id(), "get_drop_cost")
            .args_json(json!({"funder": creator.id(), "drop": {"FT": {

                "public_keys": [secret_key.public_key()],
                "ft_contract": ft_contract.id(),
                "amount_per_drop": NearToken::from_yoctonear(1),
            }}}))
            .await?
            .json()?;
        if registered {
            creator
                .call(ft_contract.id(), "storage_deposit")
                .args_json(json!({"account_id": alice.id()}))
                .deposit(NearToken::from_millinear(20))
                .gas(ONE_HUNDRED_TGAS)
                .transact()
                .await?
                .into_result()?;
        }
        // Settle gas refunds from creation/funding before measuring the claim refund.
        worker.fast_forward(10).await?;
        let funder_before = creator.view_account().await?.balance;

        let claimer = Account::from_secret_key(contract.id().clone(), secret_key, &worker);
        let claim = claimer
            .call(contract.id(), "claim_for")
            .args_json(json!({"account_id": alice.id()}))
            .gas(CLAIM_GAS)
            .transact()
            .await?;
        assert!(claim.is_success(), "claim failed: {claim:#?}");
        assert!(claim.receipt_failures().is_empty(), "{claim:?}");
        assert!(claim.json::<bool>()?);

        let refunded = creator
            .view_account()
            .await?
            .balance
            .saturating_sub(funder_before);

        assert_eq!(
            refunded,
            cost.saturating_sub(CLAIM_GAS_BUDGET)
                .saturating_sub(if registered {
                    NearToken::from_yoctonear(0)
                } else {
                    NearToken::from_yoctonear(1_260_000_000_000_000_000_000)
                }),
            "registration refunded exactly when skipped (registered={registered})"
        );
        let recipient_balance: String = ft_contract
            .view("ft_balance_of")
            .args_json(json!({"account_id": alice.id()}))
            .await?
            .json()?;
        assert_eq!(
            recipient_balance, "1",
            "claim must register and pay the recipient"
        );
    }
    Ok(())
}

// When the claim's ft_transfer fails, the whole pre-paid FT_REGISTER comes back to
// the funder (the storage_deposit + ft_transfer batch reverts atomically).
#[tokio::test]
async fn ft_claim_failure_refunds_registration_to_funder() -> anyhow::Result<()> {
    for registered in [false, true] {
        let worker = near_workspaces::sandbox().await?;
        let root = worker.root_account().unwrap();
        let (contract, creator, alice) = init(&root, INITIAL_CONTRACT_BALANCE).await?;
        // This FT's ft_transfer always panics, so the claim hits its failure branch.
        let ft_contract = init_failing_ft_contract(&worker, &creator).await?;

        let (_drop_id, secret_key) =
            create_and_fund_ft_drop(&contract, &creator, &ft_contract).await?;

        let cost: NearToken = contract
            .view(contract.id(), "get_drop_cost")
            .args_json(json!({"funder": creator.id(), "drop": {"FT": {

                "public_keys": [secret_key.public_key()],
                "ft_contract": ft_contract.id(),
                "amount_per_drop": NearToken::from_yoctonear(1),
            }}}))
            .await?
            .json()?;
        if registered {
            creator
                .call(ft_contract.id(), "storage_deposit")
                .args_json(json!({"account_id": alice.id()}))
                .deposit(NearToken::from_millinear(20))
                .gas(ONE_HUNDRED_TGAS)
                .transact()
                .await?
                .into_result()?;
        }
        // Settle gas refunds from creation/funding before measuring the claim refund.
        worker.fast_forward(10).await?;
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
        assert!(!claim.json::<bool>()?);

        let refunded = creator
            .view_account()
            .await?
            .balance
            .saturating_sub(funder_before);

        // Return the fixed registration budget along with freed storage.
        assert_eq!(
            refunded,
            cost.saturating_sub(CLAIM_GAS_BUDGET),
            "funder must be refunded FT_REGISTER when the claim fails"
        );
    }
    Ok(())
}
