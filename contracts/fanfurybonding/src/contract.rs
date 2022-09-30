#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult, Uint128, from_binary,
    WasmMsg, WasmQuery, QueryRequest, Order, Addr, CosmosMsg, QuerierWrapper, Storage
};
use cw2::{get_contract_version, set_contract_version};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg, Denom};
use cw_storage_plus::Bound;
use cw_utils::{maybe_addr};
use crate::error::ContractError;
use crate::msg::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg, BondStateResponse, BondingRecord, AllBondStateResponse
};

use crate::state::{
    Config, CONFIG, BONDING
};
use cw20::Balance;
use crate::util;
use crate::util::{NORMAL_DECIMAL, THOUSAND};
use wasmswap::msg::{QueryMsg as WasmswapQueryMsg, Token1ForToken2PriceResponse, Token2ForToken1PriceResponse};
// Version info, for migration info
const CONTRACT_NAME: &str = "fanfurybonding";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let config = Config {
        owner: msg.owner,
        pool_address: msg.pool_address,
        treasury_address: msg.treasury_address,
        fury_token_address: msg.fury_token_address,
        lock_seconds: msg.lock_seconds,
        discount: msg.discount,
        usdc_denom: msg.usdc_denom,
        is_native_bonding: msg.is_native_bonding,
        tx_fee: msg.tx_fee,
        platform_fee: msg.platform_fee,
        enabled: true,
        daily_vesting_amount: msg.daily_vesting_amount,
        cumulated_amount: Uint128::zero(),
        daily_current_bond_amount: Uint128::zero(),
        last_timestamp: env.block.time.seconds()
    };
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::UpdateOwner{owner} => execute_update_owner(deps, env, info, owner),
        ExecuteMsg::UpdateEnabled{enabled} => execute_update_enabled(deps, env, info, enabled),
        ExecuteMsg::UpdateConfig{treasury_address, lock_seconds, discount, tx_fee, platform_fee, daily_vesting_amount} => execute_update_config(deps, env, info, treasury_address, lock_seconds, discount, tx_fee, platform_fee, daily_vesting_amount),
        ExecuteMsg::Bond { amount } => execute_bond(deps, env, info, amount),
        ExecuteMsg::LpBond {address, amount} => execute_lp_bond(deps, env, info, address, amount),
        ExecuteMsg::Unbond { } => execute_unbond(deps, env, info),
        ExecuteMsg::Withdraw { amount } => execute_withdraw(deps, env, info, amount)
    }
}

pub fn check_enabled(
    storage: &mut dyn Storage,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(storage)?;
    if !cfg.enabled {
        return Err(ContractError::Disabled {})
    }
    Ok(Response::new().add_attribute("action", "check_enabled"))
}

pub fn check_owner(
    storage: &mut dyn Storage,
    address: Addr
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(storage)?;
    if cfg.owner != address {
        return Err(ContractError::Disabled {})
    }
    Ok(Response::new().add_attribute("action", "check_owner"))
}

pub fn execute_update_owner(
    deps: DepsMut, 
    env: Env,
    info: MessageInfo, 
    owner: Addr
) -> Result<Response, ContractError> {
    
    check_owner(deps.storage, info.sender.clone())?;

    let mut cfg = CONFIG.load(deps.storage)?;
    
    CONFIG.save(deps.storage, &cfg)?;

    return Ok(Response::new()
        .add_attributes(vec![
            attr("action", "update_owner"),
            attr("owner", owner),
        ]));
}


pub fn execute_update_enabled(
    deps: DepsMut, 
    env: Env,
    info: MessageInfo, 
    enabled: bool
) -> Result<Response, ContractError> {
    
    check_owner(deps.storage, info.sender.clone())?;

    let mut cfg = CONFIG.load(deps.storage)?;

    cfg.enabled = enabled;
    CONFIG.save(deps.storage, &cfg)?;

    return Ok(Response::new()
        .add_attributes(vec![
            attr("action", "update_enabled"),
            attr("enabled", enabled.to_string()),
        ]));
}


pub fn execute_update_config(
    deps: DepsMut, 
    env: Env,
    info: MessageInfo, 
    treasury_address: Addr,
    lock_seconds: u64,
    discount: u64,
    tx_fee: u64,
    platform_fee: u64,
    daily_vesting_amount: Uint128
) -> Result<Response, ContractError> {
    check_owner(deps.storage, info.sender.clone())?;

    let mut cfg = CONFIG.load(deps.storage)?;
    cfg.treasury_address = treasury_address.clone();
    cfg.lock_seconds = lock_seconds;
    cfg.discount = discount;
    cfg.tx_fee = tx_fee;
    cfg.platform_fee = platform_fee;
    if cfg.daily_vesting_amount != daily_vesting_amount {
        cfg.daily_vesting_amount = daily_vesting_amount;
        //cfg.cumulated_amount = Uint128::zero();
    }
    
    CONFIG.save(deps.storage, &cfg)?;

    return Ok(Response::new()
        .add_attributes(vec![
            attr("action", "update_config"),
            attr("treasury_address", treasury_address.clone()),
            attr("lock_seconds", lock_seconds.to_string()),
            attr("discount", discount.to_string()),
            attr("tx_fee", tx_fee.to_string()),
            attr("platform_fee", platform_fee.to_string()),
            attr("daily_vesting_amount", daily_vesting_amount.to_string()),
        ]));
}


pub fn check_daily_vesting_amount(
    storage: &mut dyn Storage,
    timestamp: u64,
    receiving_amount: Uint128
) -> Result<Response, ContractError> {
    let mut cfg = CONFIG.load(storage)?;

    if timestamp / 86400 != cfg.last_timestamp / 86400 {
        // This is new day.
        if cfg.daily_current_bond_amount <= cfg.daily_vesting_amount {
            cfg.cumulated_amount += cfg.daily_vesting_amount - cfg.daily_current_bond_amount;
        }
        cfg.daily_current_bond_amount = Uint128::zero();
    }

    if cfg.daily_current_bond_amount + receiving_amount > cfg.daily_vesting_amount + cfg.cumulated_amount {
        return Err(ContractError::MaxBondingExceed {  })
    }
    cfg.last_timestamp = timestamp;
    cfg.daily_current_bond_amount += receiving_amount;
    CONFIG.save(storage, &cfg)?;
    
    Ok(Response::new().add_attribute("action", "check_daily_vesting_amount"))
}


pub fn execute_bond(
    deps: DepsMut, 
    env: Env,
    info: MessageInfo,
    amount: Uint128
) -> Result<Response, ContractError> {
    
    check_enabled(deps.storage)?;

    let cfg = CONFIG.load(deps.storage)?;

    if !cfg.is_native_bonding {
        return Err(ContractError::NotAllowedBondingType {  })
    }

    let balance = Balance::from(info.funds);
    let usdc_amount = util::get_amount_of_denom(balance, Denom::Native(cfg.usdc_denom.clone()))?;

    if usdc_amount == Uint128::zero() {
        return Err(ContractError::NativeInputZero {  })
    }
    let fee_amount = amount * Uint128::from(cfg.platform_fee + cfg.tx_fee) / Uint128::from(THOUSAND);

    if usdc_amount < fee_amount + amount {
        return Err(ContractError::InsufficientFee {  })
    }

    let token2_price_response: Token1ForToken2PriceResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: cfg.pool_address.clone().into(),
        msg: to_binary(&WasmswapQueryMsg::Token1ForToken2Price {
            token1_amount: amount
        })?,
    }))?;

    let receiving_amount = token2_price_response.token2_amount * Uint128::from(THOUSAND) / Uint128::from(THOUSAND - cfg.discount);

    check_daily_vesting_amount(deps.storage, env.block.time.seconds(), receiving_amount)?;

    let mut messages:Vec<CosmosMsg> = vec![];
    messages.push(util::transfer_token_message(Denom::Native(cfg.usdc_denom.clone()), usdc_amount, cfg.treasury_address.clone())?);

    let mut list:Vec<BondingRecord> = BONDING.load(deps.storage, info.sender.clone()).unwrap_or(vec![]);
    list.push(BondingRecord {
        amount: receiving_amount,
        timestamp: env.block.time.seconds() + cfg.lock_seconds
    });
    BONDING.save(deps.storage, info.sender.clone(), &list)?;
    
    
    return Ok(Response::new()
        .add_messages(messages)
        .add_attributes(vec![
            attr("action", "bond"),
            attr("bond_usdc_amount", amount),
            attr("receiving_fury_amount", receiving_amount),
            attr("address", info.sender.clone()),
        ]));
}


pub fn execute_lp_bond(
    deps: DepsMut, 
    env: Env,
    info: MessageInfo,
    address: Addr,
    amount: Uint128
) -> Result<Response, ContractError> {
    
    check_enabled(deps.storage)?;

    let cfg = CONFIG.load(deps.storage)?;

    if info.sender.clone() != cfg.pool_address {
        return Err(ContractError::Unauthorized {  });
    }

    if amount == Uint128::zero() {
        return Err(ContractError::Cw20InputZero {  })
    }

    if cfg.is_native_bonding {
        return Err(ContractError::NotAllowedBondingType {  })
    }
    
    // On lp bonding, the platform fee and tx fee is already stolen from swap contract
    let receiving_amount = amount * Uint128::from(THOUSAND) / Uint128::from(THOUSAND - cfg.discount);
    
    check_daily_vesting_amount(deps.storage, env.block.time.seconds(), receiving_amount)?;

    let mut list:Vec<BondingRecord> = BONDING.load(deps.storage, address.clone()).unwrap_or(vec![]);
    list.push(BondingRecord {
        amount: receiving_amount,
        timestamp: env.block.time.seconds() + cfg.lock_seconds
    });
    BONDING.save(deps.storage, address.clone(), &list)?;
    
    return Ok(Response::new()
        .add_attributes(vec![
            attr("action", "lp_bond"),
            attr("bond_fury_amount", amount),
            attr("receiving_amount", receiving_amount),
            attr("address", address.clone()),
        ]));
}
        
pub fn execute_unbond(
    deps: DepsMut, 
    env: Env,
    info: MessageInfo
) -> Result<Response, ContractError> {
    
    check_enabled(deps.storage)?;


    let cfg = CONFIG.load(deps.storage)?;

    let list = BONDING.load(deps.storage, info.sender.clone())?;

    let bond_state = get_bond_state(
        cfg.clone(), 
        list.clone(),
        get_usdc_price(cfg.clone(), deps.querier)?,
        env.block.time.seconds(), 
        info.sender.clone()
    )?;

    if bond_state.unbond_amount == Uint128::zero() {
        return Err(ContractError::NothingToUnbond {})
    }

    let mut new_list: Vec<BondingRecord> = vec![];
    for item in list {
        if item.timestamp > env.block.time.seconds() {
            new_list.push(item);
        }
    }
    BONDING.save(deps.storage, info.sender.clone(), &new_list)?;    
    
    let balance = Balance::from(info.funds);
    //calculate tx fee
    let usdc_amount = util::get_amount_of_denom(balance, Denom::Native(cfg.usdc_denom.clone()))?;

    if usdc_amount < bond_state.fee_amount {
        return Err(ContractError::InsufficientFee { })
    }

    let fury_balance = util::get_token_amount(deps.querier, Denom::Cw20(cfg.fury_token_address.clone()), env.contract.address.clone())?;
    if fury_balance < bond_state.unbond_amount {
        return Err(ContractError::InsufficientFury {})
    }

    let mut messages:Vec<CosmosMsg> = vec![];
    messages.push(util::transfer_token_message(Denom::Cw20(cfg.fury_token_address.clone()), bond_state.unbond_amount, info.sender.clone())?);
    messages.push(util::transfer_token_message(Denom::Native(cfg.usdc_denom.clone()), usdc_amount, cfg.treasury_address.clone())?);
    
    return Ok(Response::new()
        .add_messages(messages)
        .add_attributes(vec![
            attr("action", "unbond"),
            attr("receiving_amount", bond_state.unbond_amount),
            attr("address", info.sender.clone()),
        ]));
}

pub fn execute_withdraw(
    deps: DepsMut, 
    env: Env,
    info: MessageInfo,
    amount: Uint128
) -> Result<Response, ContractError> {
    
    check_owner(deps.storage, info.sender.clone())?;

    let cfg = CONFIG.load(deps.storage)?;

    let fury_balance = util::get_token_amount(deps.querier, Denom::Cw20(cfg.fury_token_address.clone()), env.contract.address.clone())?;
    if fury_balance < amount {
        return Err(ContractError::InsufficientFury {})
    }

    let mut messages:Vec<CosmosMsg> = vec![];
    messages.push(util::transfer_token_message(Denom::Cw20(cfg.fury_token_address.clone()), amount, info.sender.clone())?);

    return Ok(Response::new()
        .add_messages(messages)
        .add_attributes(vec![
            attr("action", "withdraw"),
            attr("receiving_amount", amount),
            attr("address", info.sender.clone()),
        ]));
}


    
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} 
            => to_binary(&query_config(deps)?),
        QueryMsg::BondState {address} 
            => to_binary(&query_bond_state(deps, env, address)?),
        QueryMsg::AllBondState {start_after, limit} 
            => to_binary(&query_all_bond_state(deps, env, start_after, limit)?),
    }
}

pub fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let cfg = CONFIG.load(deps.storage)?;
    Ok(ConfigResponse {
        owner: cfg.owner,
        pool_address: cfg.pool_address,
        treasury_address: cfg.treasury_address,
        fury_token_address: cfg.fury_token_address,
        lock_seconds: cfg.lock_seconds,
        discount: cfg.discount,
        usdc_denom: cfg.usdc_denom,
        is_native_bonding: cfg.is_native_bonding,
        tx_fee: cfg.tx_fee,
        platform_fee: cfg.platform_fee,
        enabled: cfg.enabled,
        daily_current_bond_amount: cfg.daily_current_bond_amount,
        cumulated_amount: cfg.cumulated_amount,
        daily_vesting_amount: cfg.daily_vesting_amount,
        last_timestamp: cfg.last_timestamp
    })
}

pub fn get_bond_state(
    cfg: Config,
    list: Vec<BondingRecord>,
    usdc_price: Uint128,
    current_timestamp: u64,
    address: Addr
) -> StdResult<BondStateResponse> {

    let mut unbond_amount = Uint128::zero();
    let mut fee_amount = Uint128::zero();

    for item in list.clone() {
        if item.timestamp > current_timestamp {
            continue;
        }
        unbond_amount += item.amount
    }
    
    if unbond_amount > Uint128::zero() {
        fee_amount = usdc_price * unbond_amount * Uint128::from(cfg.platform_fee + cfg.tx_fee) / Uint128::from(THOUSAND) / Uint128::from(NORMAL_DECIMAL);
    }

    Ok(BondStateResponse {
        address,
        list,
        unbond_amount,
        fee_amount
    })
}

pub fn get_usdc_price(
    cfg: Config,
    querier: QuerierWrapper
) -> StdResult<Uint128> {
    let usdc_price_response: Token2ForToken1PriceResponse = querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: cfg.pool_address.clone().into(),
        msg: to_binary(&WasmswapQueryMsg::Token2ForToken1Price {
            token2_amount: Uint128::from(NORMAL_DECIMAL)
        })?,
    }))?;
    Ok(usdc_price_response.token1_amount)
}

pub fn query_bond_state(deps: Deps, env: Env, address: Addr) -> StdResult<BondStateResponse> {
    let cfg = CONFIG.load(deps.storage)?;

    get_bond_state(
        cfg.clone(), 
        BONDING.load(deps.storage, address.clone()).unwrap_or(vec![]),
        get_usdc_price(cfg.clone(), deps.querier)?,
        env.block.time.seconds(), 
        address.clone()
    )
    
}

// settings for pagination
const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

fn map_bonding(
    cfg: Config,
    current_timestamp: u64,
    usdc_price: Uint128,
    item: StdResult<(Addr, Vec<BondingRecord>)>,
) -> StdResult<BondStateResponse> {
    item.map(|(address, list)| {

        get_bond_state(
            cfg, 
            list,
            usdc_price,
            current_timestamp, 
            address.clone()
        ).unwrap()
        
    })
}


fn query_all_bond_state(
    deps: Deps,
    env: Env,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<AllBondStateResponse> {
    let cfg = CONFIG.load(deps.storage).unwrap();

    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let addr = maybe_addr(deps.api, start_after)?;
    let start = addr.map(|addr| Bound::exclusive(addr));

    let list:StdResult<Vec<_>> = BONDING
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| map_bonding(cfg.clone(), env.block.time.seconds(), get_usdc_price(cfg.clone(), deps.querier)?, item))
        .collect();

    Ok(AllBondStateResponse { list: list? })
}


#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    let version = get_contract_version(deps.storage)?;
    if version.contract != CONTRACT_NAME {
        return Err(ContractError::CannotMigrate {
            previous_contract: version.contract,
        });
    }
    Ok(Response::default())
}

