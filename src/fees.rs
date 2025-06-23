use crate::balance::{read_balance, receive_balance, spend_balance};
use crate::config::{Config, read_config};
use soroban_sdk::{Address, Env};

pub struct FeeCalculation {
    pub mediator_fee: i128,
    pub buyer_fee: i128,
    pub net_amount: i128,
}

impl FeeCalculation {
    pub fn calculate(amount: i128, config: &Config) -> Self {
        // Calculate fees in basis points (10000 = 100%)
        let mediator_fee = (amount * config.mediator_fee as i128) / 10000;
        let buyer_fee = (amount * config.buyer_fee as i128) / 10000;
        let total_fees = mediator_fee + buyer_fee;
        let net_amount = amount - total_fees;

        Self {
            mediator_fee,
            buyer_fee,
            net_amount,
        }
    }
}

pub fn process_fees_and_transfer(
    e: &Env,
    from: &Address,
    to: &Address,
    amount: i128,
) -> Result<(), &'static str> {
    let config = read_config(e).expect("Config must be initialized");
    let fee_calc = FeeCalculation::calculate(amount, &config);

    // Check if sender has enough balance for amount + fees
    let current_balance = read_balance(e, from.clone());
    if current_balance < amount {
        return Err("Insufficient balance for amount and fees");
    }

    // Deduct full amount from sender
    spend_balance(e, from.clone(), amount);

    // Transfer net amount to recipient
    receive_balance(e, to.clone(), fee_calc.net_amount);

    // Transfer fees to mediator and buyer
    if fee_calc.mediator_fee > 0 {
        receive_balance(e, config.mediator_address, fee_calc.mediator_fee);
    }
    if fee_calc.buyer_fee > 0 {
        receive_balance(e, config.buyer_address, fee_calc.buyer_fee);
    }

    Ok(())
}

pub fn process_fees_and_burn(e: &Env, from: &Address, amount: i128) -> Result<(), &'static str> {
    let config = read_config(e).expect("Config must be initialized");
    let fee_calc = FeeCalculation::calculate(amount, &config);

    // Check if sender has enough balance for amount + fees
    let current_balance = read_balance(e, from.clone());
    if current_balance < amount {
        return Err("Insufficient balance for amount and fees");
    }

    // Deduct full amount from sender (this burns the net amount)
    spend_balance(e, from.clone(), amount);

    // Transfer fees to mediator and buyer (they don't get burned)
    if fee_calc.mediator_fee > 0 {
        receive_balance(e, config.mediator_address, fee_calc.mediator_fee);
    }
    if fee_calc.buyer_fee > 0 {
        receive_balance(e, config.buyer_address, fee_calc.buyer_fee);
    }

    Ok(())
}
