#![no_std]

mod admin;
mod allowance;
mod balance;
mod config;
mod contract;
mod fees;
mod metadata;
mod storage_types;

#[cfg(test)]
mod test;

pub use crate::contract::TokenClient;
