use crate::drop_types::{Dropper, Setters};
use crate::{Contract, ContractExt, Drop};

use near_sdk::{env, near, AccountId, NearToken, Promise, PromiseOrValue, PublicKey};

#[near]
impl Contract {
    #[private]
    pub fn claim_for(&mut self, account_id: AccountId) -> PromiseOrValue<bool> {
        let (drop, storage_refund) = self.consume_key(env::signer_account_pk());
        if !drop.is_ready() {
            self.refund_keys(drop, 1, storage_refund, false).detach();
            return PromiseOrValue::Value(false);
        }
        PromiseOrValue::Promise(drop.promise_for_claiming(account_id, storage_refund))
    }
}

impl Contract {
    // Consume the key before any asynchronous work, even if that work fails.
    fn consume_key(&mut self, public_key: PublicKey) -> (Drop, NearToken) {
        let drop_id = *self
            .drop_id_by_key
            .get(&public_key)
            .expect("No drop for public key");
        let drop = self.drop_by_id.get(&drop_id).expect("Missing drop").clone();
        let mut keys = self
            .keys_by_drop
            .get(&drop_id)
            .expect("Missing drop keys")
            .clone();
        let position = keys
            .iter()
            .position(|key| key == &public_key)
            .expect("Missing drop key");
        let storage_before = env::storage_usage();
        keys.swap_remove(position);
        self.drop_id_by_key.remove(&public_key);
        Promise::new(env::current_account_id())
            .delete_key(public_key)
            .detach();
        if keys.is_empty() {
            self.drop_by_id.remove(&drop_id);
            self.keys_by_drop.remove(&drop_id);
        } else {
            let mut updated = drop.clone();
            updated
                .set_counter(keys.len() as u32)
                .expect("Drop has no counter");
            self.drop_by_id.insert(drop_id, updated);
            self.keys_by_drop.insert(drop_id, keys);
        }
        self.flush_maps();
        let freed = storage_before.saturating_sub(env::storage_usage());
        let refund = env::storage_byte_cost()
            .saturating_mul(freed as u128)
            .saturating_add(drop.unused_storage_padding());
        (drop, refund)
    }
}
