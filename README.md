# Connect Four Server

Play connect four remotely.

This server hosts and provides remote play functionality for
[Connect Four website](https://github.com/glubian/connect-four).


# Development setup

You will need to have [Rust](https://www.rust-lang.org/learn/get-started)
installed.

Clone the repository:

```sh
git clone 'https://github.com/glubian/connect-four-server.git'
```

Since this server uses HTTPS, you will need to generate an SSL certificate
for development. By default, the server will look for `./certs/key.pem` and
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

...and that's it! Cargo will handle everything else for you.

## Cargo commands

### Run in development mode

```sh
RUST_LOG=connect_four_server=debug cargo run --bin server
```

`RUST_LOG` controls logging, see the [log](https://docs.rs/log/latest/log/)
crate for more info.

By default, the server is serving files from `./static`
at `https://localhost:8080`.

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

By default, the server should work for development:

- Hosts contents of `./static` directory at `https://localhost:8080` and accepts
  WebSocket connections at `wss://localhost:8080/ws`
- localhost does not require HTTPS, so it works with Vue development server,
  usually hosted at `http://localhost:3333`
- Looks for private key file under `./certs/key.pem`
- Looks for certificate chain file under `./certs/cert.pem`
- Hosts up to 100 concurrent lobbies, each can hold up to 20 players

If you are comfortable with CLI and know how cargo works, 
`--print-config` and `--help` should get you started right away. Otherwise
the rest of this section will guide you through the configuration.

## Passing flags with cargo

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

## The default configuration

To print the default config file, run:

```sh
./server --print-config
```

The config written is in [TOML](https://github.com/toml-lang/toml). 
It can be saved to a file using:

```sh
./server --print-config > config.toml
```

Now we can apply settings from `config.toml` using:

```sh
./server --config config.toml
```

You can modify the configuration by hand, or using command line options:

```sh
./server --config config.toml -a 192.168.0.1 -s 443 --print-config > config.toml
```

This command:

1. Reads all settings from `config.toml`
2. Sets the address to `192.168.0.1`
3. Sets the port to `443`
4. Writes all changes to `config.toml`

Any options you set will override those read from the config.
See `--help` for the list of all options.

## Hosting configuration example

```toml
# replace this with the URL of your domain
url_base = "https://yourdomain/"

# configure which address and socket to use
address = "192.168.0.101"
socket = 443

# set this to your hosting directory
serve_from = "./static"

# point these to your certificate files
private_key_file = "./certs/key.pem"
certificate_chain_file = "./certs/cert.pem"

# in a production environment, these will need to be higher
max_lobbies = 100 # maximum concurrent lobbies
max_players = 20  # maximum players in a lobby
```



# License

[MIT](https://github.com/glubian/connect-four-server/blob/main/LICENSE)
