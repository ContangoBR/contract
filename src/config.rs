use soroban_sdk::{Address, String, contracttype};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    pub name: String,
    pub symbol: String,
    pub admin: Address,
    pub platform_address: Address,
}

/// Rates in basis points, frozen per series when the tokenization contract is minted (ADR_037).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeriesFees {
    pub mint_platform_bps: u32,
    pub mint_storage_bps: u32,
    pub transfer_platform_bps: u32,
    pub transfer_storage_bps: u32,
    pub burn_platform_bps: u32,
    pub burn_storage_bps: u32,
}
