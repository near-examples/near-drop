use near_sdk::borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::json_types::U128;
use near_sdk::serde_json::json;
use near_sdk::{
    env, log, near, AccountId, GasWeight, NearToken, Promise, PromiseError, PromiseOrValue,
};

use crate::constants::*;
use crate::drop_types::{Dropper, Getters, Setters};
use crate::Drop;
use crate::{Contract, ContractExt};

const FT_REGISTER: NearToken = NearToken::from_yoctonear(12_500_000_000_000_000_000_000);

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize)]
#[near(serializers = [json])]
#[borsh(crate = "near_sdk::borsh")]
pub struct FTDrop {
    funder: AccountId,      // Account which created the drop and funded it
    amount: NearToken,      // Reflects how much fungible tokens will be transfer to claiming user
    ft_contract: AccountId, // Contract of fungible tokens which will be transfer to claiming user
    counter: u32,           // Reflects how much times the drop can be claimed
    funded: bool,           // Reflects if the drop is funded
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

    fn promise_to_resolve_claim(&self, account_created: bool, drop_deleted: bool) -> Promise {
        Contract::ext(env::current_account_id())
            .with_static_gas(FT_CLAIM_CALLBACK_GAS)
            .with_unused_gas_weight(0)
            .resolve_ft_claim(
                account_created,
                drop_deleted,
                self.funder.clone(),
                self.amount,
                self.ft_contract.clone(),
            )
    }
}

impl Getters for FTDrop {
    fn get_counter(&self) -> Result<u32, &str> {
        Ok(self.counter)
    }

    fn get_amount_per_drop(&self) -> Result<NearToken, &str> {
        Ok(self.amount)
    }
}

impl Setters for FTDrop {
    fn set_counter(&mut self, value: u32) -> Result<(), &str> {
        self.counter = value;
        Ok(())
    }
}

pub fn required_deposit_per_key() -> NearToken {
  CREATE_ACCOUNT_FEE
      .saturating_add(ACCESS_KEY_ALLOWANCE)
      .saturating_add(ACCESS_KEY_STORAGE)
}

pub fn required_storage_drop(num_access_keys: u32) -> NearToken {
  NearToken::from_yoctonear(
      // DropId -> Drop::Near
      ID_STORAGE + ENUM_STORAGE + ACC_STORAGE * 2 + TOKEN_AMOUNT_STORAGE + 8 
      // PublicKey -> DropId
      + num_access_keys as u128 * (PK_STORAGE + ID_STORAGE)
  )
}

pub fn create(ft_contract: AccountId, amount_per_drop: NearToken, num_of_keys: u32) -> Drop {
    let funder = env::predecessor_account_id();

    let attached_deposit = env::attached_deposit();
    let required_deposit = // required_storage_drop + (required_deposit_per_key * num_of_keys)
        required_storage_drop(num_of_keys)
        .saturating_add(
            required_deposit_per_key()
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
    pub fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: NearToken,
        msg: String,
    ) -> PromiseOrValue<U128> {
        let drop_id: u32 = msg.parse().unwrap(); 
        let drop = self.drop_by_id.get(&drop_id).expect("Missing such drop_id");
        let counter = drop.get_counter().unwrap();
        let amount_per_drop = drop.get_amount_per_drop().unwrap();
        let required_amount = amount_per_drop.saturating_mul(counter.into());
        assert_eq!(
            amount, required_amount,
            "Wrong FT amount, expected {required_amount}"
        );

        // Make sure the drop exists
        if let Drop::FT(FTDrop {
            funder,
            ft_contract,
            amount,
            counter,
            funded,
        }) = &drop
        {
            assert_eq!(
                ft_contract,
                &env::predecessor_account_id(),
                "Wrong FTs, expected {ft_contract}"
            );
            // Update and insert again
            self.drop_by_id.insert(
                drop_id,
                Drop::FT(FTDrop {
                    funder: funder.clone(),
                    ft_contract: ft_contract.clone(),
                    amount: amount.clone(),
                    counter: counter.clone(),
                    funded: true,
                }),
            )
        } else {
            panic!("Not an FT drop")
        };

        // We do not return any tokens
        PromiseOrValue::Value(U128(0))
    }

    pub fn resolve_ft_claim(
        account_created: bool,
        drop_deleted: bool,
        funder: AccountId,
        amount: NearToken,
        ft_contract: AccountId,
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
