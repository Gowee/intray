extern crate actix_web;
#[macro_use]
extern crate rust_embed;
extern crate mime_guess;
extern crate futures;
extern crate chrono;

use std::fs;
use std::io::Write;

use futures::future;
use futures::{Future, Stream};
use actix_web::{server, error, Error, multipart, App, FutureResponse, HttpRequest, HttpResponse,
                HttpMessage};
use actix_web::dev::Handler;
use actix_web::http::Method;
use mime_guess::guess_mime_type;

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

    fn handle(&mut self, req: HttpRequest<S>) -> Self::Result {
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
        Some(content) => {
            HttpResponse::Ok()
                .content_type(guess_mime_type(path).as_ref())
                .body(content)
        }
        None => HttpResponse::NotFound().body("404 Not Found"),
    }
}

fn default(_req: HttpRequest) -> HttpResponse {
    handle_embedded_file("upload.html")
}

// Copied from https://github.com/actix/examples/blob/master/multipart/src/main.rs, almost verbatim.
// Too obscure for me to understand now.
pub fn save_file(field: multipart::Field<HttpRequest>) -> Box<Future<Item = i64, Error = Error>> {
    // TODO:: handle overwriting issues; handle file name encoding
    println!("{:?}", field.headers());
    println!("{:?}", field.headers().get("filename"));
    let file_name = match field.headers().get("filename") {
        Some(file_name) => String::from_utf8_lossy(file_name.as_bytes()).into_owned(), 
        None => chrono::Local::now().format("%Y-%m-%d_%H:%M:%S").to_string(),
    };

    println!("Receiving file: {}", file_name);
    let mut file = match fs::File::create(file_name) {
        Ok(file) => file,
        Err(e) => return Box::new(future::err(error::ErrorInternalServerError(e))),
    };
    Box::new(
        field
            .fold(0i64, move |acc, bytes| {
                let rt = file.write_all(bytes.as_ref())
                    .map(|_| acc + bytes.len() as i64)
                    .map_err(|e| {
                        eprintln!("file.write_all failed: {:?}", e);
                        error::MultipartError::Payload(error::PayloadError::Io(e))
                    });
                future::result(rt)
            })
            .map_err(|e| {
                eprintln!("save_file failed, {:?}", e);
                error::ErrorInternalServerError(e)
            }),
    )
}

pub fn handle_multipart_item(
    item: multipart::MultipartItem<HttpRequest>,
) -> Box<Stream<Item = i64, Error = Error>> {
    match item {
        multipart::MultipartItem::Field(field) => Box::new(save_file(field).into_stream()),
        multipart::MultipartItem::Nested(mp) => Box::new(
            mp.map_err(error::ErrorInternalServerError)
                .map(handle_multipart_item)
                .flatten(),
        ),
    }
}

pub fn upload(req: HttpRequest) -> FutureResponse<HttpResponse> {
    Box::new(
        req.multipart()
            .map_err(error::ErrorInternalServerError)
            .map(handle_multipart_item)
            .flatten()
            .collect()
            .map(|sizes| HttpResponse::Ok().json(sizes))
            .map_err(|e| {
                eprintln!("failed: {}", e);
                e
            }),
    )
}

fn main() {
    server::new(|| {
        App::new()
            .route("/", Method::GET, default)
            .handler("/assets", StaticFilesHandler::new(""))
            .route("/upload", Method::POST, upload)
        //            .resource("/", |r| r.f(index))
    }).bind("127.0.0.1:8088")
        .unwrap()
        .run();
}
