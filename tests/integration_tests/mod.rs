//! Integration tests module
//!
//! This module provides end-to-end integration tests for the Baram crawler system,
//! including:
//! - Complete crawl → parse → store pipeline
//! - Distributed crawler coordination
//! - Error handling and recovery scenarios

pub mod pipeline_test;
pub mod distributed_test;
pub mod error_scenarios;
pub mod fixtures;
