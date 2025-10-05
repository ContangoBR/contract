#[cfg(test)]
mod comprehensive_tests {
    use crate::contract::{ContangoToken, ContangoTokenClient, Distribution, SeriesMetadata};
    use soroban_sdk::testutils::arbitrary::std::println;
    use soroban_sdk::{Address, Env, String, testutils::Address as _};

    fn setup_test_env() -> (Env, ContangoTokenClient<'static>, TestAddresses) {
        let env = Env::default();
        let contract_id = env.register(ContangoToken, ());
        let client = ContangoTokenClient::new(&env, &contract_id);

        let addresses = TestAddresses {
            admin: Address::generate(&env),
            storage: Address::generate(&env),
            producer: Address::generate(&env),
            buyer: Address::generate(&env),
            guarantee_agent: Address::generate(&env),
            third_party: Address::generate(&env),
        };

        client.initialize(
            &String::from_str(&env, "Contango Token"),
            &String::from_str(&env, "CTG"),
            &addresses.admin,
            &addresses.storage,
        );

        (env, client, addresses)
    }

    struct TestAddresses {
        admin: Address,
        storage: Address,
        producer: Address,
        buyer: Address,
        guarantee_agent: Address,
        third_party: Address,
    }

    #[test]
    fn test_initialization_parameters() {
        let (env, client, addresses) = setup_test_env();

        let config = client.get_config();
        assert_eq!(config.name, String::from_str(&env, "Contango Token"));
        assert_eq!(config.symbol, String::from_str(&env, "CTG"));
        assert_eq!(config.admin, addresses.admin);
        assert_eq!(config.storage_address, addresses.storage);
        assert_eq!(config.transfer_fee_percent, 0);
        assert_eq!(config.burn_fee_percent, 50);
        assert_eq!(config.platform_fee_percent, 50);
        assert_eq!(config.storage_fee_percent, 50);
    }

    // Test 2: Cannot reinitialize
    #[test]
    #[should_panic(expected = "Contract already initialized")]
    fn test_cannot_reinitialize() {
        let (env, client, addresses) = setup_test_env();

        client.initialize(
            &String::from_str(&env, "Another Token"),
            &String::from_str(&env, "ATK"),
            &addresses.admin,
            &addresses.storage,
        );
    }

    // Test 3: Spot minting with correct distribution
    #[test]
    fn test_spot_minting_distribution() {
        let (env, client, addresses) = setup_test_env();
        env.mock_all_auths();

        let metadata = create_spot_metadata(&env, &addresses.producer);
        let distribution = Distribution {
            producer_address: addresses.producer.clone(),
            storage_address: addresses.storage.clone(),
            producer_percent: 9900, // 99%
            platform_percent: 50,   // 0.5%
            storage_percent: 50,    // 0.5%
        };

        client.mint_spot(
            &String::from_str(&env, "CTGSoy-BRL-2025Q1"),
            &metadata,
            &distribution,
            &1_000_000,
        );

        // Verify exact distributions
        assert_eq!(client.balance_of(&addresses.producer), 990_000);
        assert_eq!(client.balance_of(&addresses.admin), 5_000);
        assert_eq!(client.balance_of(&addresses.storage), 5_000);
        assert_eq!(client.total_supply(), 1_000_000);
    }

    // Test 4: Invalid distribution percentages
    #[test]
    #[should_panic(expected = "Distribution percentages must sum to 100%")]
    fn test_invalid_distribution_percentages() {
        let (env, client, addresses) = setup_test_env();
        env.mock_all_auths();

        let metadata = create_spot_metadata(&env, &addresses.producer);
        let distribution = Distribution {
            producer_address: addresses.producer.clone(),
            storage_address: addresses.storage.clone(),
            producer_percent: 9800, // 98% - doesn't sum to 100%
            platform_percent: 50,
            storage_percent: 50,
        };

        client.mint_spot(
            &String::from_str(&env, "CTGSoy-BRL-2025Q1"),
            &metadata,
            &distribution,
            &1_000_000,
        );
    }

    // Test 5: Future contract complete flow
    #[test]
    fn test_future_contract_complete_flow() {
        let (env, client, addresses) = setup_test_env();
        env.mock_all_auths();

        let metadata = create_future_metadata(&env, &addresses);

        // Step 1: Mint future tokens
        client.mint_future(
            &String::from_str(&env, "CTGSoy-USD-2025Q4"),
            &metadata,
            &addresses.buyer,
            &addresses.guarantee_agent,
            &500_000,
        );

        // Verify locked tokens
        assert_eq!(client.locked_balance_of(&addresses.buyer), 495_000); // 99%
        assert_eq!(client.balance_of(&addresses.buyer), 0);
        assert_eq!(client.balance_of(&addresses.admin), 2_500); // 0.5%
        assert_eq!(client.balance_of(&addresses.guarantee_agent), 2_500); // 0.5%

        // Step 2: Confirm delivery
        client.confirm_delivery(
            &String::from_str(&env, "CTGSoy-USD-2025Q4"),
            &addresses.storage,
        );

        // Verify unlocked tokens
        assert_eq!(client.locked_balance_of(&addresses.buyer), 0);
        assert_eq!(client.balance_of(&addresses.buyer), 495_000);
    }

    // Test 6: Cannot confirm delivery for spot contract
    #[test]
    #[should_panic(expected = "Not a future contract")]
    fn test_cannot_confirm_delivery_spot() {
        let (env, client, addresses) = setup_test_env();
        env.mock_all_auths();

        let metadata = create_spot_metadata(&env, &addresses.producer);
        let distribution = create_standard_distribution(&addresses);

        client.mint_spot(
            &String::from_str(&env, "CTGSoy-BRL-2025Q1"),
            &metadata,
            &distribution,
            &1_000_000,
        );

        client.confirm_delivery(
            &String::from_str(&env, "CTGSoy-BRL-2025Q1"),
            &addresses.storage,
        );
    }

    // Test 7: Transfer without fees
    #[test]
    fn test_transfer_without_fees() {
        let (env, client, addresses) = setup_test_env();
        env.mock_all_auths();

        // Setup initial balance
        mint_spot_tokens(&env, &client, &addresses, 1_000_000);

        let initial_balance = client.balance_of(&addresses.producer);

        // Transfer without fee
        client.transfer(
            &addresses.producer,
            &addresses.third_party,
            &100_000,
            &false, // no fee
        );

        assert_eq!(
            client.balance_of(&addresses.producer),
            initial_balance - 100_000
        );
        assert_eq!(client.balance_of(&addresses.third_party), 100_000);
        assert_eq!(client.balance_of(&addresses.admin), 5_000); // Unchanged
    }

    // Test 8: Transfer with fees
    #[test]
    fn test_transfer_with_fees() {
        let (env, client, addresses) = setup_test_env();
        env.mock_all_auths();

        // Set transfer fee to 1%
        client.set_transfer_fee(&100);

        // Setup initial balance
        mint_spot_tokens(&env, &client, &addresses, 1_000_000);

        let initial_admin = client.balance_of(&addresses.admin);

        // Transfer with fee
        client.transfer(
            &addresses.producer,
            &addresses.third_party,
            &100_000,
            &true, // apply fee
        );

        assert_eq!(client.balance_of(&addresses.producer), 890_000); // 990k - 100k
        assert_eq!(client.balance_of(&addresses.third_party), 99_000); // 100k - 1%
        assert_eq!(
            client.balance_of(&addresses.admin),
            initial_admin + 1_000
        ); // Fee collected
    }

    // Test 9: Burn with fee distribution
    #[test]
    fn test_burn_with_fee_distribution() {
        let (env, client, addresses) = setup_test_env();
        env.mock_all_auths();

        mint_spot_tokens(&env, &client, &addresses, 1_000_000);

        let initial_supply = client.total_supply();
        let initial_admin = client.balance_of(&addresses.admin);
        let initial_storage = client.balance_of(&addresses.storage);

        // Burn 100k tokens
        client.burn(
            &addresses.producer,
            &String::from_str(&env, "CTGSoy-BRL-2025Q1"),
            &100_000,
        );

        // Verify burn fee distribution (0.5% = 500 tokens)
        assert_eq!(client.balance_of(&addresses.producer), 890_000); // 990k - 100k
        assert_eq!(
            client.balance_of(&addresses.admin),
            initial_admin + 250
        ); // Half of fee
        assert_eq!(client.balance_of(&addresses.storage), initial_storage + 250); // Half of fee
        assert_eq!(client.total_supply(), initial_supply - 99_500); // Burned amount minus fees
    }

    // Test 10: Insufficient balance operations
    #[test]
    #[should_panic(expected = "Insufficient balance")]
    fn test_insufficient_balance_transfer() {
        let (env, client, addresses) = setup_test_env();
        env.mock_all_auths();

        client.transfer(
            &addresses.producer,
            &addresses.third_party,
            &100_000,
            &false,
        );
    }

    // Test 11: Series metadata retrieval
    #[test]
    fn test_series_metadata_storage() {
        let (env, client, addresses) = setup_test_env();
        env.mock_all_auths();

        let metadata = create_spot_metadata(&env, &addresses.producer);
        let distribution = create_standard_distribution(&addresses);
        let series_id = String::from_str(&env, "CTGSoy-BRL-2025Q1");

        client.mint_spot(&series_id, &metadata, &distribution, &1_000_000);

        let retrieved = client.get_series(&series_id).unwrap();
        assert_eq!(retrieved.asset_type, metadata.asset_type);
        assert_eq!(retrieved.currency, metadata.currency);
        assert_eq!(retrieved.delivery_date, metadata.delivery_date);
        assert_eq!(retrieved.producer, metadata.producer);
    }

    // Test 12: Swap functionality
    #[test]
    fn test_swap_same_asset_type() {
        let (env, client, addresses) = setup_test_env();
        env.mock_all_auths();

        // Create two series of same asset type but different currencies
        let metadata_brl = create_spot_metadata(&env, &addresses.producer);
        let mut metadata_usd = metadata_brl.clone();
        metadata_usd.currency = String::from_str(&env, "USD");

        let distribution = create_standard_distribution(&addresses);

        // Mint both series
        client.mint_spot(
            &String::from_str(&env, "CTGSoy-BRL-2025Q1"),
            &metadata_brl,
            &distribution,
            &1_000_000,
        );

        client.mint_spot(
            &String::from_str(&env, "CTGSoy-USD-2025Q1"),
            &metadata_usd,
            &distribution,
            &0, // Just create the series
        );

        // Perform swap (oracle price 5500 = 0.55 BRL/USD)
        client.swap(
            &addresses.producer,
            &String::from_str(&env, "CTGSoy-BRL-2025Q1"),
            &String::from_str(&env, "CTGSoy-USD-2025Q1"),
            &100_000,
            &5500,
        );

        // Verify swap executed
        assert_eq!(client.balance_of(&addresses.producer), 890_000 + 55_000);
    }

    // Test 13: Admin-only functions
    #[test]
    #[should_panic]
    fn test_non_admin_cannot_set_fee() {
        let (env, client, addresses) = setup_test_env();
        env.mock_all_auths_allowing_non_root_auth();

        addresses.producer.require_auth();
        client.set_transfer_fee(&200);
    }

    // Test 14: Maximum fee limits
    #[test]
    #[should_panic(expected = "Fee too high")]
    fn test_maximum_fee_limit() {
        let (env, client, _addresses) = setup_test_env();
        env.mock_all_auths();

        client.set_transfer_fee(&600); // 6% - too high
    }

    // Helper functions
    fn create_spot_metadata(env: &Env, producer: &Address) -> SeriesMetadata {
        SeriesMetadata {
            id: String::from_str(env, "CTGSoy-BRL-2025Q1"),
            asset_type: String::from_str(env, "soy"),
            currency: String::from_str(env, "BRL"),
            delivery_date: 1735689600,
            producer: producer.clone(),
            storage_facility: String::from_str(env, "AGRARIA"),
            buyer: None,
            location: String::from_str(env, "Paraná"),
            quantity_kg: 1_000_000,
            contract_hash: String::from_str(env, "0x123456789abcdef"),
            is_future: false,
            guarantee_agent: None,
        }
    }

    fn create_future_metadata(env: &Env, addresses: &TestAddresses) -> SeriesMetadata {
        SeriesMetadata {
            id: String::from_str(env, "CTGSoy-USD-2025Q4"),
            asset_type: String::from_str(env, "soy"),
            currency: String::from_str(env, "USD"),
            delivery_date: 1751328000, // Q4 2025
            producer: addresses.producer.clone(),
            storage_facility: String::from_str(env, "SLC"),
            buyer: Some(addresses.buyer.clone()),
            location: String::from_str(env, "MT"),
            quantity_kg: 500_000,
            contract_hash: String::from_str(env, "0xfedcba9876543210"),
            is_future: true,
            guarantee_agent: Some(addresses.guarantee_agent.clone()),
        }
    }

    fn create_standard_distribution(addresses: &TestAddresses) -> Distribution {
        Distribution {
            producer_address: addresses.producer.clone(),
            storage_address: addresses.storage.clone(),
            producer_percent: 9900, // 99%
            platform_percent: 50,   // 0.5%
            storage_percent: 50,    // 0.5%
        }
    }

    fn mint_spot_tokens(
        env: &Env,
        client: &ContangoTokenClient,
        addresses: &TestAddresses,
        amount: i128,
    ) {
        let metadata = create_spot_metadata(env, &addresses.producer);
        let distribution = create_standard_distribution(addresses);

        client.mint_spot(
            &String::from_str(env, "CTGSoy-BRL-2025Q1"),
            &metadata,
            &distribution,
            &amount,
        );
    }

    // Test 15: Complex multi-party scenario
    #[test]
    fn test_complex_multiparty_scenario() {
        let (env, client, addresses) = setup_test_env();
        env.mock_all_auths();

        // Step 1: Producer tokenizes 1M kg of soy
        mint_spot_tokens(&env, &client, &addresses, 1_000_000);
        assert_eq!(client.balance_of(&addresses.producer), 990_000);

        // Step 2: Producer sells 200k tokens to buyer
        client.transfer(&addresses.producer, &addresses.buyer, &200_000, &false);
        assert_eq!(client.balance_of(&addresses.producer), 790_000);
        assert_eq!(client.balance_of(&addresses.buyer), 200_000);

        // Step 3: Buyer burns 50k tokens for physical delivery
        client.burn(
            &addresses.buyer,
            &String::from_str(&env, "CTGSoy-BRL-2025Q1"),
            &50_000,
        );
        assert_eq!(client.balance_of(&addresses.buyer), 150_000);

        // Step 4: Verify fee accumulation
        assert_eq!(client.balance_of(&addresses.admin), 5_000 + 125); // Initial + burn fee
        assert_eq!(client.balance_of(&addresses.storage), 5_000 + 125); // Initial + burn fee

        // Step 5: Verify total supply reduced
        assert_eq!(client.total_supply(), 950_250); // 1M - 50k + 250 fees
    }

    // Test 16: Future contract default scenario
    #[test]
    #[should_panic(expected = "No locked tokens for this buyer")]
    fn test_future_contract_no_locked_tokens() {
        let (env, client, addresses) = setup_test_env();
        env.mock_all_auths();

        let metadata = create_future_metadata(&env, &addresses);

        // Mint future tokens
        client.mint_future(
            &String::from_str(&env, "CTGSoy-USD-2025Q4"),
            &metadata,
            &addresses.buyer,
            &addresses.guarantee_agent,
            &500_000,
        );

        // Confirm delivery twice should fail
        client.confirm_delivery(
            &String::from_str(&env, "CTGSoy-USD-2025Q4"),
            &addresses.storage,
        );

        client.confirm_delivery(
            &String::from_str(&env, "CTGSoy-USD-2025Q4"),
            &addresses.storage,
        );
    }

    // Test 17: Swap between incompatible assets
    #[test]
    #[should_panic(expected = "Can only swap between same asset types")]
    fn test_swap_incompatible_assets() {
        let (env, client, addresses) = setup_test_env();
        env.mock_all_auths();

        // Create soy series
        let metadata_soy = create_spot_metadata(&env, &addresses.producer);
        let distribution = create_standard_distribution(&addresses);

        client.mint_spot(
            &String::from_str(&env, "CTGSoy-BRL-2025Q1"),
            &metadata_soy,
            &distribution,
            &1_000_000,
        );

        // Create corn series
        let mut metadata_corn = metadata_soy.clone();
        metadata_corn.asset_type = String::from_str(&env, "corn");

        client.mint_spot(
            &String::from_str(&env, "CTGCorn-BRL-2025Q1"),
            &metadata_corn,
            &distribution,
            &0,
        );

        // Try to swap between different asset types
        client.swap(
            &addresses.producer,
            &String::from_str(&env, "CTGSoy-BRL-2025Q1"),
            &String::from_str(&env, "CTGCorn-BRL-2025Q1"),
            &100_000,
            &10000,
        );
    }

    // Test 18: Edge case - zero amount operations
    #[test]
    fn test_zero_amount_operations() {
        let (env, client, addresses) = setup_test_env();
        env.mock_all_auths();

        mint_spot_tokens(&env, &client, &addresses, 1_000_000);

        // Zero transfer should work
        client.transfer(&addresses.producer, &addresses.buyer, &0, &false);

        // Zero burn should work
        client.burn(
            &addresses.producer,
            &String::from_str(&env, "CTGSoy-BRL-2025Q1"),
            &0,
        );

        // Balances should remain unchanged
        assert_eq!(client.balance_of(&addresses.producer), 990_000);
        assert_eq!(client.total_supply(), 1_000_000);
    }

    // Test 19: Metadata validation for future contracts
    #[test]
    #[should_panic(expected = "Metadata must indicate future contract")]
    fn test_future_mint_requires_future_flag() {
        let (env, client, addresses) = setup_test_env();
        env.mock_all_auths();

        let mut metadata = create_spot_metadata(&env, &addresses.producer);
        metadata.is_future = false; // This should cause panic

        client.mint_future(
            &String::from_str(&env, "CTGSoy-USD-2025Q4"),
            &metadata,
            &addresses.buyer,
            &addresses.guarantee_agent,
            &500_000,
        );
    }

    // Test 20: Comprehensive balance tracking
    #[test]
    fn test_comprehensive_balance_tracking() {
        let (env, client, addresses) = setup_test_env();
        env.mock_all_auths();

        // Create multiple series
        let metadata1 = create_spot_metadata(&env, &addresses.producer);
        let mut metadata2 = metadata1.clone();
        metadata2.id = String::from_str(&env, "CTGSoy-BRL-2025Q2");

        let distribution = create_standard_distribution(&addresses);

        // Mint from multiple series
        client.mint_spot(
            &String::from_str(&env, "CTGSoy-BRL-2025Q1"),
            &metadata1,
            &distribution,
            &500_000,
        );

        client.mint_spot(
            &String::from_str(&env, "CTGSoy-BRL-2025Q2"),
            &metadata2,
            &distribution,
            &500_000,
        );

        // Verify combined balance
        assert_eq!(client.balance_of(&addresses.producer), 990_000); // 495k + 495k
        assert_eq!(client.balance_of(&addresses.admin), 5_000); // 2.5k + 2.5k
        assert_eq!(client.balance_of(&addresses.storage), 5_000); // 2.5k + 2.5k
        assert_eq!(client.total_supply(), 1_000_000);

        // Transfer some tokens
        client.transfer(&addresses.producer, &addresses.buyer, &100_000, &false);

        // Burn some tokens from different party
        client.burn(
            &addresses.buyer,
            &String::from_str(&env, "CTGSoy-BRL-2025Q1"),
            &50_000,
        );

        // Final balance check
        assert_eq!(client.balance_of(&addresses.producer), 890_000);
        assert_eq!(client.balance_of(&addresses.buyer), 50_000);
        assert_eq!(client.balance_of(&addresses.admin), 5_125); // +125 from burn fee
        assert_eq!(client.balance_of(&addresses.storage), 5_125); // +125 from burn fee
        assert_eq!(client.total_supply(), 950_250); // 1M - 50k + 250 fees
    }

    // Performance test - Large scale operations
    #[test]
    fn test_performance_large_operations() {
        let (env, client, addresses) = setup_test_env();
        env.mock_all_auths();

        // Mint large amount
        let metadata = create_spot_metadata(&env, &addresses.producer);
        let distribution = create_standard_distribution(&addresses);

        client.mint_spot(
            &String::from_str(&env, "CTGSoy-BRL-2025Q1"),
            &metadata,
            &distribution,
            &100_000_000, // 100M tokens
        );

        assert_eq!(client.balance_of(&addresses.producer), 99_000_000);
        assert_eq!(client.balance_of(&addresses.admin), 500_000);
        assert_eq!(client.balance_of(&addresses.storage), 500_000);

        // Multiple small transfers
        for _i in 0..10 {
            client.transfer(&addresses.producer, &addresses.buyer, &1_000_000, &false);
        }

        assert_eq!(client.balance_of(&addresses.producer), 89_000_000);
        assert_eq!(client.balance_of(&addresses.buyer), 10_000_000);
    }

    // Integration test - Complete business flow
    #[test]
    fn test_complete_business_flow() {
        let (env, client, addresses) = setup_test_env();
        env.mock_all_auths();

        // Scenario: Producer has soy stored, tokenizes it, sells part to trader,
        // trader sells to end buyer who burns for delivery

        // 1. Producer tokenizes stored soy
        let metadata = create_spot_metadata(&env, &addresses.producer);
        let distribution = create_standard_distribution(&addresses);

        client.mint_spot(
            &String::from_str(&env, "CTGSoy-BRL-2025Q1"),
            &metadata,
            &distribution,
            &1_000_000, // 1000 tons
        );

        println!(
            "Producer balance after minting: {}",
            client.balance_of(&addresses.producer)
        );

        // 2. Producer sells 300k tokens to trader
        let trader = Address::generate(&env);
        client.transfer(&addresses.producer, &trader, &300_000, &false);

        println!(
            "Trader balance after purchase: {}",
            client.balance_of(&trader)
        );

        // 3. Enable transfer fees for secondary market
        client.set_transfer_fee(&50); // 0.5%

        // 4. Trader sells to end buyer with fee
        client.transfer(
            &trader,
            &addresses.buyer,
            &300_000,
            &true, // Apply fee
        );

        println!(
            "Buyer balance after purchase: {}",
            client.balance_of(&addresses.buyer)
        );
        println!(
            "Platform earned from transfer fee: {}",
            client.balance_of(&addresses.admin) - 5_000
        );

        // 5. End buyer burns tokens for physical delivery
        client.burn(
            &addresses.buyer,
            &String::from_str(&env, "CTGSoy-BRL-2025Q1"),
            &100_000,
        );

        // Verify final state
        assert_eq!(client.balance_of(&addresses.producer), 690_000); // Initial - sold
        assert_eq!(client.balance_of(&trader), 0); // Sold all
        assert_eq!(client.balance_of(&addresses.buyer), 198_500); // Bought - fee - burned

        // Platform and storage earned from:
        // - Initial mint: 5k each
        // - Transfer fee: 1.5k to platform
        // - Burn fee: 250 each
        assert_eq!(client.balance_of(&addresses.admin), 6_750);
        assert_eq!(client.balance_of(&addresses.storage), 5_250);
    }
}
