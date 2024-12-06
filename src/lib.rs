use constants::ACCESS_KEY_ALLOWANCE;
use drop_types::DropType;
use near_sdk::json_types::{U128, U64};
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
    iDrops,
}

#[derive(Clone, Debug)]
#[near]
pub struct DropData {
    counter: u64,
    drop: DropType,
}

#[derive(PanicOnDefault)]
#[near(contract_state)]
pub struct Contract {
    pub top_level_account: AccountId,
    pub drop_for_key: LookupMap<PublicKey, u32>,
    pub idrops: LookupMap<u32, DropData>,
}

#[near]
impl Contract {
    #[init]
    #[private]
    pub fn new(top_level_account: AccountId) -> Self {
        Self {
            top_level_account,
            drop_for_key: LookupMap::new(StorageKey::DropForPublicKey),
            idrops: LookupMap::new(StorageKey::iDrops),
        }
    }

    #[payable]
    pub fn create_near_drop(
        &mut self,
        public_key: Vec<PublicKey>,
        amount: U128,
    ) -> Promise {
        let funder = env::predecessor_account_id();

        // check that there is enough NEAR attached for the drop
        // storage + amount * counter

        let drop = near_drop::create(funder, NearToken::from_yoctonear(amount.0), counter.0);

        // add it to self.idrops which gives you an ID

        // for each public_key:
        //      drop_for_key[key] = ID

        self.save_drop_and_key(public_key, drop, counter.0)
    }

    #[payable]
    pub fn create_ft_drop(
        &mut self,
        public_key: PublicKey,
        ft_contract: AccountId,
        amount_per_drop: U128,
        counter: U64,
    ) -> Promise {
        let funder = env::predecessor_account_id();
        let drop = ft_drop::create(funder, ft_contract);
        self.save_drop_and_key(public_key, drop, counter.0)
    }

    #[payable]
    pub fn create_nft_drop(
        &mut self,
        public_key: PublicKey,
        nft_contract: AccountId,
        counter: U64,
    ) -> Promise {
        let funder = env::predecessor_account_id();
        let drop = nft_drop::create(funder, nft_contract);
        self.save_drop_and_key(public_key, drop, counter.0)
    }

    fn save_drop_and_key(
        &mut self,
        public_key: PublicKey,
        drop: DropType,
        counter: u64,
    ) -> Promise {
        self.drop_for_key
            .insert(public_key.clone(), DropData { counter, drop });

        // Add key so it can be used to call `claim_for` and `create_account_and_claim`
        Promise::new(env::current_account_id()).add_access_key_allowance(
            public_key,
            Allowance::limited(ACCESS_KEY_ALLOWANCE).unwrap(),
            env::current_account_id(),
            "claim_for,create_account_and_claim".to_string(),
        )
    }
}
