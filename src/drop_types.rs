use near_sdk::{near, AccountId, Promise};

use crate::ft_drop::FTDrop;
use crate::near_drop::NearDrop;
use crate::nft_drop::NFTDrop;

#[derive(PartialEq, Clone, Debug)]
#[near(serializers = [borsh])]
pub enum DropType {
    NEAR(NearDrop),
    FT(FTDrop),
    NFT(NFTDrop),
}

pub trait Dropper {
    fn promise_for_claiming(&self, account_id: AccountId) -> Promise;
    fn promise_to_resolve_claim(&self, created: bool) -> Promise;
}

impl Dropper for DropType {
    fn promise_for_claiming(&self, account_id: AccountId) -> Promise {
        match self {
            DropType::NEAR(near_drop) => near_drop.promise_for_claiming(account_id),
            DropType::FT(ft_drop) => ft_drop.promise_for_claiming(account_id),
            DropType::NFT(nft_drop) => nft_drop.promise_for_claiming(account_id),
        }
    }

    fn promise_to_resolve_claim(&self, created: bool) -> Promise {
        match self {
            DropType::NEAR(near_drop) => near_drop.promise_to_resolve_claim(created),
            DropType::FT(ft_drop) => ft_drop.promise_to_resolve_claim(created),
            DropType::NFT(nft_drop) => nft_drop.promise_to_resolve_claim(created),
        }
    }
}
