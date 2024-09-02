# Rush

`rush` is short for "Rush Deployment". The key aim of this tool is to bring the development experience as close to the production experience as possible.

`rush` solves a few different problems:

1. It compiles `x86` Docker images on `arm64` to make it possible to deploy `x86` images from Apple Silicon.
2. Images a build by cross compiling into the target architecture. This makes it superfast to build your docker images.
3. Its standard development mode is running multiple containers on your local machine including ingress to route traffic.
4. It makes it easy to manage multiple products in a single repository.


## Quick start

This is how you install:

```bash
cargo install --git https://github.com/wonop-io/rush.git rush
```

Note, however, that `rush` is making use of cross compiling and also compiles frontends, so you will need a few extras to make it work.

## Prerequisite

Make sure you have rustup installed:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

For frontends, you will need to have `trunk` and the wasm target installed:

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk
```

You might want to rename `trunk` as to avoid name conflicts with the CI tool `trunk` (from trunk.io):

```
pushd $HOME/.cargo/bin
mv trunk wasm-trunk
popd
```

`rush` works with either `trunk` or `wasm-trunk` as the executable name. 

Next we install `rush`, the Rust based deployment tool:

```sh
cargo install --git https://github.com/wonop-io/rush.git rush
```

Make sure that the cargo bin directory is in your path:

```sh
source $HOME/.cargo/env
```

If you are on Apple Silicon, `rush` will cross compile all your components into `x86` docker images to make sure that you can deploy them from Apple Silicon onto a Kubernetes cluster. In order to perform cross compiliation, `rush` requires `Docker` and `buildx` to be installed alongside with an `x86_64` toolchain. This  can be installed via brew:

```sh
arch -arm64 brew install SergioBenitez/osxct/x86_64-unknown-linux-gnu
```

You will also need to add the `x86` target for Rust:

```sh
rustup target add x86_64-unknown-linux-gnu
```


## Example

After following the installation above, you can try out one of our examples by checking out this repository and running following command (from anywhere within the repository):

```sh
rush helloworld.com dev
```

Over time we will add more examples in to `products` directory.