use soroban_sdk::{Address, String, contracttype};

#[contracttype]
#[derive(Clone)]
pub struct Config {
    pub name: String,
    pub symbol: String,
    pub admin: Address,
    pub storage_address: Address,
    pub transfer_fee_percent: u32,
    pub burn_fee_percent: u32,
    pub platform_fee_percent: u32,
    pub storage_fee_percent: u32,
}
