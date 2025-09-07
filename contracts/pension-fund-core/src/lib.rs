#![no_std]

mod test;

use soroban_sdk::{
    contract, contractimpl, contracttype, log, token, Address, Env, Map, String, Symbol, Vec,
    symbol_short,
};
use soroban_token_sdk::TokenClient;

// Storage keys
const ADMIN: Symbol = symbol_short!("ADMIN");
const USER_DATA: Symbol = symbol_short!("USER_DATA");
const FUND_CONFIG: Symbol = symbol_short!("FUND_CFG");
const TOTAL_LOCKED: Symbol = symbol_short!("TOT_LOCK");

#[derive(Clone)]
#[contracttype]
pub struct UserAccount {
    pub balance: i128,
    pub risk_profile: RiskProfile,
    pub locked_until: u64,
    pub last_deposit: u64,
    pub total_deposits: i128,
    pub total_withdrawals: i128,
    pub rewards_earned: i128,
    pub referral_code: String,
}

#[derive(Clone)]
#[contracttype]
pub enum RiskProfile {
    Conservative = 1,
    Moderate = 2,
    Aggressive = 3,
}

#[derive(Clone)]
#[contracttype]
pub struct FundConfig {
    pub kale_token: Address,
    pub min_deposit: i128,
    pub max_deposit: i128,
    pub lock_period: u64,
    pub withdrawal_fee: u32, // basis points (100 = 1%)
    pub performance_fee: u32, // basis points
    pub early_withdrawal_penalty: u32, // basis points
    pub referral_bonus: u32, // basis points
}

#[derive(Clone)]
#[contracttype]
pub struct DepositResult {
    pub user: Address,
    pub amount: i128,
    pub new_balance: i128,
    pub lock_until: u64,
    pub referral_bonus: i128,
}

#[derive(Clone)]
#[contracttype]
pub struct WithdrawalResult {
    pub user: Address,
    pub amount: i128,
    pub fee: i128,
    pub penalty: i128,
    pub net_amount: i128,
    pub new_balance: i128,
}

#[contract]
pub struct PensionFundContract;

#[contractimpl]
impl PensionFundContract {
    
    /// Initialize the pension fund contract
    pub fn initialize(
        env: Env,
        admin: Address,
        kale_token: Address,
        min_deposit: i128,
        max_deposit: i128,
        lock_period: u64,
        withdrawal_fee: u32,
        performance_fee: u32,
        early_withdrawal_penalty: u32,
        referral_bonus: u32,
    ) {
        admin.require_auth();
        
        let config = FundConfig {
            kale_token: kale_token.clone(),
            min_deposit,
            max_deposit,
            lock_period,
            withdrawal_fee,
            performance_fee,
            early_withdrawal_penalty,
            referral_bonus,
        };
        
        env.storage().instance().set(&ADMIN, &admin);
        env.storage().instance().set(&FUND_CONFIG, &config);
        env.storage().instance().set(&TOTAL_LOCKED, &0i128);
        
        log!(
            &env,
            "PensionFund initialized: admin={}, kale_token={}, min_deposit={}, lock_period={}",
            admin,
            kale_token,
            min_deposit,
            lock_period
        );
    }
    
    /// Deposit KALE tokens into the pension fund
    pub fn deposit(
        env: Env,
        user: Address,
        amount: i128,
        risk_profile: RiskProfile,
        referral: Option<Address>,
    ) -> DepositResult {
        user.require_auth();
        
        let config: FundConfig = env.storage().instance().get(&FUND_CONFIG).unwrap();
        
        // Validate deposit amount
        if amount < config.min_deposit || amount > config.max_deposit {
            panic!("Invalid deposit amount");
        }
        
        // Transfer KALE tokens from user to contract
        let token_client = TokenClient::new(&env, &config.kale_token);
        token_client.transfer(&user, &env.current_contract_address(), &amount);
        
        // Calculate referral bonus
        let mut referral_bonus = 0i128;
        if let Some(ref_addr) = referral {
            referral_bonus = (amount * config.referral_bonus as i128) / 10000;
            if referral_bonus > 0 {
                token_client.transfer(&env.current_contract_address(), &ref_addr, &referral_bonus);
            }
        }
        
        // Get or create user account
        let mut user_account = Self::get_user_account(&env, &user);
        let current_time = env.ledger().timestamp();
        
        // Update user account
        user_account.balance += amount;
        user_account.risk_profile = risk_profile;
        user_account.locked_until = current_time + config.lock_period;
        user_account.last_deposit = current_time;
        user_account.total_deposits += amount;
        
        // Store updated account
        env.storage().persistent().set(&user, &user_account);
        
        // Update total locked value
        let mut total_locked: i128 = env.storage().instance().get(&TOTAL_LOCKED).unwrap_or(0);
        total_locked += amount;
        env.storage().instance().set(&TOTAL_LOCKED, &total_locked);
        
        log!(
            &env,
            "Deposit: user={}, amount={}, new_balance={}, lock_until={}",
            user,
            amount,
            user_account.balance,
            user_account.locked_until
        );
        
        DepositResult {
            user: user.clone(),
            amount,
            new_balance: user_account.balance,
            lock_until: user_account.locked_until,
            referral_bonus,
        }
    }
    
    /// Withdraw KALE tokens from the pension fund
    pub fn withdraw(env: Env, user: Address, amount: i128) -> WithdrawalResult {
        user.require_auth();
        
        let config: FundConfig = env.storage().instance().get(&FUND_CONFIG).unwrap();
        let mut user_account = Self::get_user_account(&env, &user);
        
        if user_account.balance < amount {
            panic!("Insufficient balance");
        }
        
        let current_time = env.ledger().timestamp();
        let mut fee = 0i128;
        let mut penalty = 0i128;
        
        // Calculate withdrawal fee
        fee = (amount * config.withdrawal_fee as i128) / 10000;
        
        // Calculate early withdrawal penalty if still locked
        if current_time < user_account.locked_until {
            penalty = (amount * config.early_withdrawal_penalty as i128) / 10000;
        }
        
        let net_amount = amount - fee - penalty;
        
        // Update user account
        user_account.balance -= amount;
        user_account.total_withdrawals += amount;
        
        // Store updated account
        env.storage().persistent().set(&user, &user_account);
        
        // Update total locked value
        let mut total_locked: i128 = env.storage().instance().get(&TOTAL_LOCKED).unwrap_or(0);
        total_locked -= amount;
        env.storage().instance().set(&TOTAL_LOCKED, &total_locked);
        
        // Transfer tokens back to user
        let token_client = TokenClient::new(&env, &config.kale_token);
        token_client.transfer(&env.current_contract_address(), &user, &net_amount);
        
        log!(
            &env,
            "Withdrawal: user={}, amount={}, fee={}, penalty={}, net_amount={}",
            user,
            amount,
            fee,
            penalty,
            net_amount
        );
        
        WithdrawalResult {
            user: user.clone(),
            amount,
            fee,
            penalty,
            net_amount,
            new_balance: user_account.balance,
        }
    }
    
    /// Get user account information
    pub fn get_account(env: Env, user: Address) -> UserAccount {
        Self::get_user_account(&env, &user)
    }
    
    /// Get fund configuration
    pub fn get_config(env: Env) -> FundConfig {
        env.storage().instance().get(&FUND_CONFIG).unwrap()
    }
    
    /// Get total value locked in the fund
    pub fn get_total_locked(env: Env) -> i128 {
        env.storage().instance().get(&TOTAL_LOCKED).unwrap_or(0)
    }
    
    /// Update fund configuration (admin only)
    pub fn update_config(
        env: Env,
        caller: Address,
        min_deposit: Option<i128>,
        max_deposit: Option<i128>,
        withdrawal_fee: Option<u32>,
        performance_fee: Option<u32>,
        early_withdrawal_penalty: Option<u32>,
    ) {
        let admin: Address = env.storage().instance().get(&ADMIN).unwrap();
        if caller != admin {
            panic!("Unauthorized");
        }
        caller.require_auth();
        
        let mut config: FundConfig = env.storage().instance().get(&FUND_CONFIG).unwrap();
        
        if let Some(min_dep) = min_deposit {
            config.min_deposit = min_dep;
        }
        if let Some(max_dep) = max_deposit {
            config.max_deposit = max_dep;
        }
        if let Some(w_fee) = withdrawal_fee {
            config.withdrawal_fee = w_fee;
        }
        if let Some(p_fee) = performance_fee {
            config.performance_fee = p_fee;
        }
        if let Some(penalty) = early_withdrawal_penalty {
            config.early_withdrawal_penalty = penalty;
        }
        
        env.storage().instance().set(&FUND_CONFIG, &config);
        
        log!(&env, "Fund config updated by admin: {}", caller);
    }
    
    /// Distribute rewards to users (admin only)
    pub fn distribute_rewards(env: Env, caller: Address, total_rewards: i128) {
        let admin: Address = env.storage().instance().get(&ADMIN).unwrap();
        if caller != admin {
            panic!("Unauthorized");
        }
        caller.require_auth();
        
        let total_locked: i128 = env.storage().instance().get(&TOTAL_LOCKED).unwrap_or(0);
        if total_locked == 0 {
            return;
        }
        
        // Rewards distribution logic would be implemented here
        // This would iterate through all users and distribute proportional rewards
        
        log!(&env, "Rewards distributed: total={}", total_rewards);
    }
    
    /// Internal helper to get user account
    fn get_user_account(env: &Env, user: &Address) -> UserAccount {
        env.storage().persistent().get(user).unwrap_or(UserAccount {
            balance: 0,
            risk_profile: RiskProfile::Conservative,
            locked_until: 0,
            last_deposit: 0,
            total_deposits: 0,
            total_withdrawals: 0,
            rewards_earned: 0,
            referral_code: String::from_str(env, ""),
        })
    }
}
