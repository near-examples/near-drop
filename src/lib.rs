use constants::ACCESS_KEY_ALLOWANCE;
use drop_types::DropType;
use near_sdk::json_types::U128;
use near_sdk::store::LookupMap;
use near_sdk::{
    env, near, AccountId, Allowance, BorshStorageKey, NearToken, PanicOnDefault, Promise, PublicKey,
};

mod claim;
mod constants;
mod drop_types;
mod ft_drop;
mod near_drop;
mod nft_drop;
mod storage;

#[derive(BorshStorageKey)]
#[near]
enum StorageKey {
    DropForPublicKey,
}

#[derive(PanicOnDefault)]
#[near(contract_state)]
pub struct Contract {
    pub top_level_account: AccountId,
    pub drop_for_key: LookupMap<PublicKey, DropType>,
}

#[near]
impl Contract {
    #[init]
    #[private]
    pub fn new(top_level_account: AccountId) -> Self {
        Self {
            top_level_account,
            drop_for_key: LookupMap::new(StorageKey::DropForPublicKey),
        }
    }

    #[payable]
    pub fn create_near_drop(&mut self, public_key: PublicKey, amount: U128) -> Promise {
        let funder = env::predecessor_account_id();
        let drop = near_drop::create(funder, NearToken::from_yoctonear(amount.0));
        self.save_drop_and_key(public_key, drop)
    }

    #[payable]
    pub fn create_ft_drop(&mut self, public_key: PublicKey, ft_contract: AccountId) -> Promise {
        let funder = env::predecessor_account_id();
        let drop = ft_drop::create(funder, ft_contract);
        self.save_drop_and_key(public_key, drop)
    }

    #[payable]
    pub fn create_nft_drop(&mut self, public_key: PublicKey, nft_contract: AccountId) -> Promise {
        let funder = env::predecessor_account_id();
        let drop = nft_drop::create(funder, nft_contract);
        self.save_drop_and_key(public_key, drop)
    }

    fn save_drop_and_key(&mut self, public_key: PublicKey, drop: DropType) -> Promise {
        self.drop_for_key.insert(public_key.clone(), drop);

        // Add key so it can be used to call `claim_for` and `create_account_and_claim`
        Promise::new(env::current_account_id()).add_access_key_allowance(
            public_key,
            Allowance::limited(ACCESS_KEY_ALLOWANCE).unwrap(),
            env::current_account_id(),
            "claim_for,create_account_and_claim".to_string(),
        )
    }
}
