#![cfg(test)]

use std::borrow::BorrowMut;

use cosmwasm_std::{coins, Addr, Coin, Empty, Uint128};
use cw20::Expiration;

use crate::{error::ContractError, msg::ConfigResponse};
use cw20::{Cw20Coin, Cw20Contract, Cw20ExecuteMsg, Denom};
use cw_multi_test::{App, Contract, ContractWrapper, Executor};
use fanfurybonding;

use crate::msg::{ExecuteMsg, InfoResponse, InstantiateMsg, QueryMsg, TokenSelect};

fn mock_app() -> App {
    App::default()
}

pub fn contract_amm() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    )
    .with_reply(crate::contract::reply);
    Box::new(contract)
}

pub fn contract_bonding() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        fanfurybonding::contract::execute,
        fanfurybonding::contract::instantiate,
        fanfurybonding::contract::query,
    );
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

fn get_info(router: &App, contract_addr: &Addr) -> InfoResponse {
    router
        .wrap()
        .query_wasm_smart(contract_addr, &QueryMsg::Info {})
        .unwrap()
}

fn get_bonding_address(router: &App, contract_addr:&Addr) -> Addr {
    let config:ConfigResponse = router
        .wrap()
        .query_wasm_smart(contract_addr, &QueryMsg::Config {})
        .unwrap();
    config.bonding_contract_address
}
fn get_bonding_info(router: &App, contract_addr: &Addr, user: &Addr) -> fanfurybonding::msg::BondStateResponse {
    
    router
        .wrap()
        .query_wasm_smart(get_bonding_address(router, contract_addr), &fanfurybonding::msg::QueryMsg::BondState { address: user.clone() })
        .unwrap()
}


fn create_amm(router: &mut App, owner: &Addr, cash: &Cw20Contract, native_denom: String) -> Addr {
    // set up amm contract
    let cw20_id = router.store_code(contract_cw20());
    let amm_id = router.store_code(contract_amm());
    let bonding_id = router.store_code(contract_bonding());
    let msg = InstantiateMsg {

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

    assert_ne!(cw20_token.addr(), amm_addr);

    let info = get_info(&router, &amm_addr);
    // println!("{:?}", info.lp_token_address);
    assert_eq!(info.lp_token_address, "contract2".to_string())
}

#[test]
// receive cw20 tokens and release upon approval
fn amm_add_and_remove_liquidity() {
    let mut router = mock_app();

    const NATIVE_TOKEN_DENOM: &str = "usdc";

    let owner = Addr::unchecked("owner");
    let funds = coins(2000, NATIVE_TOKEN_DENOM);
    router.borrow_mut().init_modules(|router, _, storage| {
        router.bank.init_balance(storage, &owner, funds).unwrap()
    });

    let cw20_token = create_cw20(
        &mut router,
        &owner,
        "fury".to_string(),
        "FURY".to_string(),
        Uint128::new(5000),
    );

    let amm_addr = create_amm(&mut router, &owner, &cw20_token, NATIVE_TOKEN_DENOM.into());

    assert_ne!(cw20_token.addr(), amm_addr);

    let info = get_info(&router, &amm_addr);
    // set up cw20 helpers
    let lp_token = Cw20Contract(Addr::unchecked(info.lp_token_address));

    // check initial balances
    let owner_balance = cw20_token.balance::<_,_,Empty>(&router, owner.clone()).unwrap();
    assert_eq!(owner_balance, Uint128::new(5000));

    // send tokens to contract address
    let allowance_msg = Cw20ExecuteMsg::IncreaseAllowance {
        spender: amm_addr.to_string(),
        amount: Uint128::new(100u128),
        expires: None,
    };
    let _res = router
        .execute_contract(owner.clone(), cw20_token.addr(), &allowance_msg, &[])
        .unwrap();

    let add_liquidity_msg = ExecuteMsg::AddLiquidity {
        token1_amount: Uint128::new(100),
        min_liquidity: Uint128::new(100),
        max_token2: Uint128::new(100),
        fee_amount: Uint128::new(13),
        expiration: None,
    };
    let _res = router
        .execute_contract(
            owner.clone(),
            amm_addr.clone(),
            &add_liquidity_msg,
            &[Coin {
                denom: NATIVE_TOKEN_DENOM.into(),
                amount: Uint128::new(113),
            }],
        )
        .unwrap();

    // ensure balances updated
    let owner_balance = cw20_token.balance::<_,_,Empty>(&router, owner.clone()).unwrap();
    assert_eq!(owner_balance, Uint128::new(4900));
    let amm_balance = cw20_token.balance::<_,_,Empty>(&router, amm_addr.clone()).unwrap();
    assert_eq!(amm_balance, Uint128::new(100));
    let crust_balance = lp_token.balance::<_,_,Empty>(&router, owner.clone()).unwrap();
    assert_eq!(crust_balance, Uint128::new(100));

    // send tokens to contract address
    let allowance_msg = Cw20ExecuteMsg::IncreaseAllowance {
        spender: amm_addr.to_string(),
        amount: Uint128::new(51u128),
        expires: None,
    };
    let _res = router
        .execute_contract(owner.clone(), cw20_token.addr(), &allowance_msg, &[])
        .unwrap();

    let add_liquidity_msg = ExecuteMsg::AddLiquidity {
        token1_amount: Uint128::new(50),
        min_liquidity: Uint128::new(50),
        max_token2: Uint128::new(51),
        fee_amount: Uint128::new(6),
        expiration: None,
    };
    let _res = router
        .execute_contract(
            owner.clone(),
            amm_addr.clone(),
            &add_liquidity_msg,
            &[Coin {
                denom: NATIVE_TOKEN_DENOM.into(),
                amount: Uint128::new(56),
            }],
        )
        .unwrap();

    // ensure balances updated
    
    let owner_balance = cw20_token.balance::<_,_,Empty>(&router, owner.clone()).unwrap();
    assert_eq!(owner_balance, Uint128::new(4849));
    let amm_balance = cw20_token.balance::<_,_,Empty>(&router, amm_addr.clone()).unwrap();
    assert_eq!(amm_balance, Uint128::new(151));
    let crust_balance = lp_token.balance::<_,_,Empty>(&router, owner.clone()).unwrap();
    assert_eq!(crust_balance, Uint128::new(150));

    
    // Remove some liquidity
    let allowance_msg = Cw20ExecuteMsg::IncreaseAllowance {
        spender: amm_addr.to_string(),
        amount: Uint128::new(50u128),
        expires: None,
    };
    let _res = router
        .execute_contract(owner.clone(), lp_token.addr(), &allowance_msg, &[])
        .unwrap();

    let remove_liquidity_msg = ExecuteMsg::RemoveLiquidity {
        amount: Uint128::new(50),
        min_token1: Uint128::new(50),
        min_token2: Uint128::new(50),
        expiration: None,
    };
    let _res = router
        .execute_contract(
            owner.clone(),
            amm_addr.clone(),
            &remove_liquidity_msg,
            &[Coin {
                denom: NATIVE_TOKEN_DENOM.into(),
                amount: Uint128::new(50),
            }],
        )
        .unwrap();

    // ensure balances updated
    let owner_balance = cw20_token.balance::<_,_,Empty>(&router, owner.clone()).unwrap();
    
    assert_eq!(owner_balance, Uint128::new(4899));
    let amm_balance = cw20_token.balance::<_,_,Empty>(&router, amm_addr.clone()).unwrap();
    assert_eq!(amm_balance, Uint128::new(101));
    let crust_balance = lp_token.balance::<_,_,Empty>(&router, owner.clone()).unwrap();
    assert_eq!(crust_balance, Uint128::new(100));

    // Remove rest of liquidity
    let allowance_msg = Cw20ExecuteMsg::IncreaseAllowance {
        spender: amm_addr.to_string(),
        amount: Uint128::new(100u128),
        expires: None,
    };
    let _res = router
        .execute_contract(owner.clone(), lp_token.addr(), &allowance_msg, &[])
        .unwrap();

    let remove_liquidity_msg = ExecuteMsg::RemoveLiquidity {
        amount: Uint128::new(100),
        min_token1: Uint128::new(100),
        min_token2: Uint128::new(100),
        expiration: None,
    };
    let _res = router
        .execute_contract(
            owner.clone(),
            amm_addr.clone(),
            &remove_liquidity_msg,
            &[Coin {
                denom: NATIVE_TOKEN_DENOM.into(),
                amount: Uint128::new(50),
            }],
        )
        .unwrap();

    // ensure balances updated
    let owner_balance = cw20_token.balance::<_,_,Empty>(&router, owner.clone()).unwrap();
    
    assert_eq!(owner_balance, Uint128::new(5000));
    let amm_balance = cw20_token.balance::<_,_,Empty>(&router, amm_addr).unwrap();
    assert_eq!(amm_balance, Uint128::new(0));
    let crust_balance = lp_token.balance::<_,_,Empty>(&router, owner.clone()).unwrap();
    assert_eq!(crust_balance, Uint128::new(0));
}

#[test]
fn swap_tokens_happy_path() {
    let mut router = mock_app();

    const NATIVE_TOKEN_DENOM: &str = "usdc";

    let owner = Addr::unchecked("owner");
    let funds = coins(50000, NATIVE_TOKEN_DENOM);
    router.borrow_mut().init_modules(|router, _, storage| {
        router.bank.init_balance(storage, &owner, funds).unwrap()
    });

    let cw20_token = create_cw20(
        &mut router,
        &owner,
        "fury".to_string(),
        "FURY".to_string(),
        Uint128::new(50000),
    );

    let amm_addr = create_amm(
        &mut router,
        &owner,
        &cw20_token,
        NATIVE_TOKEN_DENOM.to_string(),
    );

    assert_ne!(cw20_token.addr(), amm_addr);

    // check initial balances
    let owner_balance = cw20_token.balance::<_,_,Empty>(&router, owner.clone()).unwrap();
    assert_eq!(owner_balance, Uint128::new(50000));

    // send tokens to contract address
    let allowance_msg = Cw20ExecuteMsg::IncreaseAllowance {
        spender: amm_addr.to_string(),
        amount: Uint128::new(20000u128),
        expires: None,
    };
    let _res = router
        .execute_contract(owner.clone(), cw20_token.addr(), &allowance_msg, &[])
        .unwrap();

    let add_liquidity_msg = ExecuteMsg::AddLiquidity {
        token1_amount: Uint128::new(20000),
        min_liquidity: Uint128::new(20000),
        max_token2: Uint128::new(20000),
        fee_amount: Uint128::new(520),
        expiration: None,
    };
    let res = router
        .execute_contract(
            owner.clone(),
            amm_addr.clone(),
            &add_liquidity_msg,
            &[Coin {
                denom: NATIVE_TOKEN_DENOM.into(),
                amount: Uint128::new(20520),
            }],
        )
        .unwrap();
    
    let info = get_info(&router, &amm_addr);
    assert_eq!(info.token1_reserve, Uint128::new(20000));
    assert_eq!(info.token2_reserve, Uint128::new(20000));

    let buyer = Addr::unchecked("buyer");
    let funds = coins(20000, NATIVE_TOKEN_DENOM);
    router.borrow_mut().init_modules(|router, _, storage| {
        router.bank.init_balance(storage, &buyer, funds).unwrap()
    });

    let add_liquidity_msg = ExecuteMsg::Swap {
        input_token: TokenSelect::Token1,
        input_amount: Uint128::new(1000),
        min_output: Uint128::new(949),
        fee_amount: Uint128::new(13),
        expiration: None,
    };
    let res = router
        .execute_contract(
            buyer.clone(),
            amm_addr.clone(),
            &add_liquidity_msg,
            &[Coin {
                denom: NATIVE_TOKEN_DENOM.into(),
                amount: Uint128::new(1013),
            }],
        )
        .unwrap();
    
    let info = get_info(&router, &amm_addr);
    
    assert_eq!(info.token1_reserve, Uint128::new(21000));
    assert_eq!(info.token2_reserve, Uint128::new(19051));

    // ensure balances updated
    let buyer_balance = cw20_token.balance::<_,_,Empty>(&router, buyer.clone()).unwrap();
    assert_eq!(buyer_balance, Uint128::new(949));

    // Check balances of owner and buyer reflect the sale transaction
    let balance: Coin = bank_balance(&mut router, &buyer, NATIVE_TOKEN_DENOM.to_string());
    assert_eq!(balance.amount, Uint128::new(18987));

    let swap_msg = ExecuteMsg::Swap {
        input_token: TokenSelect::Token1,
        input_amount: Uint128::new(5000),
        min_output: Uint128::new(3654),
        fee_amount: Uint128::new(65),
        expiration: None,
    };
    let _res = router
        .execute_contract(
            buyer.clone(),
            amm_addr.clone(),
            &swap_msg,
            &[Coin {
                denom: NATIVE_TOKEN_DENOM.into(),
                amount: Uint128::new(5065),
            }],
        )
        .unwrap();
        
    let info = get_info(&router, &amm_addr);
    
    assert_eq!(info.token1_reserve, Uint128::new(26000));
    assert_eq!(info.token2_reserve, Uint128::new(15397));

}

#[test]
fn bonding() {
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

    let add_liquidity_msg = ExecuteMsg::AddLiquidity {
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

    
    // check bonding contract info
    let res:fanfurybonding::msg::ConfigResponse = router
        .wrap()
        .query_wasm_smart(get_bonding_address(&router, &amm), &fanfurybonding::msg::QueryMsg::Config { })
        .unwrap();
    // println!("{:?}", res);


    // Check bonding record
    let record = get_bonding_info(&router, &amm, &bonder);
    // println!("{:?}", record);

    assert_eq!(record.list[0].amount, Uint128::new(201005));
}