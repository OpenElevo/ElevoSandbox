//! gRPC ProcessService implementation

use std::pin::Pin;
use std::sync::Arc;

use futures::Stream;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use tracing::debug;

use crate::domain::types::ProcessEvent as DomainProcessEvent;
use crate::error::Error;
use crate::proto::{
    process_event, process_service_server::ProcessService, CommandResult as ProtoCommandResult,
    ErrorEvent, ExitEvent, KillProcessRequest, KillProcessResponse,
    ProcessEvent as ProtoProcessEvent, RunCommandRequest, RunCommandResponse, StderrEvent,
    StdoutEvent,
};
use crate::service::process::{ProcessService as ProcessServiceImpl, RunCommandOptions};

/// Convert domain Error to gRPC Status
fn error_to_status(err: Error) -> Status {
    match &err {
        Error::SandboxNotFound(_) => Status::not_found(err.to_string()),
        Error::InvalidSandboxState { .. } => Status::failed_precondition(err.to_string()),
        Error::AgentNotConnected(_) => Status::unavailable(err.to_string()),
        Error::ProcessTimeout => Status::deadline_exceeded(err.to_string()),
        Error::ProcessExecutionFailed(_) => Status::internal(err.to_string()),
        Error::InvalidParameter(_) | Error::InvalidRequest(_) => {
            Status::invalid_argument(err.to_string())
        }
        _ => Status::internal(err.to_string()),
    }
}

/// Convert domain ProcessEvent to proto ProcessEvent
fn event_to_proto(event: DomainProcessEvent) -> ProtoProcessEvent {
    let event = match event {
        DomainProcessEvent::Stdout { data } => process_event::Event::Stdout(StdoutEvent { data }),
        DomainProcessEvent::Stderr { data } => process_event::Event::Stderr(StderrEvent { data }),
        DomainProcessEvent::Exit { code } => process_event::Event::Exit(ExitEvent { code }),
        DomainProcessEvent::Error { message } => {
            process_event::Event::Error(ErrorEvent { message })
        }
    };
    ProtoProcessEvent { event: Some(event) }
}

/// gRPC ProcessService implementation
pub struct GrpcProcessService {
    service: Arc<ProcessServiceImpl>,
}

impl GrpcProcessService {
    pub fn new(service: Arc<ProcessServiceImpl>) -> Self {
        Self { service }
    }
}

#[tonic::async_trait]
impl ProcessService for GrpcProcessService {
    async fn run_command(
        &self,
        request: Request<RunCommandRequest>,
    ) -> Result<Response<RunCommandResponse>, Status> {
        let req = request.into_inner();
        debug!(
            sandbox_id = %req.sandbox_id,
            command = %req.command,
            args = ?req.args,
            "run_command"
        );

        let opts = RunCommandOptions {
            command: req.command,
            args: req.args,
            env: req.env,
            cwd: req.cwd,
            timeout_ms: req.timeout_ms.unwrap_or(0),
        };

        let result = self
            .service
            .run(&req.sandbox_id, opts)
            .await
            .map_err(error_to_status)?;

        Ok(Response::new(RunCommandResponse {
            result: Some(ProtoCommandResult {
                exit_code: result.exit_code,
                stdout: result.stdout,
                stderr: result.stderr,
            }),
        }))
    }

    type RunCommandStreamStream =
        Pin<Box<dyn Stream<Item = Result<ProtoProcessEvent, Status>> + Send>>;

    async fn run_command_stream(
        &self,
        request: Request<RunCommandRequest>,
    ) -> Result<Response<Self::RunCommandStreamStream>, Status> {
        let req = request.into_inner();
        debug!(
            sandbox_id = %req.sandbox_id,
            command = %req.command,
            args = ?req.args,
            "run_command_stream"
        );

        let opts = RunCommandOptions {
            command: req.command,
            args: req.args,
            env: req.env,
            cwd: req.cwd,
            timeout_ms: req.timeout_ms.unwrap_or(0),
        };

        let event_stream = self
            .service
            .run_stream(&req.sandbox_id, opts)
            .await
            .map_err(error_to_status)?;

        let (tx, rx) = mpsc::channel(32);

        tokio::spawn(async move {
            use futures::StreamExt;
            let mut stream = Box::pin(event_stream);
            while let Some(event) = stream.next().await {
                let proto_event = event_to_proto(event);
                if tx.send(Ok(proto_event)).await.is_err() {
                    break;
                }
            }
        });

        let output_stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(output_stream)))
    }

    async fn kill_process(
        &self,
        request: Request<KillProcessRequest>,
    ) -> Result<Response<KillProcessResponse>, Status> {
        let req = request.into_inner();
        debug!(
            sandbox_id = %req.sandbox_id,
            pid = req.pid,
            signal = ?req.signal,
            "kill_process"
        );

        self.service
            .kill(&req.sandbox_id, req.pid, req.signal)
            .await
            .map_err(error_to_status)?;

        Ok(Response::new(KillProcessResponse { success: true }))
    }
}
