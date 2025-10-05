use crate::config::Config;
use soroban_sdk::{Address, Env, Map, String, Symbol, contract, contractimpl, contracttype};

#[contracttype]
#[derive(Clone)]
pub struct SeriesMetadata {
    pub id: String,
    pub asset_type: String, // e.g., "soy", "corn", "fertilizer"
    pub currency: String,   // "BRL" or "USD"
    pub delivery_date: u64, // Unix timestamp
    pub producer: Address,
    pub storage_facility: String,         // e.g., "AGRARIA", "SLC"
    pub buyer: Option<Address>,           // For future contracts
    pub location: String,                 // e.g., "FOB Santos"
    pub quantity_kg: u64,                 // Total quantity in kg
    pub contract_hash: String,            // Hash of the digital contract
    pub is_future: bool,                  // true for future contracts, false for spot
    pub guarantee_agent: Option<Address>, // For future contracts
}

#[contracttype]
#[derive(Clone)]
pub struct TokenState {
    pub total_supply: i128,
    pub balances: Map<Address, i128>,
    pub series: Map<String, SeriesMetadata>,
    pub locked_tokens: Map<Address, i128>, // For future contracts until delivery
}

#[contracttype]
#[derive(Clone)]
pub struct Distribution {
    pub producer_address: Address,
    pub storage_address: Address,
    pub producer_percent: u32, // e.g., 9900 = 99%
    pub platform_percent: u32, // e.g., 50 = 0.5%
    pub storage_percent: u32,  // e.g., 50 = 0.5%
}

#[contracttype]
pub enum DataKey {
    Config,
    State,
    SeriesMetadata(String),
    Balance(Address),
    LockedBalance(Address),
    Allowance(Address, Address),
}

#[contract]
pub struct ContangoToken;

#[contractimpl]
impl ContangoToken {
    /// Initialize the contract with configuration
    pub fn initialize(
        env: Env,
        name: String,
        symbol: String,
        admin: Address,
        storage_address: Address,
    ) {
        if env.storage().instance().has(&DataKey::Config) {
            panic!("Contract already initialized");
        }

        let config = Config {
            name,
            symbol,
            admin: admin.clone(),
            storage_address: storage_address.clone(),
            transfer_fee_percent: 0,  // No fee on transfers by default
            burn_fee_percent: 50,     // 0.5% burn fee
            platform_fee_percent: 50, // 0.5% platform fee
            storage_fee_percent: 50,  // 0.5% storage fee
        };

        let state = TokenState {
            total_supply: 0,
            balances: Map::new(&env),
            series: Map::new(&env),
            locked_tokens: Map::new(&env),
        };

        env.storage().instance().set(&DataKey::Config, &config);
        env.storage().instance().set(&DataKey::State, &state);
    }

    /// Mint tokens for spot contracts (grains already stored)
    pub fn mint_spot(
        env: Env,
        series_id: String,
        metadata: SeriesMetadata,
        distribution: Distribution,
        amount: i128,
    ) {
        let config = Self::get_config(&env);
        let mut state = Self::get_state(&env);

        // Verify admin authorization
        config.admin.require_auth();

        // Validate distribution percentages (should sum to 10000 = 100%)
        let total_percent = distribution.producer_percent
            + distribution.platform_percent
            + distribution.storage_percent;
        if total_percent != 10000 {
            panic!("Distribution percentages must sum to 100%");
        }

        // Store series metadata
        env.storage()
            .instance()
            .set(&DataKey::SeriesMetadata(series_id.clone()), &metadata);
        state.series.set(series_id.clone(), metadata);

        // Calculate distributions
        let producer_amount = (amount * distribution.producer_percent as i128) / 10000;
        let platform_amount = (amount * distribution.platform_percent as i128) / 10000;
        let storage_amount = (amount * distribution.storage_percent as i128) / 10000;

        // Update balances atomically
        Self::increase_balance(&env, &distribution.producer_address, producer_amount);
        Self::increase_balance(&env, &config.admin, platform_amount);
        Self::increase_balance(&env, &distribution.storage_address, storage_amount);

        // Update total supply
        state.total_supply += amount;
        env.storage().instance().set(&DataKey::State, &state);

        // Emit events
        env.events()
            .publish((Symbol::new(&env, "mint_spot"), series_id), amount);
    }

    /// Mint tokens for future contracts (payment received, delivery pending)
    pub fn mint_future(
        env: Env,
        series_id: String,
        metadata: SeriesMetadata,
        buyer: Address,
        guarantee_agent: Address,
        amount: i128,
    ) {
        let config = Self::get_config(&env);
        let mut state = Self::get_state(&env);

        // Verify admin authorization
        config.admin.require_auth();

        // Ensure this is marked as a future contract
        if !metadata.is_future {
            panic!("Metadata must indicate future contract");
        }

        // Store series metadata with buyer and guarantee agent
        let mut future_metadata = metadata.clone();
        future_metadata.buyer = Some(buyer.clone());
        future_metadata.guarantee_agent = Some(guarantee_agent.clone());

        env.storage().instance().set(
            &DataKey::SeriesMetadata(series_id.clone()),
            &future_metadata,
        );
        state.series.set(series_id.clone(), future_metadata);

        // Calculate distributions for future contracts
        let buyer_amount = (amount * 9900) / 10000; // 99% to buyer
        let platform_amount = (amount * 50) / 10000; // 0.5% to platform
        let guarantee_amount = (amount * 50) / 10000; // 0.5% to guarantee agent

        // For future contracts, buyer tokens are locked until delivery
        Self::increase_locked_balance(&env, &buyer, buyer_amount);
        Self::increase_balance(&env, &config.admin, platform_amount);
        Self::increase_balance(&env, &guarantee_agent, guarantee_amount);

        // Update total supply
        state.total_supply += amount;
        env.storage().instance().set(&DataKey::State, &state);

        env.events()
            .publish((Symbol::new(&env, "mint_future"), series_id), amount);
    }

    pub fn confirm_delivery(env: Env, series_id: String, storage_validator: Address) {
        Self::get_config(&env);
        Self::get_state(&env);

        // Require storage validator authorization
        storage_validator.require_auth();

        // Get series metadata
        let metadata = match env
            .storage()
            .instance()
            .get::<DataKey, SeriesMetadata>(&DataKey::SeriesMetadata(series_id.clone()))
        {
            Some(m) => m,
            None => panic!("Series not found"),
        };

        if !metadata.is_future {
            panic!("Not a future contract");
        }

        let buyer = metadata.buyer.unwrap();
        let locked_amount = Self::get_locked_balance(&env, &buyer);

        if locked_amount == 0 {
            panic!("No locked tokens for this buyer");
        }

        // Unlock tokens by moving from locked to regular balance
        Self::decrease_locked_balance(&env, &buyer, locked_amount);
        Self::increase_balance(&env, &buyer, locked_amount);

        // Emit delivery confirmation event
        env.events().publish(
            (Symbol::new(&env, "delivery_confirmed"), series_id),
            locked_amount,
        );
    }

    /// Burn tokens with fee distribution
    pub fn burn(env: Env, from: Address, series_id: String, amount: i128) {
        from.require_auth();

        let config = Self::get_config(&env);
        let mut state = Self::get_state(&env);

        let balance = Self::get_balance(&env, &from);
        if balance < amount {
            panic!("Insufficient balance");
        }

        // Calculate burn fee
        let fee_amount = (amount * config.burn_fee_percent as i128) / 10000;
        let burn_amount = amount - fee_amount;

        // Distribute fees (50/50 between platform and storage)
        let platform_fee = fee_amount / 2;
        let storage_fee = fee_amount - platform_fee;

        // Execute burn
        Self::decrease_balance(&env, &from, amount);
        Self::increase_balance(&env, &config.admin, platform_fee);
        Self::increase_balance(&env, &config.storage_address, storage_fee);

        // Update total supply
        state.total_supply -= burn_amount;
        env.storage().instance().set(&DataKey::State, &state);

        // Emit burn event
        env.events()
            .publish((Symbol::new(&env, "burn"), series_id, from), amount);
    }

    /// Transfer tokens between addresses (optional fee)
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128, apply_fee: bool) {
        from.require_auth();

        let config = Self::get_config(&env);
        let from_balance = Self::get_balance(&env, &from);

        if from_balance < amount {
            panic!("Insufficient balance");
        }

        let transfer_amount;
        if apply_fee && config.transfer_fee_percent > 0 {
            let fee = (amount * config.transfer_fee_percent as i128) / 10000;
            transfer_amount = amount - fee;

            // Transfer fee to platform
            Self::decrease_balance(&env, &from, amount);
            Self::increase_balance(&env, &to, transfer_amount);
            Self::increase_balance(&env, &config.admin, fee);
        } else {
            // No fee transfer
            transfer_amount = amount;
            Self::decrease_balance(&env, &from, amount);
            Self::increase_balance(&env, &to, transfer_amount);
        }

        env.events()
            .publish((Symbol::new(&env, "transfer"), from, to), transfer_amount);
    }

    /// Set transfer fee (admin only)
    pub fn set_transfer_fee(env: Env, fee_percent: u32) {
        let mut config = Self::get_config(&env);
        config.admin.require_auth();

        if fee_percent > 500 {
            // Max 5%
            panic!("Fee too high");
        }

        config.transfer_fee_percent = fee_percent;
        env.storage().instance().set(&DataKey::Config, &config);
    }

    pub fn swap(
        env: Env,
        from: Address,
        from_series: String,
        to_series: String,
        amount: i128,
        oracle_price: i128,
    ) {
        from.require_auth();

        Self::get_config(&env);
        let from_balance = Self::get_balance(&env, &from);

        if from_balance < amount {
            panic!("Insufficient balance");
        }

        // Get series metadata to validate swap compatibility
        let from_metadata = match env
            .storage()
            .instance()
            .get::<DataKey, SeriesMetadata>(&DataKey::SeriesMetadata(from_series.clone()))
        {
            Some(m) => m,
            None => panic!("From series not found"),
        };

        let to_metadata = match env
            .storage()
            .instance()
            .get::<DataKey, SeriesMetadata>(&DataKey::SeriesMetadata(to_series.clone()))
        {
            Some(m) => m,
            None => panic!("To series not found"),
        };

        // Validate swap compatibility (same asset type)
        if from_metadata.asset_type != to_metadata.asset_type {
            panic!("Can only swap between same asset types");
        }

        // Calculate swap amount based on oracle price
        let swap_amount = (amount * oracle_price) / 10000; // Assuming oracle price is in basis points

        // Execute swap by burning from one series and minting in another
        Self::decrease_balance(&env, &from, amount);
        Self::increase_balance(&env, &from, swap_amount);

        // Emit swap event
        env.events()
            .publish((Symbol::new(&env, "swap"), from_series, to_series), amount);
    }

    /// Get balance of an address
    pub fn balance_of(env: Env, owner: Address) -> i128 {
        Self::get_balance(&env, &owner)
    }

    /// Get locked balance (for future contracts)
    pub fn locked_balance_of(env: Env, owner: Address) -> i128 {
        Self::get_locked_balance(&env, &owner)
    }

    /// Get total supply
    pub fn total_supply(env: Env) -> i128 {
        let state = Self::get_state(&env);
        state.total_supply
    }

    /// Get series metadata
    pub fn get_series(env: Env, series_id: String) -> Option<SeriesMetadata> {
        env.storage()
            .instance()
            .get(&DataKey::SeriesMetadata(series_id))
    }

    /// Get contract configuration
    pub fn get_config(env: &Env) -> Config {
        env.storage().instance().get(&DataKey::Config).unwrap()
    }

    // Helper functions
    fn get_state(env: &Env) -> TokenState {
        env.storage().instance().get(&DataKey::State).unwrap()
    }

    fn get_balance(env: &Env, addr: &Address) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::Balance(addr.clone()))
            .unwrap_or(0)
    }

    fn get_locked_balance(env: &Env, addr: &Address) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::LockedBalance(addr.clone()))
            .unwrap_or(0)
    }

    fn increase_balance(env: &Env, addr: &Address, amount: i128) {
        let balance = Self::get_balance(env, addr);
        env.storage()
            .instance()
            .set(&DataKey::Balance(addr.clone()), &(balance + amount));
    }

    fn decrease_balance(env: &Env, addr: &Address, amount: i128) {
        let balance = Self::get_balance(env, addr);
        if balance < amount {
            panic!("Insufficient balance");
        }
        env.storage()
            .instance()
            .set(&DataKey::Balance(addr.clone()), &(balance - amount));
    }

    fn increase_locked_balance(env: &Env, addr: &Address, amount: i128) {
        let balance = Self::get_locked_balance(env, addr);
        env.storage()
            .instance()
            .set(&DataKey::LockedBalance(addr.clone()), &(balance + amount));
    }

    fn decrease_locked_balance(env: &Env, addr: &Address, amount: i128) {
        let balance = Self::get_locked_balance(env, addr);
        if balance < amount {
            panic!("Insufficient locked balance");
        }
        env.storage()
            .instance()
            .set(&DataKey::LockedBalance(addr.clone()), &(balance - amount));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{Address, Env, testutils::Address as _};

    #[test]
    fn test_initialize() {
        let env = Env::default();
        let contract_id = env.register(ContangoToken, ());
        let client = ContangoTokenClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let storage = Address::generate(&env);

        client.initialize(
            &String::from_str(&env, "Contango Token"),
            &String::from_str(&env, "CTG"),
            &admin,
            &storage,
        );

        let config = client.get_config();
        assert_eq!(config.name, String::from_str(&env, "Contango Token"));
        assert_eq!(config.symbol, String::from_str(&env, "CTG"));
    }

    #[test]
    fn test_mint_spot_distribution() {
        let env = Env::default();
        let contract_id = env.register(ContangoToken, ());
        let client = ContangoTokenClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let storage = Address::generate(&env);
        let producer = Address::generate(&env);

        client.initialize(
            &String::from_str(&env, "Contango Token"),
            &String::from_str(&env, "CTG"),
            &admin,
            &storage,
        );

        let metadata = SeriesMetadata {
            id: String::from_str(&env, "CTGSoy-BRL-2025Q1"),
            asset_type: String::from_str(&env, "soy"),
            currency: String::from_str(&env, "BRL"),
            delivery_date: 1735689600, // 2025-01-01
            producer: producer.clone(),
            storage_facility: String::from_str(&env, "AGRARIA"),
            buyer: None,
            location: String::from_str(&env, "Paraná"),
            quantity_kg: 1000000,
            contract_hash: String::from_str(&env, "0x1234..."),
            is_future: false,
            guarantee_agent: None,
        };

        let distribution = Distribution {
            producer_address: producer.clone(),
            storage_address: storage.clone(),
            producer_percent: 9900, // 99%
            platform_percent: 50,   // 0.5%
            storage_percent: 50,    // 0.5%
        };

        env.mock_all_auths();

        client.mint_spot(
            &String::from_str(&env, "CTGSoy-BRL-2025Q1"),
            &metadata,
            &distribution,
            &1000000,
        );

        assert_eq!(client.balance_of(&producer), 990000); // 99%
        assert_eq!(client.balance_of(&admin), 5000); // 0.5%
        assert_eq!(client.balance_of(&storage), 5000); // 0.5%
        assert_eq!(client.total_supply(), 1000000);
    }

    #[test]
    fn test_future_contract_flow() {
        let env = Env::default();
        let contract_id = env.register(ContangoToken, ());
        let client = ContangoTokenClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let storage = Address::generate(&env);
        let producer = Address::generate(&env);
        let buyer = Address::generate(&env);
        let guarantee_agent = Address::generate(&env);

        client.initialize(
            &String::from_str(&env, "Contango Token"),
            &String::from_str(&env, "CTG"),
            &admin,
            &storage,
        );

        let metadata = SeriesMetadata {
            id: String::from_str(&env, "CTGSoy-USD-2025Q4"),
            asset_type: String::from_str(&env, "soy"),
            currency: String::from_str(&env, "USD"),
            delivery_date: 1735689600,
            producer: producer.clone(),
            storage_facility: String::from_str(&env, "SLC"),
            buyer: Some(buyer.clone()),
            location: String::from_str(&env, "MT"),
            quantity_kg: 500000,
            contract_hash: String::from_str(&env, "0x5678..."),
            is_future: true,
            guarantee_agent: Some(guarantee_agent.clone()),
        };

        env.mock_all_auths();

        client.mint_future(
            &String::from_str(&env, "CTGSoy-USD-2025Q4"),
            &metadata,
            &buyer,
            &guarantee_agent,
            &500000,
        );

        assert_eq!(client.locked_balance_of(&buyer), 495000); // 99% locked
        assert_eq!(client.balance_of(&buyer), 0); // Not available yet
        assert_eq!(client.balance_of(&admin), 2500); // 0.5%
        assert_eq!(client.balance_of(&guarantee_agent), 2500); // 0.5%

        client.confirm_delivery(&String::from_str(&env, "CTGSoy-USD-2025Q4"), &storage);

        assert_eq!(client.locked_balance_of(&buyer), 0); // Unlocked
        assert_eq!(client.balance_of(&buyer), 495000); // Now available
    }
}
