# Rush README

## Overview

`Rush` (Rush Deployment) is a Rust-based deployment tool that aims to bridge the gap between development and production environments by allowing cross-compilation of `x86` Docker images on `arm64` platforms, such as Apple Silicon. This ensures developers can build and deploy `x86` images from Apple Silicon without the need for separate environments. It also simplifies managing multiple products in a single repository and running multiple containers locally for development with ingress routing traffic.

## Key Features

- **Cross-compilation:** Automatically cross-compiles Docker images to the target architecture (e.g., `x86` on Apple Silicon).
- **Fast builds:** Efficient cross-compiling speeds up Docker image building.
- **Multi-container support:** Easily run multiple containers locally, complete with ingress routing.
- **Multi-product management:** Simplifies the handling of multiple products within a monorepo structure.
- **Multi-environment support:** Supports different environments (`local`, `dev`, `staging` and `prod`) with separate configurations and secrets.
- **Secret management:** Manages secrets for local development and deployment. Supports 1Password and Kubeseal out of the box.

## Demo
![Hello World Demo](demos/hello_world.gif)

---

## Installation

### Prerequisites

Before installing `rush`, ensure that you have the following prerequisites installed:

1. **Rust** via `rustup`:
   ```sh
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Trunk** (for frontend builds):
   - Install the WebAssembly (WASM) target:
     ```sh
     rustup target add wasm32-unknown-unknown
     ```
   - Install `trunk`:
     ```sh
     cargo install trunk
     ```
   - (Optional) Rename `trunk` to avoid conflicts with other CI tools:
     ```sh
     pushd $HOME/.cargo/bin
     mv trunk wasm-trunk
     popd
     ```

3. **Docker** and **buildx**: Ensure that Docker and Docker Buildx are installed for cross-compilation.

4. **Toolchains (Apple Silicon)**: For Apple Silicon users, cross-compilation requires installing the `x86_64` toolchain and Rust targets:
   ```sh
   arch -arm64 brew install SergioBenitez/osxct/x86_64-unknown-linux-gnu
   rustup target add x86_64-unknown-linux-gnu
   ```

### Installing `rush`

To install `rush`, run the following command:
```bash
cargo install --git https://github.com/wonop-io/rush.git rush
```

Make sure that the cargo binary directory is in your `PATH`:
```sh
source $HOME/.cargo/env
```

If you have already installed `rush`, you can update it by running the installation command again.

---

## Quick Start

### Running a Simple Example

Once you’ve followed the installation steps, you can test `rush` with one of its examples. From anywhere within the cloned repository, run:

```sh
rush helloworld.wonop.io dev
```

This will start the development server for the `helloworld.wonop.io` example.

### Local Development Setup

1. **Initialize Secrets:**
   Initialize the required secrets for your local environment:
   ```sh
   rush helloworld.wonop.io secrets init
   ```
   This will generate `.env` files containing environment variables and secrets necessary for running your platform locally. For PostgreSQL and Redis, use the following connection strings:
   - PostgreSQL: `postgres://admin:admin@localhost:5433/backend`
   - Redis: `redis://localhost:6379`

2. **Start the Development Server:**
   Once the secrets and environment variables are set, start the development server:
   ```sh
   rush helloworld.wonop.io dev
   ```
   The application will be available at `http://localhost:9000`.

---

## Kubernetes Deployment

For deploying to a Kubernetes cluster, `rush` provides seamless integration. Follow these steps for configuring secrets and setting up the environment:

1. **Initialize Secrets for Staging:**
   Run the following command to configure secrets for the staging environment:
   ```sh
   rush --env staging helloworld.wonop.io secrets init
   ```

2. **Enter Database and Redis URLs:**
   During the initialization, you’ll be prompted to enter the database and Redis URLs:
   - Example database URL: `postgres://[user]:[password]@[host]:[port]/[database_name]`
   - Example Redis URL: `rediss://[user]:[password]@[host]:[port]`

---

## Cross-Compilation on Apple Silicon

For Apple Silicon users, `rush` cross-compiles `x86` images for deployment onto `x86` Kubernetes clusters. Ensure that you have the `x86_64` toolchain and Docker Buildx configured. Follow these steps:

1. Install the necessary toolchain:
   ```sh
   arch -arm64 brew install SergioBenitez/osxct/x86_64-unknown-linux-gnu
   ```
   
2. Add the `x86_64` target for Rust:
   ```sh
   rustup target add x86_64-unknown-linux-gnu
   ```

`rush` will automatically handle cross-compiling Docker images into `x86` format, making them compatible with your production environment.

---

## Advanced Usage

### Managing Multiple Products in a Monorepo

`rush` is designed to handle multiple products within a single repository. You can create a new product by simply structuring your directory as follows:
```
/products
  /io.wonop.helloworld
  /io.wonop.app
  /io.wonop.api
```

Running `rush` from the repository root will manage all products simultaneously. This simplifies development workflows when working with large, multi-product projects.

