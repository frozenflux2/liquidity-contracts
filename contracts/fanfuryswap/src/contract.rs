use cosmwasm_std::{
    attr, entry_point, to_binary, Addr, Binary, BlockInfo, Coin, CosmosMsg, Deps, DepsMut, Env,
    MessageInfo, Reply, Response, StdError, StdResult, SubMsg, Uint128, WasmMsg, BankMsg, from_binary, ReplyOn
};
use cw2::{get_contract_version, set_contract_version};
use cw20::Cw20ReceiveMsg;
use cw0::parse_reply_instantiate_data;
use cw20::Denom::Cw20;
use cw20::{Cw20ExecuteMsg, Denom, Expiration, MinterResponse};
use cw20_base::contract::query_balance;

use crate::error::ContractError;
use crate::msg::{
    ExecuteMsg, InfoResponse, InstantiateMsg, QueryMsg, Token1ForToken2PriceResponse,
    Token2ForToken1PriceResponse, TokenSelect, StakeReceiveMsg, ConfigResponse, MigrateMsg
};
use crate::state::{Token, LP_TOKEN, TOKEN1, TOKEN2, Config, CONFIG};
use crate::util::{NORMAL_DECIMAL, THOUSAND};
use crate::util;

// Version info for migration info
pub const CONTRACT_NAME: &str = "fanfuryswap";
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const INSTANTIATE_LP_TOKEN_REPLY_ID: u64 = 0;
const INSTANTIATE_BONDING_ID:u64 = 1;
use fanfurybonding::msg::{InstantiateMsg as BondingInstantiateMsg, ExecuteMsg as BondingExecuteMsg};


// Note, you can use StdResult in some functions where you do not
// make use of the custom errors
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let config = Config {
        owner: msg.owner.clone(),
        bonding_code_id: msg.bonding_code_id,
        bonding_contract_address: msg.owner.clone(),
        fury_token_address: msg.fury_token_address.clone(),
        treasury_address: msg.treasury_address,
        usdc_denom: msg.usdc_denom.clone(),
        tx_fee: msg.tx_fee,
        platform_fee: msg.platform_fee,
        lock_seconds: msg.lock_seconds,
        discount: msg.discount,
        daily_vesting_amount: msg.daily_vesting_amount
    };
    CONFIG.save(deps.storage, &config)?;

    let token1 = Token {
        reserve: Uint128::zero(),
        denom: Denom::Native(msg.usdc_denom.clone()),
    };

    TOKEN1.save(deps.storage, &token1)?;

    let token2 = Token {
        denom: Denom::Cw20(msg.fury_token_address),
        reserve: Uint128::zero(),
    };

    TOKEN2.save(deps.storage, &token2)?;


    let instantiate_lp_token_msg = WasmMsg::Instantiate {
        code_id: msg.lp_token_code_id,
        funds: vec![],
        admin: None,
        label: "lp_token".to_string(),
        msg: to_binary(&cw20_base::msg::InstantiateMsg {
            name: "FanFurySwap_Liquidity_Token".into(),
            symbol: "ffslpt".into(),
            decimals: 6,
            initial_balances: vec![],
            mint: Some(MinterResponse {
                minter: env.contract.address.into(),
                cap: None,
            }),
            marketing: None,
        })?,
    };

    let reply_msg =
        SubMsg::reply_on_success(instantiate_lp_token_msg, INSTANTIATE_LP_TOKEN_REPLY_ID);

    Ok(Response::new().add_submessage(reply_msg))
}

// And declare a custom Error variant for the ones where you will want to make use of it
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::UpdateConfig {
            owner,
            bonding_contract_address,
            treasury_address
        } => execute_update_config(info, deps, owner, bonding_contract_address, treasury_address),
        ExecuteMsg::AddLiquidity {
            token1_amount,
            min_liquidity,
            max_token2,
            fee_amount,
            expiration,
        } => execute_add_liquidity(
            deps,
            &info,
            env,
            min_liquidity,
            token1_amount,
            max_token2,
            fee_amount,
            expiration,
        ),
        ExecuteMsg::RemoveLiquidity {
            amount,
            min_token1,
            min_token2,
            expiration,
        } => execute_remove_liquidity(deps, info, env, amount, min_token1, min_token2, expiration),
        ExecuteMsg::Swap {
            input_token,
            input_amount,
            min_output,
            fee_amount,
            expiration,
            ..
        } => execute_swap(
            deps,
            &info,
            input_amount,
            env,
            input_token,
            &info.sender,
            min_output,
            fee_amount,
            expiration,
        ),
        ExecuteMsg::AddToken {
            input_token,
            amount
        } => execute_add_token(
            deps,
            env,
            &info,
            input_token,
            amount
        )
        
    }
}



pub fn execute_update_config(
    info: MessageInfo,
    deps: DepsMut,
    owner: Addr,
    bonding_contract_address: Addr,
    treasury_address: Addr
) -> Result<Response, ContractError> {
    let mut config: Config = CONFIG.load(deps.storage)?;

    if info.sender.clone() != config.owner {
        return Err(ContractError::Unauthorized {});
    };
    
    config.owner = owner;
    config.bonding_contract_address = bonding_contract_address;
    config.treasury_address = treasury_address;

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new()
        .add_attribute("action", "update_config")
        .add_attribute(
            "owner",
            config
                .owner
                .to_string(),
        )
        .add_attribute(
            "staking_address",
            config
                .bonding_code_id
                .to_string(),
        ))
}

fn check_expiration(
    expiration: &Option<Expiration>,
    block: &BlockInfo,
) -> Result<(), ContractError> {
    match expiration {
        Some(e) => {
            if e.is_expired(block) {
                return Err(ContractError::MsgExpirationError {});
            }
            Ok(())
        }
        None => Ok(()),
    }
}

fn get_lp_token_amount_to_mint(
    token1_amount: Uint128,
    liquidity_supply: Uint128,
    token1_reserve: Uint128,
) -> Result<Uint128, ContractError> {
    if liquidity_supply == Uint128::zero() {
        Ok(token1_amount)
    } else {
        Ok(token1_amount
            .checked_mul(liquidity_supply)
            .map_err(StdError::overflow)?
            .checked_div(token1_reserve)
            .map_err(StdError::divide_by_zero)?)
    }
}

fn get_token2_amount_required(
    max_token: Uint128,
    token1_amount: Uint128,
    liquidity_supply: Uint128,
    token2_reserve: Uint128,
    token1_reserve: Uint128,
) -> Result<Uint128, StdError> {
    if liquidity_supply == Uint128::zero() {
        Ok(max_token)
    } else {
        Ok(token1_amount
            .checked_mul(token2_reserve)
            .map_err(StdError::overflow)?
            .checked_div(token1_reserve)
            .map_err(StdError::divide_by_zero)?
            .checked_add(Uint128::new(1))
            .map_err(StdError::overflow)?)
    }
}

pub fn execute_add_liquidity(
    deps: DepsMut,
    info: &MessageInfo,
    env: Env,
    min_liquidity: Uint128,
    token1_amount: Uint128,
    max_token2: Uint128,
    fee_amount: Uint128,
    expiration: Option<Expiration>,
) -> Result<Response, ContractError> {
    check_expiration(&expiration, &env.block)?;

    let token1 = TOKEN1.load(deps.storage)?;
    let token2 = TOKEN2.load(deps.storage)?;
    let lp_token_addr = LP_TOKEN.load(deps.storage)?;

    // validate funds
    validate_input_amount(&info.funds, token1_amount + fee_amount, &token1.denom)?;
    validate_input_amount(&info.funds, max_token2, &token2.denom)?;

    let lp_token_supply = get_lp_token_supply(deps.as_ref(), &lp_token_addr)?;
    let liquidity_amount =
        get_lp_token_amount_to_mint(token1_amount, lp_token_supply, token1.reserve)?;

    let token2_amount = get_token2_amount_required(
        max_token2,
        token1_amount,
        lp_token_supply,
        token2.reserve,
        token1.reserve,
    )?;

    if liquidity_amount < min_liquidity {
        return Err(ContractError::MinLiquidityError {
            min_liquidity,
            liquidity_available: liquidity_amount,
        });
    }

    if token2_amount > max_token2 {
        return Err(ContractError::MaxTokenError {
            max_token: max_token2,
            tokens_required: token2_amount,
        });
    }

    // Generate cw20 transfer messages if necessary
    let mut transfer_msgs: Vec<CosmosMsg> = vec![];
    if let Cw20(addr) = token1.denom.clone() {
        transfer_msgs.push(get_cw20_transfer_from_msg(
            &info.sender,
            &env.contract.address,
            &addr,
            token1_amount,
        )?)
    }
    if let Cw20(addr) = token2.denom.clone() {
        transfer_msgs.push(get_cw20_transfer_from_msg(
            &info.sender,
            &env.contract.address,
            &addr,
            token2_amount,
        )?)
    }

    // Refund token 2 if is a native token and not all is spent
    if let Denom::Native(denom) = token2.denom.clone() {
        if token2_amount < max_token2 {
            transfer_msgs.push(get_bank_transfer_to_msg(
                &info.sender,
                &denom,
                max_token2 - token2_amount,
            ))
        }
    }

    TOKEN1.update(deps.storage, |mut token1| -> Result<_, ContractError> {
        token1.reserve += token1_amount;
        Ok(token1)
    })?;
    TOKEN2.update(deps.storage, |mut token2| -> Result<_, ContractError> {
        token2.reserve += token2_amount;
        Ok(token2)
    })?;

    let mut config = CONFIG.load(deps.storage)?;
    // let mint_msg = mint_lp_tokens(&info.sender, liquidity_amount, &lp_token_addr)?;
    let mint_msg = mint_lp_tokens(&config.owner, liquidity_amount, &lp_token_addr)?;

    // Send Fee
    // check if the fee is larger than required
    if fee_amount < token1_amount * Uint128::from(config.platform_fee + config.tx_fee) * Uint128::from(2u128) / Uint128::from(THOUSAND) {
        return Err(ContractError::InsufficientFee {  })
    }
    
    transfer_msgs.push(util::transfer_token_message(token1.clone().denom.clone(), fee_amount, config.treasury_address.clone())?);
    ///////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
    ///////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
    ///////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
    ///////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
    /// Bonding Part
    
    let mut bond_msgs:Vec<CosmosMsg> = vec![];
    // Just bond if the info.sender is not owner or treasury( the wallet that contains lp token)
    if info.sender.clone() != config.owner.clone() && info.sender.clone() != config.treasury_address.clone() {
    
        bond_msgs.push(WasmMsg::Execute {
            contract_addr: config.bonding_contract_address.into(),
            msg: to_binary(&BondingExecuteMsg::LpBond { 
                address: info.sender.clone(), 
                amount: token2_amount * Uint128::from(2u128) 
            })?,
            funds: vec![],
        }.into());
    }

    Ok(Response::new()
        .add_messages(transfer_msgs)
        .add_message(mint_msg)
        .add_messages(bond_msgs)
        .add_attributes(vec![
            attr("token1_amount", token1_amount),
            attr("token2_amount", token2_amount),
            attr("liquidity_received", liquidity_amount),
        ]))
}

fn get_lp_token_supply(deps: Deps, lp_token_addr: &Addr) -> StdResult<Uint128> {
    let resp: cw20::TokenInfoResponse = deps
        .querier
        .query_wasm_smart(lp_token_addr, &cw20_base::msg::QueryMsg::TokenInfo {})?;
    Ok(resp.total_supply)
}

fn mint_lp_tokens(
    recipient: &Addr,
    liquidity_amount: Uint128,
    lp_token_address: &Addr,
) -> StdResult<CosmosMsg> {
    let mint_msg = cw20_base::msg::ExecuteMsg::Mint {
        recipient: recipient.into(),
        amount: liquidity_amount,
    };
    Ok(WasmMsg::Execute {
        contract_addr: lp_token_address.to_string(),
        msg: to_binary(&mint_msg)?,
        funds: vec![],
    }
    .into())
}

fn get_token_balance(deps: Deps, contract: &Addr, addr: &Addr) -> StdResult<Uint128> {
    let resp: cw20::BalanceResponse = deps.querier.query_wasm_smart(
        contract,
        &cw20_base::msg::QueryMsg::Balance {
            address: addr.to_string(),
        },
    )?;
    Ok(resp.balance)
}

fn validate_input_amount(
    actual_funds: &[Coin],
    given_amount: Uint128,
    given_denom: &Denom,
) -> Result<(), ContractError> {
    match given_denom {
        Denom::Cw20(_) => Ok(()),
        Denom::Native(denom) => {
            let actual = get_amount_for_denom(actual_funds, denom);
            if actual.amount != given_amount {
                return Err(ContractError::InsufficientFunds {});
            }
            if &actual.denom != denom {
                return Err(ContractError::IncorrectNativeDenom {
                    provided: actual.denom,
                    required: denom.to_string(),
                });
            };
            Ok(())
        }
    }
}

fn get_cw20_transfer_from_msg(
    owner: &Addr,
    recipient: &Addr,
    token_addr: &Addr,
    token_amount: Uint128,
) -> StdResult<CosmosMsg> {
    // create transfer cw20 msg
    let transfer_cw20_msg = Cw20ExecuteMsg::TransferFrom {
        owner: owner.into(),
        recipient: recipient.into(),
        amount: token_amount,
    };
    let exec_cw20_transfer = WasmMsg::Execute {
        contract_addr: token_addr.into(),
        msg: to_binary(&transfer_cw20_msg)?,
        funds: vec![],
    };
    let cw20_transfer_cosmos_msg: CosmosMsg = exec_cw20_transfer.into();
    Ok(cw20_transfer_cosmos_msg)
}

fn get_cw20_increase_allowance_msg(
    token_addr: &Addr,
    spender: &Addr,
    amount: Uint128,
    expires: Option<Expiration>,
) -> StdResult<CosmosMsg> {
    // create transfer cw20 msg
    let increase_allowance_msg = Cw20ExecuteMsg::IncreaseAllowance {
        spender: spender.to_string(),
        amount,
        expires,
    };
    let exec_allowance = WasmMsg::Execute {
        contract_addr: token_addr.into(),
        msg: to_binary(&increase_allowance_msg)?,
        funds: vec![],
    };
    Ok(exec_allowance.into())
}

pub fn execute_remove_liquidity(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    amount: Uint128,
    min_token1: Uint128,
    min_token2: Uint128,
    expiration: Option<Expiration>,
) -> Result<Response, ContractError> {
    check_expiration(&expiration, &env.block)?;

    let lp_token_addr = LP_TOKEN.load(deps.storage)?;
    let balance = get_token_balance(deps.as_ref(), &lp_token_addr, &info.sender)?;
    let lp_token_supply = get_lp_token_supply(deps.as_ref(), &lp_token_addr)?;
    let token1 = TOKEN1.load(deps.storage)?;
    let token2 = TOKEN2.load(deps.storage)?;

    if amount > balance {
        return Err(ContractError::InsufficientLiquidityError {
            requested: amount,
            available: balance,
        });
    }

    let token1_amount = amount
        .checked_mul(token1.reserve)
        .map_err(StdError::overflow)?
        .checked_div(lp_token_supply)
        .map_err(StdError::divide_by_zero)?;
    if token1_amount < min_token1 {
        return Err(ContractError::MinToken1Error {
            requested: min_token1,
            available: token1_amount,
        });
    }

    let token2_amount = amount
        .checked_mul(token2.reserve)
        .map_err(StdError::overflow)?
        .checked_div(lp_token_supply)
        .map_err(StdError::divide_by_zero)?;
    if token2_amount < min_token2 {
        return Err(ContractError::MinToken2Error {
            requested: min_token2,
            available: token2_amount,
        });
    }

    TOKEN1.update(deps.storage, |mut token1| -> Result<_, ContractError> {
        token1.reserve = token1
            .reserve
            .checked_sub(token1_amount)
            .map_err(StdError::overflow)?;
        Ok(token1)
    })?;

    TOKEN2.update(deps.storage, |mut token2| -> Result<_, ContractError> {
        token2.reserve = token2
            .reserve
            .checked_sub(token2_amount)
            .map_err(StdError::overflow)?;
        Ok(token2)
    })?;

    let token1_transfer_msg = match token1.denom {
        Denom::Cw20(addr) => get_cw20_transfer_to_msg(&info.sender, &addr, token1_amount)?,
        Denom::Native(denom) => get_bank_transfer_to_msg(&info.sender, &denom, token1_amount),
    };
    let token2_transfer_msg = match token2.denom {
        Denom::Cw20(addr) => get_cw20_transfer_to_msg(&info.sender, &addr, token2_amount)?,
        Denom::Native(denom) => get_bank_transfer_to_msg(&info.sender, &denom, token2_amount),
    };
        
    let mut messages = vec![];
    messages.push(token1_transfer_msg);
    messages.push(token2_transfer_msg);

    let lp_token_burn_msg = get_burn_msg(&lp_token_addr, &info.sender, amount)?;
    messages.push(lp_token_burn_msg);
    
    Ok(Response::new()
    .add_messages(messages)
    .add_attributes(vec![
        attr("liquidity_burned", amount),
        attr("token1_returned", token1_amount),
        attr("token2_returned", token2_amount),
    ]))

    
}

fn get_burn_msg(contract: &Addr, owner: &Addr, amount: Uint128) -> StdResult<CosmosMsg> {
    let msg = cw20_base::msg::ExecuteMsg::BurnFrom {
        owner: owner.to_string(),
        amount,
    };
    Ok(WasmMsg::Execute {
        contract_addr: contract.to_string(),
        msg: to_binary(&msg)?,
        funds: vec![],
    }
    .into())
}

fn get_cw20_transfer_to_msg(
    recipient: &Addr,
    token_addr: &Addr,
    token_amount: Uint128,
) -> StdResult<CosmosMsg> {
    // create transfer cw20 msg
    let transfer_cw20_msg = Cw20ExecuteMsg::Transfer {
        recipient: recipient.into(),
        amount: token_amount,
    };
    let exec_cw20_transfer = WasmMsg::Execute {
        contract_addr: token_addr.into(),
        msg: to_binary(&transfer_cw20_msg)?,
        funds: vec![],
    };
    let cw20_transfer_cosmos_msg: CosmosMsg = exec_cw20_transfer.into();
    Ok(cw20_transfer_cosmos_msg)
}

fn get_bank_transfer_to_msg(recipient: &Addr, denom: &str, native_amount: Uint128) -> CosmosMsg {
    let transfer_bank_msg = BankMsg::Send {
        to_address: recipient.into(),
        amount: vec![Coin {
            denom: denom.to_string(),
            amount: native_amount,
        }],
    };

    let transfer_bank_cosmos_msg: CosmosMsg = transfer_bank_msg.into();
    transfer_bank_cosmos_msg
}

fn get_input_price(
    input_amount: Uint128,
    input_reserve: Uint128,
    output_reserve: Uint128,
) -> StdResult<Uint128> {
    if input_reserve == Uint128::zero() || output_reserve == Uint128::zero() {
        return Err(StdError::generic_err("No liquidity"));
    };

    let input_amount_with_fee = input_amount
        .checked_mul(Uint128::new(997))
        .map_err(StdError::overflow)?;
    let numerator = input_amount_with_fee
        .checked_mul(output_reserve)
        .map_err(StdError::overflow)?;
    let denominator = input_reserve
        .checked_mul(Uint128::new(1000))
        .map_err(StdError::overflow)?
        .checked_add(input_amount_with_fee)
        .map_err(StdError::overflow)?;

    numerator
        .checked_div(denominator)
        .map_err(StdError::divide_by_zero)
}

fn get_amount_for_denom(coins: &[Coin], denom: &str) -> Coin {
    let amount: Uint128 = coins
        .iter()
        .filter(|c| c.denom == denom)
        .map(|c| c.amount)
        .sum();
    Coin {
        amount,
        denom: denom.to_string(),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn execute_swap(
    deps: DepsMut,
    info: &MessageInfo,
    input_amount: Uint128,
    _env: Env,
    input_token_enum: TokenSelect,
    recipient: &Addr,
    min_token: Uint128,
    fee_amount: Uint128,
    expiration: Option<Expiration>,
) -> Result<Response, ContractError> {
    check_expiration(&expiration, &_env.block)?;

    let cfg = CONFIG.load(deps.storage)?;

    let input_token_item = match input_token_enum {
        TokenSelect::Token1 => TOKEN1,
        TokenSelect::Token2 => TOKEN2,
    };
    let input_token = input_token_item.load(deps.storage)?;
    let output_token_item = match input_token_enum {
        TokenSelect::Token1 => TOKEN2,
        TokenSelect::Token2 => TOKEN1,
    };
    let output_token = output_token_item.load(deps.storage)?;

    // validate input_amount if native input token
    match input_token_enum.clone() {
        TokenSelect::Token1 => validate_input_amount(&info.funds, input_amount + fee_amount, &input_token.denom)?,
        TokenSelect::Token2 => validate_input_amount(&info.funds, fee_amount, &input_token.denom)?
    }

    let token_bought = get_input_price(input_amount, input_token.reserve, output_token.reserve)?;

    if min_token > token_bought {
        return Err(ContractError::SwapMinError {
            min: min_token,
            available: token_bought,
        });
    }

    // Create transfer from message
    let mut transfer_msgs = match input_token.denom.clone() {
        Denom::Cw20(addr) => vec![get_cw20_transfer_from_msg(
            &info.sender,
            &_env.contract.address,
            &addr,
            input_amount,
        )?],
        Denom::Native(_) => vec![],
    };

    // Create transfer to message
    transfer_msgs.push(match output_token.denom.clone() {
        Denom::Cw20(addr) => get_cw20_transfer_to_msg(recipient, &addr, token_bought)?,
        Denom::Native(denom) => get_bank_transfer_to_msg(recipient, &denom, token_bought),
    });

    //check fee is equal or larger than expected
    match input_token_enum.clone() {
        TokenSelect::Token1 => {
            if fee_amount < input_amount * Uint128::from(cfg.platform_fee + cfg.tx_fee) / Uint128::from(THOUSAND) {
                return Err(ContractError::InsufficientFee {  })
            }
            
        }
        TokenSelect::Token2 => {
            if fee_amount < token_bought * Uint128::from(cfg.platform_fee + cfg.tx_fee) / Uint128::from(THOUSAND) {
                return Err(ContractError::InsufficientFee {  })
            }
        }
    }

    // Create fee transfer message
    transfer_msgs.push(
        util::transfer_token_message(Denom::Native(cfg.usdc_denom), fee_amount, cfg.treasury_address.clone())?
    );

    // Update token balances
    input_token_item.update(
        deps.storage,
        |mut input_token| -> Result<_, ContractError> {
            input_token.reserve = input_token
                .reserve
                .checked_add(input_amount)
                .map_err(StdError::overflow)?;
            Ok(input_token)
        },
    )?;

    output_token_item.update(
        deps.storage,
        |mut output_token| -> Result<_, ContractError> {
            output_token.reserve = output_token
                .reserve
                .checked_sub(token_bought)
                .map_err(StdError::overflow)?;
            Ok(output_token)
        },
    )?;

    Ok(Response::new()
        .add_messages(transfer_msgs)
        .add_attributes(vec![
            attr("native_sold", input_amount),
            attr("token_bought", token_bought),
        ]))
}


pub fn execute_add_token(
    deps: DepsMut,
    env: Env,
    info: &MessageInfo,
    input_token_enum: TokenSelect,
    amount: Uint128,
) -> Result<Response, ContractError> {

    
    let cfg = CONFIG.load(deps.storage)?;
    if cfg.owner != info.sender.clone() {
        return Err(ContractError::Unauthorized {  });
    }

    let input_token_item = match input_token_enum {
        TokenSelect::Token1 => TOKEN1,
        TokenSelect::Token2 => TOKEN2,
    };
    let input_token = input_token_item.load(deps.storage)?;
    
    // validate input_amount if native input token
    match input_token_enum.clone() {
        TokenSelect::Token1 => validate_input_amount(&info.funds, amount, &input_token.denom)?,
        TokenSelect::Token2 => {}
    }

    // Create transfer from message
    let mut transfer_msgs = match input_token.denom.clone() {
        Denom::Cw20(addr) => vec![get_cw20_transfer_from_msg(
            &info.sender,
            &env.contract.address,
            &addr,
            amount,
        )?],
        Denom::Native(_) => vec![],
    };

    // Update token balances
    input_token_item.update(
        deps.storage,
        |mut input_token| -> Result<_, ContractError> {
            input_token.reserve = input_token
                .reserve
                .checked_add(amount)
                .map_err(StdError::overflow)?;
            Ok(input_token)
        },
    )?;

    Ok(Response::new()
        .add_messages(transfer_msgs)
        .add_attributes(vec![
            attr("add_token", amount)
        ]))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} 
            => to_binary(&query_config(deps)?),
        QueryMsg::Balance { address } => to_binary(&query_balance(deps, address)?),
        QueryMsg::Info {} => to_binary(&query_info(deps)?),
        QueryMsg::Token1ForToken2Price { token1_amount } => {
            to_binary(&query_token1_for_token2_price(deps, token1_amount)?)
        }
        QueryMsg::Token2ForToken1Price { token2_amount } => {
            to_binary(&query_token2_for_token1_price(deps, token2_amount)?)
        }
    }
}


pub fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let cfg = CONFIG.load(deps.storage)?;
    Ok(ConfigResponse {
        owner: cfg.owner,
        bonding_code_id: cfg.bonding_code_id,
        bonding_contract_address: cfg.bonding_contract_address,
        fury_token_address: cfg.fury_token_address,
        treasury_address: cfg.treasury_address,
        usdc_denom: cfg.usdc_denom,
        tx_fee: cfg.tx_fee,
        platform_fee: cfg.platform_fee,
        lock_seconds: cfg.lock_seconds,
        discount: cfg.discount,
        daily_vesting_amount: cfg.daily_vesting_amount
    })
}

pub fn query_info(deps: Deps) -> StdResult<InfoResponse> {
    let token1 = TOKEN1.load(deps.storage)?;
    let token2 = TOKEN2.load(deps.storage)?;
    let lp_token_address = LP_TOKEN.load(deps.storage)?;
    // TODO get total supply
    Ok(InfoResponse {
        token1_reserve: token1.reserve,
        token1_denom: token1.denom,
        token2_reserve: token2.reserve,
        token2_denom: token2.denom,
        lp_token_supply: get_lp_token_supply(deps, &lp_token_address)?,
        lp_token_address: lp_token_address.to_string(),
    })
}

pub fn query_token1_for_token2_price(
    deps: Deps,
    token1_amount: Uint128,
) -> StdResult<Token1ForToken2PriceResponse> {
    let token1 = TOKEN1.load(deps.storage)?;
    let token2 = TOKEN2.load(deps.storage)?;
    let token2_amount = get_input_price(token1_amount, token1.reserve, token2.reserve)?;
    Ok(Token1ForToken2PriceResponse { token2_amount })
}

pub fn query_token2_for_token1_price(
    deps: Deps,
    token2_amount: Uint128,
) -> StdResult<Token2ForToken1PriceResponse> {
    let token1 = TOKEN1.load(deps.storage)?;
    let token2 = TOKEN2.load(deps.storage)?;
    let token1_amount = get_input_price(token2_amount, token2.reserve, token1.reserve)?;
    Ok(Token2ForToken1PriceResponse { token1_amount })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response, ContractError> {
    if msg.id != INSTANTIATE_LP_TOKEN_REPLY_ID && msg.id != INSTANTIATE_BONDING_ID {
        return Err(ContractError::UnknownReplyId { id: msg.id });
    };
    let res = parse_reply_instantiate_data(msg.clone());
    match res {
        Ok(res) => {
            if msg.id == INSTANTIATE_LP_TOKEN_REPLY_ID {
                // Validate contract address
                let cw20_addr = deps.api.addr_validate(&res.contract_address)?;

                // Save gov token
                LP_TOKEN.save(deps.storage, &cw20_addr)?;

                //Instantiate bonding contract

                let cfg = CONFIG.load(deps.storage)?;
                let mut sub_msg: Vec<SubMsg> = vec![];

                sub_msg.push(SubMsg {
                    msg: WasmMsg::Instantiate {
                        code_id: cfg.bonding_code_id,
                        funds: vec![],
                        admin: Some(cfg.owner.clone().into()),
                        label: String::from("USDC_Fury_LP_Bonding"),
                        msg: to_binary(&BondingInstantiateMsg {
                            owner: cfg.owner.clone(),
                            pool_address: env.contract.address.clone(),
                            treasury_address: cfg.treasury_address.clone(),
                            fury_token_address: cfg.fury_token_address.clone(),
                            lock_seconds: cfg.lock_seconds,
                            discount: cfg.discount,
                            usdc_denom: cfg.usdc_denom,
                            is_native_bonding: false,
                            tx_fee: cfg.tx_fee,
                            platform_fee: cfg.platform_fee,
                            daily_vesting_amount: cfg.daily_vesting_amount
                        })?,
                    }.into(),
                    id: INSTANTIATE_BONDING_ID,
                    gas_limit: None,
                    reply_on: ReplyOn::Success,
                });

                Ok(Response::new().add_submessages(sub_msg))
            } else if msg.id == INSTANTIATE_BONDING_ID {
                let bonding_addr = deps.api.addr_validate(&res.contract_address)?;
                let mut cfg = CONFIG.load(deps.storage)?;
                cfg.bonding_contract_address = bonding_addr;
                // Save gov token
                CONFIG.save(deps.storage, &cfg)?;
                Ok(Response::new())
            } else {
                Ok(Response::new())
            }
            
        }
        Err(_) => Err(ContractError::InstantiateLpTokenError {}),
    }
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
