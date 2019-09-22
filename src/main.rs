#![feature(async_await)]

extern crate futures;
extern crate tide;
extern crate tokio;
#[macro_use]
extern crate rust_embed;
extern crate bytes;
extern crate mime_guess;
extern crate uuid;
#[macro_use]
extern crate log;
extern crate env_logger;
extern crate structopt;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate failure;

use futures::{compat::Executor01CompatExt, future::FutureExt, task::SpawnExt};
use mime_guess::guess_mime_type;
use tide::{
    http::{response::Builder as ResponseBuilder, StatusCode},
    middleware::RequestLogger,
    App, Context, EndpointResult, Response,
};
use tokio::{prelude::Future as Future01, runtime::Runtime};

use std::env;

mod api;
mod bitmap;
mod error;
mod opt;
mod state;

use api::*;
use opt::OPT;
use state::State;

#[derive(RustEmbed)]
#[folder = "web/"]
struct Asset;

fn handle_embedded_file(mut path: &str) -> Response {
    if path.starts_with("/") {
        path = &path[1..];
    }
    match Asset::get(path) {
        Some(content) => ResponseBuilder::new()
            .status(StatusCode::OK)
            .header("Content-Type", guess_mime_type(path).as_ref())
            .body(content.as_ref().into())
            .unwrap(),
        None => ResponseBuilder::new()
            .status(StatusCode::NOT_FOUND)
            .body("404 Not Found".into())
            .unwrap(),
    }
}

async fn handle_index(_ctx: Context<State>) -> EndpointResult {
    Ok(handle_embedded_file("/index.html"))
}

async fn handle_assets(ctx: Context<State>) -> EndpointResult {
    let path = ctx.uri().path();
    Ok(handle_embedded_file(&path))
}

fn main() {
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "intray=info");
    }
    env_logger::init();
    OPT.warn_if_invalid();

    let app_state = State::new();
    let expiration_task = app_state.expire();
    let mut app = App::with_state(app_state);
    app.middleware(RequestLogger::new());
    app.at("/").get(handle_index);
    app.at("/assets/*path").get(handle_assets);
    app.at("/upload/start").post(handle_upload_start);
    app.at("/upload/:file/:chunk").post(handle_upload_chunk);
    app.at("/upload/finish").post(handle_upload_finish);
    app.at("/upload/full").post(handle_upload_full_unnamed);
    app.at("/upload/full/:name").post(handle_upload_full_named);
    let app_task = app.serve(OPT.socket_addr());

    let runtime = Runtime::new().expect("runtime");
    let mut spawner = runtime.executor().compat();
    spawner.spawn(app_task.map(|_| ())).expect("App task");
    spawner.spawn(expiration_task).expect("Expiration task");
    info!("Running at {}...", OPT.socket_addr());
    runtime.shutdown_on_idle().wait().expect("Runtime shutdown");
    // TODO: manually handle SIGINT to clean up resources gracefully
}
