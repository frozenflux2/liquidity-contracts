use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Uint128};
use cw20::Denom;
use cw_storage_plus::Item;

pub const LP_TOKEN: Item<Addr> = Item::new("lp_token");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Token {
    pub reserve: Uint128,
    pub denom: Denom,
}

pub const TOKEN1: Item<Token> = Item::new("token1");
pub const TOKEN2: Item<Token> = Item::new("token2");


#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: Addr,
    pub bonding_code_id: u64,
    pub bonding_contract_address: Addr,
    pub fury_token_address: Addr,
    pub treasury_address: Addr,
    pub usdc_denom: String,
    pub tx_fee: u64,
    pub platform_fee: u64,
    pub lock_seconds: u64,
    pub discount: u64,
    pub daily_vesting_amount: Uint128,
}

pub const CONFIG: Item<Config> = Item::new("config");