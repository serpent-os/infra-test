use proto::service::auth::auth_client::AuthClient;
use tonic::transport;

// Nice re-exports and renames so we don't need to
// deal with protobuf crate directly

pub use proto::service::auth::auth_server::Auth as Service;
pub use proto::service::auth::auth_server::AuthServer as Server;

pub type Client = AuthClient<transport::Channel>;

pub mod authenticate {
    pub use proto::service::auth::AuthenticateRequest as Request;
    pub use proto::service::auth::AuthenticateResponse as Response;
}
