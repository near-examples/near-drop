use near_sdk::borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::{near, AccountId, NearToken, Promise};

use crate::ft_drop::FTDrop;
use crate::near_drop::NearDrop;
use crate::nft_drop::NFTDrop;

// This Drop enum stores drop details such as funder, amount to drop or token id, etc.
#[derive(Clone, Debug, BorshDeserialize, BorshSerialize)]
#[near(serializers = [json])]
#[borsh(crate = "near_sdk::borsh")]
pub enum Drop {
    NEAR(NearDrop),
    FT(FTDrop),
    NFT(NFTDrop),
}

pub trait Dropper {
    fn promise_for_claiming(&self, account_id: AccountId) -> Promise;
    fn promise_to_resolve_claim(&self, created: bool) -> Promise;
}

pub trait Getters {
    fn get_amount_per_drop(&self) -> Result<NearToken, &str>;
    fn get_counter(&self) -> Result<u64, &str>;
}

pub trait Setters {
    fn set_counter(&mut self, value: u64) -> Result<(), &str>;
}

impl Dropper for Drop {
    fn promise_for_claiming(&self, account_id: AccountId) -> Promise {
        match self {
            Drop::NEAR(near_drop) => near_drop.promise_for_claiming(account_id),
            Drop::FT(ft_drop) => ft_drop.promise_for_claiming(account_id),
            Drop::NFT(nft_drop) => nft_drop.promise_for_claiming(account_id),
        }
    }

    fn promise_to_resolve_claim(&self, created: bool) -> Promise {
        match self {
            Drop::NEAR(near_drop) => near_drop.promise_to_resolve_claim(created),
            Drop::FT(ft_drop) => ft_drop.promise_to_resolve_claim(created),
            Drop::NFT(nft_drop) => nft_drop.promise_to_resolve_claim(created),
        }
    }
}

impl Getters for Drop {
    fn get_amount_per_drop(&self) -> Result<NearToken, &str> {
        match self {
            Drop::NEAR(near_drop) => near_drop.get_amount_per_drop(),
            Drop::FT(ft_drop) => ft_drop.get_amount_per_drop(),
            _ => Err("There is no amount_per_drop field for NFT drop structure"),
        }
    }

    fn get_counter(&self) -> Result<u64, &str> {
        match self {
            Drop::NEAR(near_drop) => near_drop.get_counter(),
            Drop::FT(ft_drop) => ft_drop.get_counter(),
            _ => Err("There is no amount_per_drop field for NFT drop structure"),
        }
    }
}

impl Setters for Drop {
    fn set_counter(&mut self, value: u64) -> Result<(), &str> {
        match self {
            Drop::NEAR(near_drop) => near_drop.set_counter(value),
            Drop::FT(ft_drop) => ft_drop.set_counter(value),
            _ => Err("There is no counter field for NFT drop structure"),
        }
    }
}
