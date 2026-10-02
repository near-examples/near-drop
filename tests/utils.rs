use near_sdk::{Gas, NearToken};
use near_workspaces::{types::AccountDetails, Account};

pub const ONE_HUNDRED_TGAS: Gas = Gas::from_tgas(100);
// Calls signed with the drop key must fit in ACCESS_KEY_ALLOWANCE (0.033 N)
pub const CLAIM_GAS: Gas = Gas::from_tgas(32);
// Must match the contribution charged by the contract, independently of key allowance.
pub const CLAIM_GAS_BUDGET: NearToken = NearToken::from_yoctonear(3_200_000_000_000_000_000_000);

pub const INITIAL_CONTRACT_BALANCE: NearToken = NearToken::from_near(4);

pub async fn get_user_balance(user: &Account) -> NearToken {
    let details: AccountDetails = user
        .view_account()
        .await
        .expect("Account has to have some balance");
    details.balance
}
