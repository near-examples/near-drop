use near_sdk::{Gas, NearToken};

pub type DropId = u32;

// Covers a 32 TGas claim at NEP-642's minimum purchase price, plus transaction fees.
// This is a spending limit, not a balance reserved when the key is added.
pub const ACCESS_KEY_ALLOWANCE: NearToken = NearToken::from_millinear(33); // 0.033 N

// Funder contribution per key: covers 32 TGas burnt at 0.0001 NEAR/TGas.
// A shared liquid balance fronts the larger purchase price until gas refunds arrive.
pub const CLAIM_GAS_BUDGET: NearToken = NearToken::from_yoctonear(3_200_000_000_000_000_000_000);

pub const CLAIM_CALLBACK_GAS: Gas = Gas::from_tgas(5);

// FT
pub const MIN_GAS_FOR_FT_STORAGE_DEPOSIT: Gas = Gas::from_tgas(5); // 5 TGas
pub const MIN_GAS_FOR_FT_TRANSFER: Gas = Gas::from_tgas(5); // 5 TGas
pub const FT_STORAGE_QUERY_GAS: Gas = Gas::from_tgas(2);
pub const FT_REGISTRATION_CALLBACK_GAS: Gas = Gas::from_tgas(23);
pub const FT_CLAIM_CALLBACK_GAS: Gas = Gas::from_tgas(8); // Includes a possible token refund

// NFT
// token_id is only known in nft_on_approve (no deposit there), so create_drop
// pre-pays storage for the longest one allowed.
pub const MAX_TOKEN_ID_LEN: usize = 128;
pub const MIN_GAS_FOR_NFT_TRANSFER: Gas = Gas::from_tgas(5); // 5 TGas
pub const NFT_CLAIM_CALLBACK_GAS: Gas = Gas::from_tgas(10); // 10 TGas

/*
    minimum amount of storage required to store an access key on the contract
    Simple linkdrop: 0.00133 $NEAR
    NFT Linkdrop: 0.00242 $NEAR
*/
pub const ACCESS_KEY_STORAGE: NearToken = NearToken::from_millinear(1); // 0.001 N
