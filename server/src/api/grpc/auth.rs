//! gRPC authentication interceptor for FileSystemService
//!
//! Provides Bearer token authentication for the FileSystemService.

use tonic::{Request, Status};

/// Authentication interceptor that validates Bearer tokens.
///
/// This interceptor extracts the token from the `authorization` header
/// and validates it against a configured secret token.
#[derive(Clone)]
pub struct AuthInterceptor {
    valid_token: String,
}

impl AuthInterceptor {
    /// Create a new authentication interceptor with the given valid token.
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            valid_token: token.into(),
        }
    }

    /// Validate the request and return the authenticated request or an error.
    #[allow(clippy::result_large_err)]
    pub fn authenticate<T>(&self, request: Request<T>) -> Result<Request<T>, Status> {
        // Extract the Authorization header
        let token = request
            .metadata()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));

        match token {
            Some(t) if t == self.valid_token => Ok(request),
            Some(_) => Err(Status::unauthenticated("invalid token")),
            None => Err(Status::unauthenticated("missing authorization header")),
        }
    }
}

impl tonic::service::Interceptor for AuthInterceptor {
    fn call(&mut self, request: Request<()>) -> Result<Request<()>, Status> {
        self.authenticate(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tonic::metadata::MetadataValue;

    #[test]
    fn test_valid_token() {
        let interceptor = AuthInterceptor::new("secret123");
        let mut request = Request::new(());
        request.metadata_mut().insert(
            "authorization",
            MetadataValue::try_from("Bearer secret123").unwrap(),
        );

        assert!(interceptor.authenticate(request).is_ok());
    }

    #[test]
    fn test_invalid_token() {
        let interceptor = AuthInterceptor::new("secret123");
        let mut request = Request::new(());
        request.metadata_mut().insert(
            "authorization",
            MetadataValue::try_from("Bearer wrongtoken").unwrap(),
        );

        let result = interceptor.authenticate(request);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn test_missing_token() {
        let interceptor = AuthInterceptor::new("secret123");
        let request = Request::new(());

        let result = interceptor.authenticate(request);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn test_malformed_header() {
        let interceptor = AuthInterceptor::new("secret123");
        let mut request = Request::new(());
        request.metadata_mut().insert(
            "authorization",
            MetadataValue::try_from("Basic secret123").unwrap(),
        );

        let result = interceptor.authenticate(request);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::Unauthenticated);
    }
}
