use crate::init::TestAccount as Account;
use near_sdk::{Gas, NearToken};

pub const ONE_HUNDRED_TGAS: Gas = Gas::from_tgas(100);
// Calls signed with the drop key must fit in ACCESS_KEY_ALLOWANCE (0.033 N)
pub const CLAIM_GAS: Gas = Gas::from_tgas(32);
// Must match the contribution charged by the contract, independently of key allowance.
pub const CLAIM_GAS_BUDGET: NearToken = NearToken::from_yoctonear(3_200_000_000_000_000_000_000);

pub const INITIAL_CONTRACT_BALANCE: NearToken = NearToken::from_near(4);

pub async fn get_user_balance(user: &Account) -> NearToken {
    near_api::Account(user.id().clone())
        .view()
        .fetch_from(&user.network)
        .await
        .expect("account balance")
        .data
        .amount
}
