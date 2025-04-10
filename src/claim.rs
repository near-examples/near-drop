use crate::constants::*;

use crate::drop_types::{Dropper, Getters, Setters};
use crate::{Contract, ContractExt};

use near_sdk::serde_json::json;
use near_sdk::{env, near, AccountId, Promise, PromiseError};

#[near]
impl Contract {
    #[private]
    pub fn claim_for(&mut self, account_id: AccountId) -> Promise {
        self.internal_claim(account_id, false)
    }

    #[private]
    pub fn create_account_and_claim(&mut self, account_id: AccountId) -> Promise {
        let public_key = env::signer_account_pk();

        if let None = self.drop_id_by_key.get(&public_key) {
            panic!("No drop for public key")
        }

        let create_args = json!({ "new_account_id": account_id, "new_public_key": public_key })
            .to_string()
            .into_bytes()
            .to_vec();

        Promise::new(self.top_level_account.clone())
            .function_call(
                "create_account".to_string(),
                create_args,
                CREATE_ACCOUNT_FEE,
                GAS_FOR_CREATE_ACCOUNT,
            )
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(CREATE_CALLBACK_GAS)
                    .resolve_account_create(account_id),
            )
    }

    #[private]
    pub fn resolve_account_create(
        &mut self,
        account_id: AccountId,
        #[callback_result] created: Result<bool, PromiseError>,
    ) -> Promise {
        // The first step of creating an account has finished
        if let Err(_) = created {
            panic!("Creating account failed")
        }

        // Creating the account was successful, we can continue with the claim
        self.internal_claim(account_id, true)
    }

    fn internal_claim(&mut self, account_id: AccountId, account_created: bool) -> Promise {
        let public_key = env::signer_account_pk();

        // get the id for the public_key
        let drop_id = self
            .drop_id_by_key
            .remove(&public_key)
            .expect("No drop for public key");

        let drop = self
            .drop_by_id
            .remove(&drop_id)
            .expect("No drop information for such drop_id");
        let counter = drop.get_counter().unwrap_or(1);
        let updated_counter = counter - 1;
        let mut drop_deleted = true;

        if updated_counter > 0 {
            let mut updated_drop = drop.clone();
            let _ = updated_drop.set_counter(updated_counter);

            self.drop_by_id.insert(drop_id.clone(), updated_drop);
            drop_deleted = false;
        }

        drop.promise_for_claiming(account_id)
            .then(drop.promise_to_resolve_claim(account_created, drop_deleted))
    }
}
