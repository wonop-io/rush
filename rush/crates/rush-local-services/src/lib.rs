//! # Rush Local Services
//!
//! This crate provides persistent local development services for Rush applications.
//!
//! ## Overview
//!
//! Local services are Docker containers that persist between application rebuilds,
//! providing stable development infrastructure like databases, caches, and cloud
//! service emulators. Unlike regular application containers that restart on code
//! changes, local services maintain their state throughout the development session.
//!
//! ## Features
//!
//! - **Service Persistence**: Data persists between application restarts
//! - **Dependency Management**: Services can depend on other services
//! - **Health Checks**: Wait for services to be healthy before starting applications
//! - **Connection Strings**: Automatically generate and inject database URLs
//! - **Built-in Service Types**: PostgreSQL, Redis, MinIO, LocalStack, and more
//! - **Custom Services**: Support for any Docker image
//!
//! ## Usage
//!
//! Local services are configured in `stack.spec.yaml`:
//!
//! ```yaml
//! postgres:
//!   build_type:
//!     LocalService:
//!       service_type: postgresql
//!       persist_data: true
//!       ports:
//!         - "5432:5432"
//!       env:
//!         POSTGRES_USER: myuser
//!         POSTGRES_PASSWORD: mypass
//!         POSTGRES_DB: mydb
//!       health_check: "pg_isready -U myuser"
//! ```
//!
//! ## Service Types
//!
//! ### Built-in Services
//! - `postgresql` - PostgreSQL database with automatic connection strings
//! - `redis` - Redis cache with connection URL generation
//! - `minio` - S3-compatible object storage
//! - `localstack` - AWS service emulator
//! - `stripe-cli` - Stripe webhook forwarding
//!
//! ### Custom Services
//! Any Docker image can be used as a local service:
//!
//! ```yaml
//! custom:
//!   build_type:
//!     LocalService:
//!       service_type: custom
//!       image: "my-image:latest"
//!       command: "my-command"
//! ```

pub mod config;
pub mod docker;
pub mod docker_service;
pub mod error;
pub mod health;
pub mod manager;
pub mod process_service;
pub mod service;
#[allow(clippy::module_name_repetitions)]
pub mod r#trait;
pub mod types;

pub use config::{LocalServiceConfig, ServiceDefaults};
pub use docker_service::DockerLocalService;
pub use error::{Error, Result};
pub use health::{HealthCheck, HealthStatus};
pub use manager::LocalServiceManager;
pub use process_service::ProcessLocalService;
pub use r#trait::LocalService;
pub use service::{LocalServiceHandle, ServiceStatus};
pub use types::{LocalServiceType, PortMapping, VolumeMapping};
