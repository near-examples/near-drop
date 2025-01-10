use near_sdk::{Gas, NearToken};
use near_workspaces::{types::AccountDetails, Account};

pub const ONE_HUNDRED_TGAS: Gas = Gas::from_tgas(100);

pub const INITIAL_CONTRACT_BALANCE: NearToken = NearToken::from_near(4);

pub async fn get_user_balance(user: &Account) -> NearToken {
    let details: AccountDetails = user
        .view_account()
        .await
        .expect("Account has to have some balance");
    details.balance
}
