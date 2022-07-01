use std::os::raw::c_float;

use serde::{Deserialize, Serialize};

/// Represents a customer
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Server {
    pub auth: String
}

#[derive(Deserialize, Debug, Serialize)]
pub struct IpResponse {
    pub country: String,
    pub region: String,
    pub eu: bool,
    pub city: String,
    pub latitude: c_float,
    pub longitude: c_float,
    pub metro: i16,
    pub radius: i16,
    pub timezone: String
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Configuration {
    pub check_key: String,
    pub cloudflare_key: String,
    pub database_key: String
}

#[derive(Serialize)]
pub struct RegistryReturn {
    pub key: String,
    pub cert: String,
    pub ip: String,
    pub res: IpResponse,
    pub id: String
}