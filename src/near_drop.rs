use near_sdk::borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::{env, near, AccountId, NearToken, Promise, PromiseError};

use crate::constants::*;
use crate::drop_types::{Dropper, Getters, Setters};
use crate::{Contract, ContractExt, Drop};

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize)]
#[near(serializers = [json])]
#[borsh(crate = "near_sdk::borsh")]
pub struct NearDrop {
    funder: AccountId, // An account which created the drop and funded it
    amount: NearToken, // Reflects how much NEAR tokens will be transfer to claiming user
    counter: u32,      // Reflects how much times the drop can be claimed
}

impl Dropper for NearDrop {
    fn promise_for_claiming(&self, account_id: AccountId) -> Promise {
        Promise::new(account_id).transfer(self.amount)
    }

    fn promise_to_resolve_claim(&self, account_created: bool, storage_refund: NearToken) -> Promise {
        Contract::ext(env::current_account_id())
            .with_static_gas(CLAIM_CALLBACK_GAS)
            .with_unused_gas_weight(0)
            .resolve_near_claim(account_created, storage_refund, self.funder.clone(), self.amount)
    }
}

impl Getters for NearDrop {
    fn get_counter(&self) -> Result<u32, &str> {
        Ok(self.counter)
    }
}

impl Setters for NearDrop {
    fn set_counter(&mut self, value: u32) -> Result<(), &str> {
        self.counter = value;
        Ok(())
    }
}

pub fn required_deposit_per_key(drop_amount: NearToken) -> NearToken {
    drop_amount
        .saturating_add(CREATE_ACCOUNT_FEE)
        .saturating_add(ACCESS_KEY_ALLOWANCE)
        .saturating_add(ACCESS_KEY_STORAGE)
}

// Storage is measured on-chain by the caller (see Contract::charge_storage_and_refund),
// so this only validates the business rule and builds the drop.
pub fn create(amount_per_drop: NearToken, num_of_keys: u32) -> Drop {
    assert!(
        amount_per_drop.ge(&NearToken::from_yoctonear(1)),
        "Amount per drop should be at least 1 yN"
    );

    Drop::NEAR(NearDrop {
        funder: env::predecessor_account_id(),
        amount: amount_per_drop,
        counter: num_of_keys,
    })
}

#[near]
impl Contract {
    #[private]
    pub fn resolve_near_claim(
      account_created: bool,
        storage_refund: NearToken,
        funder: AccountId,
        amount: NearToken,
        #[callback_result] result: Result<(), PromiseError>,
    ) -> bool {
        let mut to_refund = ACCESS_KEY_STORAGE.saturating_add(storage_refund);

        if !account_created {
            to_refund = to_refund.saturating_add(CREATE_ACCOUNT_FEE);
        }

        if result.is_err() {
            to_refund = to_refund.saturating_add(amount);
        }

        // Return the money
        Promise::new(funder).transfer(to_refund).detach();
        true
    }
}
