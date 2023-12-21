#[cfg(feature = "collectable")]
pub mod collectable {
    tonic::include_proto!("com.serpentos.collectable");
}
#[cfg(feature = "service")]
pub mod service {
    #[cfg(feature = "vessel")]
    pub mod vessel {
        tonic::include_proto!("com.serpentos.service.vessel");
    }
    #[cfg(feature = "auth")]
    pub mod auth {
        tonic::include_proto!("com.serpentos.service.auth");
    }
}
