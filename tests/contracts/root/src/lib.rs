// Find all our documentation at https://docs.near.org
use near_sdk::{near, AccountId, Promise, PublicKey};

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
            .then(
                Self::ext_self(env::current_account_id())
                    .create_account_callback()
            )
    }

    pub fn create_account_callback(&self, #[callback_result] created: Result<(), PromiseError>) -> bool {
        match created {
            Ok(_) => {
                true
            }
            Err(e) => {
                false
            }
        }
        
    }
}
