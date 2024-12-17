use near_sdk::{env, NearToken};

// Bytes used to store common data types
pub const PK_STORAGE: u128 = 32; // PublicKey
pub const ACC_STORAGE: u128 = 32 + 64; // AccountId

pub fn basic_storage() -> NearToken {
    // Amount needed to store the NEAR{account,u128} struct in the contract
    env::storage_byte_cost().saturating_mul(PK_STORAGE + ACC_STORAGE + 128)
}
