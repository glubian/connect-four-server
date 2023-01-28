use std::{
    fmt, io,
    process::{self, ExitCode},
    str::FromStr,
};

use actix::{Actor, Addr, MailboxError};
use actix_files::{Files, NamedFile};
use actix_web::{rt, web};
use actix_web::{App, HttpRequest, HttpResponse, HttpServer};
use actix_web_actors::ws::WsResponseBuilder;
use log::debug;
use openssl::ssl::{SslAcceptor, SslFiletype, SslMethod};
use qstring::QString;
use uuid::Uuid;

use web::Data;

use connect_four_server::server::actor::{
    self,
    lobby_router::{CreateLobby, JoinLobby},
    player::Disconnect,
};
use connect_four_server::server::{AppArgs, AppConfig};

fn get_config() -> AppConfig {
    let args = match AppArgs::from_env() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("{e}");
            process::exit(1);
        }
    };

    let mut cfg = match &args.config {
        Some(path) => match AppConfig::from_file(path) {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!("{e}");
                process::exit(1);
            }
        },
        None => AppConfig::default(),
    };

    cfg.apply_partial(args.partial_config);

    if args.print_config {
        match toml::to_string_pretty(&cfg) {
            Ok(cfg_contents) => {
                print!("{cfg_contents}");
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("{e}");
                process::exit(1);
            }
        }
    }

    cfg
}

fn main() -> ExitCode {
    let cfg = get_config();

    env_logger::init();

    match rt::System::new().block_on(main_actix(cfg)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

async fn main_actix(cfg: AppConfig) -> Result<(), ServerError> {
    let cfg = Data::new(cfg);

    let mut builder = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
    builder
        .set_private_key_file(&cfg.private_key_file, SslFiletype::PEM)
        .map_err(|e| ServerError::OpenSsl(e))?;
    builder
        .set_certificate_chain_file(&cfg.certificate_chain_file)
        .map_err(|e| ServerError::OpenSsl(e))?;

    let lobby_router = actor::LobbyRouter::new(Data::clone(&cfg).into_inner()).start();
    let cfg_1 = Data::clone(&cfg);
    HttpServer::new(move || {
        let mut app = App::new()
            .app_data(Data::new(lobby_router.clone()))
            .app_data(Data::clone(&cfg_1))
            .route("/ws", web::get().to(ws_route));

        if let Ok(index) = NamedFile::open(cfg_1.serve_from.join("index.html")) {
            app = app.route("/", web::get().service(index));
        }

        app.service(Files::new("/", &cfg_1.serve_from).prefer_utf8(true))
            .default_service(web::get().to(not_found))
    })
    .bind_openssl((cfg.address.clone(), cfg.socket), builder)
    .map_err(|e| ServerError::IO(e))?
    .run()
    .await
    .map_err(|e| ServerError::IO(e))
}

async fn not_found() -> HttpResponse {
    HttpResponse::NotFound().body("404 Not Found")
}

async fn ws_route(
    req: HttpRequest,
    stream: web::Payload,
    cfg: Data<AppConfig>,
    router: Data<Addr<actor::LobbyRouter>>,
) -> Result<HttpResponse, actix_web::Error> {
    let qs = QString::from(req.query_string());
    let id_str = qs.get(&cfg.url_lobby_parameter);
    if let Some(Ok(id)) = id_str.map(Uuid::from_str) {
        let actor_cfg = Data::clone(&cfg).into_inner();
        let actor = actor::Player::new(actor_cfg);
        let (addr, res) = WsResponseBuilder::new(actor, &req, stream).start_with_addr()?;
        let msg = JoinLobby {
            id,
            player: addr.clone(),
        };

        match router.send(msg).await {
            Ok(()) => (),
            Err(MailboxError::Closed) => addr.do_send(Disconnect::ShuttingDown),
            Err(MailboxError::Timeout) => {
                debug!("Encountered an error while trying to route player to lobby {}, connection will be terminated", id);
                addr.do_send(Disconnect::ServerOverloaded);
            }
        }

        Ok(res)
    } else if id_str.is_some() {
        let actor_cfg = Data::clone(&cfg).into_inner();
        let actor = actor::Player::new(actor_cfg);
        let (addr, res) = WsResponseBuilder::new(actor, &req, stream).start_with_addr()?;
        addr.do_send(Disconnect::InviteInvalid);
        Ok(res)
    } else {
        let actor_app_config = cfg.clone().into_inner();
        let actor = actor::Player::new(actor_app_config);
        let (addr, res) = WsResponseBuilder::new(actor, &req, stream).start_with_addr()?;
        let msg = CreateLobby { host: addr.clone() };

        match router.send(msg).await {
            Ok(()) => (),
            Err(MailboxError::Closed) => addr.do_send(Disconnect::ShuttingDown),
            Err(MailboxError::Timeout) => {
                debug!("Encountered an error while trying to route player to a new lobby, connection will be terminated");
                addr.do_send(Disconnect::ServerOverloaded);
            }
        }

        Ok(res)
    }
}

#[derive(Debug)]
enum ServerError {
    IO(io::Error),
    OpenSsl(openssl::error::ErrorStack),
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IO(e) => write!(f, "io error: {e}"),
            Self::OpenSsl(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ServerError {}
