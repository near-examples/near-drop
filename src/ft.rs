use near_sdk::json_types::U128;
use near_sdk::serde_json::json;
use near_sdk::{
    env, near, AccountId, GasWeight, NearToken, Promise, PromiseError, PromiseOrValue, PublicKey,
};

use crate::constants::*;
use crate::drop_types::Dropper;
use crate::DropType;
use crate::{Contract, ContractExt};

const FT_REGISTER: NearToken = NearToken::from_yoctonear(12_500_000_000_000_000_000_000);

#[derive(PartialEq)]
#[near(serializers = [borsh])]
pub struct FTDrop {
    funder: AccountId,
    amount: u128,
    ft_contract: AccountId,
}

impl Dropper for FTDrop {
    fn promise_for_claiming(&self, account_id: AccountId) -> Promise {
        assert!(self.amount > 0, "No tokens to drop");

        let deposit_args = json!({ "account_id": account_id })
            .to_string()
            .into_bytes()
            .to_vec();
        let transfer_args = json!({"receiver_id": account_id, "amount": U128(self.amount)})
            .to_string()
            .into_bytes()
            .to_vec();

        Promise::new(self.ft_contract.clone())
            .function_call_weight(
                "storage_deposit".to_string(),
                deposit_args,
                FT_REGISTER,
                MIN_GAS_FOR_STORAGE_DEPOSIT,
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

fn ft_storage() -> NearToken {
    env::storage_byte_cost().saturating_mul(ACC_STORAGE * 2 + 128)
}

pub fn create_ft_drop(funder: AccountId, ft_contract: AccountId) -> DropType {
    let attached = env::attached_deposit();
    let required = ft_storage()
        .saturating_add(ACCESS_KEY_ALLOWANCE)
        .saturating_add(ACCESS_KEY_STORAGE)
        .saturating_add(CREATE_ACCOUNT_FEE);

    assert!(
        attached.ge(&required),
        "Please attach exactly {required}. You attached {attached}"
    );

    // TODO: Add refund

    DropType::FT(FTDrop {
        funder,
        ft_contract,
        amount: 0,
    })
}

#[near]
impl Contract {
    // Fund an existing drop
    pub fn ft_on_transfer(
        &mut self,
        _sender_id: AccountId,
        _amount: NearToken,
        msg: PublicKey,
    ) -> PromiseOrValue<U128> {
        // Make sure the drop exists
        if let DropType::FT(FTDrop {
            funder,
            ft_contract,
            amount,
        }) = self.drop_for_key.get(&msg).expect("Missing Key")
        {
            assert!(
                ft_contract == &env::predecessor_account_id(),
                "Wrong FTs, expected {ft_contract}"
            );

            // Update and insert again
            self.drop_for_key.insert(
                msg,
                DropType::FT(FTDrop {
                    funder: funder.clone(),
                    ft_contract: ft_contract.clone(),
                    amount: amount.saturating_add(*amount),
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
        tokens: u128,
        ft_contract: AccountId,
        #[callback_result] result: Result<(), PromiseError>,
    ) -> bool {
        let mut to_refund = ft_storage().saturating_add(ACCESS_KEY_STORAGE);

        if !created {
            to_refund = to_refund.saturating_add(CREATE_ACCOUNT_FEE);
        }

        if result.is_err() {
            // Return Tokens
            let transfer_args = json!({"receiver_id": funder, "amount": U128(tokens)})
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
