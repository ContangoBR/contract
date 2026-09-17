#![no_std]

mod config;
mod contract;

#[cfg(test)]
mod test;

pub use crate::contract::ContangoTokenClient;
