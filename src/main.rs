#![feature(extern_prelude)]

extern crate actix_web;
#[macro_use]
extern crate rust_embed;
extern crate chrono;
extern crate encoding;
extern crate futures;
extern crate mime_guess;
extern crate percent_encoding;
#[macro_use]
extern crate log;
extern crate env_logger;
#[macro_use]
extern crate structopt;
#[macro_use]
extern crate lazy_static;

use std::env;
use std::ffi::{OsStr, OsString};
use std::fs::{File, OpenOptions};
use std::io;
use std::io::Write;
use std::path::PathBuf;
use std::str::FromStr;

use actix_web::dev::{Handler, Payload};
use actix_web::http::Method;
use actix_web::{
    error, middleware, multipart, server, App, Error, FutureResponse, HttpMessage, HttpRequest,
    HttpResponse,
};
use futures::future;
use futures::{Future, Stream};
use mime_guess::{get_mime_extensions, guess_mime_type, octet_stream, Mime};
#[allow(unused_imports)]
use structopt::StructOpt;

mod opt;
use opt::OPT;

#[derive(RustEmbed)]
#[folder = "web/"]
struct Asset;

struct StaticFilesHandler {
    prefix: String,
}

impl StaticFilesHandler {
    fn new(prefix: &str) -> StaticFilesHandler {
        let mut prefix = prefix.to_owned();
        if !prefix.ends_with("/") {
            prefix.push_str("/");
        }
        StaticFilesHandler { prefix }
    }
}

impl<S> Handler<S> for StaticFilesHandler {
    type Result = HttpResponse;

    fn handle(&self, req: &HttpRequest<S>) -> Self::Result {
        let path = req.path();
        if !path.starts_with(&self.prefix) {
            return HttpResponse::NotFound().finish();
        }
        let path = &path[self.prefix.len()..];
        handle_embedded_file(path)
    }
}

fn handle_embedded_file(path: &str) -> HttpResponse {
    match Asset::get(path) {
        Some(content) => HttpResponse::Ok()
            .content_type(guess_mime_type(path).as_ref())
            .body(content),
        None => HttpResponse::NotFound().body("404 Not Found"),
    }
}

fn default(_req: HttpRequest) -> HttpResponse {
    handle_embedded_file("upload.html")
}

fn create_file<T: AsRef<OsStr>, U: AsRef<OsStr>>(
    file_name: T,
    ext_hint: Option<U>,
) -> io::Result<File> {
    let path = PathBuf::from(file_name.as_ref());
    let stem = path.file_stem().unwrap_or(OsStr::new("UnnamedFile"));
    let ext = path.extension().or(ext_hint.as_ref().map(|i| i.as_ref()));

    let mut count = 0;
    loop {
        let file_name = if count == 0 {
            let mut s = OsString::from(stem);
            if let Some(ext) = ext {
                s.push(".");
                s.push(ext);
            }
            s
        } else {
            let mut s = OsString::from(stem);
            s.push("_");
            s.push(count.to_string());
            if let Some(ext) = ext {
                s.push(".");
                s.push(ext);
            }
            s
        };
        let result = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(OPT.dir().join(file_name));
        match result {
            Err(ref e) if e.kind() == io::ErrorKind::AlreadyExists => (),
            other => {
                return other;
            }
        }
        count += 1;
    }
}

pub fn save_file(field: multipart::Field<Payload>) -> Box<Future<Item = i64, Error = Error>> {
    let file_name = field.content_disposition().and_then(|cd| {
        if cd.is_form_data() && cd.get_name() == Some("file") {
            Some(
                cd.get_filename()
                    .map(|n| n.to_owned())
                    .unwrap_or(String::from("UnnamedFile")),
            )
        } else {
            None
        }
    });
    let file = if let Some(file_name) = file_name {
        create_file(
            file_name,
            get_mime_extensions(
                &Mime::from_str(field.content_type().as_ref()).unwrap_or_else(|_| octet_stream()),
            ).and_then(|el| el.first().map(|e| *e)),
        )
    } else {
        return Box::new(future::ok(-1));
    };
    let mut file = match file {
        Ok(file) => file,
        Err(e) => return Box::new(future::err(error::ErrorInternalServerError(e))),
    };
    println!("Saving file: {:?}", file);
    Box::new(
        field
            .fold(0i64, move |acc, bytes| {
                let rt = file
                    .write_all(bytes.as_ref())
                    .map(|_| acc + bytes.len() as i64)
                    .map_err(|e| {
                        warn!("file.write_all failed: {:?}", e);
                        error::MultipartError::Payload(error::PayloadError::Io(e))
                    });
                future::result(rt)
            }).map_err(|e| {
                warn!("Failed to save file: , erorr: {:?}", e);
                error::ErrorInternalServerError(e)
            }),
    )
}

pub fn handle_multipart_item(
    item: multipart::MultipartItem<Payload>,
) -> Box<Stream<Item = i64, Error = Error>> {
    match item {
        multipart::MultipartItem::Field(field) => Box::new(save_file(field).into_stream()),
        multipart::MultipartItem::Nested(mp) => Box::new(
            mp.map_err(|e| error::ErrorInternalServerError(e))
                .map(handle_multipart_item)
                .flatten(),
        ),
    }
}

pub fn upload(req: HttpRequest) -> FutureResponse<HttpResponse> {
    Box::new(
        req.multipart()
            .map_err(|e| error::ErrorInternalServerError(e))
            .map(handle_multipart_item)
            .flatten()
            .collect()
            .map(|sizes| HttpResponse::Ok().json(sizes))
            .map_err(|e| {
                warn!("Error: {}", e);
                e
            }),
    )
}

fn main() {
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "actix_web=warn");
    }
    env_logger::init();

    println!("Running at {}...", OPT.socket_addr());
    server::new(|| {
        App::new()
            .middleware(middleware::Logger::default())
            .route("/", Method::GET, default)
            .handler("/assets", StaticFilesHandler::new(""))
            .route("/upload", Method::POST, upload)
    }).bind(OPT.socket_addr())
    .unwrap()
    .run();
}
