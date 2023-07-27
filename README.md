# Connect Four Server

This server hosts and provides remote play functionality for
[connect four website](https://github.com/glubian/connect-four).


# Setup

You will need to have [Rust](https://www.rust-lang.org/learn/get-started)
installed.

Clone the repository:

```sh
git clone 'https://github.com/glubian/connect-four-server.git'
```

Build the executable:

```sh
# for development
cargo build --bin server

# for production
cargo build --bin server --release
```

You can find compiled binaries inside `target` directory.

## Development

Since this server uses HTTPS, you will need to generate an SSL certificate. 
By default, the server will look for `./certs/key.pem` and
`./certs/cert.pem`:

```sh
mkdir certs
cd certs
openssl req -x509 \
            -newkey rsa:4096 \
            -keyout key.pem \
            -out cert.pem \
            -sha256 \
            -days 365 \
            -subj '/CN=localhost'
```

Run this to compile and run the server in development mode:

```sh
RUST_LOG=connect_four_server=debug cargo run --bin server
```

Connecting to the server on `localhost` from the website requires these
few steps:

1. Connect to the server in your browser using HTTPS. The server is hosted at 
   `https://localhost:8080` by default.
2. You might see a warning that the connection is unsecure, press the button to
   temporarily allow connections. This is fine as the server is running locally
   and no data ever leaves your computer.
3. If everything works, you should see HTTP error 400 (Bad Request), as the 
   server expects only WebSocket connection requests.

**IMPORTANT:** Repeat these steps if at some point you get errors while trying
to connect.

## Cargo commands

### Run in development mode

```sh
RUST_LOG=connect_four_server=debug cargo run --bin server
```

`RUST_LOG` controls logging, see the 
[env_logger](https://docs.rs/env_logger/0.10.0/env_logger/#enabling-logging)
crate for more info.

### Run a production build

```sh
cargo build --bin server --release
```

### Run unit tests

```sh
cargo test
```

### Format the entire project

```sh
cargo fmt
```

### Play connect four in your command line
```sh
cargo run --bin cli
```
Runs a small command line application intended for testing.
The code is in `src/bin/cli.rs`.


# Configuring

## Default settings

By default, the server should work well for development purposes:

- Accepts WebSocket connections at `wss://localhost:8080`
- Looks for private key file under `./certs/key.pem`
- Looks for certificate chain file under `./certs/cert.pem`
- Hosts up to 100 concurrent lobbies, each can hold up to 20 players

It works well with Vue development server, usually hosted at
`http://localhost:3333`.

## Working with CLI

If you are comfortable with CLI and know how cargo works, 
`--print-config` and `--help` should get you started right away. Otherwise
the rest of this section will guide you through the configuration.

### Passing flags with cargo

For brevity, I will use `./server` to start the server. To pass arguments using
cargo, replace it with:

```sh
cargo run --bin server --
```

That `--` at the end tells cargo to forward any remaining
arguments to the binary. This means

```sh
cargo run --bin server -- --help
```

passes the same arguments to the server as

```sh
./server --help
```

### Saving and loading configurations

To print the default config file, run:

```sh
./server --print-config
```

The config written is in [TOML](https://github.com/toml-lang/toml). 
`--print-config` always prints all existing settings regardless of whether
they were modified.

It can be saved to a file using:

```sh
./server --print-config > config.toml
```

The settings can be loaded from `config.toml` with:

```sh
./server --config config.toml
```

You can modify the configuration by hand, or through command line:

```sh
./server --config config.toml -a 192.168.0.1 -s 443 --print-config > config.toml
```

This command:

1. Reads settings from `config.toml`
2. Sets the address to `192.168.0.1`
3. Sets the port to `443`
4. Writes a new `config.toml`, containing all settings

Any options you set will override those read from the config.
See `--help` for the list of all options.

## Hosting configuration example

```toml
# replace this with the URL of your domain
url_base = "https://yourdomain/"

# configure which address and socket to use
address = "192.168.0.101"
socket = 443

# point these to your certificate files
private_key_file = "./certs/key.pem"
certificate_chain_file = "./certs/cert.pem"

# in a production environment, these will need to be higher
max_lobbies = 100 # maximum concurrent lobbies
max_players = 20  # maximum players in a lobby
```



# License

[MIT](https://github.com/glubian/connect-four-server/blob/main/LICENSE)
