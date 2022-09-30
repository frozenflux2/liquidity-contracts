use cosmwasm_std::{
    to_binary,  Response, StdResult, Uint128, Coin, BankMsg,
    WasmMsg, WasmQuery, QueryRequest, Addr, Storage, CosmosMsg,  QuerierWrapper, BalanceResponse as NativeBalanceResponse, BankQuery
};
use cw20::{Balance, Cw20ExecuteMsg, Denom, BalanceResponse as CW20BalanceResponse, Cw20QueryMsg};
use crate::error::ContractError;
use crate::state::CONFIG;
// use wasmswap::msg::{ExecuteMsg as WasmswapExecuteMsg, QueryMsg as WasmswapQueryMsg, Token1ForToken2PriceResponse, Token2ForToken1PriceResponse, InfoResponse as WasmswapInfoResponse, TokenSelect};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub const NORMAL_DECIMAL:u128 = 1000000u128;
pub const THRESHOLD:u128 = 3000000u128;
pub const THOUSAND:u64 = 1000u64;

//Manager Config Response

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct ManagerConfigResponse {
    pub owner: Addr,
    pub stkn_address: Addr,
    pub pusd_address: Addr,
    
    pub cw20_code_id: u64,
    pub stock_code_id: u64,
    pub pool_code_id: u64,

    pub shorting_code_id: u64,
    pub trading_code_id: u64,
    pub providing_code_id: u64,

    pub price: Uint128,
    pub stkn_amount: Uint128,

    pub max_stock_id: u32,
    pub enabled: bool,

    pub providing_sync_interval: u64

}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ManagerQueryMsg {
    Config {},
    Stock {
        id: u32
    },
    ListStocks {
    },
    CheckStockSubcontract {
        id: u32,
        address: Addr
    }
}


#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StockQueryMsg {
    /// Returns the current balance of the given address, 0 if unset.
    /// Return type: BalanceResponse.
    Balance { address: String },
    /// Returns metadata on the contract - name, decimals, supply, etc.
    /// Return type: TokenInfoResponse.
    TokenInfo {},
    /// Only with "mintable" extension.
    /// Returns who can mint and the hard cap on maximum tokens after minting.
    /// Return type: MinterResponse.
    Minter {},
    /// Only with "allowance" extension.
    /// Returns how much spender can use from owner account, 0 if unset.
    /// Return type: AllowanceResponse.
    Allowance { owner: String, spender: String },
    /// Only with "enumerable" extension (and "allowances")
    /// Returns all allowances this owner has approved. Supports pagination.
    /// Return type: AllAllowancesResponse.
    AllAllowances {
        owner: String,
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Only with "enumerable" extension (and "allowances")
    /// Returns all allowances this spender has been granted. Supports pagination.
    /// Return type: AllSpenderAllowancesResponse.
    AllSpenderAllowances {
        spender: String,
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Only with "enumerable" extension
    /// Returns all accounts that have balances. Supports pagination.
    /// Return type: AllAccountsResponse.
    AllAccounts {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Only with "marketing" extension
    /// Returns more metadata on the contract to display in the client:
    /// - description, logo, project url, etc.
    /// Return type: MarketingInfoResponse
    MarketingInfo {},
    /// Only with "marketing" extension
    /// Downloads the embedded logo data (if stored on chain). Errors if no logo data is stored for this
    /// contract.
    /// Return type: DownloadLogoResponse.
    DownloadLogo {},

    Config {}
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct StockConfigResponse {
    pub id: u32,
    pub manager_address: Addr,

    pub pool_address: Addr,
    pub shorting_address: Addr,
    pub trading_address: Addr,
    pub providing_address: Addr,

    pub pool_code_id: u64,
    pub shorting_code_id: u64,
    pub trading_code_id: u64,
    pub providing_code_id: u64,
    
    pub price: Uint128,
    pub reward: Uint128
}

//This is the struct of Controller contract
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct MintPusd {
    pub id: u32,
    pub recipient: Addr,
    pub amount: Uint128
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct MintStock {
    pub id: u32,
    pub recipient: Addr,
    pub amount: Uint128
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct TransferStkn {
    pub id: u32, 
    pub recipient: Addr,
    pub amount: Uint128
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct Swap {
}





// pub fn check_token_and_pool (
//     querier: QuerierWrapper,
//     denom: Denom,
//     pool_address: Addr,
// ) -> Result<bool, ContractError> {
//     let pool_info_response: WasmswapInfoResponse = querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
//         contract_addr: pool_address.clone().into(),
//         msg: to_binary(&WasmswapQueryMsg::Info {})?,
//     }))?;

//     if denom != pool_info_response.token1_denom && denom != pool_info_response.token2_denom {
//         return Err(ContractError::PoolAndTokenMismatch{});
//     }

//     if denom == pool_info_response.token1_denom {
//         return Ok(true);
//     }

//     if denom == pool_info_response.token2_denom {
//         return Ok(false);
//     }
//     return Err(ContractError::PoolAndTokenMismatch{});
// }

pub fn get_amount_of_denom(
    balance: Balance,
    denom: Denom
) -> Result<Uint128, ContractError> {

    match denom.clone() {
        Denom::Native(native_str) => {
            match balance {
                Balance::Native(native_balance) => {
                    let zero_coin = &Coin {
                        denom: String::from("empty"),
                        amount: Uint128::zero()
                    };
                    let (_index, coin) =native_balance.0.iter().enumerate().find(|(_i, c)| c.denom == native_str)
                    .unwrap_or((0, zero_coin));

                    if coin.amount == Uint128::zero() {
                        return Err(ContractError::NativeInputZero {});
                    }
                    return Ok(coin.amount);
                },
                Balance::Cw20(_) => {
                    return Err(ContractError::TokenTypeMismatch {});
                }
            }
        },
        Denom::Cw20(cw20_address) => {
            match balance {
                Balance::Native(_) => {
                    return Err(ContractError::TokenTypeMismatch {});
                },
                Balance::Cw20(token) => {
                    if cw20_address != token.address {
                        return Err(ContractError::TokenTypeMismatch {});
                    }
                    if token.amount == Uint128::zero() {
                        return Err(ContractError::Cw20InputZero {});
                    }
                    return Ok(token.amount);
                }
            }
        }
    }
}

// pub fn get_swap_amount_and_denom_and_message(
//     querier: QuerierWrapper,
//     pool_address: Addr,
//     denom: Denom,
//     amount: Uint128,
// ) -> Result<(Uint128, Denom, Vec<CosmosMsg>), ContractError> {

//     let pool_info_response: WasmswapInfoResponse = querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
//         contract_addr: pool_address.clone().into(),
//         msg: to_binary(&WasmswapQueryMsg::Info {})?,
//     }))?;

//     if denom != pool_info_response.token1_denom && denom != pool_info_response.token2_denom {
//         return Err(ContractError::PoolAndTokenMismatch{});
//     }

//     let mut messages: Vec<CosmosMsg> = vec![];
//     let swap_amount;
//     let other_denom: Denom;
//     if denom == pool_info_response.token1_denom {
//         let token2_price_response: Token1ForToken2PriceResponse = querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
//             contract_addr: pool_address.clone().into(),
//             msg: to_binary(&WasmswapQueryMsg::Token1ForToken2Price {
//                 token1_amount: amount
//             })?,
//         }))?;

//         other_denom = pool_info_response.token2_denom;
//         swap_amount = token2_price_response.token2_amount;
//         let messages_swap = swap_token_messages(denom, TokenSelect::Token1, amount, swap_amount, pool_address.clone())?;
//         for i in 0..messages_swap.len() {
//             messages.push(messages_swap[i].clone());
//         }

//     } else {
//         let token1_price_response: Token2ForToken1PriceResponse = querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
//             contract_addr: pool_address.clone().into(),
//             msg: to_binary(&WasmswapQueryMsg::Token2ForToken1Price {
//                 token2_amount: amount
//             })?,
//         }))?;

//         other_denom = pool_info_response.token1_denom;
//         swap_amount = token1_price_response.token1_amount;

//         let messages_swap = swap_token_messages(denom, TokenSelect::Token2, amount, swap_amount, pool_address.clone())?;
//         for i in 0..messages_swap.len() {
//             messages.push(messages_swap[i].clone());
//         }
//     }
//     Ok((swap_amount, other_denom, messages))
// }

// pub fn swap_token_messages(
//     denom: Denom,
//     input_token: TokenSelect,
//     input_amount: Uint128,
//     min_output: Uint128,
//     pool_address: Addr
// ) -> Result<Vec<CosmosMsg>, ContractError> {

//     let mut messages: Vec<CosmosMsg> = vec![];
//     match denom.clone() {
//         Denom::Native(native_str) => {
//             messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
//                 contract_addr: pool_address.clone().into(),
//                 funds: vec![Coin {
//                     denom: native_str,
//                     amount: input_amount
//                 }],
//                 msg: to_binary(&WasmswapExecuteMsg::Swap {
//                     input_token,
//                     input_amount,
//                     min_output,
//                     expiration: None
//                 })?,
//             }));

//         },
//         Denom::Cw20(cw20_address) => {
//             messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
//                 contract_addr: cw20_address.clone().into(),
//                 funds: vec![],
//                 msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
//                     spender: pool_address.clone().into(),
//                     amount: input_amount,
//                     expires: None
//                 })?,
//             }));
//             messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
//                 contract_addr: pool_address.clone().into(),
//                 funds: vec![],
//                 msg: to_binary(&WasmswapExecuteMsg::Swap {
//                     input_token,
//                     input_amount,
//                     min_output,
//                     expiration: None
//                 })?,
//             }));
//         }
//     }
//     return Ok(messages);
// }


pub fn transfer_token_message(
    denom: Denom,
    amount: Uint128,
    receiver: Addr
) -> Result<CosmosMsg, ContractError> {

    match denom.clone() {
        Denom::Native(native_str) => {
            return Ok(BankMsg::Send {
                to_address: receiver.clone().into(),
                amount: vec![Coin{
                    denom: native_str,
                    amount
                }]
            }.into());
        },
        Denom::Cw20(cw20_address) => {
            return Ok(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: cw20_address.clone().into(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: receiver.clone().into(),
                    amount
                })?,
            }));
        }
    }
}


pub fn get_token_amount(
    querier: QuerierWrapper,
    denom: Denom,
    contract_addr: Addr
) -> Result<Uint128, ContractError> {

    match denom.clone() {
        Denom::Native(native_str) => {
            let native_response: NativeBalanceResponse = querier.query(&QueryRequest::Bank(BankQuery::Balance {
                address: contract_addr.clone().into(),
                denom: native_str
            }))?;
            return Ok(native_response.amount.amount);
        },
        Denom::Cw20(cw20_address) => {
            let balance_response: CW20BalanceResponse = querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: cw20_address.clone().into(),
                msg: to_binary(&Cw20QueryMsg::Balance {address: contract_addr.clone().into()})?,
            }))?;
            return Ok(balance_response.balance);
        }
    }
}


pub fn get_manager_config(
    querier: QuerierWrapper,
    manager_addr: Addr
) -> Result<ManagerConfigResponse, ContractError> {

    let response: ManagerConfigResponse = querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: manager_addr.clone().into(),
        msg: to_binary(&ManagerQueryMsg::Config {})?,
    }))?;
    Ok(response)
}

pub fn get_stock_config(
    querier: QuerierWrapper,
    stock_address: Addr
) -> Result<StockConfigResponse, ContractError> {

    let response: StockConfigResponse = querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: stock_address.clone().into(),
        msg: to_binary(&StockQueryMsg::Config {})?,
    }))?;
    Ok(response)
}

pub fn check_enabled(
    querier: QuerierWrapper,
    manager_address: Addr
) -> Result<Response, ContractError> {
    let response = get_manager_config(querier, manager_address)?;
    if !response.enabled {
        return Err(ContractError::Disabled {})
    }
    Ok(Response::new().add_attribute("action", "check_enabled"))
}

pub fn check_owner(
    querier: QuerierWrapper,
    manager_addr: Addr,
    address: Addr
) -> Result<Response, ContractError> {
    let response = get_manager_config(querier, manager_addr)?;
    if response.owner != address {
        return Err(ContractError::Unauthorized {})
    }

    Ok(Response::new().add_attribute("action", "check_owner"))
}

pub fn check_stock_enabled(
    querier: QuerierWrapper,
    stock_address: Addr
) -> Result<Response, ContractError> {
    let response = get_stock_config(querier, stock_address)?;
    check_enabled(querier, response.manager_address)
}

pub fn check_stock_owner(
    querier: QuerierWrapper,
    stock_address: Addr,
    address: Addr
) -> Result<Response, ContractError> {
    if stock_address == address {
        return Ok(Response::new().add_attribute("action", "check_owner"));
    }
    let response = get_stock_config(querier, stock_address)?;
    check_owner(querier, response.manager_address, address)
}

pub fn check_stock_subcontract(
    querier: QuerierWrapper,
    stock_address: Addr,
    address: Addr
) -> Result<Response, ContractError> {
    
    let stock_response = get_stock_config(querier, stock_address)?;
    let check_subcontract = querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: stock_response.manager_address.clone().into(),
        msg: to_binary(&ManagerQueryMsg::CheckStockSubcontract { id: stock_response.id, address })?,
    }))?;

    if check_subcontract {
        Ok(Response::default())
    } else {
        return Err(ContractError::Unauthorized {  })
    }
}


