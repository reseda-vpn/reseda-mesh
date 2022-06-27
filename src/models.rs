use serde::{Deserialize, Serialize};

/// Represents a customer
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Server {
    /// A unique identifier for a customer record
    pub guid: String,

    /// First name
    pub first_name: String,

    /// Last name
    pub last_name: String,

    /// Email address
    pub email: String,

    /// Physical address
    pub address: String,
}