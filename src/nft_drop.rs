use near_contract_standards::non_fungible_token::TokenId;
use near_sdk::borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::json_types::U128;
use near_sdk::serde_json::json;
use near_sdk::{
    env, log, near, AccountId, GasWeight, NearToken, Promise, PromiseError, PromiseOrValue,
};

use crate::constants::*;
use crate::drop_types::{Dropper, Getters};
use crate::Drop;
use crate::{Contract, ContractExt};

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize)]
#[near(serializers = [json])]
#[borsh(crate = "near_sdk::borsh")]
pub struct NFTDrop {
    funder: AccountId,       // Account which created the drop and funded it
    token_id: String,        // Id of token which will be transfer to claiming user
    nft_contract: AccountId, // Contract of non-fungible token which will be transfer to claiming user
}

impl Dropper for NFTDrop {
    fn promise_for_claiming(&self, account_id: AccountId) -> Promise {
        assert_ne!(self.token_id, "", "No tokens to drop");

        let transfer_args = json!({"receiver_id": account_id, "token_id": self.token_id})
            .to_string()
            .into_bytes()
            .to_vec();

        Promise::new(self.nft_contract.clone()).function_call_weight(
            "nft_transfer".to_string(),
            transfer_args,
            NearToken::from_yoctonear(1),
            MIN_GAS_FOR_NFT_TRANSFER,
            GasWeight(0),
        )
    }

    fn promise_to_resolve_claim(&self, account_created: bool, storage_refund: NearToken) -> Promise {
        Contract::ext(env::current_account_id())
            .with_static_gas(NFT_CLAIM_CALLBACK_GAS)
            .with_unused_gas_weight(0)
            .resolve_nft_claim(
                account_created,
                storage_refund,
                self.funder.clone(),
                self.token_id.clone(),
                self.nft_contract.clone(),
            )
    }
}

impl Getters for NFTDrop {
    fn get_counter(&self) -> Result<u32, &str> {
        Err("There is no counter field for NFT drop structure")
    }
}

pub fn required_deposit_per_key() -> NearToken {
  CREATE_ACCOUNT_FEE
      .saturating_add(ACCESS_KEY_ALLOWANCE)
      .saturating_add(ACCESS_KEY_STORAGE)
}

// Storage is measured on-chain by the caller (see Contract::charge_storage_and_refund),
// so this only builds the drop.
pub fn create(nft_contract: AccountId) -> Drop {
    Drop::NFT(NFTDrop {
        funder: env::predecessor_account_id(),
        nft_contract,
        token_id: "".to_string(),
    })
}

#[near]
impl Contract {
    // Fund an existing drop
    #[allow(unused_variables)]
    pub fn nft_on_approve(
        &mut self,
        token_id: TokenId,
        owner_id: AccountId,
        approval_id: u32,
        msg: String,
    ) -> PromiseOrValue<U128> {
        let drop_id: u32 = msg.parse().unwrap();
        let token_id_to_drop = token_id.clone();
        let drop = self.drop_by_id.get(&drop_id).expect("Missing Drop");

        // Make sure the drop exists
        if let Drop::NFT(NFTDrop {
            funder,
            nft_contract,
            token_id: _,
        }) = &drop
        {
            assert!(
                nft_contract == &env::predecessor_account_id(),
                "Wrong NFT contract, expected {nft_contract}",
            );

            // Update and insert again
            self.drop_by_id.insert(
                drop_id,
                Drop::NFT(NFTDrop {
                    funder: funder.clone(),
                    nft_contract: nft_contract.clone(),
                    token_id: token_id_to_drop,
                }),
            )
        } else {
            panic!("Not an NFT drop")
        };

        // We do not return any tokens
        PromiseOrValue::Value(U128(0))
    }

    #[private]
    #[allow(unused_variables)]
    pub fn resolve_nft_claim(
        account_created: bool,
        storage_refund: NearToken,
        funder: AccountId,
        token_id: String,
        nft_contract: AccountId,
        #[callback_result] result: Result<(), PromiseError>,
    ) -> bool {
        let mut to_refund = ACCESS_KEY_STORAGE.saturating_add(storage_refund);

        if !account_created {
            to_refund = to_refund.saturating_add(CREATE_ACCOUNT_FEE);
        }

        if result.is_err() {
            log!(
                "There is error during claiming the drop: {:?}",
                result.err().unwrap()
            )
        }

        // Return NEAR
        Promise::new(funder.clone()).transfer(to_refund).detach();

        true
    }
}
