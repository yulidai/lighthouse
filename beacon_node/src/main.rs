mod run;

use clap::{App, Arg};
use client::ClientConfig;
use env_logger::{Builder, Env};
use slog::error;

fn main() {
    // debugging output for libp2p and external crates
    Builder::from_env(Env::default()).init();

    let matches = App::new("Lighthouse")
        .version(version::version().as_str())
        .author("Sigma Prime <contact@sigmaprime.io>")
        .about("Eth 2.0 Client")
        // file system related arguments
        .arg(
            Arg::with_name("datadir")
                .long("datadir")
                .value_name("DIR")
                .help("Data directory for keys and databases.")
                .takes_value(true),
        )
        // network related arguments
        .arg(
            Arg::with_name("listen-address")
                .long("listen-address")
                .value_name("Listen Address")
                .help("The Network address to listen for p2p connections.")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("port")
                .long("port")
                .value_name("PORT")
                .help("Network listen port for p2p connections.")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("boot-nodes")
                .long("boot-nodes")
                .value_name("BOOTNODES")
                .help("A list of comma separated multi addresses representing bootnodes to connect to.")
                .takes_value(true),
        )
        // rpc related arguments
        .arg(
            Arg::with_name("rpc")
                .long("rpc")
                .value_name("RPC")
                .help("Enable the RPC server.")
                .takes_value(false),
        )
        .arg(
            Arg::with_name("rpc-address")
                .long("rpc-address")
                .value_name("RPCADDRESS")
                .help("Listen address for RPC endpoint.")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("rpc-port")
                .long("rpc-port")
                .value_name("RPCPORT")
                .help("Listen port for RPC endpoint.")
                .takes_value(true),
        )
        // logging related arguments
        .arg(
            Arg::with_name("verbosity")
                .short("v")
                .multiple(true)
                .help("Sets the verbosity level")
                .takes_value(true),
        )
        .get_matches();

    // invalid arguments, panic
    let (logger, config) = ClientConfig::parse_args(matches).unwrap();

    match run::run_beacon_node(config, &logger) {
        Ok(_) => {}
        Err(e) => error!(logger, "Beacon node failed because {:?}", e),
    }
}
