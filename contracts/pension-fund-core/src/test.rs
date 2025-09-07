#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env, String};

#[test]
fn test_initialize_contract() {
    let env = Env::default();
    let contract_id = env.register_contract(None, PensionFundContract);
    let client = PensionFundContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let kale_token = Address::generate(&env);
    
    client.initialize(
        &admin,
        &kale_token,
        &1000000, // min_deposit: 1 KALE
        &10000000000, // max_deposit: 10,000 KALE
        &2592000, // lock_period: 30 days
        &100, // withdrawal_fee: 1%
        &200, // performance_fee: 2%
        &500, // early_withdrawal_penalty: 5%
        &50, // referral_bonus: 0.5%
    );
    
    let config = client.get_config();
    assert_eq!(config.kale_token, kale_token);
    assert_eq!(config.min_deposit, 1000000);
    assert_eq!(config.lock_period, 2592000);
}

#[test]
fn test_deposit_success() {
    let env = Env::default();
    let contract_id = env.register_contract(None, PensionFundContract);
    let client = PensionFundContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let kale_token = Address::generate(&env);
    
    // Initialize contract
    client.initialize(
        &admin,
        &kale_token,
        &1000000,
        &10000000000,
        &2592000,
        &100,
        &200,
        &500,
        &50,
    );
    
    // Mock token contract for testing
    env.mock_all_auths();
    
    let deposit_amount = 5000000; // 5 KALE
    let result = client.deposit(
        &user,
        &deposit_amount,
        &RiskProfile::Moderate,
        &None::<Address>,
    );
    
    assert_eq!(result.amount, deposit_amount);
    assert_eq!(result.new_balance, deposit_amount);
    assert_eq!(result.referral_bonus, 0);
    
    // Check user account
    let account = client.get_account(&user);
    assert_eq!(account.balance, deposit_amount);
    assert_eq!(account.total_deposits, deposit_amount);
}

#[test]
fn test_deposit_with_referral() {
    let env = Env::default();
    let contract_id = env.register_contract(None, PensionFundContract);
    let client = PensionFundContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let referrer = Address::generate(&env);
    let kale_token = Address::generate(&env);
    
    client.initialize(
        &admin,
        &kale_token,
        &1000000,
        &10000000000,
        &2592000,
        &100,
        &200,
        &500,
        &50, // 0.5% referral bonus
    );
    
    env.mock_all_auths();
    
    let deposit_amount = 10000000; // 10 KALE
    let expected_bonus = (deposit_amount * 50) / 10000; // 0.5%
    
    let result = client.deposit(
        &user,
        &deposit_amount,
        &RiskProfile::Aggressive,
        &Some(referrer),
    );
    
    assert_eq!(result.referral_bonus, expected_bonus);
}

#[test]
#[should_panic(expected = "Invalid deposit amount")]
fn test_deposit_below_minimum() {
    let env = Env::default();
    let contract_id = env.register_contract(None, PensionFundContract);
    let client = PensionFundContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let kale_token = Address::generate(&env);
    
    client.initialize(
        &admin,
        &kale_token,
        &1000000, // min_deposit: 1 KALE
        &10000000000,
        &2592000,
        &100,
        &200,
        &500,
        &50,
    );
    
    env.mock_all_auths();
    
    // Try to deposit below minimum
    client.deposit(
        &user,
        &500000, // 0.5 KALE (below minimum)
        &RiskProfile::Conservative,
        &None::<Address>,
    );
}

#[test]
fn test_withdraw_success() {
    let env = Env::default();
    let contract_id = env.register_contract(None, PensionFundContract);
    let client = PensionFundContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let kale_token = Address::generate(&env);
    
    client.initialize(
        &admin,
        &kale_token,
        &1000000,
        &10000000000,
        &2592000,
        &100, // 1% withdrawal fee
        &200,
        &500, // 5% early withdrawal penalty
        &50,
    );
    
    env.mock_all_auths();
    
    // First deposit
    let deposit_amount = 10000000; // 10 KALE
    client.deposit(
        &user,
        &deposit_amount,
        &RiskProfile::Moderate,
        &None::<Address>,
    );
    
    // Fast forward time past lock period
    env.ledger().with_mut(|ledger| {
        ledger.timestamp = 2592001; // 30 days + 1 second
    });
    
    // Withdraw after lock period (no penalty)
    let withdraw_amount = 5000000; // 5 KALE
    let expected_fee = (withdraw_amount * 100) / 10000; // 1%
    let expected_net = withdraw_amount - expected_fee;
    
    let result = client.withdraw(&user, &withdraw_amount);
    
    assert_eq!(result.amount, withdraw_amount);
    assert_eq!(result.fee, expected_fee);
    assert_eq!(result.penalty, 0); // No penalty after lock period
    assert_eq!(result.net_amount, expected_net);
    assert_eq!(result.new_balance, deposit_amount - withdraw_amount);
}

#[test]
fn test_early_withdrawal_penalty() {
    let env = Env::default();
    let contract_id = env.register_contract(None, PensionFundContract);
    let client = PensionFundContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let kale_token = Address::generate(&env);
    
    client.initialize(
        &admin,
        &kale_token,
        &1000000,
        &10000000000,
        &2592000,
        &100, // 1% withdrawal fee
        &200,
        &500, // 5% early withdrawal penalty
        &50,
    );
    
    env.mock_all_auths();
    
    // Deposit
    let deposit_amount = 10000000; // 10 KALE
    client.deposit(
        &user,
        &deposit_amount,
        &RiskProfile::Moderate,
        &None::<Address>,
    );
    
    // Withdraw immediately (early withdrawal)
    let withdraw_amount = 5000000; // 5 KALE
    let expected_fee = (withdraw_amount * 100) / 10000; // 1%
    let expected_penalty = (withdraw_amount * 500) / 10000; // 5%
    let expected_net = withdraw_amount - expected_fee - expected_penalty;
    
    let result = client.withdraw(&user, &withdraw_amount);
    
    assert_eq!(result.fee, expected_fee);
    assert_eq!(result.penalty, expected_penalty);
    assert_eq!(result.net_amount, expected_net);
}

#[test]
#[should_panic(expected = "Insufficient balance")]
fn test_withdraw_insufficient_balance() {
    let env = Env::default();
    let contract_id = env.register_contract(None, PensionFundContract);
    let client = PensionFundContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let kale_token = Address::generate(&env);
    
    client.initialize(
        &admin,
        &kale_token,
        &1000000,
        &10000000000,
        &2592000,
        &100,
        &200,
        &500,
        &50,
    );
    
    env.mock_all_auths();
    
    // Try to withdraw without deposit
    client.withdraw(&user, &1000000);
}

#[test]
fn test_update_config() {
    let env = Env::default();
    let contract_id = env.register_contract(None, PensionFundContract);
    let client = PensionFundContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let kale_token = Address::generate(&env);
    
    client.initialize(
        &admin,
        &kale_token,
        &1000000,
        &10000000000,
        &2592000,
        &100,
        &200,
        &500,
        &50,
    );
    
    env.mock_all_auths();
    
    // Update configuration
    client.update_config(
        &admin,
        &Some(2000000), // new min_deposit
        &Some(20000000000), // new max_deposit
        &Some(150), // new withdrawal_fee
        &None::<u32>, // keep performance_fee
        &Some(600), // new early_withdrawal_penalty
    );
    
    let updated_config = client.get_config();
    assert_eq!(updated_config.min_deposit, 2000000);
    assert_eq!(updated_config.max_deposit, 20000000000);
    assert_eq!(updated_config.withdrawal_fee, 150);
    assert_eq!(updated_config.performance_fee, 200); // unchanged
    assert_eq!(updated_config.early_withdrawal_penalty, 600);
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_update_config_unauthorized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, PensionFundContract);
    let client = PensionFundContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let unauthorized_user = Address::generate(&env);
    let kale_token = Address::generate(&env);
    
    client.initialize(
        &admin,
        &kale_token,
        &1000000,
        &10000000000,
        &2592000,
        &100,
        &200,
        &500,
        &50,
    );
    
    env.mock_all_auths();
    
    // Try to update config with unauthorized user
    client.update_config(
        &unauthorized_user,
        &Some(2000000),
        &None::<i128>,
        &None::<u32>,
        &None::<u32>,
        &None::<u32>,
    );
}

#[test]
fn test_total_locked_tracking() {
    let env = Env::default();
    let contract_id = env.register_contract(None, PensionFundContract);
    let client = PensionFundContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let kale_token = Address::generate(&env);
    
    client.initialize(
        &admin,
        &kale_token,
        &1000000,
        &10000000000,
        &2592000,
        &100,
        &200,
        &500,
        &50,
    );
    
    env.mock_all_auths();
    
    // Initial total should be 0
    assert_eq!(client.get_total_locked(), 0);
    
    // User1 deposits
    let deposit1 = 5000000; // 5 KALE
    client.deposit(
        &user1,
        &deposit1,
        &RiskProfile::Conservative,
        &None::<Address>,
    );
    assert_eq!(client.get_total_locked(), deposit1);
    
    // User2 deposits
    let deposit2 = 8000000; // 8 KALE
    client.deposit(
        &user2,
        &deposit2,
        &RiskProfile::Aggressive,
        &None::<Address>,
    );
    assert_eq!(client.get_total_locked(), deposit1 + deposit2);
    
    // User1 withdraws partially
    let withdraw1 = 2000000; // 2 KALE
    client.withdraw(&user1, &withdraw1);
    assert_eq!(client.get_total_locked(), deposit1 + deposit2 - withdraw1);
}
