use cosmwasm_std::{StdError};
use hex::FromHexError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    Hex(#[from] FromHexError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Disabled")]
    Disabled {},

    #[error("InvalidInput")]
    InvalidInput {},

    #[error("NothingToUnbond")]
    NothingToUnbond {},

    #[error("Not Reward or Stake token")]
    UnacceptableToken {},

    #[error("Not enough Fund")]
    NotEnoughFund { },

    #[error("Wrong length")]
    WrongLength {},

    #[error("InsufficientFury")]
    InsufficientFury{},

    #[error("MaxBondingExceed")]
    MaxBondingExceed{},

    #[error("InsufficientFee")]
    InsufficientFee{},

    #[error("Already started shorting")]
    AlreadyStarted {},

    #[error("Not Allowed Bonding Typ")]
    NotAllowedBondingType {},

    #[error("Price got up too high and cannnot recompense")]
    TooHigh {},

    #[error("Map2List failed")]
    Map2ListFailed {},

    #[error("Cannot migrate from different contract type: {previous_contract}")]
    CannotMigrate { previous_contract: String },

    #[error("Count {count}")]
    Count { count: u64 },

    #[error("Amount of the native coin inputed is zero")]
    NativeInputZero {},

    #[error("Amount of the cw20 coin inputed is zero")]
    Cw20InputZero {},

    #[error("Token type mismatch")]
    TokenTypeMismatch {},
}
