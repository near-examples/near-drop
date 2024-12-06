use near_sdk::NearToken;
use near_workspaces::{types::AccountDetails, Account};

pub async fn get_user_balance(user: &Account) -> NearToken {
    let details: AccountDetails = user
        .view_account()
        .await
        .expect("Account has to have some balance");
    details.balance
}
