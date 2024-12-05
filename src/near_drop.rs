use near_sdk::{env, near, AccountId, NearToken, Promise, PromiseError};

use crate::constants::*;
use crate::drop_types::Dropper;
use crate::storage::basic_storage;
use crate::{Contract, ContractExt, DropType};

#[derive(PartialEq, Clone, Debug)]
#[near(serializers = [borsh])]
pub struct NearDrop {
    funder: AccountId,
    amount: NearToken,
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

pub fn create(funder: AccountId, amount: NearToken, quantity: u64) -> DropType {
    assert!(
        amount.ge(&NearToken::from_yoctonear(1)),
        "Give at least 1 yN"
    );

    let attached = env::attached_deposit();
    let required = basic_storage()
        .saturating_add(amount.saturating_mul(quantity.into()))
        .saturating_add(CREATE_ACCOUNT_FEE)
        .saturating_add(ACCESS_KEY_ALLOWANCE)
        .saturating_add(ACCESS_KEY_STORAGE);

    assert!(attached >= required, "Please attach at least {required}");

    let extra = attached.saturating_sub(required);
    if extra.gt(&NearToken::from_yoctonear(0)) {
        // refund the user, we don't need that money
        Promise::new(env::predecessor_account_id()).transfer(extra);
    }

    DropType::NEAR(NearDrop { funder, amount })
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
