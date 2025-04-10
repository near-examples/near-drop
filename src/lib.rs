use constants::{DropId, ACCESS_KEY_ALLOWANCE};
use drop_types::Drop;
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
    pub next_drop_id: DropId,
    pub drop_by_id: LookupMap<DropId, Drop>,
    pub drop_id_by_key: LookupMap<PublicKey, DropId>,
}

#[near]
impl Contract {
    #[init]
    #[private]
    pub fn new(top_level_account: AccountId) -> Self {
        Self {
            top_level_account,
            next_drop_id: 0,
            drop_id_by_key: LookupMap::new(StorageKey::DropIdByKey),
            drop_by_id: LookupMap::new(StorageKey::DropById),
        }
    }

    #[payable]
    pub fn create_near_drop(
        &mut self,
        public_keys: Vec<PublicKey>,
        amount_per_drop: NearToken,
    ) -> DropId {
        // check that the access keys are not already used
        // TODO: add test for that case
        for public_key in public_keys.iter() {
            assert!(
                self.drop_id_by_key.get(public_key).is_none(),
                "Public key is already used for a drop"
            );
        }

        let num_of_keys = public_keys.len().try_into().unwrap();

        let drop = near_drop::create(amount_per_drop, num_of_keys);
        let drop_id = self.save_drop(drop);
        self.save_drop_id_by_keys(&public_keys, drop_id);

        drop_id
    }

    #[payable]
    pub fn create_ft_drop(
        &mut self,
        public_keys: Vec<PublicKey>,
        ft_contract: AccountId,
        amount_per_drop: NearToken,
    ) -> DropId {
        // check that the access keys are not already used
        for public_key in public_keys.iter() {
            assert!(
                self.drop_id_by_key.get(public_key).is_none(),
                "Public key is already used for a drop"
            );
        }

        let num_of_keys = public_keys.len().try_into().unwrap();
        let drop = ft_drop::create(ft_contract, amount_per_drop, num_of_keys);
        let drop_id = self.save_drop(drop);
        self.save_drop_id_by_keys(&public_keys, drop_id);

        drop_id
    }

    #[payable]
    pub fn create_nft_drop(&mut self, public_key: PublicKey, nft_contract: AccountId) -> DropId {
        assert!(
            self.drop_id_by_key.get(&public_key).is_none(),
            "Public key is already used for a drop"
        );

        let drop = nft_drop::create(nft_contract);
        let drop_id = self.save_drop(drop);
        self.save_drop_id_by_key(public_key, drop_id);

        drop_id
    }

    pub fn get_drop_by_id(&self, drop_id: DropId) -> Drop {
        self.drop_by_id
            .get(&drop_id)
            .expect("No drop information for such drop_id")
            .to_owned()
    }

    pub fn get_drop_id_by_key(&self, public_key: &PublicKey) -> &DropId {
        self.drop_id_by_key
            .get(public_key)
            .expect("No drop for public key")
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

    fn save_drop_id_by_keys(&mut self, public_keys: &Vec<PublicKey>, drop_id: DropId) {
        for public_key in public_keys.iter() {
            self.save_drop_id_by_key(public_key.clone(), drop_id.clone());
        }
    }

    fn save_drop(&mut self, drop: Drop) -> DropId {
        let drop_id = self.next_drop_id;
        self.drop_by_id.insert(drop_id, drop);
        self.next_drop_id += 1;
        drop_id
    }
}
