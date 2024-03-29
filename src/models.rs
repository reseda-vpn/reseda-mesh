use std::{os::raw::c_float, sync::Arc, collections::{HashMap, VecDeque}};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

/// Represents a customer
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Server {
    pub auth: String
}

#[derive(Deserialize, Debug, Serialize, Clone)]
pub struct IpResponse {
    pub country: String,
    pub countryCode: String,
    pub region: String,
    pub city: String,
    pub lat: c_float,
    pub lon: c_float,
    pub timezone: String
}

#[derive(Deserialize, Debug)]
pub struct CloudflareReturn {
    pub success: bool,
    pub result: CloudflareResult
}

#[derive(Deserialize, Debug)]
pub struct CloudflareResult {
    pub certificate: String,
    pub id: String
}

#[derive(Deserialize, Debug)]

pub struct CloudflareDNSRecordCreate {
    pub success: bool,
    pub result: CloudflareDNSRecordCreateResult
}

#[derive(Deserialize, Debug)]
pub struct CloudflareDNSRecordCreateResult {
    pub id: String
}

#[derive(Deserialize, Debug)]
pub struct NodeStatusResponse {
    // The nodes current information so we can verify it is ready to be publicized 
    pub status: String,
    pub usage: String,

    // This is information the client has which we request back so that we can verify the server which was booted **matches** the one we have in the local storage
    pub ip: String,
    pub cert: String,
    pub record_id: String
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Configuration {
    pub check_key: String,
    pub cloudflare_key: String,
    pub database_key: String
}

#[derive(Serialize, Clone, Debug)]
pub struct RegistryReturn {
    pub key: String,
    pub cert: String,
    pub ip: String,

    pub record_id: String,
    pub record_dns_id: String,
    pub cert_id: String,
    
    pub res: IpResponse,
    pub id: String
}

pub type Stack = Arc<Mutex<HashMap<String, Node>>>;

#[derive(Clone, Debug)]
pub struct Node {
    /// This row is all the information exclusively accessible known by the server that was initialized. 
    /// Note, we need to ensure this is all valid and correct, justified and all...
    pub information: RegistryReturn,
    pub state: NodeState
}

#[derive(PartialEq, Clone, Debug)]
pub enum NodeState {
    Online,
    Offline,
    Registering
}

/// For queueing tasks.
pub type TaskQueue = Arc<Mutex<VecDeque<Task>>>;

/// Relative to the server, task to manage or migrate server items, dynamically created as threads with the multi threaded locked storage.
#[derive(Debug)]
pub enum TaskType {
    CheckStatus(Tries),
    Instantiate(Tries),
    Dismiss(Tries),
    Purge
}

pub type Tries = i16;


#[derive(Debug)]
pub struct Task {
    pub task_type: TaskType,
    pub action_object: String,
    pub exec_at: u128
}
