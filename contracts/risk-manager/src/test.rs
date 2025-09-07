#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_initialize_risk_manager() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RiskManagerContract);
    let client = RiskManagerContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    
    client.initialize(
        &admin,
        &3000, // max_position_size: 30%
        &1000, // max_daily_volatility: 10%
        &7000, // correlation_threshold: 70%
        &2000, // stress_test_threshold: 20%
        &500,  // rebalance_threshold: 5%
    );
    
    let params = client.get_risk_parameters();
    assert_eq!(params.max_position_size, 3000);
    assert_eq!(params.max_daily_volatility, 1000);
    assert_eq!(params.correlation_threshold, 7000);
}

#[test]
fn test_get_allocation_conservative() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RiskManagerContract);
    let client = RiskManagerContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    
    client.initialize(&admin, &3000, &1000, &7000, &2000, &500);
    
    let allocation = client.get_allocation(&RiskProfile::Conservative);
    
    // Conservative should have higher USDC allocation
    assert!(allocation.usdc_percentage >= 3000); // At least 30%
    assert!(allocation.kale_percentage <= 3000); // At most 30%
    
    // Total should equal 100%
    let total = allocation.kale_percentage + allocation.btc_percentage + 
                allocation.usdc_percentage + allocation.xlm_percentage;
    assert_eq!(total, 10000);
}

#[test]
fn test_get_allocation_aggressive() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RiskManagerContract);
    let client = RiskManagerContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    
    client.initialize(&admin, &3000, &1000, &7000, &2000, &500);
    
    let allocation = client.get_allocation(&RiskProfile::Aggressive);
    
    // Aggressive should have higher KALE allocation
    assert!(allocation.kale_percentage >= 4000); // At least 40%
    assert!(allocation.usdc_percentage <= 2000); // At most 20%
    
    // Total should equal 100%
    let total = allocation.kale_percentage + allocation.btc_percentage + 
                allocation.usdc_percentage + allocation.xlm_percentage;
    assert_eq!(total, 10000);
}

#[test]
fn test_should_rebalance() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RiskManagerContract);
    let client = RiskManagerContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    
    client.initialize(&admin, &3000, &1000, &7000, &2000, &500); // 5% threshold
    
    // Test allocation that's within threshold (no rebalancing needed)
    let current_allocation = AssetAllocation {
        kale_percentage: 4800, // Close to target of 5000 for aggressive
        btc_percentage: 3600,  // Close to target of 3500
        usdc_percentage: 1100, // Close to target of 1000
        xlm_percentage: 500,   // Exact target
    };
    
    let should_rebal = client.should_rebalance(&RiskProfile::Aggressive, &current_allocation);
    assert!(!should_rebal);
    
    // Test allocation that's outside threshold (rebalancing needed)
    let off_allocation = AssetAllocation {
        kale_percentage: 7000, // Way off target
        btc_percentage: 2000,
        usdc_percentage: 500,
        xlm_percentage: 500,
    };
    
    let should_rebal = client.should_rebalance(&RiskProfile::Aggressive, &off_allocation);
    assert!(should_rebal);
}

#[test]
fn test_update_allocation() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RiskManagerContract);
    let client = RiskManagerContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    
    client.initialize(&admin, &3000, &1000, &7000, &2000, &500);
    
    env.mock_all_auths();
    
    // Update conservative allocation
    let new_allocation = AssetAllocation {
        kale_percentage: 1500, // 15%
        btc_percentage: 2500,  // 25%
        usdc_percentage: 5000, // 50%
        xlm_percentage: 1000,  // 10%
    };
    
    client.update_allocation(&admin, &RiskProfile::Conservative, &new_allocation);
    
    let updated = client.get_allocation(&RiskProfile::Conservative);
    assert_eq!(updated.kale_percentage, 1500);
    assert_eq!(updated.usdc_percentage, 5000);
}

#[test]
#[should_panic(expected = "Allocation percentages must sum to 100%")]
fn test_update_allocation_invalid_total() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RiskManagerContract);
    let client = RiskManagerContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    
    client.initialize(&admin, &3000, &1000, &7000, &2000, &500);
    
    env.mock_all_auths();
    
    // Invalid allocation that doesn't sum to 100%
    let invalid_allocation = AssetAllocation {
        kale_percentage: 5000,
        btc_percentage: 3000,
        usdc_percentage: 1000,
        xlm_percentage: 500, // Total = 9500, not 10000
    };
    
    client.update_allocation(&admin, &RiskProfile::Conservative, &invalid_allocation);
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_update_allocation_unauthorized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RiskManagerContract);
    let client = RiskManagerContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let unauthorized = Address::generate(&env);
    
    client.initialize(&admin, &3000, &1000, &7000, &2000, &500);
    
    env.mock_all_auths();
    
    let allocation = AssetAllocation {
        kale_percentage: 2500,
        btc_percentage: 2500,
        usdc_percentage: 2500,
        xlm_percentage: 2500,
    };
    
    client.update_allocation(&unauthorized, &RiskProfile::Conservative, &allocation);
}
