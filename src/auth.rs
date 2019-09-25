use futures::future::BoxFuture;
// use log::trace;

use base64::decode as base64_decode;
use tide::{
    http::{response::Builder as ResponseBuilder, StatusCode},
    middleware::{Middleware, Next},
    Context, Response,
};

use crate::{opt::OPT, web::Assets};

pub type HTTPBasicAuth = SimplisticHTTPBasicAuth;

/// Middleware for HTTP Basic Authentication as defined in [RFC 2617](https://tools.ietf.org/html/rfc2617) and
/// [RFC 7617](https://tools.ietf.org/html/rfc7617) (simplistic implementation).
#[derive(Default)]
pub struct SimplisticHTTPBasicAuth;

impl SimplisticHTTPBasicAuth {
    /// Construct a new instance with an empty list of headers.
    pub fn new() -> Self {
        Default::default()
    }

    /// Match the provided credentials against all the credentials specified, return true if any matches.
    fn authenticate(&self, credentials: impl AsRef<str>) -> bool {
        OPT.credentials_match(credentials)
    }

    /// Generate a HTTP 401 Unauthorized response.
    fn unauthorized(&self) -> Response {
        ResponseBuilder::new()
            .header(
                "WWW-Authenticate",
                format!("Basic realm=\"{}\", charset=\"UTF-8\"", &OPT.auth_realm),
            )
            .status(StatusCode::UNAUTHORIZED)
            .body(
                Assets::get("401.html")
                    .expect("HTTP 401 Error Page")
                    .as_ref()
                    .into(),
            )
            .unwrap()
    }
    // pub fn auth(&self, cre)
}

impl<State: Send + Sync + 'static> Middleware<State> for SimplisticHTTPBasicAuth {
    fn handle<'a>(&'a self, cx: Context<State>, next: Next<'a, State>) -> BoxFuture<'a, Response> {
        Box::pin(async move {
            let t = cx.headers().get("Authorization").and_then(|value| {
                let authorization = value.to_str().ok()?;
                let (_type, credentials) = {
                    // let _type = &_type[.._type.len() - 1];
                    let (t, c) = authorization.split_at(authorization.find(' ')? + 1);
                    (t.trim(), c.trim())
                };
                if _type.eq_ignore_ascii_case("Basic") {
                    Some(String::from_utf8(base64_decode(credentials).ok()?).ok()?)
                } else {
                    None
                }
            });
            match t {
                Some(ref credentials) if self.authenticate(credentials) => {
                    trace!("An request is authenticated with {}", credentials);
                    next.run(cx).await
                }
                _ => self.unauthorized(),
            }
        })
    }
}

// unimplemented!();
// let mut res = next.run(cx).await;
// let headers = res.headers_mut();
// for (key, value) in self.headers.iter() {
//     trace!("add default: {} {:?}", &key, &value);
//     headers.entry(key).unwrap().or_insert_with(|| value.clone());
// }
//
