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

    fn promise_to_resolve_claim(&self, account_created: bool, drop_deleted: bool) -> Promise {
        Contract::ext(env::current_account_id())
            .with_static_gas(CLAIM_CALLBACK_GAS)
            .with_unused_gas_weight(0)
            .resolve_near_claim(account_created, drop_deleted, self.funder.clone(), self.amount)
    }
}

impl Getters for NearDrop {
    fn get_counter(&self) -> Result<u32, &str> {
        Ok(self.counter)
    }

    fn get_amount_per_drop(&self) -> Result<NearToken, &str> {
        Ok(self.amount)
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

pub fn required_storage_drop(num_access_keys: u32) -> NearToken {
    NearToken::from_yoctonear(
        // DropId -> Drop::Near
        ID_STORAGE + ENUM_STORAGE + ACC_STORAGE + TOKEN_AMOUNT_STORAGE + 8 
        // PublicKey -> DropId
        + num_access_keys as u128 * (PK_STORAGE + ID_STORAGE)
    )
}

pub fn create(amount_per_drop: NearToken, num_of_keys: u32) -> Drop {
    let funder = env::predecessor_account_id();

    let attached_deposit = env::attached_deposit();
    let required_deposit = // required_storage_drop + (required_deposit_per_key * num_of_keys)
        required_storage_drop(num_of_keys)
        .saturating_add(
            required_deposit_per_key(amount_per_drop)
                .saturating_mul(num_of_keys as u128),
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
        amount_per_drop.ge(&NearToken::from_yoctonear(1)),
        "Amount per drop should be at least 1 yN"
    );

    Drop::NEAR(NearDrop {
        funder,
        amount: amount_per_drop,
        counter: num_of_keys,
    })
}

#[near]
impl Contract {
    pub fn resolve_near_claim(
      account_created: bool,
        drop_deleted: bool,
        funder: AccountId,
        amount: NearToken,
        #[callback_result] result: Result<(), PromiseError>,
    ) -> bool {
        let mut to_refund = ACCESS_KEY_STORAGE;

        if !account_created {
            to_refund = to_refund.saturating_add(CREATE_ACCOUNT_FEE);
        }

        if drop_deleted {
            to_refund = to_refund.saturating_add(required_storage_drop(0));
        }

        if result.is_err() {
            to_refund = to_refund.saturating_add(amount);
        }

        // Return the money
        Promise::new(funder).transfer(to_refund);
        true
    }
}
