use near_sdk::borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::{env, near, AccountId, NearToken, Promise, PublicKey};

use crate::constants::MAX_TOKEN_ID_LEN;

use crate::ft_drop::{self, FTDrop};
use crate::near_drop::{self, NearDrop};
use crate::nft_drop::{self, NFTDrop};

/// Inputs for creating a drop; the contract assigns ownership and funding state.
#[near(serializers = [json])]
pub enum CreateDrop {
    NEAR {
        public_keys: Vec<PublicKey>,
        amount_per_drop: NearToken,
    },
    FT {
        public_keys: Vec<PublicKey>,
        ft_contract: AccountId,
        amount_per_drop: NearToken,
    },
    NFT {
        public_key: PublicKey,
        nft_contract: AccountId,
    },
}

impl CreateDrop {
    pub(crate) fn build(self, funder: AccountId) -> (Drop, Vec<PublicKey>, NearToken) {
        match self {
            Self::NEAR {
                public_keys,
                amount_per_drop,
            } => (
                near_drop::create(
                    funder,
                    amount_per_drop,
                    public_keys.len().try_into().unwrap(),
                ),
                public_keys,
                near_drop::required_deposit_per_key(amount_per_drop),
            ),
            Self::FT {
                public_keys,
                ft_contract,
                amount_per_drop,
            } => (
                ft_drop::create(
                    funder,
                    ft_contract,
                    amount_per_drop,
                    public_keys.len().try_into().unwrap(),
                ),
                public_keys,
                ft_drop::required_deposit_per_key(),
            ),
            Self::NFT {
                public_key,
                nft_contract,
            } => (
                nft_drop::create(funder, nft_contract),
                vec![public_key],
                nft_drop::required_deposit_per_key(),
            ),
        }
    }
}

// This Drop enum stores drop details such as funder, amount to drop or token id, etc.
#[derive(Clone, Debug, BorshDeserialize, BorshSerialize)]
#[near(serializers = [json])]
#[borsh(crate = "near_sdk::borsh")]
pub enum Drop {
    NEAR(NearDrop),
    FT(FTDrop),
    NFT(NFTDrop),
}

impl Drop {
    pub(crate) fn funder(&self) -> &AccountId {
        match self {
            Self::NEAR(drop) => &drop.funder,
            Self::FT(drop) => &drop.funder,
            Self::NFT(drop) => &drop.funder,
        }
    }

    pub(crate) fn is_ready(&self) -> bool {
        match self {
            Self::NEAR(_) => true,
            Self::FT(drop) => drop.funded,
            Self::NFT(drop) => !drop.token_id.is_empty(),
        }
    }

    // Used padding is included in measured freed storage; return the unused part too.
    pub(crate) fn unused_storage_padding(&self) -> NearToken {
        match self {
            Self::NFT(drop) => env::storage_byte_cost()
                .saturating_mul(MAX_TOKEN_ID_LEN.saturating_sub(drop.token_id.len()) as u128),
            _ => NearToken::from_yoctonear(0),
        }
    }
}

pub trait Dropper {
    fn promise_for_claiming(&self, account_id: AccountId, storage_refund: NearToken) -> Promise;
}

pub trait Setters {
    fn set_counter(&mut self, value: u32) -> Result<(), &str>;
}

impl Dropper for Drop {
    fn promise_for_claiming(&self, account_id: AccountId, storage_refund: NearToken) -> Promise {
        match self {
            Drop::NEAR(drop) => drop.promise_for_claiming(account_id, storage_refund),
            Drop::FT(drop) => drop.promise_for_claiming(account_id, storage_refund),
            Drop::NFT(drop) => drop.promise_for_claiming(account_id, storage_refund),
        }
    }
}

impl Setters for Drop {
    fn set_counter(&mut self, value: u32) -> Result<(), &str> {
        match self {
            Drop::NEAR(near_drop) => near_drop.set_counter(value),
            Drop::FT(ft_drop) => ft_drop.set_counter(value),
            _ => Err("There is no counter field for NFT drop structure"),
        }
    }
}
