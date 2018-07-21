extern crate actix_web;
#[macro_use]
extern crate rust_embed;
extern crate mime_guess;
extern crate futures;
extern crate chrono;
extern crate percent_encoding;
extern crate encoding;

use std::fs;
use std::io::Write;

use futures::future;
use futures::{Future, Stream};
use actix_web::{server, error, Error, multipart, App, FutureResponse, HttpRequest, HttpResponse,
                HttpMessage, Responder};
use actix_web::dev::{Handler, Payload};
use actix_web::http::Method;
use actix_web::http::header::{ContentDisposition, DispositionType, DispositionParam};
use actix_web::http::header::Charset;
use mime_guess::guess_mime_type;
use percent_encoding::percent_decode;
use encoding::label::encoding_from_whatwg_label;
use encoding::DecoderTrap;

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

fn get_file_name_from_multipart_field(field: &multipart::Field<Payload>) -> Option<String> {
    field.content_disposition()
         .and_then(|ContentDisposition { disposition, parameters }| {
             match disposition {
                 DispositionType::Ext(ref dt) if dt == "form-data" => {
                    let mut field_name = None;
                    let mut file_name = None;
                    for param in parameters.iter() {
                        match param {
                            DispositionParam::Ext(ref dp, ref name) if dp == "name" =>
                            {
                                field_name = Some(name.to_owned());
                            },
                            DispositionParam::Filename(charset, _, content) => {
                                file_name = encoding_from_whatwg_label(&charset.to_string())
                                    .and_then(|codec| {
                                        let raw_file_name = codec.decode(content, DecoderTrap::Replace).expect("DecoderTrap::Replace is used");
                                        Some(raw_file_name.replace("\\\"", "\"").replace("/", "_"))
                                        })
                            },
                            _ => ()
                        }
                    }
                    match field_name {
                        Some(ref name) if name == "file" => {
                            file_name.or(Some(String::from("Unnamed_file.temp"))) // TODO: Generating unique file names
                            // chrono::Local::now().format("%Y-%m-%d_%H:%M:%S").to_string()
                        },
                        _ => None
                    }
                 },
                 _ => None
             }
         })
}

// Copied from https://github.com/actix/examples/blob/master/multipart/src/main.rs.
// Too obscure for me to understand now.
pub fn save_file(field: multipart::Field<Payload>) -> Box<Future<Item = i64, Error = Error>> {
    // TODO:: handle overwriting issues; handle file name encoding
    println!("{:?}", field.content_disposition()); // field.content_disposition()
    // .filter(|param| match param { DispositionParam::Filename(_, _, _) => true, _ => false })
    let file_name = match get_file_name_from_multipart_field(&field) {
        Some(f) => f,
        None => return Box::new(future::ok(-1)),
    };
    println!("File name: {:}", file_name);
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
            })
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
    Box::new(req.multipart()
        .map_err(|e| error::ErrorInternalServerError(e))
        .map(handle_multipart_item)
        .flatten()
        .collect()
        .map(|sizes| HttpResponse::Ok().json(sizes))
        .map_err(|e| {
            eprintln!("failed: {}", e);
            e
        }))
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
