use serde::{Deserialize, Serialize};
use tide::{error::ResultExt, response, Context, EndpointResult};
use uuid::Uuid as UUID;

use crate::state::State;

#[derive(Debug, Deserialize)]
struct RequestUploadStart {
    file_name: String,
    file_size: usize,
    chunk_size: usize,
}

#[derive(Debug, Serialize)]
struct ResponseUploadStart {
    ok: bool,
    file_token: Option<String>,
    error: Option<String>,
}

pub async fn handle_upload_start(mut ctx: Context<State>) -> EndpointResult {
    let req: RequestUploadStart = ctx.body_json().await.client_err()?;
    match ctx
        .state()
        .start_upload(req.file_name, req.file_size, req.chunk_size)
        .await
    {
        Ok(token) => {
            debug!("Upload starts with UUID: {}", token.to_hyphenated());
            Ok(response::json(ResponseUploadStart {
                ok: true,
                file_token: Some(token.to_hyphenated().to_string()),
                error: None,
            }))
        }
        Err(e) => Ok(response::json(ResponseUploadStart {
            ok: false,
            file_token: None,
            error: Some(e.to_string()),
        })),
    }
}

/* #[derive(Debug, Deserialize)]
struct RequestUploadChunk {
    token: UUID,
    file_size: usize,
    chunk_size: usize,
} */

#[derive(Debug, Serialize)]
struct ResponseUploadChunk {
    ok: bool,
    error: Option<String>,
}

pub async fn handle_upload_chunk(mut ctx: Context<State>) -> EndpointResult {
    let file_token: UUID = ctx.param("file").client_err()?;
    let chunk_index: usize = ctx.param("chunk").client_err()?;
    let data = ctx.take_body();
    Ok(response::json(
        match ctx.state().put_chunk(file_token, chunk_index, data).await {
            Ok(_) => ResponseUploadChunk {
                ok: true,
                error: None,
            },
            Err(e) => ResponseUploadChunk {
                ok: false,
                error: Some(e.to_string()),
            },
        },
    ))
}

#[derive(Debug, Deserialize)]
struct RequestUploadFinish {
    file_token: UUID,
}

#[derive(Debug, Serialize)]
struct ResponseUploadFinish {
    ok: bool,
    error: Option<String>,
}

pub async fn handle_upload_finish(mut ctx: Context<State>) -> EndpointResult {
    let req: RequestUploadFinish = ctx.body_json().await.client_err()?;
    Ok(response::json(
        match ctx.state().finish_upload(req.file_token).await {
            Ok(_) => ResponseUploadFinish {
                ok: true,
                error: None,
            },
            Err(e) => ResponseUploadFinish {
                ok: false,
                error: Some(e.to_string()),
            },
        },
    ))
}

/* #[derive(Debug, Deserialize)]
struct RequestUploadChunk {
    token: UUID,
    file_size: usize,
    chunk_size: usize,
} */

#[derive(Debug, Serialize)]
struct ResponseUploadFull {
    ok: bool,
    written: Option<usize>,
    error: Option<String>,
}

// TODO: redundant trivial functions calling
pub async fn handle_upload_full_unnamed(ctx: Context<State>) -> EndpointResult {
    handle_upload_full(ctx, String::from("")).await
}

pub async fn handle_upload_full_named(ctx: Context<State>) -> EndpointResult {
    let file_name = ctx.param("name").unwrap_or_else(|_| String::from(""));
    handle_upload_full(ctx, file_name).await
}

pub async fn handle_upload_full(mut ctx: Context<State>, file_name: String) -> EndpointResult {
    // TODO: Cow <str>
    let size: Option<usize> = match ctx.headers().get("Content-Length") {
        Some(v) => Some(v.to_str().client_err()?.parse().client_err()?),
        None => None,
    };
    let data = ctx.take_body();
    Ok(response::json(
        match ctx.state().put_full(file_name, size, data).await {
            Ok(written) => ResponseUploadFull {
                ok: true,
                written: Some(written),
                error: None,
            },
            Err(e) => ResponseUploadFull {
                ok: false,
                written: None,
                error: Some(e.to_string()),
            },
        },
    ))
}
