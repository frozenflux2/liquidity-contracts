#![cfg(test)]

use std::borrow::BorrowMut;

use cosmwasm_std::{coins, Addr, Coin, Empty, Uint128};
use cw20::Expiration;

use crate::{error::ContractError};
use cw20::{Cw20Coin, Cw20Contract, Cw20ExecuteMsg, Denom};
use cw_multi_test::{App, Contract, ContractWrapper, Executor};
use fanfuryswap;

use crate::msg::{ExecuteMsg, ConfigResponse, InstantiateMsg, QueryMsg, BondStateResponse};

fn mock_app() -> App {
    App::default()
}

pub fn contract_bonding() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    );
    
    Box::new(contract)
}

pub fn contract_amm() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        fanfuryswap::contract::execute,
        fanfuryswap::contract::instantiate,
        fanfuryswap::contract::query,
    ).with_reply(fanfuryswap::contract::reply);
    Box::new(contract)
}

pub fn contract_cw20() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        cw20_base::contract::execute,
        cw20_base::contract::instantiate,
        cw20_base::contract::query,
    );
    Box::new(contract)
}

fn get_config(router: &App, contract_addr: &Addr) -> ConfigResponse {
    router
        .wrap()
        .query_wasm_smart(contract_addr, &QueryMsg::Config {})
        .unwrap()
}

fn get_pool_config(router: &App, contract_addr: &Addr) -> fanfuryswap::msg::ConfigResponse {
    router
        .wrap()
        .query_wasm_smart(contract_addr, &QueryMsg::Config {})
        .unwrap()
}



fn get_pool_address(router: &App, contract_addr:&Addr) -> Addr {
    get_config(router, contract_addr).pool_address
}

fn get_bonding_info(router: &App, contract_addr: &Addr, user: &Addr) -> BondStateResponse {
    router
        .wrap()
        .query_wasm_smart(contract_addr, &QueryMsg::BondState { address: user.clone() })
        .unwrap()
}

fn get_pair_bonding_contract(router: &App, contract_addr: &Addr, user: &Addr) -> Addr {
    let res : fanfuryswap::msg::ConfigResponse= router
    .wrap()
    .query_wasm_smart(contract_addr, &fanfuryswap::msg::QueryMsg::Config {  })
    .unwrap();

    res.bonding_contract_address
}

fn create_amm(router: &mut App, owner: &Addr, cash: &Cw20Contract, native_denom: String) -> Addr {
    // set up amm contract
    let cw20_id = router.store_code(contract_cw20());
    let amm_id = router.store_code(contract_amm());
    let bonding_id = router.store_code(contract_bonding());
    let msg = fanfuryswap::msg::InstantiateMsg {

        lp_token_code_id: cw20_id,
        bonding_code_id: bonding_id,
        owner: owner.clone(),
        treasury_address: owner.clone(),
        fury_token_address: cash.addr(),
        usdc_denom: native_denom,
        lock_seconds: 7u64,
        discount: 5u64,
        tx_fee: 3u64,
        platform_fee: 10u64,
        daily_vesting_amount: Uint128::from(10000000000u128)
    };
    router
        .instantiate_contract(amm_id, owner.clone(), &msg, &[], "amm", None)
        .unwrap()
}

fn create_native_bonding(router: &mut App, owner: &Addr, pool_address: &Addr, cash: &Cw20Contract, native_denom: String) -> Addr {
    // set up bond contract
    let bonding_id = router.store_code(contract_bonding());
    let msg = InstantiateMsg {
        owner: owner.clone(),
        pool_address: pool_address.clone(),
        treasury_address: owner.clone(),
        fury_token_address: cash.addr(),
        usdc_denom: native_denom,

        lock_seconds: 5u64,
        discount: 7u64,
        tx_fee: 3u64,
        platform_fee: 10u64,
        daily_vesting_amount: Uint128::from(10000000000u128),
        is_native_bonding: true
    };
    router
        .instantiate_contract(bonding_id, owner.clone(), &msg, &[], "nativebond", None)
        .unwrap()
}


// CreateCW20 create new cw20 with given initial balance belonging to owner
fn create_cw20(
    router: &mut App,
    owner: &Addr,
    name: String,
    symbol: String,
    balance: Uint128,
) -> Cw20Contract {
    // set up cw20 contract with some tokens
    let cw20_id = router.store_code(contract_cw20());
    let msg = cw20_base::msg::InstantiateMsg {
        name,
        symbol,
        decimals: 2,
        initial_balances: vec![Cw20Coin {
            address: owner.to_string(),
            amount: balance,
        }],
        mint: None,
        marketing: None,
    };
    let addr = router
        .instantiate_contract(cw20_id, owner.clone(), &msg, &[], "CASH", None)
        .unwrap();
    Cw20Contract(addr)
}

fn bank_balance(router: &mut App, addr: &Addr, denom: String) -> Coin {
    router
        .wrap()
        .query_balance(addr.to_string(), denom)
        .unwrap()
}

#[test]
// receive cw20 tokens and release upon approval
fn test_instantiate() {
    let mut router = mock_app();

    const NATIVE_TOKEN_DENOM: &str = "usdc";

    let owner = Addr::unchecked("owner");
    let funds = coins(2000000, NATIVE_TOKEN_DENOM);
    router.borrow_mut().init_modules(|router, _, storage| {
        router.bank.init_balance(storage, &owner, funds).unwrap()
    });

    let cw20_token = create_cw20(
        &mut router,
        &owner,
        "fury".to_string(),
        "FURY".to_string(),
        Uint128::new(2000000),
    );

    let amm_addr = create_amm(&mut router, &owner, &cw20_token, NATIVE_TOKEN_DENOM.into());
    let native_bonding_addr = create_native_bonding(&mut router, &owner, &amm_addr, &cw20_token, NATIVE_TOKEN_DENOM.into());

    assert_ne!(cw20_token.addr(), amm_addr);

    let info = get_config(&router, &native_bonding_addr);
    println!("{:?}", info);
    assert_eq!(info.pool_address, "contract1".to_string())
}
#[test]
fn lp_bonding() {
    let mut router = mock_app();

    const NATIVE_TOKEN_DENOM: &str = "usdc";

    let owner = Addr::unchecked("owner");
    let bonder = Addr::unchecked("bonder");
    let funds = coins(200000, NATIVE_TOKEN_DENOM);

    router.borrow_mut().init_modules(|router, _, storage| {
        router.bank.init_balance(storage, &bonder, funds).unwrap()
    });

    let token = create_cw20(
        &mut router,
        &bonder,
        "fury".to_string(),
        "FURY".to_string(),
        Uint128::new(500000),
    );

    let amm = create_amm(&mut router, &owner, &token, NATIVE_TOKEN_DENOM.to_string());

    // Add initial liquidity to pools
    let allowance_msg = Cw20ExecuteMsg::IncreaseAllowance {
        spender: amm.to_string(),
        amount: Uint128::new(100000),
        expires: None,
    };
    let _res = router
        .execute_contract(bonder.clone(), token.addr(), &allowance_msg, &[])
        .unwrap();

    let add_liquidity_msg = fanfuryswap::msg::ExecuteMsg::AddLiquidity {
        token1_amount: Uint128::new(100000),
        min_liquidity: Uint128::new(100000),
        max_token2: Uint128::new(100000),
        fee_amount: Uint128::new(2600),
        expiration: None,
    };
    let res = router
        .execute_contract(
            bonder.clone(),
            amm.clone(),
            &add_liquidity_msg,
            &[Coin {
                denom: NATIVE_TOKEN_DENOM.into(),
                amount: Uint128::new(102600),
            }],
        )
        .unwrap();
    
    // ensure balances updated
    let token1_balance = token.balance::<_,_,Empty>(&router, bonder.clone()).unwrap();
    assert_eq!(token1_balance, Uint128::new(400000));

    
    // check pool contract info
    let res:fanfuryswap::msg::ConfigResponse = get_pool_config(&router, &amm);
    
    let bonding_contract = res.bonding_contract_address;

    // println!("{:?}", res);


    // Check bonding record
    let record = get_bonding_info(&router, &bonding_contract, &bonder);
    // println!("{:?}", record);

    // println!("{:?}", router.block_info());
    assert_eq!(record.list[0].amount, Uint128::new(201005));
    
}


#[test]
fn native_bonding() {
    let mut router = mock_app();

    const NATIVE_TOKEN_DENOM: &str = "usdc";

    let owner = Addr::unchecked("owner");
    let bonder = Addr::unchecked("bonder");
    let funds = coins(200000, NATIVE_TOKEN_DENOM);

    router.borrow_mut().init_modules(|router, _, storage| {
        router.bank.init_balance(storage, &bonder, funds).unwrap()
    });

    let token = create_cw20(
        &mut router,
        &bonder,
        "fury".to_string(),
        "FURY".to_string(),
        Uint128::new(500000),
    );

    let amm = create_amm(&mut router, &owner, &token, NATIVE_TOKEN_DENOM.to_string());
    
    // Add initial liquidity to pools
    let allowance_msg = Cw20ExecuteMsg::IncreaseAllowance {
        spender: amm.to_string(),
        amount: Uint128::new(100000),
        expires: None,
    };
    let _res = router
        .execute_contract(bonder.clone(), token.addr(), &allowance_msg, &[])
        .unwrap();

    let add_liquidity_msg = fanfuryswap::msg::ExecuteMsg::AddLiquidity {
        token1_amount: Uint128::new(100000),
        min_liquidity: Uint128::new(100000),
        max_token2: Uint128::new(100000),
        fee_amount: Uint128::new(2600),
        expiration: None,
    };
    let res = router
        .execute_contract(
            bonder.clone(),
            amm.clone(),
            &add_liquidity_msg,
            &[Coin {
                denom: NATIVE_TOKEN_DENOM.into(),
                amount: Uint128::new(102600),
            }],
        )
        .unwrap();
    
    // ensure balances updated
    let token1_balance = token.balance::<_,_,Empty>(&router, bonder.clone()).unwrap();
    assert_eq!(token1_balance, Uint128::new(400000));

    
    // check pool contract info
    let res:fanfuryswap::msg::ConfigResponse = get_pool_config(&router, &amm);

    //make native bonding
    let bonding_contract = create_native_bonding(&mut router, &owner,&amm, &token, NATIVE_TOKEN_DENOM.to_string());
    // println!("{:?}", res);

    let bond_msg = ExecuteMsg::Bond { amount: Uint128::new(10000) };
    let res = router
        .execute_contract(
            bonder.clone(),
            bonding_contract.clone(),
            &bond_msg,
            &[Coin {
                denom: NATIVE_TOKEN_DENOM.into(),
                amount: Uint128::new(10130),
            }],
        )
        .unwrap();

    // println!("{:?}", res);
    // Check bonding record
    let record = get_bonding_info(&router, &bonding_contract, &bonder);
    println!("{:?}", record);

    assert_eq!(record.list[0].amount, Uint128::new(9129));

    // check the fee
    
    
}
