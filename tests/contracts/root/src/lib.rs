// Find all our documentation at https://docs.near.org
use near_sdk::{env, log, near, AccountId, Promise, PromiseError, PromiseResult, PublicKey};

// Define the contract structure
#[near(contract_state)]
pub struct Contract {}

// Define the default, which automatically initializes the contract
impl Default for Contract {
    fn default() -> Self {
        Self {}
    }
}

fn is_promise_success() -> bool {
    assert_eq!(
        env::promise_results_count(),
        1,
        "Contract expected a result on the callback"
    );
    match env::promise_result(0) {
        PromiseResult::Successful(_) => true,
        _ => false,
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
    pub fn create_account_callback(&self) -> bool {
        assert_eq!(
            env::predecessor_account_id(),
            env::current_account_id(),
            "Callback can only be called from the contract"
        );
        let creation_succeeded = is_promise_success();

        creation_succeeded
    }
}
