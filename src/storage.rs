use crate::constants::*;
use near_sdk::{env, NearToken};

pub fn basic_storage() -> NearToken {
    // Amount needed to store the NEAR{account,u128} struct in the contract
    env::storage_byte_cost().saturating_mul(PK_STORAGE + ACC_STORAGE + 128)
}
