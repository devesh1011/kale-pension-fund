#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, log, token, Address, Env, Map, Symbol, Vec,
    symbol_short,
};
use soroban_token_sdk::TokenClient;

// Storage keys
const ADMIN: Symbol = symbol_short!("ADMIN");
const REBAL_CONFIG: Symbol = symbol_short!("REB_CFG");
const ASSET_POOLS: Symbol = symbol_short!("AS_POOLS");
const LAST_REBALANCE: Symbol = symbol_short!("LST_REB");

#[derive(Clone)]
#[contracttype]
pub struct RebalanceConfig {
    pub min_rebalance_amount: i128,
    pub max_slippage: u32,           // basis points
    pub rebalance_frequency: u64,    // seconds
    pub gas_limit: u32,
    pub max_trades_per_rebalance: u32,
}

#[derive(Clone)]
#[contracttype]
pub struct AssetPool {
    pub asset_address: Address,
    pub current_balance: i128,
    pub target_percentage: u32,      // basis points
    pub last_price: i128,            // price in USD (scaled by 1e7)
    pub liquidity_score: u32,        // 0-10000
}

#[derive(Clone)]
#[contracttype]
pub struct RebalanceOrder {
    pub from_asset: Address,
    pub to_asset: Address,
    pub amount: i128,
    pub min_received: i128,
    pub max_slippage: u32,
    pub priority: u32,               // 1-10 (10 = highest)
}

#[derive(Clone)]
#[contracttype]
pub struct RebalanceResult {
    pub total_value_before: i128,
    pub total_value_after: i128,
    pub orders_executed: u32,
    pub gas_used: u32,
    pub slippage_incurred: u32,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contracttype]
pub struct PortfolioSnapshot {
    pub total_value_usd: i128,
    pub kale_balance: i128,
    pub btc_balance: i128,
    pub usdc_balance: i128,
    pub xlm_balance: i128,
    pub kale_percentage: u32,
    pub btc_percentage: u32,
    pub usdc_percentage: u32,
    pub xlm_percentage: u32,
}

#[contract]
pub struct RebalancerContract;

#[contractimpl]
impl RebalancerContract {
    
    /// Initialize the rebalancer contract
    pub fn initialize(
        env: Env,
        admin: Address,
        min_rebalance_amount: i128,
        max_slippage: u32,
        rebalance_frequency: u64,
        gas_limit: u32,
        max_trades_per_rebalance: u32,
    ) {
        admin.require_auth();
        
        let config = RebalanceConfig {
            min_rebalance_amount,
            max_slippage,
            rebalance_frequency,
            gas_limit,
            max_trades_per_rebalance,
        };
        
        env.storage().instance().set(&ADMIN, &admin);
        env.storage().instance().set(&REBAL_CONFIG, &config);
        env.storage().instance().set(&LAST_REBALANCE, &0u64);
        
        log!(
            &env,
            "Rebalancer initialized: admin={}, min_amount={}, max_slippage={}",
            admin,
            min_rebalance_amount,
            max_slippage
        );
    }
    
    /// Execute automatic rebalancing based on target allocations
    pub fn rebalance(
        env: Env,
        caller: Address,
        target_allocations: Map<Address, u32>, // asset -> percentage (basis points)
        current_prices: Map<Address, i128>,    // asset -> USD price
    ) -> RebalanceResult {
        caller.require_auth();
        
        let config: RebalanceConfig = env.storage().instance().get(&REBAL_CONFIG).unwrap();
        let current_time = env.ledger().timestamp();
        let last_rebalance: u64 = env.storage().instance().get(&LAST_REBALANCE).unwrap_or(0);
        
        // Check if enough time has passed since last rebalance
        if current_time < last_rebalance + config.rebalance_frequency {
            panic!("Rebalance frequency not met");
        }
        
        // Get current portfolio snapshot
        let portfolio = Self::get_portfolio_snapshot(&env, &current_prices);
        
        // Validate total allocation equals 100%
        let total_allocation: u32 = target_allocations.values().iter().sum();
        if total_allocation != 10000 {
            panic!("Target allocations must sum to 100%");
        }
        
        // Check if rebalancing is needed
        if !Self::needs_rebalancing(&env, &portfolio, &target_allocations) {
            log!(&env, "No rebalancing needed");
            return RebalanceResult {
                total_value_before: portfolio.total_value_usd,
                total_value_after: portfolio.total_value_usd,
                orders_executed: 0,
                gas_used: 0,
                slippage_incurred: 0,
                timestamp: current_time,
            };
        }
        
        // Generate rebalance orders
        let orders = Self::generate_rebalance_orders(
            &env,
            &portfolio,
            &target_allocations,
            &current_prices,
        );
        
        // Execute rebalance orders
        let result = Self::execute_rebalance_orders(&env, orders, &config);
        
        // Update last rebalance timestamp
        env.storage().instance().set(&LAST_REBALANCE, &current_time);
        
        log!(
            &env,
            "Rebalance completed: orders={}, gas_used={}, slippage={}",
            result.orders_executed,
            result.gas_used,
            result.slippage_incurred
        );
        
        result
    }
    
    /// Get current portfolio snapshot
    pub fn get_portfolio_snapshot(
        env: &Env,
        current_prices: &Map<Address, i128>,
    ) -> PortfolioSnapshot {
        // This would integrate with actual token balances
        // For now, we'll use mock data that would come from the pension fund contract
        
        let kale_balance = 1000000i128; // 1M KALE tokens
        let btc_balance = 50000000i128;  // 0.5 BTC (in stroops)
        let usdc_balance = 2000000000i128; // 2000 USDC (in stroops)
        let xlm_balance = 500000000i128;   // 500 XLM (in stroops)
        
        // Calculate USD values (prices should be in 1e7 scale)
        let kale_price = current_prices.get(symbol_short!("KALE")).unwrap_or(100000000); // $10
        let btc_price = current_prices.get(symbol_short!("BTC")).unwrap_or(430000000000); // $43,000
        let usdc_price = current_prices.get(symbol_short!("USDC")).unwrap_or(10000000); // $1
        let xlm_price = current_prices.get(symbol_short!("XLM")).unwrap_or(11000000); // $0.11
        
        let kale_value_usd = (kale_balance * kale_price) / 10000000;
        let btc_value_usd = (btc_balance * btc_price) / 10000000;
        let usdc_value_usd = (usdc_balance * usdc_price) / 10000000;
        let xlm_value_usd = (xlm_balance * xlm_price) / 10000000;
        
        let total_value_usd = kale_value_usd + btc_value_usd + usdc_value_usd + xlm_value_usd;
        
        // Calculate percentages
        let kale_percentage = if total_value_usd > 0 { (kale_value_usd * 10000) / total_value_usd } else { 0 } as u32;
        let btc_percentage = if total_value_usd > 0 { (btc_value_usd * 10000) / total_value_usd } else { 0 } as u32;
        let usdc_percentage = if total_value_usd > 0 { (usdc_value_usd * 10000) / total_value_usd } else { 0 } as u32;
        let xlm_percentage = if total_value_usd > 0 { (xlm_value_usd * 10000) / total_value_usd } else { 0 } as u32;
        
        PortfolioSnapshot {
            total_value_usd,
            kale_balance,
            btc_balance,
            usdc_balance,
            xlm_balance,
            kale_percentage,
            btc_percentage,
            usdc_percentage,
            xlm_percentage,
        }
    }
    
    /// Check if rebalancing is needed
    pub fn needs_rebalancing(
        env: &Env,
        portfolio: &PortfolioSnapshot,
        target_allocations: &Map<Address, u32>,
    ) -> bool {
        let config: RebalanceConfig = env.storage().instance().get(&REBAL_CONFIG).unwrap();
        
        // Check if portfolio value meets minimum threshold
        if portfolio.total_value_usd < config.min_rebalance_amount {
            return false;
        }
        
        // Check deviations from target allocations
        let kale_target = target_allocations.get(symbol_short!("KALE")).unwrap_or(0);
        let btc_target = target_allocations.get(symbol_short!("BTC")).unwrap_or(0);
        let usdc_target = target_allocations.get(symbol_short!("USDC")).unwrap_or(0);
        let xlm_target = target_allocations.get(symbol_short!("XLM")).unwrap_or(0);
        
        let kale_deviation = Self::abs_diff(portfolio.kale_percentage, kale_target);
        let btc_deviation = Self::abs_diff(portfolio.btc_percentage, btc_target);
        let usdc_deviation = Self::abs_diff(portfolio.usdc_percentage, usdc_target);
        let xlm_deviation = Self::abs_diff(portfolio.xlm_percentage, xlm_target);
        
        // Rebalance if any asset deviates more than 5% (500 basis points)
        let rebalance_threshold = 500u32;
        kale_deviation > rebalance_threshold ||
        btc_deviation > rebalance_threshold ||
        usdc_deviation > rebalance_threshold ||
        xlm_deviation > rebalance_threshold
    }
    
    /// Generate optimal rebalance orders
    pub fn generate_rebalance_orders(
        env: &Env,
        portfolio: &PortfolioSnapshot,
        target_allocations: &Map<Address, u32>,
        current_prices: &Map<Address, i128>,
    ) -> Vec<RebalanceOrder> {
        let mut orders = Vec::new(&env);
        
        // Calculate target values
        let kale_target = target_allocations.get(symbol_short!("KALE")).unwrap_or(0);
        let btc_target = target_allocations.get(symbol_short!("BTC")).unwrap_or(0);
        let usdc_target = target_allocations.get(symbol_short!("USDC")).unwrap_or(0);
        let xlm_target = target_allocations.get(symbol_short!("XLM")).unwrap_or(0);
        
        let kale_target_value = (portfolio.total_value_usd * kale_target as i128) / 10000;
        let btc_target_value = (portfolio.total_value_usd * btc_target as i128) / 10000;
        let usdc_target_value = (portfolio.total_value_usd * usdc_target as i128) / 10000;
        let xlm_target_value = (portfolio.total_value_usd * xlm_target as i128) / 10000;
        
        // Calculate current values
        let kale_price = current_prices.get(symbol_short!("KALE")).unwrap_or(100000000);
        let btc_price = current_prices.get(symbol_short!("BTC")).unwrap_or(430000000000);
        let usdc_price = current_prices.get(symbol_short!("USDC")).unwrap_or(10000000);
        let xlm_price = current_prices.get(symbol_short!("XLM")).unwrap_or(11000000);
        
        let kale_current_value = (portfolio.kale_balance * kale_price) / 10000000;
        let btc_current_value = (portfolio.btc_balance * btc_price) / 10000000;
        let usdc_current_value = (portfolio.usdc_balance * usdc_price) / 10000000;
        let xlm_current_value = (portfolio.xlm_balance * xlm_price) / 10000000;
        
        // Generate orders for assets that need to be sold (over-allocated)
        if kale_current_value > kale_target_value {
            let excess_value = kale_current_value - kale_target_value;
            let excess_tokens = (excess_value * 10000000) / kale_price;
            
            // For simplicity, sell excess KALE for USDC
            orders.push_back(RebalanceOrder {
                from_asset: Address::from_contract_data(&env, symbol_short!("KALE")),
                to_asset: Address::from_contract_data(&env, symbol_short!("USDC")),
                amount: excess_tokens,
                min_received: (excess_value * 9800) / 10000, // 2% slippage tolerance
                max_slippage: 200, // 2%
                priority: 5,
            });
        }
        
        // Similar logic would be implemented for other assets...
        
        orders
    }
    
    /// Execute rebalance orders
    pub fn execute_rebalance_orders(
        env: &Env,
        orders: Vec<RebalanceOrder>,
        config: &RebalanceConfig,
    ) -> RebalanceResult {
        let mut orders_executed = 0u32;
        let mut total_gas_used = 0u32;
        let mut total_slippage = 0u32;
        let start_time = env.ledger().timestamp();
        
        // Execute orders up to the maximum limit
        for (i, order) in orders.iter().enumerate() {
            if i >= config.max_trades_per_rebalance as usize {
                break;
            }
            
            // In a real implementation, this would interface with DEX contracts
            // For now, we simulate the execution
            let gas_used = Self::simulate_trade_execution(env, &order);
            let slippage = Self::calculate_actual_slippage(&order);
            
            total_gas_used += gas_used;
            total_slippage = total_slippage.max(slippage);
            orders_executed += 1;
            
            log!(
                &env,
                "Order executed: from={:?} to={:?} amount={}",
                order.from_asset,
                order.to_asset,
                order.amount
            );
        }
        
        RebalanceResult {
            total_value_before: 0, // Would be calculated from portfolio
            total_value_after: 0,  // Would be calculated after trades
            orders_executed,
            gas_used: total_gas_used,
            slippage_incurred: total_slippage,
            timestamp: start_time,
        }
    }
    
    /// Update rebalance configuration (admin only)
    pub fn update_config(
        env: Env,
        caller: Address,
        config: RebalanceConfig,
    ) {
        let admin: Address = env.storage().instance().get(&ADMIN).unwrap();
        if caller != admin {
            panic!("Unauthorized");
        }
        caller.require_auth();
        
        env.storage().instance().set(&REBAL_CONFIG, &config);
        
        log!(&env, "Rebalance config updated by admin: {}", caller);
    }
    
    /// Get rebalance configuration
    pub fn get_config(env: Env) -> RebalanceConfig {
        env.storage().instance().get(&REBAL_CONFIG).unwrap()
    }
    
    /// Get last rebalance timestamp
    pub fn get_last_rebalance(env: Env) -> u64 {
        env.storage().instance().get(&LAST_REBALANCE).unwrap_or(0)
    }
    
    // Internal helper functions
    
    fn abs_diff(a: u32, b: u32) -> u32 {
        if a > b { a - b } else { b - a }
    }
    
    fn simulate_trade_execution(_env: &Env, _order: &RebalanceOrder) -> u32 {
        // Simulate gas usage for trade execution
        50000 // Mock gas cost
    }
    
    fn calculate_actual_slippage(order: &RebalanceOrder) -> u32 {
        // Simulate actual slippage incurred
        // In real implementation, this would be calculated from actual trade results
        order.max_slippage / 2 // Assume half of max slippage
    }
}
