//! Shared application services for native desktop and HTTP clients.
pub mod applog;
pub mod auth;
pub mod db;
pub mod error;
pub mod seed;
pub mod service;
#[cfg(feature = "server")]
pub mod web;
