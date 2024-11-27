use near_sdk::{near, AccountId, Promise};

use crate::ft_drop::FTDrop;
use crate::near_drop::TokenDrop;

#[derive(PartialEq)]
#[near(serializers = [borsh])]
pub enum DropType {
    NEAR(TokenDrop),
    FT(FTDrop),
}

pub trait Dropper {
    fn promise_for_claiming(&self, account_id: AccountId) -> Promise;
    fn promise_to_resolve_claim(&self, created: bool) -> Promise;
}

impl Dropper for DropType {
    fn promise_for_claiming(&self, account_id: AccountId) -> Promise {
        match self {
            DropType::NEAR(tkdrop) => tkdrop.promise_for_claiming(account_id),
            DropType::FT(ftdrop) => ftdrop.promise_for_claiming(account_id),
        }
    }

    fn promise_to_resolve_claim(&self, created: bool) -> Promise {
        match self {
            DropType::NEAR(tk_drop) => tk_drop.promise_to_resolve_claim(created),
            DropType::FT(ft_drop) => ft_drop.promise_to_resolve_claim(created),
        }
    }
}
