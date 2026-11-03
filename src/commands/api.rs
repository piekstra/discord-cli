//! `discord api` — raw authenticated passthrough against the Discord REST
//! API (v10). Output is always the JSON payload, per the family contract.

use pk_cli_core::{output, CliError};
use pk_cli_http::ApiArgs;

use crate::client::API_BASE;
use crate::commands::Ctx;

pub fn run(ctx: &Ctx, args: &ApiArgs) -> Result<(), CliError> {
    // The bot token goes with this request, so an `http://` URL is refused
    // before the keychain is touched.
    let url = args.secure_url(API_BASE)?;
    let method = args.parsed_method()?;
    let body = args.parsed_body()?;
    let dc = ctx.connect()?;
    let v = dc.request(method, &url, body.as_ref())?;
    output::json(&v);
    Ok(())
}
