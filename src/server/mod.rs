//! API Server Module
//!
//! Production HTTP API service with:
//! - Async task queue for long-running pipelines
//! - SQLite persistence with task recovery
//! - Clean separation of concerns
//!
//! Note: rate limiting is delegated to external services like Cloudflare WAF.
//! A temporary site-wide login gate is currently enabled in `temp_auth`.

pub mod admin;
pub mod config;
pub mod handlers;
pub mod middleware;
pub mod pipeline;
pub mod recovery;
pub mod responses;
pub mod routes;
pub mod state;
pub mod task;
pub mod temp_auth;
