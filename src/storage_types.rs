use soroban_sdk::{Address, String, contracttype};

#[derive(Clone)]
#[contracttype]
pub struct AllowanceDataKey {
    pub from: Address,
    pub spender: Address,
}

#[contracttype]
pub struct AllowanceValue {
    pub amount: i128,
    pub expiration_ledger: u32,
}

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    State,
    SeriesMetadata(String),
    Balance(Address),
    LockedBalance(Address),
    Allowance(AllowanceDataKey),
    Admin,
    Config,
}
