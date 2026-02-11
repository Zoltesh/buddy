// buddy-core: shared library for the buddy application.

pub mod types;
pub mod config;
pub mod store;
pub mod embedding;
pub mod memory;
pub mod provider;
pub mod skill;
pub mod reload;
pub mod warning;
pub mod state;

// Test utilities - always available for use by buddy-server and tests
pub mod testutil;
