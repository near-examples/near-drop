use constants::{DropId, ACCESS_KEY_ALLOWANCE};
use drop_types::{Drop, Getters};
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
        let attached_deposit = env::attached_deposit();
        let required_deposit = near_drop::required_deposit(
            NearToken::from_yoctonear(amount_per_drop.0)
                .saturating_mul(public_keys.len().try_into().unwrap()),
        );
        assert!(
            attached_deposit >= required_deposit,
            "Please attach at least {required_deposit}"
        );

        let extra_deposit = attached_deposit.saturating_sub(required_deposit);
        if extra_deposit.gt(&NearToken::from_yoctonear(0)) {
            // refund the user, we don't need that money
            Promise::new(env::predecessor_account_id()).transfer(extra_deposit);
        }
        assert!(
            NearToken::from_yoctonear(amount_per_drop.0).ge(&NearToken::from_yoctonear(1)),
            "Give at least 1 yN" // TODO Proper message about required amount_per_drop at least 1
        );

        let funder = env::predecessor_account_id();
        let drop = near_drop::create(
            funder,
            NearToken::from_yoctonear(amount_per_drop.0),
            public_keys.clone(),
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
        let attached_deposit = env::attached_deposit();
        let required_deposit = ft_drop::required_deposit();
        assert!(
            attached_deposit >= required_deposit,
            "Please attach at least {required_deposit}"
        );

        let extra_deposit = attached_deposit.saturating_sub(required_deposit);
        if extra_deposit.gt(&NearToken::from_yoctonear(0)) {
            // refund the user, we don't need that money
            Promise::new(env::predecessor_account_id()).transfer(extra_deposit);
        }
        assert!(
            NearToken::from_yoctonear(amount_per_drop.0).ge(&NearToken::from_yoctonear(1)),
            "Give at least 1 yN" // TODO Proper message about required amount_per_drop at least 1
        );

        let funder = env::predecessor_account_id();
        let drop = ft_drop::create(
            funder,
            ft_contract,
            NearToken::from_yoctonear(amount_per_drop.0),
            public_keys.clone(),
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
        let attached_deposit = env::attached_deposit();
        let required_deposit = nft_drop::required_deposit();
        assert!(
            attached_deposit >= required_deposit,
            "Please attach at least {required_deposit}"
        );

        let extra_deposit = attached_deposit.saturating_sub(required_deposit);
        if extra_deposit.gt(&NearToken::from_yoctonear(0)) {
            // refund the user, we don't need that money
            Promise::new(env::predecessor_account_id()).transfer(extra_deposit);
        }

        let funder = env::predecessor_account_id();
        let drop = nft_drop::create(funder, nft_contract, public_key.clone());
        self.save_drop_by_id(drop_id.clone(), drop);
        self.save_drop_id_by_key(public_key, drop_id);
    }

    #[payable]
    pub fn delete_drop_by_id(&mut self, drop_id: DropId) {
        assert!(
            env::attached_deposit().ge(&NearToken::from_yoctonear(1)),
            "Attach at least 1 yN"
        );

        let drop = self
            .drop_by_id
            .remove(&drop_id)
            .expect("No drop information for drop_id");

        let public_keys = drop.get_public_keys().unwrap();

        for public_key in public_keys.iter() {
            self.drop_id_by_key
                .remove(public_key)
                .expect("No drop for this key");
        }
    }

    pub fn get_drop_by_id(&self, drop_id: DropId) -> Drop {
        self.drop_by_id
            .get(&drop_id)
            .expect("No drop information for drop_id")
            .to_owned()
    }

    pub fn get_drop_id_by_key(&self, public_key: &PublicKey) -> DropId {
        self.drop_id_by_key
            .get(public_key)
            .expect("No drop for this key")
            .into()
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
