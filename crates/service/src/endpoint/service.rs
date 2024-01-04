use self::proto::endpoint_server::Endpoint as EndpointService;
pub use self::proto::{
    EndpointStatus, EnrollmentRequest, EnrollmentRole, EnumerateResponse, TokenResponse,
};
use crate::Token;

pub type Client<T> = proto::endpoint_client::EndpointClient<T>;
pub type Server = proto::endpoint_server::EndpointServer<Service>;

pub struct Service;

#[tonic::async_trait]
impl EndpointService for Service {
    async fn enroll(
        &self,
        _request: tonic::Request<EnrollmentRequest>,
    ) -> std::result::Result<tonic::Response<()>, tonic::Status> {
        todo!();
    }
    async fn accept(
        &self,
        request: tonic::Request<EnrollmentRequest>,
    ) -> std::result::Result<tonic::Response<()>, tonic::Status> {
        // We can define a middleware that extracts token from auth header,
        // verifies, then adds as an extension for use in handlers here
        let _token = request.extensions().get::<Token>();
        todo!();
    }
    async fn decline(
        &self,
        _request: tonic::Request<()>,
    ) -> std::result::Result<tonic::Response<()>, tonic::Status> {
        todo!();
    }
    async fn leave(
        &self,
        _request: tonic::Request<()>,
    ) -> std::result::Result<tonic::Response<()>, tonic::Status> {
        todo!();
    }
    async fn enumerate(
        &self,
        _request: tonic::Request<()>,
    ) -> std::result::Result<tonic::Response<EnumerateResponse>, tonic::Status> {
        todo!();
    }
    async fn refresh_token(
        &self,
        _request: tonic::Request<()>,
    ) -> std::result::Result<tonic::Response<TokenResponse>, tonic::Status> {
        todo!();
    }
    async fn refresh_issue_token(
        &self,
        _request: tonic::Request<()>,
    ) -> std::result::Result<tonic::Response<TokenResponse>, tonic::Status> {
        todo!();
    }
}

mod proto {
    use tonic::include_proto;

    include_proto!("endpoint");
}
