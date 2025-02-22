#[macro_use]
extern crate clap;

mod cli;
mod config;

pub use beacon_chain;
pub use cli::cli_app;
pub use client::{Client, ClientBuilder, ClientConfig, ClientGenesis};
pub use eth2_config::Eth2Config;

use beacon_chain::{
    builder::Witness, eth1_chain::CachingEth1Backend, events::WebSocketSender,
    lmd_ghost::ThreadSafeReducedTree, slot_clock::SystemTimeSlotClock,
};
use clap::ArgMatches;
use config::get_configs;
use environment::RuntimeContext;
use futures::{Future, IntoFuture};
use slog::{info, warn};
use std::ops::{Deref, DerefMut};
use store::{migrate::BackgroundMigrator, DiskStore};
use types::EthSpec;

/// A type-alias to the tighten the definition of a production-intended `Client`.
pub type ProductionClient<E> = Client<
    Witness<
        DiskStore,
        BackgroundMigrator<E>,
        SystemTimeSlotClock,
        ThreadSafeReducedTree<DiskStore, E>,
        CachingEth1Backend<E, DiskStore>,
        E,
        WebSocketSender<E>,
    >,
>;

/// The beacon node `Client` that will be used in production.
///
/// Generic over some `EthSpec`.
///
/// ## Notes:
///
/// Despite being titled `Production...`, this code is not ready for production. The name
/// demonstrates an intention, not a promise.
pub struct ProductionBeaconNode<E: EthSpec>(ProductionClient<E>);

impl<E: EthSpec> ProductionBeaconNode<E> {
    /// Starts a new beacon node `Client` in the given `environment`.
    ///
    /// Identical to `start_from_client_config`, however the `client_config` is generated from the
    /// given `matches` and potentially configuration files on the local filesystem or other
    /// configurations hosted remotely.
    pub fn new_from_cli<'a, 'b>(
        mut context: RuntimeContext<E>,
        matches: &ArgMatches<'b>,
    ) -> impl Future<Item = Self, Error = String> + 'a {
        let log = context.log.clone();

        // TODO: the eth2 config in the env is being modified.
        //
        // See https://github.com/sigp/lighthouse/issues/602
        get_configs::<E>(&matches, context.eth2_config.clone(), log)
            .into_future()
            .and_then(move |(client_config, eth2_config, _log)| {
                context.eth2_config = eth2_config;
                Self::new(context, client_config)
            })
    }

    /// Starts a new beacon node `Client` in the given `environment`.
    ///
    /// Client behaviour is defined by the given `client_config`.
    pub fn new(
        context: RuntimeContext<E>,
        client_config: ClientConfig,
    ) -> impl Future<Item = Self, Error = String> {
        let http_eth2_config = context.eth2_config().clone();
        let spec = context.eth2_config().spec.clone();
        let genesis_eth1_config = client_config.eth1.clone();
        let client_genesis = client_config.genesis.clone();
        let log = context.log.clone();

        let db_path_res = client_config.create_db_path();
        let freezer_db_path_res = client_config.create_freezer_db_path();

        db_path_res
            .into_future()
            .and_then(move |db_path| {
                Ok(ClientBuilder::new(context.eth_spec_instance.clone())
                    .runtime_context(context)
                    .chain_spec(spec)
                    .disk_store(&db_path, &freezer_db_path_res?)?
                    .background_migrator()?)
            })
            .and_then(move |builder| {
                builder.beacon_chain_builder(client_genesis, genesis_eth1_config)
            })
            .and_then(move |builder| {
                let builder = if client_config.sync_eth1_chain && !client_config.dummy_eth1_backend
                {
                    info!(
                        log,
                        "Block production enabled";
                        "endpoint" => &client_config.eth1.endpoint,
                        "method" => "json rpc via http"
                    );
                    builder.caching_eth1_backend(client_config.eth1.clone())?
                } else if client_config.dummy_eth1_backend {
                    warn!(
                        log,
                        "Block production impaired";
                        "reason" => "dummy eth1 backend is enabled"
                    );
                    builder.dummy_eth1_backend()?
                } else {
                    info!(
                        log,
                        "Block production disabled";
                        "reason" => "no eth1 backend configured"
                    );
                    builder.no_eth1_backend()?
                };

                let builder = builder
                    .system_time_slot_clock()?
                    .websocket_event_handler(client_config.websocket_server.clone())?
                    .build_beacon_chain()?
                    .libp2p_network(&client_config.network)?;

                let builder = if client_config.rest_api.enabled {
                    builder.http_server(&client_config, &http_eth2_config)?
                } else {
                    builder
                };

                let builder = builder.peer_count_notifier()?.slot_notifier()?;

                Ok(Self(builder.build()))
            })
    }

    pub fn into_inner(self) -> ProductionClient<E> {
        self.0
    }
}

impl<E: EthSpec> Deref for ProductionBeaconNode<E> {
    type Target = ProductionClient<E>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<E: EthSpec> DerefMut for ProductionBeaconNode<E> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
