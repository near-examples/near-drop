use constants::{DropId, ACCESS_KEY_ALLOWANCE};
use drop_types::Drop;
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
    DropIdByKey,
    DropById,
}

#[derive(PanicOnDefault)]
#[near(contract_state)]
pub struct Contract {
    pub top_level_account: AccountId,
    pub drop_id_by_key: LookupMap<PublicKey, DropId>,
    pub drop_by_id: LookupMap<DropId, Drop>,
}

#[near]
impl Contract {
    #[init]
    #[private]
    pub fn new(top_level_account: AccountId) -> Self {
        Self {
            top_level_account,
            drop_id_by_key: LookupMap::new(StorageKey::DropIdByKey),
            drop_by_id: LookupMap::new(StorageKey::DropById),
        }
    }

    #[payable]
    pub fn create_near_drop(
        &mut self,
        drop_id: DropId,
        public_keys: Vec<PublicKey>,
        amount_per_drop: U128,
    ) {
        let funder = env::predecessor_account_id();

        // check that there is enough NEAR attached for the drop
        // storage + amount * counter

        let drop = near_drop::create(
            funder,
            NearToken::from_yoctonear(amount_per_drop.0),
            public_keys.len().try_into().unwrap(),
        );

        self.save_drop_by_id(drop_id.clone(), drop);
        self.save_drop_id_by_keys(public_keys, drop_id);
    }

    #[payable]
    pub fn create_ft_drop(
        &mut self,
        drop_id: DropId,
        public_keys: Vec<PublicKey>,
        ft_contract: AccountId,
        amount_per_drop: U128,
    ) {
        let funder = env::predecessor_account_id();
        let drop = ft_drop::create(
            funder,
            ft_contract,
            NearToken::from_yoctonear(amount_per_drop.0),
            public_keys.len().try_into().unwrap(),
        );
        self.save_drop_by_id(drop_id.clone(), drop);
        self.save_drop_id_by_keys(public_keys, drop_id);
    }

    #[payable]
    pub fn create_nft_drop(
        &mut self,
        drop_id: DropId,
        public_key: PublicKey,
        nft_contract: AccountId,
    ) {
        let funder = env::predecessor_account_id();
        let drop = nft_drop::create(funder, nft_contract);
        self.save_drop_by_id(drop_id.clone(), drop);
        self.save_drop_id_by_key(public_key, drop_id);
    }

    fn save_drop_id_by_key(&mut self, public_key: PublicKey, drop_id: DropId) -> Promise {
        self.drop_id_by_key.insert(public_key.clone(), drop_id);

        // Add key so it can be used to call `claim_for` and `create_account_and_claim`
        Promise::new(env::current_account_id()).add_access_key_allowance(
            public_key,
            Allowance::limited(ACCESS_KEY_ALLOWANCE).unwrap(),
            env::current_account_id(),
            "claim_for,create_account_and_claim".to_string(),
        )
    }

    fn save_drop_id_by_keys(&mut self, public_keys: Vec<PublicKey>, drop_id: DropId) {
        for public_key in public_keys.iter() {
            self.save_drop_id_by_key(public_key.clone(), drop_id.clone());
        }
    }

    fn save_drop_by_id(&mut self, drop_id: DropId, drop: Drop) {
        self.drop_by_id.insert(drop_id, drop);
    }
}
