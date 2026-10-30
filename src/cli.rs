//! `discord` command tree (clap derive).

use clap::{Parser, Subcommand};
use clap_complete::Shell;
use pk_cli_auth::{LoginArgs, LogoutArgs, SetCredentialArgs};
use pk_cli_core::CommonArgs;
use pk_cli_http::ApiArgs;
use pk_cli_selfupdate::SelfUpdateArgs;

pub const BIN: &str = "discord";
pub const REPO: &str = "piekstra/discord-cli";

/// Discord from the command line (conforms to piekstra-cli/1).
#[derive(Parser, Debug)]
#[command(name = BIN, version, about, long_about = None)]
pub struct Cli {
    #[command(flatten)]
    pub common: CommonArgs,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Bot-token management and status.
    #[command(subcommand)]
    Auth(AuthCmd),
    /// Show the authenticated bot user (user/v1).
    Me,
    /// List the guilds (servers) the bot has joined (guilds/v1).
    Guilds,
    /// List the channels of a guild (channels/v1).
    Channels {
        /// Guild ID or name; defaults to the configured `guild`.
        guild: Option<String>,
    },
    /// Read recent messages from a channel, oldest first (messages/v1).
    Messages(MessagesArgs),
    /// Send a message to a channel (message/v1).
    Send(SendArgs),
    /// Delete a message from a channel.
    Delete(DeleteArgs),
    /// Manage the Discord application behind the bot.
    #[command(subcommand)]
    App(AppCmd),
    /// Non-secret settings (default guild, …).
    #[command(subcommand)]
    Config(ConfigCmd),
    /// Raw authenticated request against the Discord REST API (v10).
    Api(ApiArgs),
    /// Update to the latest release from GitHub.
    SelfUpdate(SelfUpdateArgs),
    /// Print a shell completion script.
    Completions { shell: Shell },
    /// Machine-readable capability discovery (cli-info/v1).
    Info,
}

#[derive(Subcommand, Debug)]
pub enum AuthCmd {
    /// Verify a bot token and store it in the OS keychain.
    Login(LoginArgs),
    /// Report credential state (auth-status/v1).
    Status,
    /// Remove the stored bot token (with --forget).
    Logout(LogoutArgs),
    /// Raw keychain write for rotation / headless setup.
    SetCredential(SetCredentialArgs),
}

#[derive(clap::Args, Debug)]
pub struct MessagesArgs {
    /// Channel ID, or a #name resolved within --guild / the configured guild.
    pub channel: String,
    /// How many messages to fetch (1–100).
    #[arg(short = 'n', long, default_value_t = 25, value_parser = clap::value_parser!(u8).range(1..=100))]
    pub limit: u8,
    /// Only messages before this message ID (for paging back).
    #[arg(long, value_name = "MESSAGE_ID", conflicts_with = "after")]
    pub before: Option<String>,
    /// Only messages after this message ID.
    #[arg(long, value_name = "MESSAGE_ID")]
    pub after: Option<String>,
    /// Guild ID or name used to resolve a channel given by name.
    #[arg(short, long)]
    pub guild: Option<String>,
}

#[derive(clap::Args, Debug)]
pub struct SendArgs {
    /// Channel ID, or a #name resolved within --guild / the configured guild.
    pub channel: String,
    /// Message text (≤ 2000 chars); omit to read it from stdin.
    pub message: Option<String>,
    /// Send as a reply to this message ID.
    #[arg(long, value_name = "MESSAGE_ID")]
    pub reply_to: Option<String>,
    /// Guild ID or name used to resolve a channel given by name.
    #[arg(short, long)]
    pub guild: Option<String>,
}

#[derive(clap::Args, Debug)]
pub struct DeleteArgs {
    /// Channel ID, or a #name resolved within --guild / the configured guild.
    pub channel: String,
    /// ID of the message to delete.
    pub message: String,
    /// Delete without the interactive confirmation prompt.
    #[arg(long)]
    pub force: bool,
    /// Guild ID or name used to resolve a channel given by name.
    #[arg(short, long)]
    pub guild: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum AppCmd {
    /// Show the bot's application (app/v1). Uses the stored bot token.
    Get,
    /// Show or toggle privileged gateway intents (intents/v1).
    #[command(subcommand)]
    Intents(IntentsCmd),
    /// Print the OAuth2 URL that invites this bot to a server.
    InviteUrl {
        /// Permissions integer (default: view channels + send messages +
        /// read message history).
        #[arg(long, default_value_t = 68608)]
        permissions: u64,
    },
    /// List your applications. Opens an interactive browser (portal).
    List(PortalArgs),
    /// Create a new application. Opens an interactive browser (portal).
    Create {
        /// Name for the new application.
        name: String,
        #[command(flatten)]
        portal: PortalArgs,
    },
    /// Reset the bot token and store it in the keychain. Opens an
    /// interactive browser (portal).
    ResetToken {
        /// Application ID; defaults to the currently authenticated app.
        app_id: Option<String>,
        #[command(flatten)]
        portal: PortalArgs,
    },
    /// Forget the cached portal browser session (deletes its profile).
    Logout,
}

#[derive(clap::Args, Debug)]
pub struct PortalArgs {
    /// 1Password item holding your Discord login — the browser logs itself
    /// in via the `op` CLI (TOTP included). Defaults to config `op-item`.
    #[arg(long, value_name = "ITEM")]
    pub op_item: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum IntentsCmd {
    /// Show the privileged intents and raw application flags.
    Show,
    /// Enable a privileged intent: message-content, guild-members, presence.
    Enable { intent: String },
    /// Disable a privileged intent.
    Disable { intent: String },
}

#[derive(Subcommand, Debug)]
pub enum ConfigCmd {
    /// Print the resolved config file path.
    Path,
    /// Show the effective configuration.
    Show,
    /// Set a config key (e.g. `config set guild my-server`).
    Set { key: String, value: String },
    /// Remove a config key.
    Unset { key: String },
}
