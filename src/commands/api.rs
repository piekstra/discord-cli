//! `discord api` — raw authenticated passthrough against the Discord REST
//! API (v10). Output is always the JSON payload, per the family contract.

use pk_cli_core::{output, CliError};
use pk_cli_http::ApiArgs;

use crate::client::API_BASE;
use crate::commands::Ctx;

pub fn run(ctx: &Ctx, args: &ApiArgs) -> Result<(), CliError> {
    let dc = ctx.connect()?;
    let method = args.parsed_method()?;
    let body = args.parsed_body()?;
    let v = dc.request(method, &args.url(API_BASE), body.as_ref())?;
    output::json(&v);
    Ok(())
}
