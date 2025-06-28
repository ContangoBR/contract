#![no_std]

mod config;
mod contract;
mod storage_types;

#[cfg(test)]
mod test;

pub use crate::contract::ContangoTokenClient;
