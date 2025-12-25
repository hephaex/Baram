//! Coordinator server for distributed crawling
//!
//! This module provides a central coordination server that manages
//! crawler instances, distributes schedules, and monitors health.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────┐
//! │         Coordinator Server          │
//! │                                     │
//! │  ┌──────────────────────────────┐  │
//! │  │      Instance Registry       │  │
//! │  │  - Registration              │  │
//! │  │  - Heartbeat tracking        │  │
//! │  │  - Status management         │  │
//! │  └──────────────────────────────┘  │
//! │                                     │
//! │  ┌──────────────────────────────┐  │
//! │  │     Schedule Manager         │  │
//! │  │  - Daily schedule gen        │  │
//! │  │  - Distribution              │  │
//! │  │  - Override handling         │  │
//! │  └──────────────────────────────┘  │
//! │                                     │
//! │  ┌──────────────────────────────┐  │
//! │  │        REST API              │  │
//! │  │  GET  /api/health            │  │
//! │  │  GET  /api/schedule/today    │  │
//! │  │  GET  /api/instances         │  │
//! │  │  POST /api/instances/register│  │
//! │  │  POST /api/instances/heartbeat│ │
//! │  └──────────────────────────────┘  │
//! └─────────────────────────────────────┘
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use baram::coordinator::{CoordinatorServer, CoordinatorConfig};
//!
//! let config = CoordinatorConfig::default();
//! let server = CoordinatorServer::new(config)?;
//! server.start().await?;
//! ```

pub mod api;
pub mod client;
pub mod config;
pub mod registry;
pub mod server;

// Re-export main types
pub use client::{ClientConfig, CoordinatorClient};
pub use config::CoordinatorConfig;
pub use registry::{InstanceInfo, InstanceRegistry, InstanceStatus};
pub use server::CoordinatorServer;
