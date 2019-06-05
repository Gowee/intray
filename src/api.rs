use serde::{Deserialize, Serialize};
use tide::{error::ResultExt, response, Context, EndpointResult};
use uuid::Uuid as UUID;

use crate::state::State;

#[derive(Debug, Serialize, Deserialize)]
struct RequestUploadStart {
    file_name: String,
    file_size: usize,
    chunk_size: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct ResponseUploadStart {
    ok: bool,
    token: Option<String>,
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
                token: Some(token.to_hyphenated().to_string()),
                error: None,
            }))
        }
        Err(e) => Ok(response::json(ResponseUploadStart {
            ok: false,
            token: None,
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

#[derive(Debug, Serialize, Deserialize)]
struct ResponseUploadChunk {
    ok: bool,
    token: Option<String>,
}

pub async fn handle_upload_chunk(mut ctx: Context<State>) -> EndpointResult {
    let file_token: UUID = ctx.param("file").client_err()?;
    let chunk_no: usize = ctx.param("chunk").client_err()?;
    let data = ctx.take_body();
    // TODO: pass out error using json
    ctx.state()
        .put_chunk(file_token, chunk_no, data)
        .await
        .server_err()?;
    Ok(response::json(vec![0]))
}

pub async fn handle_upload_end(_ctx: Context<State>) -> EndpointResult {
    Ok(response::json(vec![0]))
}
