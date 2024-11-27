// Find all our documentation at https://docs.near.org
use near_sdk::{env, log, near, AccountId, Promise, PromiseError, PublicKey};

// Define the contract structure
#[near(contract_state)]
pub struct Contract {}

// Define the default, which automatically initializes the contract
impl Default for Contract {
    fn default() -> Self {
        Self {}
    }
}

// Implement the contract structure
#[near]
impl Contract {
    // Public method - returns the greeting saved, defaulting to DEFAULT_GREETING
    pub fn create_account(&self, new_account_id: AccountId, new_public_key: PublicKey) {
        Promise::new(new_account_id)
            .create_account()
            .add_full_access_key(new_public_key)
            .then(Self::ext(env::current_account_id()).create_account_callback());
    }

    #[private]
    pub fn create_account_callback(&self, #[callback_result] created: Result<(), PromiseError>) {
        println!("callback_result: {:?}", created);
        if let Ok(_) = created {
            log!("create_account call successed");
        } else {
            log!("create_account call failed");
        }
    }
}
