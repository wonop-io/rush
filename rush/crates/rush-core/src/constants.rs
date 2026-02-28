//! Constants used throughout the Rush CLI application
//!
//! This module centralizes all string literals, default values, and configuration
//! constants to avoid duplication and make maintenance easier.

// Configuration Files
pub const RUSHD_CONFIG_FILE: &str = "rushd.yaml";
pub const STACK_SPEC_FILE: &str = "stack.spec.yaml";
pub const RUSH_CONFIG_FILE: &str = "rush.yaml";
pub const DOTENV_FILE: &str = ".env";
pub const PACKAGE_JSON_FILE: &str = "package.json";
pub const CARGO_TOML_FILE: &str = "Cargo.toml";
pub const GIT_DIR: &str = ".git";

// Environment Names
pub const ENV_LOCAL: &str = "local";
pub const ENV_DEV: &str = "dev";
pub const ENV_PROD: &str = "prod";
pub const ENV_STAGING: &str = "staging";

/// All valid environment names
pub const VALID_ENVIRONMENTS: &[&str] = &[ENV_LOCAL, ENV_DEV, ENV_PROD, ENV_STAGING];

// Environment Variable Names
pub const DEV_CTX_VAR: &str = "DEV_CTX";
pub const PROD_CTX_VAR: &str = "PROD_CTX";
pub const STAGING_CTX_VAR: &str = "STAGING_CTX";
pub const LOCAL_CTX_VAR: &str = "LOCAL_CTX";

pub const DEV_VAULT_VAR: &str = "DEV_VAULT";
pub const PROD_VAULT_VAR: &str = "PROD_VAULT";
pub const STAGING_VAULT_VAR: &str = "STAGING_VAULT";
pub const LOCAL_VAULT_VAR: &str = "LOCAL_VAULT";

pub const DEV_DOMAIN_VAR: &str = "DEV_DOMAIN";
pub const PROD_DOMAIN_VAR: &str = "PROD_DOMAIN";
pub const STAGING_DOMAIN_VAR: &str = "STAGING_DOMAIN";
pub const LOCAL_DOMAIN_VAR: &str = "LOCAL_DOMAIN";

pub const K8S_ENCODER_DEV_VAR: &str = "K8S_ENCODER_DEV";
pub const K8S_ENCODER_PROD_VAR: &str = "K8S_ENCODER_PROD";
pub const K8S_ENCODER_STAGING_VAR: &str = "K8S_ENCODER_STAGING";
pub const K8S_ENCODER_LOCAL_VAR: &str = "K8S_ENCODER_LOCAL";

pub const K8S_VALIDATOR_DEV_VAR: &str = "K8S_VALIDATOR_DEV";
pub const K8S_VALIDATOR_PROD_VAR: &str = "K8S_VALIDATOR_PROD";
pub const K8S_VALIDATOR_STAGING_VAR: &str = "K8S_VALIDATOR_STAGING";
pub const K8S_VALIDATOR_LOCAL_VAR: &str = "K8S_VALIDATOR_LOCAL";

pub const K8S_VERSION_DEV_VAR: &str = "K8S_VERSION_DEV";
pub const K8S_VERSION_PROD_VAR: &str = "K8S_VERSION_PROD";
pub const K8S_VERSION_STAGING_VAR: &str = "K8S_VERSION_STAGING";
pub const K8S_VERSION_LOCAL_VAR: &str = "K8S_VERSION_LOCAL";

pub const INFRASTRUCTURE_REPOSITORY_VAR: &str = "INFRASTRUCTURE_REPOSITORY";
pub const ONE_PASSWORD_ACCOUNT_VAR: &str = "ONE_PASSWORD_ACCOUNT";
pub const JSON_VAULT_DIR_VAR: &str = "JSON_VAULT_DIR";
pub const HOME_VAR: &str = "HOME";

// Docker Constants
pub const DOCKER_COMMAND: &str = "docker";
pub const DOCKER_PLATFORM_LINUX_AMD64: &str = "linux/amd64";
pub const DOCKER_PLATFORM_LINUX_ARM64: &str = "linux/arm64";
pub const DOCKER_TAG_LATEST: &str = "latest";

/// Returns the Docker platform string for the current host architecture
pub fn docker_platform_native() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => DOCKER_PLATFORM_LINUX_ARM64,
        _ => DOCKER_PLATFORM_LINUX_AMD64,
    }
}

/// Returns the Docker platform string for the given architecture
pub fn docker_platform_for_arch(arch: &str) -> &'static str {
    match arch {
        "aarch64" | "arm64" => DOCKER_PLATFORM_LINUX_ARM64,
        _ => DOCKER_PLATFORM_LINUX_AMD64,
    }
}

// Network Constants
pub const NETWORK_PREFIX: &str = "net-";

// Default Ports
pub const DEFAULT_START_PORT: u16 = 8000;
pub const DEFAULT_COMPONENT_PORT: u16 = 8000;
pub const MIN_PORT: u16 = 1024;
pub const MAX_PORT: u16 = 65535;

// Output Types
pub const OUTPUT_TYPE_STDOUT: &str = "stdout";
pub const OUTPUT_TYPE_FILES: &str = "files";
pub const OUTPUT_TYPE_BOTH: &str = "both";
pub const DEFAULT_LOG_DIR: &str = "logs";

// Build Types
pub const BUILD_TYPE_RUST_BINARY: &str = "RustBinary";
pub const BUILD_TYPE_TRUNK_WASM: &str = "TrunkWasm";
pub const BUILD_TYPE_PURE_DOCKER_IMAGE: &str = "PureDockerImage";
pub const BUILD_TYPE_INGRESS: &str = "Ingress";

// Default Values
pub const DEFAULT_DOCKER_REGISTRY: &str = "";
pub const DEFAULT_ARTEFACT_OUTPUT_DIR: &str = "dist";
pub const DEFAULT_COMPONENT_PRIORITY: u64 = 100;
pub const DEFAULT_TARGET_DIR: &str = "target";

// Mount Points
pub const MOUNT_POINT_ROOT: &str = "/";
pub const MOUNT_POINT_API: &str = "/api";
pub const DEFAULT_WORKING_DIR_PREFIX: &str = "/app";

// Template Variables
pub const TEMPLATE_VAR_COMPONENT: &str = "component";
pub const TEMPLATE_VAR_ENVIRONMENT: &str = "environment";
pub const TEMPLATE_VAR_PRODUCT_NAME: &str = "product_name";
pub const TEMPLATE_VAR_PRODUCT_URI: &str = "product_uri";
pub const TEMPLATE_VAR_SUBDOMAIN: &str = "subdomain";

// Domain Template
pub const DEFAULT_DOMAIN_TEMPLATE: &str = "{{subdomain}}.{{product_uri}}";

// Log Levels and Messages
pub const LOG_PREFIX_LOADING: &str = "Loading";
pub const LOG_PREFIX_BUILDING: &str = "Building";
pub const LOG_PREFIX_LAUNCHING: &str = "Launching";

// File Extensions
pub const YAML_EXTENSION: &str = ".yaml";
pub const YML_EXTENSION: &str = ".yml";
pub const JSON_EXTENSION: &str = ".json";
pub const TOML_EXTENSION: &str = ".toml";
pub const LOG_EXTENSION: &str = ".log";

// Directories
pub const PRODUCTS_DIR: &str = "products";
pub const SRC_DIR: &str = "src";
pub const TARGET_DIR: &str = "target";
pub const DIST_DIR: &str = "dist";

// Git Constants
pub const GIT_COMMAND: &str = "git";
pub const GIT_WIP_PREFIX: &str = "-wip-";
pub const DEFAULT_GIT_HASH_LENGTH: usize = 8;

// Component Names (for default components)
pub const COMPONENT_FRONTEND: &str = "frontend";
pub const COMPONENT_BACKEND: &str = "backend";
pub const COMPONENT_DATABASE: &str = "database";
pub const COMPONENT_INGRESS: &str = "ingress";

// Kubernetes Constants
pub const K8S_DEFAULT_ENCODER: &str = "default";
pub const K8S_DEFAULT_VALIDATOR: &str = "kubevalidator";
pub const K8S_DEFAULT_VERSION: &str = "v1.24.0";

// Error Messages
pub const ERROR_VAULT_NOT_CONFIGURED: &str = "Vault not configured";
pub const ERROR_MISSING_SECRETS: &str = "Missing secrets in vault";
pub const ERROR_INVALID_ENVIRONMENT: &str = "Invalid environment";
pub const ERROR_PRODUCT_PATH_NOT_EXISTS: &str = "Product path does not exist";
pub const ERROR_FAILED_TO_READ_FILE: &str = "Failed to read file";
pub const ERROR_FAILED_TO_PARSE_YAML: &str = "Failed to parse YAML file";

// Test Constants (for test configuration)
pub const TEST_PRODUCT_NAME: &str = "test-product";
pub const TEST_PRODUCT_URI: &str = "test-app";
pub const TEST_ENVIRONMENT: &str = ENV_DEV;
pub const TEST_DOCKER_REGISTRY: &str = "ghcr.io/test";
pub const TEST_COMPONENT_NAME: &str = "test-component";
pub const TEST_DOMAIN: &str = "test.test.app";
pub const TEST_CONTEXT: &str = "test-context";
pub const TEST_VAULT: &str = "test-vault";
pub const TEST_NETWORK: &str = "test-network";

// Claude Code Attribution
pub const CLAUDE_CODE_URL: &str = "https://claude.ai/code";
pub const CLAUDE_EMAIL: &str = "noreply@anthropic.com";
pub const CLAUDE_ATTRIBUTION: &str = "🤖 Generated with [Claude Code](https://claude.ai/code)";
pub const CLAUDE_CO_AUTHOR: &str = "Co-Authored-By: Claude <noreply@anthropic.com>";
