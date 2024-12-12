use near_sdk::{env, near, AccountId, NearToken, Promise, PromiseError};

use crate::constants::*;
use crate::drop_types::{Dropper, Getters};
use crate::storage::basic_storage;
use crate::{Contract, ContractExt, Drop};

#[derive(PartialEq, Clone, Debug)]
#[near(serializers = [borsh])]
pub struct NearDrop {
    funder: AccountId,
    amount: NearToken,
    counter: u64,
}

impl Dropper for NearDrop {
    fn promise_for_claiming(&self, account_id: AccountId) -> Promise {
        Promise::new(account_id).transfer(self.amount)
    }

    fn promise_to_resolve_claim(&self, created: bool) -> Promise {
        Contract::ext(env::current_account_id())
            .with_static_gas(CLAIM_CALLBACK_GAS)
            .with_unused_gas_weight(0)
            .resolve_near_claim(created, self.funder.clone(), self.amount)
    }
}

impl Getters for NearDrop {
    fn get_counter(&self) -> Result<u64, &str> {
        Ok(self.counter)
    }

    fn get_amount_per_drop(&self) -> Result<NearToken, &str> {
        Ok(self.amount)
    }
}

pub fn required_deposit(drop_amount: NearToken) -> NearToken {
    basic_storage()
        .saturating_add(drop_amount)
        .saturating_add(CREATE_ACCOUNT_FEE)
        .saturating_add(ACCESS_KEY_ALLOWANCE)
        .saturating_add(ACCESS_KEY_STORAGE)
}

pub fn create(funder: AccountId, amount: NearToken, counter: u64) -> Drop {
    Drop::NEAR(NearDrop {
        funder,
        amount,
        counter,
    })
}

#[near]
impl Contract {
    pub fn resolve_near_claim(
        created: bool,
        funder: AccountId,
        amount: NearToken,
        #[callback_result] result: Result<(), PromiseError>,
    ) -> bool {
        let mut to_refund = basic_storage().saturating_add(ACCESS_KEY_STORAGE);

        if !created {
            to_refund = to_refund.saturating_add(CREATE_ACCOUNT_FEE);
        }

        if result.is_err() {
            to_refund = to_refund.saturating_add(amount);
        }

        // Return the money
        Promise::new(funder).transfer(to_refund);
        true
    }
}
