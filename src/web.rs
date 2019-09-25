use mime_guess::from_path as mime_guess_from_path;
use tide::{
    http::{response::Builder as ResponseBuilder, StatusCode},
    Response,
};

#[derive(RustEmbed)]
#[folder = "web/"]
pub struct Assets;

pub fn serve_embedded_file(mut path: &str) -> Response {
    if path.starts_with('/') {
        path = &path[1..];
    }
    match Assets::get(path) {
        Some(content) => ResponseBuilder::new()
            .status(StatusCode::OK)
            .header(
                "Content-Type",
                mime_guess_from_path(path).first_or_octet_stream().as_ref(),
            )
            .body(content.as_ref().into())
            .unwrap(),
        None => ResponseBuilder::new()
            .status(StatusCode::NOT_FOUND)
            .body(
                Assets::get("404.html")
                    .expect("HTTP 404 Error Page")
                    .as_ref()
                    .into(),
            )
            .unwrap(),
    }
}
