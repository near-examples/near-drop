use near_sdk::{env, NearToken};

// Bytes used to store common data types
pub const PK_STORAGE: u128 = 32; // PublicKey
pub const ID_STORAGE: u128 = 4; // PublicKey
pub const ACC_STORAGE: u128 = 4 + 8; // AccountId
pub const ENUM_STORAGE: u128 = 1; // Enum
pub const TOKEN_STORAGE: u128 = 16; // NearToken

pub fn basic_storage() -> NearToken {
    // Amount needed to store the NEAR{account,u128} struct in the contract
    env::storage_byte_cost().saturating_mul(PK_STORAGE + ID_STORAGE + ID_STORAGE + ENUM_STORAGE)
}