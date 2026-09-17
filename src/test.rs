use crate::config::SeriesFees;
use crate::contract::{ContangoToken, ContangoTokenClient, Error, SeriesMetadata};
use soroban_sdk::testutils::{Address as _, MockAuth, MockAuthInvoke};
use soroban_sdk::{Address, Env, IntoVal, String};

const UNIT: i128 = 10_000;

struct Pilot {
    env: Env,
    client: ContangoTokenClient<'static>,
    admin: Address,
    platform: Address,
    storage: Address,
    producer: Address,
    sorriso: Address,
    cerrado: Address,
}

fn contract_error(error: Error) -> soroban_sdk::Error {
    soroban_sdk::Error::from_contract_error(error as u32)
}

fn tons(value: i128) -> i128 {
    value * UNIT
}

fn setup() -> Pilot {
    let env = Env::default();
    let contract_id = env.register(ContangoToken, ());
    let client = ContangoTokenClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let platform = Address::generate(&env);
    client.initialize(
        &String::from_str(&env, "Contango Token"),
        &String::from_str(&env, "CTG"),
        &admin,
        &platform,
    );
    Pilot {
        storage: Address::generate(&env),
        producer: Address::generate(&env),
        sorriso: Address::generate(&env),
        cerrado: Address::generate(&env),
        env,
        client,
        admin,
        platform,
    }
}

fn cen01_fees() -> SeriesFees {
    SeriesFees {
        mint_platform_bps: 50,
        mint_storage_bps: 50,
        transfer_platform_bps: 5,
        transfer_storage_bps: 5,
        burn_platform_bps: 50,
        burn_storage_bps: 50,
    }
}

fn series(p: &Pilot) -> String {
    String::from_str(&p.env, "CDA-BOA-SAFRA-0001")
}

fn metadata(p: &Pilot) -> SeriesMetadata {
    SeriesMetadata {
        asset_type: String::from_str(&p.env, "SOY"),
        currency: String::from_str(&p.env, "BRL"),
        delivery_date: 0,
        producer: p.producer.clone(),
        storage_address: p.storage.clone(),
        storage_facility: String::from_str(&p.env, "Armazens Boa Safra"),
        location: String::from_str(&p.env, "Sorriso, MT"),
        quantity_kg: 987_000,
        contract_hash: String::from_str(&p.env, "hash"),
    }
}

fn mint_cen01(p: &Pilot) {
    p.env.mock_all_auths();
    p.client.mint(
        &series(p),
        &metadata(p),
        &cen01_fees(),
        &p.admin,
        &tons(987),
    );
}

#[test]
fn mint_goes_through_the_mother_wallet_and_distributes_in_the_same_call() {
    let p = setup();
    mint_cen01(&p);
    let s = series(&p);

    assert_eq!(p.client.balance(&s, &p.admin), 0);
    assert_eq!(p.client.balance(&s, &p.producer), 9_771_300);
    assert_eq!(p.client.balance(&s, &p.platform), 49_350);
    assert_eq!(p.client.balance(&s, &p.storage), 49_350);
    assert_eq!(p.client.supply(&s), tons(987));
    assert_eq!(p.client.fees(&s), cen01_fees());
}

#[test]
fn transfer_charges_the_frozen_series_fee_split_between_platform_and_storage() {
    let p = setup();
    mint_cen01(&p);
    let s = series(&p);

    p.client
        .transfer(&s, &p.producer, &p.sorriso, &9_771_300, &false);
    assert_eq!(p.client.balance(&s, &p.sorriso), 9_761_529);

    p.client
        .transfer(&s, &p.sorriso, &p.cerrado, &9_761_529, &false);
    assert_eq!(p.client.balance(&s, &p.cerrado), 9_751_767);
    assert_eq!(p.client.balance(&s, &p.producer), 0);
}

#[test]
fn burn_destroys_the_net_and_moves_the_fee_as_tokens() {
    let p = setup();
    mint_cen01(&p);
    let s = series(&p);
    p.client
        .transfer(&s, &p.producer, &p.cerrado, &tons(400), &true);

    p.client.burn(&s, &p.cerrado, &tons(400), &false);

    assert_eq!(p.client.balance(&s, &p.cerrado), 0);
    assert_eq!(p.client.supply(&s), tons(987) - tons(396));
    assert_eq!(p.client.balance(&s, &p.storage), 49_350 + 20_000);
    assert_eq!(p.client.balance(&s, &p.platform), 49_350 + 20_000);
}

#[test]
fn fee_exempt_settlements_move_the_exact_amount() {
    let p = setup();
    mint_cen01(&p);
    let s = series(&p);

    p.client
        .transfer(&s, &p.platform, &p.storage, &49_350, &true);
    p.client.burn(&s, &p.storage, &98_700, &true);

    assert_eq!(p.client.balance(&s, &p.storage), 0);
    assert_eq!(p.client.supply(&s), tons(987) - 98_700);
}

#[test]
fn fee_exempt_transfer_requires_the_admin_signature() {
    let p = setup();
    mint_cen01(&p);
    let s = series(&p);

    p.env.mock_auths(&[MockAuth {
        address: &p.producer,
        invoke: &MockAuthInvoke {
            contract: &p.client.address,
            fn_name: "transfer",
            args: (
                s.clone(),
                p.producer.clone(),
                p.sorriso.clone(),
                100_i128,
                true,
            )
                .into_val(&p.env),
            sub_invokes: &[],
        },
    }]);

    assert!(
        p.client
            .try_transfer(&s, &p.producer, &p.sorriso, &100, &true)
            .is_err()
    );
}

#[test]
fn series_are_isolated_from_each_other() {
    let p = setup();
    mint_cen01(&p);
    let other = String::from_str(&p.env, "CDA-OUTRO-0002");

    assert_eq!(p.client.balance(&other, &p.producer), 0);
    assert_eq!(
        p.client
            .try_transfer(&other, &p.producer, &p.sorriso, &1, &false),
        Err(Ok(contract_error(Error::SeriesNotFound)))
    );
}

#[test]
fn a_series_cannot_be_minted_twice() {
    let p = setup();
    mint_cen01(&p);

    assert_eq!(
        p.client.try_mint(
            &series(&p),
            &metadata(&p),
            &cen01_fees(),
            &p.admin,
            &tons(1)
        ),
        Err(Ok(contract_error(Error::SeriesAlreadyMinted)))
    );
}

#[test]
fn fees_above_five_percent_are_rejected() {
    let p = setup();
    p.env.mock_all_auths();
    let mut fees = cen01_fees();
    fees.burn_storage_bps = 460;

    assert_eq!(
        p.client
            .try_mint(&series(&p), &metadata(&p), &fees, &p.admin, &tons(1)),
        Err(Ok(contract_error(Error::FeeTooHigh)))
    );
}

#[test]
fn cannot_spend_more_than_the_balance() {
    let p = setup();
    mint_cen01(&p);

    assert_eq!(
        p.client
            .try_transfer(&series(&p), &p.sorriso, &p.cerrado, &1, &false),
        Err(Ok(contract_error(Error::InsufficientBalance)))
    );
}

#[test]
fn the_contract_initializes_once() {
    let p = setup();

    assert_eq!(
        p.client.try_initialize(
            &String::from_str(&p.env, "Again"),
            &String::from_str(&p.env, "AGN"),
            &p.admin,
            &p.platform
        ),
        Err(Ok(contract_error(Error::AlreadyInitialized)))
    );
}
