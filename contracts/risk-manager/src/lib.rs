#![no_std]

mod test;

use soroban_sdk::{
    contract, contractimpl, contracttype, log, Address, Env, Map, Symbol, Vec,
    symbol_short,
};

// Storage keys
const ADMIN: Symbol = symbol_short!("ADMIN");
const RISK_PARAMS: Symbol = symbol_short!("RISK_PRM");
const ASSET_WEIGHTS: Symbol = symbol_short!("AS_WGHT");
const VOLATILITY_DATA: Symbol = symbol_short!("VOL_DATA");

#[derive(Clone)]
#[contracttype]
pub enum RiskProfile {
    Conservative = 1,
    Moderate = 2,
    Aggressive = 3,
}

#[derive(Clone)]
#[contracttype]
pub struct AssetAllocation {
    pub kale_percentage: u32,    // basis points (10000 = 100%)
    pub btc_percentage: u32,     // basis points
    pub usdc_percentage: u32,    // basis points
    pub xlm_percentage: u32,     // basis points
}

#[derive(Clone)]
#[contracttype]
pub struct RiskParameters {
    pub max_position_size: u32,        // basis points
    pub max_daily_volatility: u32,     // basis points
    pub correlation_threshold: u32,     // basis points
    pub stress_test_threshold: u32,    // basis points
    pub rebalance_threshold: u32,      // basis points
}

#[derive(Clone)]
#[contracttype]
pub struct VolatilityData {
    pub asset: Symbol,
    pub daily_volatility: u32,    // basis points
    pub weekly_volatility: u32,   // basis points
    pub monthly_volatility: u32,  // basis points
    pub last_updated: u64,
}

#[derive(Clone)]
#[contracttype]
pub struct RiskAssessment {
    pub profile: RiskProfile,
    pub recommended_allocation: AssetAllocation,
    pub risk_score: u32,           // 0-10000 (100.00%)
    pub volatility_score: u32,     // 0-10000 (100.00%)
    pub correlation_risk: u32,     // 0-10000 (100.00%)
    pub liquidity_risk: u32,       // 0-10000 (100.00%)
}

#[contract]
pub struct RiskManagerContract;

#[contractimpl]
impl RiskManagerContract {
    
    /// Initialize the risk manager contract
    pub fn initialize(
        env: Env,
        admin: Address,
        max_position_size: u32,
        max_daily_volatility: u32,
        correlation_threshold: u32,
        stress_test_threshold: u32,
        rebalance_threshold: u32,
    ) {
        admin.require_auth();
        
        let risk_params = RiskParameters {
            max_position_size,
            max_daily_volatility,
            correlation_threshold,
            stress_test_threshold,
            rebalance_threshold,
        };
        
        env.storage().instance().set(&ADMIN, &admin);
        env.storage().instance().set(&RISK_PARAMS, &risk_params);
        
        // Initialize default asset allocations for each risk profile
        Self::set_default_allocations(&env);
        
        log!(
            &env,
            "RiskManager initialized: admin={}, max_position_size={}, max_volatility={}",
            admin,
            max_position_size,
            max_daily_volatility
        );
    }
    
    /// Get recommended asset allocation for a risk profile
    pub fn get_allocation(env: Env, profile: RiskProfile) -> AssetAllocation {
        let key = match profile {
            RiskProfile::Conservative => symbol_short!("CONS_ALL"),
            RiskProfile::Moderate => symbol_short!("MOD_ALL"),
            RiskProfile::Aggressive => symbol_short!("AGG_ALL"),
        };
        
        env.storage().persistent().get(&key).unwrap_or_else(|| {
            // Return default allocation if not found
            match profile {
                RiskProfile::Conservative => AssetAllocation {
                    kale_percentage: 2000,   // 20%
                    btc_percentage: 3000,    // 30%
                    usdc_percentage: 4000,   // 40%
                    xlm_percentage: 1000,    // 10%
                },
                RiskProfile::Moderate => AssetAllocation {
                    kale_percentage: 3500,   // 35%
                    btc_percentage: 4000,    // 40%
                    usdc_percentage: 2000,   // 20%
                    xlm_percentage: 500,     // 5%
                },
                RiskProfile::Aggressive => AssetAllocation {
                    kale_percentage: 5000,   // 50%
                    btc_percentage: 3500,    // 35%
                    usdc_percentage: 1000,   // 10%
                    xlm_percentage: 500,     // 5%
                },
            }
        })
    }
    
    /// Perform comprehensive risk assessment
    pub fn assess_risk(
        env: Env,
        profile: RiskProfile,
        current_allocation: AssetAllocation,
        market_conditions: Vec<VolatilityData>,
    ) -> RiskAssessment {
        let recommended_allocation = Self::get_allocation(env.clone(), profile.clone());
        let risk_params: RiskParameters = env.storage().instance().get(&RISK_PARAMS).unwrap();
        
        // Calculate risk score based on deviation from recommended allocation
        let allocation_risk = Self::calculate_allocation_risk(
            &current_allocation,
            &recommended_allocation,
        );
        
        // Calculate volatility score from market data
        let volatility_score = Self::calculate_volatility_score(&market_conditions);
        
        // Calculate correlation risk
        let correlation_risk = Self::calculate_correlation_risk(&market_conditions);
        
        // Calculate liquidity risk
        let liquidity_risk = Self::calculate_liquidity_risk(&current_allocation);
        
        // Overall risk score (weighted average)
        let risk_score = (allocation_risk * 30 + volatility_score * 40 + correlation_risk * 20 + liquidity_risk * 10) / 100;
        
        log!(
            &env,
            "Risk assessment: profile={:?}, risk_score={}, volatility={}, correlation={}",
            profile,
            risk_score,
            volatility_score,
            correlation_risk
        );
        
        RiskAssessment {
            profile,
            recommended_allocation,
            risk_score,
            volatility_score,
            correlation_risk,
            liquidity_risk,
        }
    }
    
    /// Update asset allocation for a risk profile (admin only)
    pub fn update_allocation(
        env: Env,
        caller: Address,
        profile: RiskProfile,
        allocation: AssetAllocation,
    ) {
        let admin: Address = env.storage().instance().get(&ADMIN).unwrap();
        if caller != admin {
            panic!("Unauthorized");
        }
        caller.require_auth();
        
        // Validate allocation percentages sum to 100%
        let total = allocation.kale_percentage + allocation.btc_percentage + 
                   allocation.usdc_percentage + allocation.xlm_percentage;
        if total != 10000 {
            panic!("Allocation percentages must sum to 100%");
        }
        
        let key = match profile {
            RiskProfile::Conservative => symbol_short!("CONS_ALL"),
            RiskProfile::Moderate => symbol_short!("MOD_ALL"),
            RiskProfile::Aggressive => symbol_short!("AGG_ALL"),
        };
        
        env.storage().persistent().set(&key, &allocation);
        
        log!(
            &env,
            "Allocation updated: profile={:?}, kale={}, btc={}, usdc={}, xlm={}",
            profile,
            allocation.kale_percentage,
            allocation.btc_percentage,
            allocation.usdc_percentage,
            allocation.xlm_percentage
        );
    }
    
    /// Update volatility data for assets
    pub fn update_volatility(
        env: Env,
        caller: Address,
        volatility_data: Vec<VolatilityData>,
    ) {
        let admin: Address = env.storage().instance().get(&ADMIN).unwrap();
        if caller != admin {
            panic!("Unauthorized");
        }
        caller.require_auth();
        
        for data in volatility_data.iter() {
            env.storage().persistent().set(&data.asset, &data);
        }
        
        log!(&env, "Volatility data updated for {} assets", volatility_data.len());
    }
    
    /// Check if rebalancing is needed based on current allocation
    pub fn should_rebalance(
        env: Env,
        profile: RiskProfile,
        current_allocation: AssetAllocation,
    ) -> bool {
        let recommended = Self::get_allocation(env.clone(), profile);
        let risk_params: RiskParameters = env.storage().instance().get(&RISK_PARAMS).unwrap();
        
        // Check if any asset allocation deviates beyond threshold
        let kale_deviation = Self::abs_diff(current_allocation.kale_percentage, recommended.kale_percentage);
        let btc_deviation = Self::abs_diff(current_allocation.btc_percentage, recommended.btc_percentage);
        let usdc_deviation = Self::abs_diff(current_allocation.usdc_percentage, recommended.usdc_percentage);
        let xlm_deviation = Self::abs_diff(current_allocation.xlm_percentage, recommended.xlm_percentage);
        
        let max_deviation = kale_deviation.max(btc_deviation).max(usdc_deviation).max(xlm_deviation);
        
        max_deviation > risk_params.rebalance_threshold
    }
    
    /// Get current risk parameters
    pub fn get_risk_parameters(env: Env) -> RiskParameters {
        env.storage().instance().get(&RISK_PARAMS).unwrap()
    }
    
    /// Update risk parameters (admin only)
    pub fn update_risk_parameters(
        env: Env,
        caller: Address,
        risk_params: RiskParameters,
    ) {
        let admin: Address = env.storage().instance().get(&ADMIN).unwrap();
        if caller != admin {
            panic!("Unauthorized");
        }
        caller.require_auth();
        
        env.storage().instance().set(&RISK_PARAMS, &risk_params);
        
        log!(&env, "Risk parameters updated by admin: {}", caller);
    }
    
    // Internal helper functions
    
    fn set_default_allocations(env: &Env) {
        let conservative = AssetAllocation {
            kale_percentage: 2000,   // 20%
            btc_percentage: 3000,    // 30%
            usdc_percentage: 4000,   // 40%
            xlm_percentage: 1000,    // 10%
        };
        
        let moderate = AssetAllocation {
            kale_percentage: 3500,   // 35%
            btc_percentage: 4000,    // 40%
            usdc_percentage: 2000,   // 20%
            xlm_percentage: 500,     // 5%
        };
        
        let aggressive = AssetAllocation {
            kale_percentage: 5000,   // 50%
            btc_percentage: 3500,    // 35%
            usdc_percentage: 1000,   // 10%
            xlm_percentage: 500,     // 5%
        };
        
        env.storage().persistent().set(&symbol_short!("CONS_ALL"), &conservative);
        env.storage().persistent().set(&symbol_short!("MOD_ALL"), &moderate);
        env.storage().persistent().set(&symbol_short!("AGG_ALL"), &aggressive);
    }
    
    fn calculate_allocation_risk(
        current: &AssetAllocation,
        recommended: &AssetAllocation,
    ) -> u32 {
        let kale_diff = Self::abs_diff(current.kale_percentage, recommended.kale_percentage);
        let btc_diff = Self::abs_diff(current.btc_percentage, recommended.btc_percentage);
        let usdc_diff = Self::abs_diff(current.usdc_percentage, recommended.usdc_percentage);
        let xlm_diff = Self::abs_diff(current.xlm_percentage, recommended.xlm_percentage);
        
        // Return average deviation as risk score
        (kale_diff + btc_diff + usdc_diff + xlm_diff) / 4
    }
    
    fn calculate_volatility_score(market_conditions: &Vec<VolatilityData>) -> u32 {
        if market_conditions.is_empty() {
            return 5000; // Medium risk if no data
        }
        
        let mut total_volatility = 0u32;
        for data in market_conditions.iter() {
            total_volatility += data.daily_volatility;
        }
        
        total_volatility / market_conditions.len() as u32
    }
    
    fn calculate_correlation_risk(_market_conditions: &Vec<VolatilityData>) -> u32 {
        // Simplified correlation risk calculation
        // In a real implementation, this would analyze asset correlations
        3000 // 30% risk score as placeholder
    }
    
    fn calculate_liquidity_risk(allocation: &AssetAllocation) -> u32 {
        // Higher USDC allocation = lower liquidity risk
        // Higher KALE allocation = higher liquidity risk
        let stable_allocation = allocation.usdc_percentage;
        let volatile_allocation = allocation.kale_percentage;
        
        // Risk score decreases with stable allocations
        if stable_allocation > 5000 {
            1000 // Low risk
        } else if volatile_allocation > 5000 {
            8000 // High risk
        } else {
            4000 // Medium risk
        }
    }
    
    fn abs_diff(a: u32, b: u32) -> u32 {
        if a > b { a - b } else { b - a }
    }
}
