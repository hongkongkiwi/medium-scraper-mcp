pub mod config;
pub mod extract;
pub mod http;
pub mod llm;
pub mod md;
pub mod medium;
pub mod crawl;
pub mod browser;
pub mod search_web;
pub mod server;
pub mod video;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
