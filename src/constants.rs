use near_sdk::{Gas, NearToken};

pub type DropId = u32;

// Allowance for the access key to cover GAS fees when the account is claimed.
// This amount will not be "reserved" on the contract but must be available when GAS is burnt using the access key.
// Since NEP-642 (nearcore 2.13) prepaid gas is bought at >= 0.001 N/TGas, so this caps a key at ~100 TGas
pub const ACCESS_KEY_ALLOWANCE: NearToken = NearToken::from_millinear(100); // 0.1 N

// Cost of creating a new account with longest possible name
pub const CREATE_ACCOUNT_FEE: NearToken = NearToken::from_yoctonear(0); // 0 N

// Minimum GAS for callback. Any unspent GAS will be added according to the weights)
pub const CREATE_CALLBACK_GAS: Gas = Gas::from_tgas(55); // 55 TGas
pub const CLAIM_CALLBACK_GAS: Gas = Gas::from_tgas(5); // 5 TGas

// Actual amount of GAS to attach when creating a new account. No unspent GAS will be attached on top of this (weight of 0)
pub const GAS_FOR_CREATE_ACCOUNT: Gas = Gas::from_tgas(28); // 28 TGas

// FT
pub const MIN_GAS_FOR_FT_STORAGE_DEPOSIT: Gas = Gas::from_tgas(5); // 5 TGas
pub const MIN_GAS_FOR_FT_TRANSFER: Gas = Gas::from_tgas(5); // 5 TGas
pub const FT_CLAIM_CALLBACK_GAS: Gas = Gas::from_tgas(10); // 10 TGas

// NFT
// token_id is only known in nft_on_approve (no deposit there), so create_nft_drop
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
