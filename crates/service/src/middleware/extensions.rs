use tonic::service::Interceptor;

use crate::{endpoint::enrollment::PendingEnrollment, Database};

#[derive(Debug, Clone)]
pub struct Extensions {
    pub db: Database,
    pub pending_enrollment: PendingEnrollment,
}

impl Interceptor for Extensions {
    fn call(
        &mut self,
        mut request: tonic::Request<()>,
    ) -> Result<tonic::Request<()>, tonic::Status> {
        request.extensions_mut().insert(self.db.clone());
        request
            .extensions_mut()
            .insert(self.pending_enrollment.clone());
        Ok(request)
    }
}
