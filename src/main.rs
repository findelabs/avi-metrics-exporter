use std::error::Error;
use chrono::Local;
use clap::{crate_version, App, Arg};
use env_logger::{Builder, Target};
use log::LevelFilter;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Server};
use std::io::Write;

mod server;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

#[tokio::main]
async fn main() -> Result<()> {
    let opts = App::new("avi-exporter")
        .version(crate_version!())
        .author("Daniel F. <Verticaleap>")
        .about("avi networks exporter")
        .arg(
            Arg::with_name("controller")
                .short("c")
                .long("controller")
                .required(true)
                .value_name("AVI_CONTROLLER")
                .env("AVI_CONTROLLER")
                .help("AVI Controller")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("username")
                .short("u")
                .long("username")
                .required(true)
                .value_name("AVI_USERNAME")
                .env("AVI_USERNAME")
                .help("AVI Username")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("password")
                .short("p")
                .long("password")
                .required(true)
                .value_name("AVI_PASSWORD")
                .env("AVI_PASSWORD")
                .help("AVI Password")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("port")
                .short("p")
                .long("port")
                .help("Set port to listen on")
                .required(false)
                .env("LISTEN_PORT")
                .default_value("8080")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("threads")
                .short("t")
                .long("threads")
                .help("Concurrent background threads to use when get'ing metrics")
                .required(false)
                .env("BACKGROUND_THREADS")
                .default_value("4")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("accept_invalid")
                .short("k")
                .long("accept-invalid")
                .help("Accept invalid certs from the connect cluster")
                .required(false)
                .env("ACCEPT_INVALID")
                .default_value("false")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("config")
                .short("c")
                .long("config")
                .required(true)
                .value_name("FILE")
                .help("Config file")
                .takes_value(true),
        )
        .get_matches();

    // Initialize log Builder
    Builder::new()
        .format(|buf, record| {
            writeln!(
                buf,
                "{{\"date\": \"{}\", \"level\": \"{}\", \"message\": \"{}\"}}",
                Local::now().format("%Y-%m-%dT%H:%M:%S:%f"),
                record.level(),
                record.args()
            )
        })
        .target(Target::Stdout)
        .filter_level(LevelFilter::Error)
        .parse_default_env()
        .init();

    // Read in config file
    let port: u16 = opts.value_of("port").unwrap().parse().unwrap_or_else(|_| {
        eprintln!("specified port isn't in a valid range, setting to 8080");
        8080
    });

    let client = server::AviClient::new(opts.clone()).await?;
    let addr = ([0, 0, 0, 0], port).into();
    let service = make_service_fn(move |_| {
        let client = client.clone();
        async move {
            Ok::<_, hyper::Error>(service_fn(move |req: Request<Body>| {
                server::main_handler(req, client.clone())
            }))
        }
    });

    let server = Server::bind(&addr).serve(service);

    println!(
        "Starting avi-exporter:{} on http://{}",
        crate_version!(),
        addr
    );

    server.await?;

    Ok(())
}
