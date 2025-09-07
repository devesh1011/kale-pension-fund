#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, log, Address, Env, Map, Symbol, Vec,
    symbol_short,
};

// Storage keys
const ADMIN: Symbol = symbol_short!("ADMIN");
const ORACLE_CONFIG: Symbol = symbol_short!("ORA_CFG");
const PRICE_FEEDS: Symbol = symbol_short!("PR_FEEDS");
const LAST_UPDATE: Symbol = symbol_short!("LST_UPD");

#[derive(Clone)]
#[contracttype]
pub struct OracleConfig {
    pub reflector_usd_oracle: Address,
    pub reflector_stellar_oracle: Address,
    pub update_frequency: u64,         // seconds
    pub price_deviation_threshold: u32, // basis points
    pub max_price_age: u64,            // seconds
    pub decimals: u32,                 // price decimals (usually 14 for Reflector)
}

#[derive(Clone)]
#[contracttype]
pub struct PriceFeed {
    pub asset: Symbol,
    pub price_usd: i128,               // price in USD (scaled by decimals)
    pub price_xlm: Option<i128>,       // price in XLM (if available)
    pub timestamp: u64,
    pub confidence: u32,               // confidence score 0-10000
    pub source: Symbol,                // data source identifier
}

#[derive(Clone)]
#[contracttype]
pub struct PriceUpdate {
    pub asset: Symbol,
    pub old_price: i128,
    pub new_price: i128,
    pub price_change: i128,            // absolute change
    pub price_change_percent: i32,     // percentage change (basis points, can be negative)
    pub timestamp: u64,
}

#[derive(Clone)]
#[contracttype]
pub struct AggregatedPrices {
    pub kale_usd: i128,
    pub xlm_usd: i128,
    pub btc_usd: i128,
    pub usdc_usd: i128,
    pub last_updated: u64,
    pub data_freshness: u64,           // seconds since last update
}

#[contract]
pub struct ReflectorAdapterContract;

#[contractimpl]
impl ReflectorAdapterContract {
    
    /// Initialize the Reflector adapter contract
    pub fn initialize(
        env: Env,
        admin: Address,
        reflector_usd_oracle: Address,
        reflector_stellar_oracle: Address,
        update_frequency: u64,
        price_deviation_threshold: u32,
        max_price_age: u64,
        decimals: u32,
    ) {
        admin.require_auth();
        
        let config = OracleConfig {
            reflector_usd_oracle,
            reflector_stellar_oracle,
            update_frequency,
            price_deviation_threshold,
            max_price_age,
            decimals,
        };
        
        env.storage().instance().set(&ADMIN, &admin);
        env.storage().instance().set(&ORACLE_CONFIG, &config);
        env.storage().instance().set(&LAST_UPDATE, &0u64);
        
        log!(
            &env,
            "ReflectorAdapter initialized: admin={}, usd_oracle={}, update_freq={}",
            admin,
            reflector_usd_oracle,
            update_frequency
        );
    }
    
    /// Fetch latest prices from Reflector oracles
    pub fn update_prices(env: Env, caller: Address) -> Vec<PriceUpdate> {
        caller.require_auth();
        
        let config: OracleConfig = env.storage().instance().get(&ORACLE_CONFIG).unwrap();
        let current_time = env.ledger().timestamp();
        let last_update: u64 = env.storage().instance().get(&LAST_UPDATE).unwrap_or(0);
        
        // Check if enough time has passed since last update
        if current_time < last_update + config.update_frequency {
            panic!("Update frequency not met");
        }
        
        let mut price_updates = Vec::new(&env);
        
        // Fetch prices for each supported asset
        let assets = vec![
            symbol_short!("KALE"),
            symbol_short!("XLM"),
            symbol_short!("BTC"),
            symbol_short!("USDC"),
        ];
        
        for asset in assets.iter() {
            if let Some(update) = Self::fetch_asset_price(&env, &config, asset.clone()) {
                price_updates.push_back(update);
            }
        }
        
        // Update last update timestamp
        env.storage().instance().set(&LAST_UPDATE, &current_time);
        
        log!(
            &env,
            "Prices updated: {} assets, timestamp={}",
            price_updates.len(),
            current_time
        );
        
        price_updates
    }
    
    /// Get current price for a specific asset
    pub fn get_price(env: Env, asset: Symbol) -> Option<PriceFeed> {
        env.storage().persistent().get(&asset)
    }
    
    /// Get aggregated prices for all supported assets
    pub fn get_all_prices(env: Env) -> AggregatedPrices {
        let current_time = env.ledger().timestamp();
        
        let kale_price = Self::get_price(env.clone(), symbol_short!("KALE"))
            .map(|feed| feed.price_usd)
            .unwrap_or(0);
            
        let xlm_price = Self::get_price(env.clone(), symbol_short!("XLM"))
            .map(|feed| feed.price_usd)
            .unwrap_or(0);
            
        let btc_price = Self::get_price(env.clone(), symbol_short!("BTC"))
            .map(|feed| feed.price_usd)
            .unwrap_or(0);
            
        let usdc_price = Self::get_price(env.clone(), symbol_short!("USDC"))
            .map(|feed| feed.price_usd)
            .unwrap_or(10000000); // Default to $1.00
        
        let last_updated = env.storage().instance().get(&LAST_UPDATE).unwrap_or(0);
        let data_freshness = if current_time > last_updated { 
            current_time - last_updated 
        } else { 
            0 
        };
        
        AggregatedPrices {
            kale_usd: kale_price,
            xlm_usd: xlm_price,
            btc_usd: btc_price,
            usdc_usd: usdc_price,
            last_updated,
            data_freshness,
        }
    }
    
    /// Calculate price impact for a trade
    pub fn calculate_price_impact(
        env: Env,
        asset: Symbol,
        trade_amount: i128,
        total_liquidity: i128,
    ) -> u32 {
        // Simple price impact calculation
        // Impact = (trade_amount / total_liquidity) * 10000 (in basis points)
        if total_liquidity == 0 {
            return 10000; // 100% impact if no liquidity
        }
        
        let impact = (trade_amount * 10000) / total_liquidity;
        
        // Cap impact at 100%
        if impact > 10000 {
            10000
        } else {
            impact as u32
        }
    }
    
    /// Validate price freshness
    pub fn is_price_fresh(env: Env, asset: Symbol) -> bool {
        let config: OracleConfig = env.storage().instance().get(&ORACLE_CONFIG).unwrap();
        let current_time = env.ledger().timestamp();
        
        if let Some(price_feed) = Self::get_price(env, asset) {
            current_time - price_feed.timestamp <= config.max_price_age
        } else {
            false
        }
    }
    
    /// Get price with staleness check
    pub fn get_fresh_price(env: Env, asset: Symbol) -> Option<PriceFeed> {
        if Self::is_price_fresh(env.clone(), asset.clone()) {
            Self::get_price(env, asset)
        } else {
            None
        }
    }
    
    /// Calculate TWAP (Time Weighted Average Price) for an asset
    pub fn calculate_twap(
        env: Env,
        asset: Symbol,
        time_window: u64, // seconds
    ) -> Option<i128> {
        // In a real implementation, this would calculate TWAP from historical data
        // For now, return current price as a placeholder
        Self::get_price(env, asset).map(|feed| feed.price_usd)
    }
    
    /// Update oracle configuration (admin only)
    pub fn update_config(
        env: Env,
        caller: Address,
        config: OracleConfig,
    ) {
        let admin: Address = env.storage().instance().get(&ADMIN).unwrap();
        if caller != admin {
            panic!("Unauthorized");
        }
        caller.require_auth();
        
        env.storage().instance().set(&ORACLE_CONFIG, &config);
        
        log!(&env, "Oracle config updated by admin: {}", caller);
    }
    
    /// Get oracle configuration
    pub fn get_config(env: Env) -> OracleConfig {
        env.storage().instance().get(&ORACLE_CONFIG).unwrap()
    }
    
    /// Emergency price override (admin only)
    pub fn emergency_price_override(
        env: Env,
        caller: Address,
        asset: Symbol,
        price: i128,
        reason: Symbol,
    ) {
        let admin: Address = env.storage().instance().get(&ADMIN).unwrap();
        if caller != admin {
            panic!("Unauthorized");
        }
        caller.require_auth();
        
        let current_time = env.ledger().timestamp();
        let emergency_feed = PriceFeed {
            asset: asset.clone(),
            price_usd: price,
            price_xlm: None,
            timestamp: current_time,
            confidence: 5000, // Medium confidence for emergency override
            source: symbol_short!("EMERGENCY"),
        };
        
        env.storage().persistent().set(&asset, &emergency_feed);
        
        log!(
            &env,
            "Emergency price override: asset={}, price={}, reason={}",
            asset,
            price,
            reason
        );
    }
    
    // Internal helper functions
    
    fn fetch_asset_price(
        env: &Env,
        config: &OracleConfig,
        asset: Symbol,
    ) -> Option<PriceUpdate> {
        // In a real implementation, this would call the Reflector oracle contracts
        // For now, we'll simulate price fetching with mock data
        
        let current_time = env.ledger().timestamp();
        let old_price_feed: Option<PriceFeed> = env.storage().persistent().get(&asset);
        
        // Mock price data (in a real implementation, this would come from Reflector)
        let new_price = match asset {
            s if s == symbol_short!("KALE") => 100000000i128,    // $10.00
            s if s == symbol_short!("XLM") => 11000000i128,      // $0.11
            s if s == symbol_short!("BTC") => 430000000000i128,  // $43,000.00
            s if s == symbol_short!("USDC") => 10000000i128,     // $1.00
            _ => return None,
        };
        
        let old_price = old_price_feed.as_ref().map(|f| f.price_usd).unwrap_or(new_price);
        
        // Calculate price change
        let price_change = new_price - old_price;
        let price_change_percent = if old_price != 0 {
            ((price_change * 10000) / old_price) as i32
        } else {
            0
        };
        
        // Create new price feed
        let new_feed = PriceFeed {
            asset: asset.clone(),
            price_usd: new_price,
            price_xlm: None,
            timestamp: current_time,
            confidence: 9500, // High confidence
            source: symbol_short!("REFLECTOR"),
        };
        
        // Store new price feed
        env.storage().persistent().set(&asset, &new_feed);
        
        Some(PriceUpdate {
            asset,
            old_price,
            new_price,
            price_change,
            price_change_percent,
            timestamp: current_time,
        })
    }
}
