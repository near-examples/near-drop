use crate::{Contract, CreateDrop};
use near_sdk::mock::MockAction;
use near_sdk::serde_json::json;
use near_sdk::test_utils::{get_created_receipts, VMContextBuilder};
use near_sdk::{env, testing_env, AccountId, CurveType, NearToken, PublicKey};

fn key(index: u8) -> PublicKey {
    PublicKey::from_parts(CurveType::ED25519, vec![index; 32]).unwrap()
}

// Inspect actual outgoing transfers so gas refunds cannot mask a missing
// registration budget or duplicate refund of prepaid NFT storage padding.
#[test]
fn deleting_ft_returns_full_near_budget_and_all_tokens() {
    let funder: AccountId = "funder.near".parse().unwrap();
    let token: AccountId = "ft.near".parse().unwrap();
    let mut context = VMContextBuilder::new();
    context
        .current_account_id("drop.near".parse().unwrap())
        .predecessor_account_id(funder.clone())
        .attached_deposit(NearToken::from_near(2));
    testing_env!(context.build());
    let mut contract = Contract::new();
    let keys = vec![key(1), key(2)];
    let amount = NearToken::from_yoctonear(7);
    let cost = contract.get_drop_cost(
        funder.clone(),
        CreateDrop::FT {
            public_keys: keys.clone(),
            ft_contract: token.clone(),
            amount_per_drop: amount,
        },
    );
    let id = contract.create_drop(CreateDrop::FT {
        public_keys: keys.clone(),
        ft_contract: token.clone(),
        amount_per_drop: amount,
    });
    context
        .storage_usage(env::storage_usage())
        .predecessor_account_id(token.clone())
        .attached_deposit(NearToken::from_yoctonear(0));
    testing_env!(context.build());
    let _ = contract.ft_on_transfer(
        funder.clone(),
        NearToken::from_yoctonear(14),
        id.to_string(),
    );
    contract.flush_maps();
    context
        .storage_usage(env::storage_usage())
        .predecessor_account_id(funder.clone());
    testing_env!(context.build());
    drop(contract.delete_drop(id));
    let receipts = get_created_receipts();
    let paid: NearToken = receipts
        .iter()
        .filter(|receipt| receipt.receiver_id == funder)
        .flat_map(|receipt| &receipt.actions)
        .fold(NearToken::from_yoctonear(0), |sum, action| match action {
            MockAction::Transfer { deposit, .. } => sum.saturating_add(*deposit),
            _ => sum,
        });
    assert_eq!(
        paid, cost,
        "return gas budgets, registration budgets, and all storage"
    );
    let transfer_args = receipts
        .iter()
        .filter(|receipt| receipt.receiver_id == token)
        .flat_map(|receipt| &receipt.actions)
        .find_map(|action| match action {
            MockAction::FunctionCallWeight {
                method_name, args, ..
            } if method_name == b"ft_transfer" => {
                Some(near_sdk::serde_json::from_slice::<near_sdk::serde_json::Value>(args).unwrap())
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(
        transfer_args,
        json!({"receiver_id": funder, "amount": "14"})
    );
    assert!(contract.drop_by_id.get(&id).is_none());
    assert!(contract.keys_by_drop.get(&id).is_none());
    for key in keys {
        assert!(contract.drop_id_by_key.get(&key).is_none());
    }
}

#[test]
fn deleting_approved_nft_refunds_padding_exactly_once() {
    let funder: AccountId = "funder.near".parse().unwrap();
    let token: AccountId = "nft.near".parse().unwrap();
    let mut context = VMContextBuilder::new();
    context
        .current_account_id("drop.near".parse().unwrap())
        .predecessor_account_id(funder.clone())
        .attached_deposit(NearToken::from_near(1));
    testing_env!(context.build());
    let mut contract = Contract::new();
    let cost = contract.get_drop_cost(
        funder.clone(),
        CreateDrop::NFT {
            public_key: key(1),
            nft_contract: token.clone(),
        },
    );
    let id = contract.create_drop(CreateDrop::NFT {
        public_key: key(1),
        nft_contract: token.clone(),
    });
    context
        .storage_usage(env::storage_usage())
        .predecessor_account_id(token)
        .attached_deposit(NearToken::from_yoctonear(0));
    testing_env!(context.build());
    let _ = contract.nft_on_approve(
        "short-token-id".to_string(),
        funder.clone(),
        0,
        id.to_string(),
    );
    contract.flush_maps();
    context
        .storage_usage(env::storage_usage())
        .predecessor_account_id(funder.clone());
    testing_env!(context.build());
    drop(contract.delete_drop(id));
    let paid = get_created_receipts()
        .iter()
        .filter(|receipt| receipt.receiver_id == funder)
        .flat_map(|receipt| &receipt.actions)
        .fold(NearToken::from_yoctonear(0), |sum, action| match action {
            MockAction::Transfer { deposit, .. } => sum.saturating_add(*deposit),
            _ => sum,
        });
    assert_eq!(
        paid, cost,
        "used and unused NFT padding must sum to the prepaid amount"
    );
}

#[test]
fn failed_ft_registration_query_returns_false_and_refunds_assets() {
    use near_sdk::{PromiseError, PromiseOrValue};
    let funder: AccountId = "funder.near".parse().unwrap();
    let token: AccountId = "ft.near".parse().unwrap();
    let mut context = VMContextBuilder::new();
    context.current_account_id("drop.near".parse().unwrap());
    testing_env!(context.build());
    let crate::Drop::FT(mut drop) = crate::ft_drop::create(
        funder.clone(),
        token.clone(),
        NearToken::from_yoctonear(7),
        1,
    ) else {
        unreachable!()
    };
    drop.funded = true;
    let storage_refund = NearToken::from_millinear(2);
    let result = Contract::resolve_ft_registration(
        "recipient.near".parse().unwrap(),
        storage_refund,
        drop,
        Err(PromiseError::Failed),
    );
    assert!(matches!(result, PromiseOrValue::Value(false)));
    let receipts = get_created_receipts();
    let refunded = receipts
        .iter()
        .filter(|receipt| receipt.receiver_id == funder)
        .flat_map(|receipt| &receipt.actions)
        .fold(NearToken::from_yoctonear(0), |sum, action| match action {
            MockAction::Transfer { deposit, .. } => sum.saturating_add(*deposit),
            _ => sum,
        });
    assert_eq!(
        refunded,
        crate::ft_drop::required_deposit_per_key()
            .saturating_sub(crate::constants::CLAIM_GAS_BUDGET)
            .saturating_add(storage_refund)
    );
    let token_refund = receipts
        .iter()
        .filter(|receipt| receipt.receiver_id == token)
        .flat_map(|receipt| &receipt.actions)
        .find_map(|action| match action {
            MockAction::FunctionCallWeight {
                method_name, args, ..
            } if method_name == b"ft_transfer" => {
                Some(near_sdk::serde_json::from_slice::<near_sdk::serde_json::Value>(args).unwrap())
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(token_refund, json!({"receiver_id": funder, "amount": "7"}));
}
