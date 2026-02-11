//! gRPC PtyService implementation

use std::pin::Pin;
use std::sync::Arc;

use futures::{Stream, StreamExt};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};
use tracing::{debug, error, warn};

use crate::domain::types::PtyOptions;
use crate::error::Error;
use crate::infra::agent_pool::{AgentConnPool, PtyOutputEvent};
use crate::proto::{
    pty_service_server::PtyService, pty_stream_request, pty_stream_response, CreatePtyRequest,
    CreatePtyResponse, KillPtyRequest, KillPtyResponse, PtyInfo, PtyStreamRequest,
    PtyStreamResponse, ResizePtyRequest, ResizePtyResponse,
};
use crate::service::pty::PtyService as PtyServiceImpl;

/// Convert domain Error to gRPC Status
fn error_to_status(err: Error) -> Status {
    match &err {
        Error::SandboxNotFound(_) => Status::not_found(err.to_string()),
        Error::InvalidSandboxState { .. } => Status::failed_precondition(err.to_string()),
        Error::AgentNotConnected(_) => Status::unavailable(err.to_string()),
        Error::PtyNotFound(_) => Status::not_found(err.to_string()),
        Error::InvalidParameter(_) | Error::InvalidRequest(_) => {
            Status::invalid_argument(err.to_string())
        }
        _ => Status::internal(err.to_string()),
    }
}

/// gRPC PtyService implementation
pub struct GrpcPtyService {
    service: Arc<PtyServiceImpl>,
    agent_pool: Arc<AgentConnPool>,
}

impl GrpcPtyService {
    pub fn new(service: Arc<PtyServiceImpl>, agent_pool: Arc<AgentConnPool>) -> Self {
        Self {
            service,
            agent_pool,
        }
    }
}

#[tonic::async_trait]
impl PtyService for GrpcPtyService {
    async fn create_pty(
        &self,
        request: Request<CreatePtyRequest>,
    ) -> Result<Response<CreatePtyResponse>, Status> {
        let req = request.into_inner();
        debug!(
            sandbox_id = %req.sandbox_id,
            cols = ?req.cols,
            rows = ?req.rows,
            "create_pty"
        );

        let opts = PtyOptions {
            cols: req.cols.map(|c| c as u16),
            rows: req.rows.map(|r| r as u16),
            shell: req.shell,
            env: if req.env.is_empty() {
                None
            } else {
                Some(req.env)
            },
        };

        let pty = self
            .service
            .create(&req.sandbox_id, opts)
            .await
            .map_err(error_to_status)?;

        Ok(Response::new(CreatePtyResponse {
            pty: Some(PtyInfo {
                id: pty.id,
                sandbox_id: pty.sandbox_id,
                cols: pty.cols as u32,
                rows: pty.rows as u32,
            }),
        }))
    }

    async fn resize_pty(
        &self,
        request: Request<ResizePtyRequest>,
    ) -> Result<Response<ResizePtyResponse>, Status> {
        let req = request.into_inner();
        debug!(
            sandbox_id = %req.sandbox_id,
            pty_id = %req.pty_id,
            cols = req.cols,
            rows = req.rows,
            "resize_pty"
        );

        self.service
            .resize(&req.sandbox_id, &req.pty_id, req.cols as u16, req.rows as u16)
            .await
            .map_err(error_to_status)?;

        Ok(Response::new(ResizePtyResponse { success: true }))
    }

    async fn kill_pty(
        &self,
        request: Request<KillPtyRequest>,
    ) -> Result<Response<KillPtyResponse>, Status> {
        let req = request.into_inner();
        debug!(
            sandbox_id = %req.sandbox_id,
            pty_id = %req.pty_id,
            "kill_pty"
        );

        self.service
            .kill(&req.sandbox_id, &req.pty_id)
            .await
            .map_err(error_to_status)?;

        Ok(Response::new(KillPtyResponse { success: true }))
    }

    type PtyStreamStream = Pin<Box<dyn Stream<Item = Result<PtyStreamResponse, Status>> + Send>>;

    async fn pty_stream(
        &self,
        request: Request<Streaming<PtyStreamRequest>>,
    ) -> Result<Response<Self::PtyStreamStream>, Status> {
        let mut inbound = request.into_inner();
        let agent_pool = self.agent_pool.clone();
        let service = self.service.clone();

        let (tx, rx) = mpsc::channel::<Result<PtyStreamResponse, Status>>(256);

        tokio::spawn(async move {
            // Wait for init message first
            let first_msg = inbound.next().await;
            let (sandbox_id, pty_id) = match first_msg {
                Some(Ok(msg)) => match msg.request {
                    Some(pty_stream_request::Request::Init(init)) => {
                        debug!(
                            "PTY stream init for {}:{}",
                            init.sandbox_id, init.pty_id
                        );
                        (init.sandbox_id, init.pty_id)
                    }
                    _ => {
                        let _ = tx
                            .send(Ok(PtyStreamResponse {
                                response: Some(pty_stream_response::Response::Error(
                                    "Expected init message".to_string(),
                                )),
                            }))
                            .await;
                        error!("Invalid first message - expected init");
                        return;
                    }
                },
                Some(Err(e)) => {
                    error!("Error receiving init: {}", e);
                    return;
                }
                None => {
                    error!("No init message received");
                    return;
                }
            };

            // Subscribe to PTY output
            let mut pty_rx = agent_pool.subscribe_pty(&sandbox_id, &pty_id);

            // Spawn task to forward PTY output to client
            // Note: unsubscribe is handled by the main task, not here
            let tx_output = tx.clone();
            let sandbox_id_clone = sandbox_id.clone();
            let pty_id_clone = pty_id.clone();
            tokio::spawn(async move {
                while let Some(event) = pty_rx.recv().await {
                    let response = match event {
                        PtyOutputEvent::Output(data) => PtyStreamResponse {
                            response: Some(pty_stream_response::Response::Output(data)),
                        },
                        PtyOutputEvent::Exit(code) => PtyStreamResponse {
                            response: Some(pty_stream_response::Response::ExitCode(code)),
                        },
                        PtyOutputEvent::Error(msg) => PtyStreamResponse {
                            response: Some(pty_stream_response::Response::Error(msg)),
                        },
                    };
                    if tx_output.send(Ok(response)).await.is_err() {
                        break;
                    }
                }
                debug!(
                    "PTY output forwarding ended for {}:{}",
                    sandbox_id_clone, pty_id_clone
                );
            });

            // Handle incoming messages from client
            while let Some(result) = inbound.next().await {
                match result {
                    Ok(msg) => match msg.request {
                        Some(pty_stream_request::Request::Input(data)) => {
                            if let Err(e) = service.send_input(&sandbox_id, &pty_id, data).await {
                                warn!("Failed to write to PTY: {}", e);
                            }
                        }
                        Some(pty_stream_request::Request::Resize(resize)) => {
                            if let Err(e) = service
                                .resize(&sandbox_id, &pty_id, resize.cols as u16, resize.rows as u16)
                                .await
                            {
                                warn!("Failed to resize PTY: {}", e);
                            }
                        }
                        Some(pty_stream_request::Request::Init(_)) => {
                            warn!("Unexpected init message after connection established");
                        }
                        None => {}
                    },
                    Err(e) => {
                        error!("Error receiving message: {}", e);
                        break;
                    }
                }
            }

            // Client disconnected
            agent_pool.unsubscribe_pty(&sandbox_id, &pty_id);
            debug!("PTY stream ended for {}:{}", sandbox_id, pty_id);
        });

        let output_stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(output_stream)))
    }
}
