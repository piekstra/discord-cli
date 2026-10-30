//! `discord` — Discord from the command line. Thin entrypoint: parse args,
//! build the shared context, dispatch to one handler per command group.

mod cli;
mod client;
mod commands;
mod config;
mod portal;

use clap::Parser;
use pk_cli_core::{output, CliError};
use pk_cli_selfupdate::Updater;

use cli::{Cli, Command, BIN, REPO};
use commands::Ctx;

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(&cli) {
        std::process::exit(output::fail(&e, cli.common.json));
    }
}

fn run(cli: &Cli) -> Result<(), CliError> {
    let ctx = Ctx::new(cli);
    match &cli.command {
        Command::Auth(cmd) => commands::auth::run(&ctx, cmd),
        Command::Me => commands::me::run(&ctx),
        Command::Guilds => commands::guilds::run(&ctx),
        Command::Channels { guild } => commands::channels::run(&ctx, guild.as_deref()),
        Command::Messages(args) => commands::messages::run(&ctx, args),
        Command::Send(args) => commands::send::run(&ctx, args),
        Command::Delete(args) => commands::delete::run(&ctx, args),
        Command::App(cmd) => commands::app::run(&ctx, cmd),
        Command::Config(cmd) => commands::config::run(&ctx, cmd),
        Command::Api(args) => commands::api::run(&ctx, args),
        Command::SelfUpdate(args) => Updater {
            repo: REPO.into(),
            binary: BIN.into(),
            target: env!("BUILD_TARGET").into(),
            current: env!("CARGO_PKG_VERSION").into(),
        }
        .run(args, cli.common.json, cli.common.quiet),
        Command::Completions { shell } => {
            use clap::CommandFactory;
            clap_complete::generate(*shell, &mut Cli::command(), BIN, &mut std::io::stdout());
            Ok(())
        }
        Command::Info => commands::info::run(),
    }
}
