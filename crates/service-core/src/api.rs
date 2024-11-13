pub use self::operation::Operation;

pub mod operation;

#[derive(Debug, Clone, strum::Display)]
#[strum(serialize_all = "lowercase")]
pub enum Version {
    V1,
}
