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
        self.assert_keys_available(&public_keys);

        let num_of_keys = public_keys.len().try_into().unwrap();

        let storage_before = env::storage_usage();
        let drop = near_drop::create(env::predecessor_account_id(), amount_per_drop, num_of_keys);
        let drop_id = self.save_drop(drop);
        self.save_drop_id_by_keys(&public_keys, drop_id);
        self.charge_storage_and_refund(
            storage_before,
            near_drop::required_deposit_per_key(amount_per_drop),
            num_of_keys,
        );

        drop_id
    }

    #[payable]
    pub fn create_ft_drop(
        &mut self,
        public_keys: Vec<PublicKey>,
        ft_contract: AccountId,
        amount_per_drop: NearToken,
    ) -> DropId {
        self.assert_keys_available(&public_keys);

        let num_of_keys = public_keys.len().try_into().unwrap();
        let storage_before = env::storage_usage();
        let drop = ft_drop::create(env::predecessor_account_id(), ft_contract, amount_per_drop, num_of_keys);
        let drop_id = self.save_drop(drop);
        self.save_drop_id_by_keys(&public_keys, drop_id);
        self.charge_storage_and_refund(
            storage_before,
            ft_drop::required_deposit_per_key(),
            num_of_keys,
        );

        drop_id
    }

    #[payable]
    pub fn create_nft_drop(&mut self, public_key: PublicKey, nft_contract: AccountId) -> DropId {
        assert!(
            self.drop_id_by_key.get(&public_key).is_none(),
            "Public key is already used for a drop"
        );

        let storage_before = env::storage_usage();
        let drop = nft_drop::create(env::predecessor_account_id(), nft_contract);
        let drop_id = self.save_drop(drop);
        self.save_drop_id_by_key(public_key, drop_id).detach();
        self.charge_storage_and_refund(storage_before, nft_drop::required_deposit_per_key(), 1);

        drop_id
    }

    // Deposit create_near_drop will charge `funder` (same formula, see drop_cost).
    pub fn get_near_drop_cost(
        &self,
        funder: AccountId,
        public_keys: Vec<PublicKey>,
        amount_per_drop: NearToken,
    ) -> NearToken {
        let drop = near_drop::create(funder, amount_per_drop, public_keys.len() as u32);
        drop_cost(&drop, &public_keys, near_drop::required_deposit_per_key(amount_per_drop))
    }

    pub fn get_ft_drop_cost(
        &self,
        funder: AccountId,
        public_keys: Vec<PublicKey>,
        ft_contract: AccountId,
        amount_per_drop: NearToken,
    ) -> NearToken {
        let drop = ft_drop::create(funder, ft_contract, amount_per_drop, public_keys.len() as u32);
        drop_cost(&drop, &public_keys, ft_drop::required_deposit_per_key())
    }

    pub fn get_nft_drop_cost(
        &self,
        funder: AccountId,
        public_key: PublicKey,
        nft_contract: AccountId,
    ) -> NearToken {
        let drop = nft_drop::create(funder, nft_contract);
        drop_cost(&drop, &[public_key], nft_drop::required_deposit_per_key())
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

    // Rejects empty key sets, keys already tied to a drop, and duplicates within
    // this call (duplicates would inflate the counter past the usable key count,
    // stranding funds in an unclaimable drop).
    // ponytail: O(n^2) dup scan; keys per drop are few. Use a HashSet if that changes.
    fn assert_keys_available(&self, public_keys: &Vec<PublicKey>) {
        assert!(!public_keys.is_empty(), "Must provide at least one public key");

        for (i, public_key) in public_keys.iter().enumerate() {
            assert!(
                self.drop_id_by_key.get(public_key).is_none(),
                "Public key is already used for a drop"
            );
            assert!(
                !public_keys[i + 1..].contains(public_key),
                "Duplicate public key in the drop"
            );
        }
    }

    // Charges the caller for the storage the drop just consumed (measured on-chain,
    // so it can't be under/over-estimated) plus the per-key allowance and fees, then
    // refunds any excess. Runs after the inserts so a shortfall panics and reverts them.
    fn charge_storage_and_refund(
        &mut self,
        storage_before: u64,
        deposit_per_key: NearToken,
        num_of_keys: u32,
    ) {
        self.flush_maps();
        let used = env::storage_usage().saturating_sub(storage_before);
        let required = env::storage_byte_cost()
            .saturating_mul(used as u128)
            .saturating_add(deposit_per_key.saturating_mul(num_of_keys as u128));

        let attached = env::attached_deposit();
        assert!(attached >= required, "Please attach at least {required}");

        let extra = attached.saturating_sub(required);
        if extra > NearToken::from_yoctonear(0) {
            Promise::new(env::predecessor_account_id())
                .transfer(extra)
                .detach();
        }
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
            self.save_drop_id_by_key(public_key.clone(), drop_id.clone()).detach();
        }
    }

    // store::LookupMap only writes on flush/drop; storage_usage() can't see
    // pending inserts/removes until this runs.
    pub(crate) fn flush_maps(&mut self) {
        self.drop_by_id.flush();
        self.drop_id_by_key.flush();
    }

    fn save_drop(&mut self, drop: Drop) -> DropId {
        let drop_id = self.next_drop_id;
        self.drop_by_id.insert(drop_id, drop);
        self.next_drop_id += 1;
        drop_id
    }
}

fn len<T: near_sdk::borsh::BorshSerialize>(v: &T) -> usize {
    near_sdk::borsh::to_vec(v).unwrap().len()
}

// Protocol overhead per storage record (num_extra_bytes_record).
const STORAGE_RECORD_OVERHEAD: u64 = 40;

// Storage the drop's map entries will take (1 prefix byte + borsh key, borsh value
// per record) plus the per-key fees. Must match what charge_storage_and_refund
// measures; tests/tests/cost.rs pins that.
fn drop_cost(drop: &Drop, public_keys: &[PublicKey], deposit_per_key: NearToken) -> NearToken {
    let record = |key: usize, value: usize| STORAGE_RECORD_OVERHEAD + (1 + key + value) as u64;

    let bytes = record(len(&(0 as DropId)), len(drop))
        + public_keys
            .iter()
            .map(|pk| record(len(pk), len(&(0 as DropId))))
            .sum::<u64>();

    env::storage_byte_cost()
        .saturating_mul(bytes as u128)
        .saturating_add(deposit_per_key.saturating_mul(public_keys.len() as u128))
}
