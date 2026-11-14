#![forbid(unsafe_code)]

pub mod catalog;
pub mod database;
pub mod evaluate;
pub mod model;
pub mod results;
pub mod runner;
pub mod sql;
pub mod sql_only;
pub mod viewer;

pub use model::{Check, ConnectionConfig, Options, Row};
pub use runner::{execute_dq_checks, list_dq_checks};
