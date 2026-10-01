use near_contract_standards::storage_management::StorageBalanceBounds;
use near_sdk::borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::json_types::U128;
use near_sdk::serde_json::json;
use near_sdk::{
    env, ext_contract, near, AccountId, GasWeight, NearToken, Promise, PromiseError, PromiseOrValue,
};

use crate::constants::*;
use crate::drop_types::{Dropper, Getters, Setters};
use crate::Drop;
use crate::{Contract, ContractExt};

// Upper bound charged to the funder up front. The exact registration cost is
// queried from the FT at funding time (see ft_on_transfer) and the unused excess
// is refunded to the funder on claim.
const FT_REGISTER: NearToken = NearToken::from_yoctonear(12_500_000_000_000_000_000_000);

#[ext_contract(ext_ft)]
#[allow(dead_code)]
trait FtStorageBounds {
    fn storage_balance_bounds(&self) -> StorageBalanceBounds;
}

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize)]
#[near(serializers = [json])]
#[borsh(crate = "near_sdk::borsh")]
pub struct FTDrop {
    funder: AccountId,      // Account which created the drop and funded it
    amount: NearToken,      // Reflects how much fungible tokens will be transfer to claiming user
    ft_contract: AccountId, // Contract of fungible tokens which will be transfer to claiming user
    counter: u32,           // Reflects how much times the drop can be claimed
    funded: bool,           // Reflects if the drop is funded
    registration_cost: NearToken, // Exact FT storage_deposit cost, queried at funding
}

impl Dropper for FTDrop {
    fn promise_for_claiming(&self, account_id: AccountId) -> Promise {
        assert!(
            self.amount.gt(&NearToken::from_yoctonear(0)),
            "No tokens to drop"
        );

        assert!(self.funded, "Drop is not funded yet");

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
                self.registration_cost,
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

    fn promise_to_resolve_claim(&self, account_created: bool, storage_refund: NearToken) -> Promise {
        Contract::ext(env::current_account_id())
            .with_static_gas(FT_CLAIM_CALLBACK_GAS)
            .with_unused_gas_weight(0)
            .resolve_ft_claim(
                account_created,
                storage_refund,
                self.funder.clone(),
                self.amount,
                self.ft_contract.clone(),
                self.registration_cost,
            )
    }
}

impl Getters for FTDrop {
    fn get_counter(&self) -> Result<u32, &str> {
        Ok(self.counter)
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
    CREATE_ACCOUNT_FEE
        .saturating_add(ACCESS_KEY_ALLOWANCE)
        .saturating_add(ACCESS_KEY_STORAGE)
        .saturating_add(FT_REGISTER)
}

// Storage is measured on-chain by the caller (see Contract::charge_storage_and_refund),
// so this only validates the business rule and builds the drop.
pub fn create(funder: AccountId, ft_contract: AccountId, amount_per_drop: NearToken, num_of_keys: u32) -> Drop {
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
        // Placeholder; set to the FT's real storage_deposit cost when funded.
        registration_cost: FT_REGISTER,
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

        // Query the FT's real registration cost, then mark the drop funded. Keeps
        // all the tokens (U128(0) unused) once funding is recorded.
        PromiseOrValue::Promise(
            ext_ft::ext(env::predecessor_account_id())
                .storage_balance_bounds()
                .then(Self::ext(env::current_account_id()).resolve_ft_funding(drop_id)),
        )
    }

    #[private]
    pub fn resolve_ft_funding(
        &mut self,
        drop_id: u32,
        #[callback_result] bounds: Result<StorageBalanceBounds, PromiseError>,
    ) -> U128 {
        let registration_cost = bounds.expect("Could not fetch FT storage bounds").min;

        let drop = self.drop_by_id.get(&drop_id).expect("Missing such drop_id");
        if let Drop::FT(ft_drop) = drop {
            let mut updated = ft_drop.clone();
            updated.funded = true;
            updated.registration_cost = registration_cost;
            self.drop_by_id.insert(drop_id, Drop::FT(updated));
        } else {
            panic!("Not an FT drop")
        }

        // Keep all the tokens
        U128(0)
    }

    #[private]
    pub fn resolve_ft_claim(
        account_created: bool,
        storage_refund: NearToken,
        funder: AccountId,
        amount: NearToken,
        ft_contract: AccountId,
        registration_cost: NearToken,
        #[callback_result] result: Result<(), PromiseError>,
    ) -> bool {
        let mut to_refund = ACCESS_KEY_STORAGE.saturating_add(storage_refund);

        if !account_created {
            to_refund = to_refund.saturating_add(CREATE_ACCOUNT_FEE);
        }

        if result.is_err() {
            // The claim batch (storage_deposit + ft_transfer) reverted atomically,
            // so the whole pre-paid FT_REGISTER came back to the contract.
            to_refund = to_refund.saturating_add(FT_REGISTER);

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
            ).detach();
        } else {
            // Only registration_cost was actually spent registering the recipient;
            // refund the funder the pre-paid excess.
            to_refund = to_refund.saturating_add(FT_REGISTER.saturating_sub(registration_cost));
        }

        // Return NEAR
        Promise::new(funder.clone()).transfer(to_refund).detach();

        true
    }
}
