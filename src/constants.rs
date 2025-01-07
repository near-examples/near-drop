use near_sdk::{Gas, NearToken};

pub type DropId = u32;

// Allowance for the access key to cover GAS fees when the account is claimed.
// This amount will not be "reserved" on the contract but must be available when GAS is burnt using the access key.
pub const ACCESS_KEY_ALLOWANCE: NearToken = NearToken::from_millinear(250); // 0.25 N

// Cost of creating a new account with longest possible name
pub const CREATE_ACCOUNT_FEE: NearToken = NearToken::from_yoctonear(1_840_000_000_000_000_000_000); // 0.00184 N

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
pub const MIN_GAS_FOR_NFT_TRANSFER: Gas = Gas::from_tgas(5); // 5 TGas
pub const NFT_CLAIM_CALLBACK_GAS: Gas = Gas::from_tgas(10); // 10 TGas

/*
    minimum amount of storage required to store an access key on the contract
    Simple linkdrop: 0.00133 $NEAR
    NFT Linkdrop: 0.00242 $NEAR
*/
pub const ACCESS_KEY_STORAGE: NearToken = NearToken::from_millinear(1); // 0.001 N

// Bytes used to store common data types
pub const PK_STORAGE: u128 = 32; // PublicKey
pub const ID_STORAGE: u128 = 4; // PublicKey
pub const ACC_STORAGE: u128 = 4 + 8; // AccountId
pub const ENUM_STORAGE: u128 = 1; // Enum
pub const TOKEN_AMOUNT_STORAGE: u128 = 16; // NearToken
pub const NFT_TOKEN_ID_STORAGE: u128 = 16; // String
