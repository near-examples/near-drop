use near_sdk::json_types::U128;
use near_sdk::serde_json::json;
use near_sdk::{env, near, GasWeight, NearToken, Promise};

use crate::constants::*;
use crate::{ft_drop, near_drop, Contract, ContractExt, Drop};

#[near]
impl Contract {
    /// Cancel a drop and refund the budgets for all its remaining keys.
    pub fn delete_drop(&mut self, drop_id: DropId) -> Promise {
        let drop = self.drop_by_id.get(&drop_id).expect("Missing drop").clone();
        assert_eq!(
            drop.funder(),
            &env::predecessor_account_id(),
            "Only the funder can delete a drop"
        );
        let storage_before = env::storage_usage();
        let keys = self
            .keys_by_drop
            .remove(&drop_id)
            .expect("Missing drop keys");
        self.drop_by_id.remove(&drop_id);
        for key in &keys {
            self.drop_id_by_key.remove(key);
            Promise::new(env::current_account_id())
                .delete_key(key.clone())
                .detach();
        }
        self.flush_maps();
        let storage_refund = env::storage_byte_cost()
            .saturating_mul(storage_before.saturating_sub(env::storage_usage()) as u128)
            .saturating_add(drop.unused_storage_padding());
        self.refund_keys(drop, keys.len() as u32, storage_refund, true)
    }
}

impl Contract {
    pub(crate) fn refund_keys(
        &self,
        drop: Drop,
        num_keys: u32,
        storage_refund: NearToken,
        refund_gas_budget: bool,
    ) -> Promise {
        let per_key = match &drop {
            Drop::NEAR(drop) => near_drop::required_deposit_per_key(drop.amount),
            Drop::FT(_) => ft_drop::required_deposit_per_key(),
            // NFT padding is already included in storage_refund.
            Drop::NFT(_) => CLAIM_GAS_BUDGET.saturating_add(ACCESS_KEY_STORAGE),
        };
        let per_key = if refund_gas_budget {
            per_key
        } else {
            per_key.saturating_sub(CLAIM_GAS_BUDGET)
        };
        let near_refund = per_key
            .saturating_mul(num_keys as u128)
            .saturating_add(storage_refund);
        let refund = Promise::new(drop.funder().clone()).transfer(near_refund);
        match drop {
            Drop::FT(drop) if drop.funded => {
                refund.detach();
                Promise::new(drop.ft_contract)
                    .function_call_weight("ft_transfer".to_string(),
                        json!({"receiver_id": drop.funder,
                            "amount": U128(drop.amount.saturating_mul(num_keys as u128).as_yoctonear())})
                            .to_string().into_bytes(),
                        NearToken::from_yoctonear(1), MIN_GAS_FOR_FT_TRANSFER, GasWeight(0))
            }
            // NFT drops use approval: the NFT is already held by the funder.
            _ => refund,
        }
    }
}
