use std::{path::PathBuf, time::Duration};

use super::config::AppConfigPartial;

const VERSION: &str = "connect-four-server, version 1.0.0";

const HELP: &str = "\
USAGE: 
  connect-four-server [OPTIONS]

OPTIONS:
  -b --url-base <URL_BASE>                  URL base used to generate invites
     --url-lobby-parameter <URL_PARAMETER>  URL lobby parameter
  -p --port <PORT>                          Port to use
  -a --address <ADDRESS>                    Address to use
     --private-key-file <FILE>              Private key file
     --cert-chain-file <FILE>               Certificate chain file
  -s --serve-from <DIR>                     Directory containing website files
     --max-lobbies <AMOUNT>                 Maximum lobbies
     --max-players <AMOUNT>                 Maximum players in a lobby (0-255)
     --heartbeat-interval <SECONDS>         Player ping interval in seconds, 0 to disable
     --heartbeat-timeout <SECONDS>          Player ping timeout in seconds, 0 to disable
  -c --config <FILE>                        Configuration file. Any command line options override configuration settings.
     --print-config                         Print configuration file and exit
     --version                              Show version and exit
  -h --help                                 Show this message and exit
";

pub struct AppArgs {
    pub partial_config: AppConfigPartial,
    pub config: Option<PathBuf>,
    pub print_config: bool,
}

impl AppArgs {
    pub fn from_env() -> Result<Self, pico_args::Error> {
        let mut pargs = pico_args::Arguments::from_env();

        if pargs.contains(["-h", "--help"]) {
            print!("{VERSION}\n\n{HELP}");
            std::process::exit(0);
        }

        if pargs.contains(["-v", "--version"]) {
            println!("{VERSION}");
            std::process::exit(0);
        }

        #[inline]
        fn exit_on_err<T>(res: Result<T, pico_args::Error>) -> T {
            match res {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
        }

        let heartbeat_interval: Option<f64> =
            exit_on_err(pargs.opt_value_from_str("--heartbeat-interval"));
        let heartbeat_timeout: Option<f64> =
            exit_on_err(pargs.opt_value_from_str("--heartbeat-timeout"));

        let partial_config = AppConfigPartial {
            url_base: exit_on_err(pargs.opt_value_from_str(["-b", "--url-base"])),
            url_lobby_parameter: exit_on_err(pargs.opt_value_from_str("--url-lobby-parameter")),
            socket: exit_on_err(pargs.opt_value_from_str(["-p", "--port"])),
            address: exit_on_err(pargs.opt_value_from_str(["-a", "--address"])),
            private_key_file: exit_on_err(pargs.opt_value_from_str("--private-key-file")),
            certificate_chain_file: exit_on_err(pargs.opt_value_from_str("--cert-chain-file")),
            serve_from: exit_on_err(pargs.opt_value_from_str(["-s", "--serve-from"])),
            max_lobbies: exit_on_err(pargs.opt_value_from_str("--max-lobbies")),
            max_players: exit_on_err(pargs.opt_value_from_str("--max-players")),
            heartbeat_interval: heartbeat_interval.map(Duration::from_secs_f64),
            heartbeat_timeout: heartbeat_timeout.map(Duration::from_secs_f64),
        };

        let args = Self {
            partial_config,
            config: exit_on_err(pargs.opt_value_from_str(["-c", "--config"])),
            print_config: pargs.contains("--print-config"),
        };

        let extra_args = pargs.finish();
        if extra_args.len() > 0 {
            let plural = if extra_args.len() == 1 { "" } else { "s" };
            let mut arg_list = String::new();

            for arg in extra_args {
                if arg_list.len() > 0 {
                    arg_list.push_str(", ");
                }
                arg_list.push_str(&arg.to_string_lossy());
            }

            eprintln!("Unknown argument{}: {}", plural, arg_list);
            std::process::exit(1);
        }

        Ok(args)
    }
}
