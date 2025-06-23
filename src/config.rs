use crate::storage_types::DataKey;
use soroban_sdk::{contracttype, Address, Env, String};

#[derive(Clone, Debug)]
#[contracttype]
pub struct Config {
    pub mediator_address: Address,
    pub mediator_fee: u32, // Using u32 for basis points (e.g., 100 = 1%)
    pub buyer_address: Address,
    pub buyer_fee: u32, // Using u32 for basis points
    pub contango_hash: String,
}

impl Config {
    pub fn new(
        mediator_address: Address,
        mediator_fee: u32,
        buyer_address: Address,
        buyer_fee: u32,
        contango_hash: String,
    ) -> Self {
        Self {
            mediator_address,
            mediator_fee,
            buyer_address,
            buyer_fee,
            contango_hash,
        }
    }
}

pub fn read_config(e: &Env) -> Option<Config> {
    let key = DataKey::Config;
    e.storage().instance().get(&key)
}

pub fn write_config(e: &Env, config: &Config) {
    let key = DataKey::Config;
    e.storage().instance().set(&key, config);
}
