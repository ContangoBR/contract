use crate::config::{Config, SeriesFees};
use soroban_sdk::{
    Address, Env, String, contract, contracterror, contractevent, contractimpl, contracttype,
    panic_with_error,
};

const BPS: i128 = 10_000;
const MAX_FEE_BPS: u32 = 500;
const TTL_THRESHOLD: u32 = 518_400;
const TTL_EXTEND_TO: u32 = 3_110_400;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    SeriesAlreadyMinted = 3,
    SeriesNotFound = 4,
    InsufficientBalance = 5,
    InvalidAmount = 6,
    FeeTooHigh = 7,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeriesMetadata {
    pub asset_type: String,
    pub currency: String,
    pub delivery_date: u64,
    pub producer: Address,
    pub storage_address: Address,
    pub storage_facility: String,
    pub location: String,
    pub quantity_kg: u64,
    pub contract_hash: String,
}

#[contracttype]
pub enum DataKey {
    Config,
    Series(String),
    Fees(String),
    Supply(String),
    Balance(String, Address),
}

#[contractevent]
pub struct Minted {
    #[topic]
    pub series_id: String,
    pub mother: Address,
    pub amount: i128,
}

#[contractevent]
pub struct Distributed {
    #[topic]
    pub series_id: String,
    #[topic]
    pub to: Address,
    pub amount: i128,
}

#[contractevent]
pub struct Transferred {
    #[topic]
    pub series_id: String,
    #[topic]
    pub from: Address,
    #[topic]
    pub to: Address,
    pub net_amount: i128,
    pub platform_fee: i128,
    pub storage_fee: i128,
}

#[contractevent]
pub struct Burned {
    #[topic]
    pub series_id: String,
    #[topic]
    pub from: Address,
    pub burned: i128,
    pub platform_fee: i128,
    pub storage_fee: i128,
}

#[contract]
pub struct ContangoToken;

#[contractimpl]
impl ContangoToken {
    pub fn initialize(
        env: Env,
        name: String,
        symbol: String,
        admin: Address,
        platform_address: Address,
    ) {
        if env.storage().instance().has(&DataKey::Config) {
            panic_with_error!(&env, Error::AlreadyInitialized);
        }
        let config = Config {
            name,
            symbol,
            admin,
            platform_address,
        };
        env.storage().instance().set(&DataKey::Config, &config);
        env.storage()
            .instance()
            .extend_ttl(TTL_THRESHOLD, TTL_EXTEND_TO);
    }

    /// Mints the whole series into the mother wallet and distributes it in the same call (ADR_036).
    pub fn mint(
        env: Env,
        series_id: String,
        metadata: SeriesMetadata,
        fees: SeriesFees,
        mother: Address,
        amount: i128,
    ) {
        let config = Self::config(env.clone());
        config.admin.require_auth();
        if amount <= 0 {
            panic_with_error!(&env, Error::InvalidAmount);
        }
        if env
            .storage()
            .persistent()
            .has(&DataKey::Series(series_id.clone()))
        {
            panic_with_error!(&env, Error::SeriesAlreadyMinted);
        }
        Self::validate_fees(&env, &fees);

        Self::store(&env, &DataKey::Series(series_id.clone()), &metadata);
        Self::store(&env, &DataKey::Fees(series_id.clone()), &fees);
        Self::store(&env, &DataKey::Supply(series_id.clone()), &amount);
        Self::credit(&env, &series_id, &mother, amount);
        Minted {
            series_id: series_id.clone(),
            mother: mother.clone(),
            amount,
        }
        .publish(&env);

        let (platform_fee, storage_fee) =
            Self::split(amount, fees.mint_platform_bps, fees.mint_storage_bps);
        let producer_amount = amount - platform_fee - storage_fee;
        Self::distribute(
            &env,
            &series_id,
            &mother,
            &config.platform_address,
            platform_fee,
        );
        Self::distribute(
            &env,
            &series_id,
            &mother,
            &metadata.storage_address,
            storage_fee,
        );
        Self::distribute(
            &env,
            &series_id,
            &mother,
            &metadata.producer,
            producer_amount,
        );
    }

    /// Fee-exempt transfers are the contract settlements of ADR_039 and require the admin as well.
    pub fn transfer(
        env: Env,
        series_id: String,
        from: Address,
        to: Address,
        amount: i128,
        fee_exempt: bool,
    ) {
        from.require_auth();
        if fee_exempt {
            Self::config(env.clone()).admin.require_auth();
        }
        if amount <= 0 {
            panic_with_error!(&env, Error::InvalidAmount);
        }
        let fees = Self::fees(env.clone(), series_id.clone());
        let metadata = Self::series(env.clone(), series_id.clone());
        let (platform_fee, storage_fee) = if fee_exempt {
            (0, 0)
        } else {
            Self::split(
                amount,
                fees.transfer_platform_bps,
                fees.transfer_storage_bps,
            )
        };
        let net_amount = amount - platform_fee - storage_fee;

        Self::debit(&env, &series_id, &from, amount);
        Self::credit(&env, &series_id, &to, net_amount);
        Self::credit(
            &env,
            &series_id,
            &Self::config(env.clone()).platform_address,
            platform_fee,
        );
        Self::credit(&env, &series_id, &metadata.storage_address, storage_fee);
        Transferred {
            series_id,
            from,
            to,
            net_amount,
            platform_fee,
            storage_fee,
        }
        .publish(&env);
    }

    /// Burns against the grain actually withdrawn; the burn fee moves as tokens, the rest is destroyed.
    pub fn burn(env: Env, series_id: String, from: Address, amount: i128, fee_exempt: bool) {
        from.require_auth();
        if fee_exempt {
            Self::config(env.clone()).admin.require_auth();
        }
        if amount <= 0 {
            panic_with_error!(&env, Error::InvalidAmount);
        }
        let fees = Self::fees(env.clone(), series_id.clone());
        let metadata = Self::series(env.clone(), series_id.clone());
        let (platform_fee, storage_fee) = if fee_exempt {
            (0, 0)
        } else {
            Self::split(amount, fees.burn_platform_bps, fees.burn_storage_bps)
        };
        let burned = amount - platform_fee - storage_fee;

        Self::debit(&env, &series_id, &from, amount);
        Self::credit(
            &env,
            &series_id,
            &Self::config(env.clone()).platform_address,
            platform_fee,
        );
        Self::credit(&env, &series_id, &metadata.storage_address, storage_fee);
        let supply = Self::supply(env.clone(), series_id.clone()) - burned;
        Self::store(&env, &DataKey::Supply(series_id.clone()), &supply);
        Burned {
            series_id,
            from,
            burned,
            platform_fee,
            storage_fee,
        }
        .publish(&env);
    }

    pub fn balance(env: Env, series_id: String, owner: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Balance(series_id, owner))
            .unwrap_or(0)
    }

    pub fn supply(env: Env, series_id: String) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Supply(series_id))
            .unwrap_or_else(|| panic_with_error!(&env, Error::SeriesNotFound))
    }

    pub fn series(env: Env, series_id: String) -> SeriesMetadata {
        env.storage()
            .persistent()
            .get(&DataKey::Series(series_id))
            .unwrap_or_else(|| panic_with_error!(&env, Error::SeriesNotFound))
    }

    pub fn fees(env: Env, series_id: String) -> SeriesFees {
        env.storage()
            .persistent()
            .get(&DataKey::Fees(series_id))
            .unwrap_or_else(|| panic_with_error!(&env, Error::SeriesNotFound))
    }

    pub fn config(env: Env) -> Config {
        env.storage()
            .instance()
            .get(&DataKey::Config)
            .unwrap_or_else(|| panic_with_error!(&env, Error::NotInitialized))
    }

    /// Same rounding as the platform (CommitmentTerm): total fee half-up, platform share half-up, storage takes the rest.
    fn split(amount: i128, platform_bps: u32, storage_bps: u32) -> (i128, i128) {
        let total_bps = (platform_bps + storage_bps) as i128;
        if total_bps == 0 {
            return (0, 0);
        }
        let total = (amount * total_bps + BPS / 2) / BPS;
        let platform = (total * platform_bps as i128 + total_bps / 2) / total_bps;
        (platform, total - platform)
    }

    fn validate_fees(env: &Env, fees: &SeriesFees) {
        let pairs = [
            fees.mint_platform_bps + fees.mint_storage_bps,
            fees.transfer_platform_bps + fees.transfer_storage_bps,
            fees.burn_platform_bps + fees.burn_storage_bps,
        ];
        if pairs.iter().any(|total| *total > MAX_FEE_BPS) {
            panic_with_error!(env, Error::FeeTooHigh);
        }
    }

    fn distribute(env: &Env, series_id: &String, mother: &Address, to: &Address, amount: i128) {
        if amount == 0 {
            return;
        }
        Self::debit(env, series_id, mother, amount);
        Self::credit(env, series_id, to, amount);
        Distributed {
            series_id: series_id.clone(),
            to: to.clone(),
            amount,
        }
        .publish(env);
    }

    fn credit(env: &Env, series_id: &String, owner: &Address, amount: i128) {
        if amount == 0 {
            return;
        }
        let key = DataKey::Balance(series_id.clone(), owner.clone());
        let balance: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        Self::store(env, &key, &(balance + amount));
    }

    fn debit(env: &Env, series_id: &String, owner: &Address, amount: i128) {
        let key = DataKey::Balance(series_id.clone(), owner.clone());
        let balance: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        if balance < amount {
            panic_with_error!(env, Error::InsufficientBalance);
        }
        Self::store(env, &key, &(balance - amount));
    }

    fn store<V: soroban_sdk::IntoVal<Env, soroban_sdk::Val>>(env: &Env, key: &DataKey, value: &V) {
        env.storage().persistent().set(key, value);
        env.storage()
            .persistent()
            .extend_ttl(key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}
