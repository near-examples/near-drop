use near_contract_standards::non_fungible_token::TokenId;
use near_sdk::json_types::U128;
use near_sdk::serde_json::json;
use near_sdk::{
    env, log, near, AccountId, GasWeight, NearToken, Promise, PromiseError, PromiseOrValue,
};

use crate::constants::*;
use crate::drop_types::Dropper;
use crate::DropType;
use crate::{Contract, ContractExt};

#[derive(PartialEq)]
#[near(serializers = [borsh])]
pub struct NFTDrop {
    funder: AccountId,
    token_id: String,
    nft_contract: AccountId,
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

    fn promise_to_resolve_claim(&self, created: bool) -> Promise {
        Contract::ext(env::current_account_id())
            .with_static_gas(NFT_CLAIM_CALLBACK_GAS)
            .with_unused_gas_weight(0)
            .resolve_nft_claim(
                created,
                self.funder.clone(),
                self.token_id.clone(),
                self.nft_contract.clone(),
            )
    }
}

fn ft_storage() -> NearToken {
    env::storage_byte_cost().saturating_mul(ACC_STORAGE * 2 + 128)
}

pub fn create(funder: AccountId, nft_contract: AccountId) -> DropType {
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

    DropType::NFT(NFTDrop {
        funder,
        nft_contract,
        token_id: "".to_string(),
    })
}

#[near]
impl Contract {
    // Fund an existing drop
    pub fn nft_on_approve(
        &mut self,
        token_id: TokenId,
        owner_id: AccountId,
        approval_id: u32,
        msg: String,
    ) -> PromiseOrValue<U128> {
        let public_key = msg.parse().unwrap();
        let token_id_to_drop = token_id.clone();

        // Make sure the drop exists
        if let DropType::NFT(NFTDrop {
            funder,
            nft_contract,
            token_id: _,
        }) = self.drop_for_key.get(&public_key).expect("Missing Key")
        {
            assert!(
                nft_contract == &env::predecessor_account_id(),
                "Wrong NFT contract, expected {nft_contract}",
            );

            // Update and insert again
            self.drop_for_key.insert(
                public_key,
                DropType::NFT(NFTDrop {
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

    pub fn resolve_nft_claim(
        created: bool,
        funder: AccountId,
        token_id: String,
        nft_contract: AccountId,
        #[callback_result] result: Result<(), PromiseError>,
    ) -> bool {
        let mut to_refund = ft_storage().saturating_add(ACCESS_KEY_STORAGE);

        if !created {
            to_refund = to_refund.saturating_add(CREATE_ACCOUNT_FEE);
        }

        if result.is_err() {
            // Return Tokens
            let transfer_args = json!({"receiver_id": funder, "token_id": token_id})
                .to_string()
                .into_bytes()
                .to_vec();

            Promise::new(nft_contract).function_call_weight(
                "nft_transfer".to_string(),
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
