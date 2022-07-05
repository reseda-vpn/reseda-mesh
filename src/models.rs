use std::{os::raw::c_float, sync::Arc, collections::{HashMap, VecDeque}, default, time::SystemTime};

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

#[derive(Serialize, Clone)]
pub struct RegistryReturn {
    pub key: String,
    pub cert: String,
    pub ip: String,
    pub res: IpResponse,
    pub id: String
}

pub type Stack = Arc<Mutex<HashMap<String, Node>>>;
pub struct Node {
    /// This row is all the information exclusively accessible known by the server that was initialized. 
    /// Note, we need to ensure this is all valid and correct, justified and all...
    pub information: RegistryReturn,
    pub state: NodeState
}

pub enum NodeState {
    Online,
    Offline,
    Registering
}

/// For queueing tasks.
pub type TaskQueue = Arc<Mutex<VecDeque<Task>>>;

/// Relative to the server, task to manage or migrate server items, dynamically created as threads with the multi threaded locked storage.
pub enum TaskType {
    CheckStatus,
    Instantiate,
    Dismiss
}

pub struct Task {
    pub task_type: TaskType,
    pub action_object: String,
    pub exec_after: SystemTime
}