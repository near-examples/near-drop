use near_sdk::NearToken;
use near_workspaces::{types::AccountDetails, Account};

pub const INITIAL_CONTRACT_BALANCE: NearToken = NearToken::from_near(4);
// pub const INITIAL_CONTRACT_BALANCE: NearToken = NearToken::from_yoctonear(
//     2072905965164945819274880u128
//         + 720000000000000000000u128
//         + 20014277188600000000u128
//         + 44131962848063280725120u128
//         + 182620304538970019274880u128
//         + 720000000000000000000u128,
// );

pub async fn get_user_balance(user: &Account) -> NearToken {
    let details: AccountDetails = user
        .view_account()
        .await
        .expect("Account has to have some balance");
    details.balance
}
