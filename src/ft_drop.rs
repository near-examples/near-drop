use near_sdk::env::log;
use near_sdk::json_types::U128;
use near_sdk::serde_json::json;
use near_sdk::{
    env, log, near, AccountId, GasWeight, NearToken, Promise, PromiseError, PromiseOrValue,
};

use crate::constants::*;
use crate::drop_types::{Dropper, Getters};
use crate::storage::basic_storage;
use crate::Drop;
use crate::{Contract, ContractExt};

const FT_REGISTER: NearToken = NearToken::from_yoctonear(12_500_000_000_000_000_000_000);

#[derive(PartialEq, Clone, Debug)]
#[near(serializers = [borsh])]
pub struct FTDrop {
    funder: AccountId,
    amount: NearToken,
    ft_contract: AccountId,
    counter: u64,
}

impl Dropper for FTDrop {
    fn promise_for_claiming(&self, account_id: AccountId) -> Promise {
        assert!(
            self.amount.gt(&NearToken::from_yoctonear(0)),
            "No tokens to drop"
        );

        let deposit_args = json!({ "account_id": account_id })
            .to_string()
            .into_bytes()
            .to_vec();
        let transfer_args =
            json!({"receiver_id": account_id, "amount": U128(self.amount.as_yoctonear())})
                .to_string()
                .into_bytes()
                .to_vec();

        Promise::new(self.ft_contract.clone())
            .function_call_weight(
                "storage_deposit".to_string(),
                deposit_args,
                FT_REGISTER,
                MIN_GAS_FOR_FT_STORAGE_DEPOSIT,
                GasWeight(0),
            )
            .function_call_weight(
                "ft_transfer".to_string(),
                transfer_args,
                NearToken::from_yoctonear(1),
                MIN_GAS_FOR_FT_TRANSFER,
                GasWeight(0),
            )
    }

    fn promise_to_resolve_claim(&self, created: bool) -> Promise {
        Contract::ext(env::current_account_id())
            .with_static_gas(FT_CLAIM_CALLBACK_GAS)
            .with_unused_gas_weight(0)
            .resolve_ft_claim(
                created,
                self.funder.clone(),
                self.amount,
                self.ft_contract.clone(),
            )
    }
}

impl Getters for FTDrop {
    fn get_counter(&self) -> Result<u64, &str> {
        Ok(self.counter)
    }

    fn get_amount_per_drop(&self) -> Result<NearToken, &str> {
        Ok(self.amount)
    }
}

pub fn create(
    funder: AccountId,
    ft_contract: AccountId,
    amount_per_drop: NearToken,
    counter: u64,
) -> Drop {
    let attached = env::attached_deposit();
    let required = basic_storage()
        .saturating_add(ACCESS_KEY_ALLOWANCE)
        .saturating_add(ACCESS_KEY_STORAGE)
        .saturating_add(CREATE_ACCOUNT_FEE);

    assert!(
        attached.ge(&required),
        "Please attach exactly {required}. You attached {attached}"
    );

    // TODO: Add refund

    Drop::FT(FTDrop {
        funder,
        ft_contract,
        amount: amount_per_drop,
        counter,
    })
}

#[near]
impl Contract {
    // Fund an existing drop
    pub fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: NearToken,
        msg: String,
    ) -> PromiseOrValue<U128> {
        let drop = self.drop_by_id.get(&msg).expect("Missing such drop_id");
        let counter = drop.get_counter().unwrap();
        let amount_per_drop = drop.get_amount_per_drop().unwrap();
        let required_amount = amount_per_drop.saturating_mul(counter.into());
        assert_eq!(
            amount, required_amount,
            "Wrong FT amount, expected {required_amount}"
        );

        // did you transfer counter * drop.amount?????

        // Make sure the drop exists
        if let Drop::FT(FTDrop {
            funder,
            ft_contract,
            amount,
            counter,
        }) = &drop
        {
            assert_eq!(
                ft_contract,
                &env::predecessor_account_id(),
                "Wrong FTs, expected {ft_contract}"
            );
            // Update and insert again
            self.drop_by_id.insert(
                msg,
                Drop::FT(FTDrop {
                    funder: funder.clone(),
                    ft_contract: ft_contract.clone(),
                    amount: amount.clone(),
                    counter: counter - 1,
                }),
            )
        } else {
            panic!("Not an FT drop")
        };

        // We do not return any tokens
        PromiseOrValue::Value(U128(0))
    }

    pub fn resolve_ft_claim(
        created: bool,
        funder: AccountId,
        amount: NearToken,
        ft_contract: AccountId,
        #[callback_result] result: Result<(), PromiseError>,
    ) -> bool {
        let mut to_refund = basic_storage().saturating_add(ACCESS_KEY_STORAGE);

        if !created {
            to_refund = to_refund.saturating_add(CREATE_ACCOUNT_FEE);
        }

        log!(
            "args: {:?}",
            json!({"receiver_id": funder, "amount": U128(amount.as_yoctonear())})
        );
        if result.is_err() {
            // Return Tokens
            let transfer_args =
                json!({"receiver_id": funder, "amount": U128(amount.as_yoctonear())})
                    .to_string()
                    .into_bytes()
                    .to_vec();

            Promise::new(ft_contract).function_call_weight(
                "ft_transfer".to_string(),
                transfer_args,
                NearToken::from_yoctonear(1),
                MIN_GAS_FOR_FT_TRANSFER,
                GasWeight(0),
            );
        }

        // Return NEAR
        Promise::new(funder.clone()).transfer(to_refund);

        true
    }
}
