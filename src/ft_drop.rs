use near_contract_standards::storage_management::StorageBalance;
use near_sdk::borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::json_types::U128;
use near_sdk::serde_json::json;
use near_sdk::{env, near, AccountId, GasWeight, NearToken, Promise, PromiseError, PromiseOrValue};

use crate::constants::*;
use crate::drop_types::{Dropper, Setters};
use crate::Drop;
use crate::{Contract, ContractExt};

// Fixed registration budget per claim: 0.00126 NEAR. Tokens requiring a larger
// deposit cannot register new recipients. Already-registered recipients get
// this budget refunded to the funder; excess on new registrations stays here.
const FT_REGISTER: NearToken = NearToken::from_yoctonear(1_260_000_000_000_000_000_000);

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize)]
#[near(serializers = [json])]
#[borsh(crate = "near_sdk::borsh")]
pub struct FTDrop {
    pub(crate) funder: AccountId, // Account which created the drop and funded it
    pub(crate) amount: NearToken, // Reflects how much fungible tokens will be transfer to claiming user
    pub(crate) ft_contract: AccountId, // Contract of fungible tokens which will be transfer to claiming user
    counter: u32,                      // Reflects how much times the drop can be claimed
    pub(crate) funded: bool,           // Reflects if the drop is funded
}

impl Dropper for FTDrop {
    fn promise_for_claiming(&self, account_id: AccountId, storage_refund: NearToken) -> Promise {
        Promise::new(self.ft_contract.clone())
            .function_call_weight(
                "storage_balance_of".to_string(),
                json!({"account_id": account_id}).to_string().into_bytes(),
                NearToken::from_yoctonear(0),
                FT_STORAGE_QUERY_GAS,
                GasWeight(0),
            )
            .then(
                Contract::ext(env::current_account_id())
                    .with_static_gas(FT_REGISTRATION_CALLBACK_GAS)
                    .with_unused_gas_weight(0)
                    .resolve_ft_registration(account_id, storage_refund, self.clone()),
            )
    }
}

impl Setters for FTDrop {
    fn set_counter(&mut self, value: u32) -> Result<(), &str> {
        self.counter = value;
        Ok(())
    }
}

pub fn required_deposit_per_key() -> NearToken {
    // Each claim pays FT_REGISTER to register the recipient on the FT contract,
    // so the funder must cover it up front.
    CLAIM_GAS_BUDGET
        .saturating_add(ACCESS_KEY_STORAGE)
        .saturating_add(FT_REGISTER)
}

// Storage is measured on-chain by the caller (see Contract::charge_storage_and_refund),
// so this only validates the business rule and builds the drop.
pub fn create(
    funder: AccountId,
    ft_contract: AccountId,
    amount_per_drop: NearToken,
    num_of_keys: u32,
) -> Drop {
    assert!(
        amount_per_drop.ge(&NearToken::from_yoctonear(1)),
        "Amount per drop cannot be 0"
    );

    Drop::FT(FTDrop {
        funder,
        ft_contract,
        amount: amount_per_drop,
        counter: num_of_keys,
        funded: false,
    })
}

#[near]
impl Contract {
    // Fund an existing drop
    #[allow(unused_variables)]
    pub fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: NearToken,
        msg: String,
    ) -> PromiseOrValue<U128> {
        let drop_id: u32 = msg.parse().unwrap();
        let drop = self.drop_by_id.get(&drop_id).expect("Missing such drop_id");

        // Make sure the drop exists and is an FT drop
        if let Drop::FT(FTDrop {
            funder,
            ft_contract,
            amount: amount_per_drop,
            counter,
            funded,
            ..
        }) = &drop
        {
            assert_eq!(
                ft_contract,
                &env::predecessor_account_id(),
                "Wrong FTs, expected {ft_contract}"
            );
            assert_eq!(&sender_id, funder, "Only the drop funder can fund it");

            // Already funded: refund the incoming tokens instead of trapping them.
            if *funded {
                return PromiseOrValue::Value(U128(amount.as_yoctonear()));
            }

            let required_amount = amount_per_drop.saturating_mul((*counter).into());
            assert_eq!(
                amount, required_amount,
                "Wrong FT amount, expected {required_amount}"
            );
        } else {
            panic!("Not an FT drop")
        };

        // Record funding immediately and keep all the incoming tokens.
        if let Drop::FT(ft_drop) = drop {
            let mut updated = ft_drop.clone();
            updated.funded = true;
            self.drop_by_id.insert(drop_id, Drop::FT(updated));
        } else {
            panic!("Not an FT drop")
        }

        // Keep all the tokens
        PromiseOrValue::Value(U128(0))
    }

    #[private]
    pub fn resolve_ft_registration(
        account_id: AccountId,
        storage_refund: NearToken,
        drop: FTDrop,
        #[callback_result] result: Result<Option<StorageBalance>, PromiseError>,
    ) -> PromiseOrValue<bool> {
        let registered = match result {
            Ok(balance) => balance.is_some(),
            Err(error) => {
                return PromiseOrValue::Value(Self::resolve_ft_claim(
                    storage_refund,
                    drop.funder,
                    drop.amount,
                    drop.ft_contract,
                    true,
                    Err(error),
                ))
            }
        };
        let mut transfer = Promise::new(drop.ft_contract.clone());
        if !registered {
            transfer = transfer.function_call_weight(
                "storage_deposit".to_string(),
                json!({"account_id": account_id, "registration_only": true})
                    .to_string()
                    .into_bytes(),
                FT_REGISTER,
                MIN_GAS_FOR_FT_STORAGE_DEPOSIT,
                GasWeight(0),
            );
        }
        PromiseOrValue::Promise(
            transfer
                .function_call_weight(
                    "ft_transfer".to_string(),
                    json!({"receiver_id": account_id, "amount": U128(drop.amount.as_yoctonear())})
                        .to_string()
                        .into_bytes(),
                    NearToken::from_yoctonear(1),
                    MIN_GAS_FOR_FT_TRANSFER,
                    GasWeight(0),
                )
                .then(
                    Contract::ext(env::current_account_id())
                        .with_static_gas(FT_CLAIM_CALLBACK_GAS)
                        .with_unused_gas_weight(0)
                        .resolve_ft_claim(
                            storage_refund,
                            drop.funder,
                            drop.amount,
                            drop.ft_contract,
                            registered,
                        ),
                ),
        )
    }

    #[private]
    pub fn resolve_ft_claim(
        storage_refund: NearToken,
        funder: AccountId,
        amount: NearToken,
        ft_contract: AccountId,
        refund_registration: bool,
        #[callback_result] result: Result<(), PromiseError>,
    ) -> bool {
        let mut to_refund = ACCESS_KEY_STORAGE.saturating_add(storage_refund);

        if refund_registration || result.is_err() {
            // Registration was skipped, or the claim batch reverted atomically.
            to_refund = to_refund.saturating_add(FT_REGISTER);
        }
        if result.is_err() {
            Promise::new(ft_contract)
                .function_call_weight(
                    "ft_transfer".to_string(),
                    json!({"receiver_id": funder, "amount": U128(amount.as_yoctonear())})
                        .to_string()
                        .into_bytes(),
                    NearToken::from_yoctonear(1),
                    MIN_GAS_FOR_FT_TRANSFER,
                    GasWeight(0),
                )
                .detach();
        }

        // Return NEAR
        Promise::new(funder.clone()).transfer(to_refund).detach();

        result.is_ok()
    }
}
