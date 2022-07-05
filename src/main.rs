use routes::json_body;
use state::MeshState;
use tokio::sync::Mutex;
use warp::{self, Filter};
use std::{sync::Arc, convert::Infallible, time::SystemTime, time::Duration};
use crate::models::{TaskType, Task};

mod handlers;
mod models;
mod routes;
mod state;

pub type Mesh = Arc<Mutex<MeshState>>;

#[tokio::main]
async fn main() {
    let config: Mesh = Arc::new(
        Mutex::new(
            MeshState::initialize().await
                .to_owned()
        )
    );

    let register_route =  warp::path!("register" / String)
        .and(warp::post())
        .and(json_body())
        .and(with_config(config.clone()))
        .and_then(handlers::register_server);
    
    let echo_route =  warp::path::end()
        .and(warp::get())
        .and_then(handlers::echo);

    let routes = register_route.or(echo_route).with(warp::cors().allow_any_origin());

    tokio::spawn(async move {
        loop {
            if let Some(current_task) = config.lock().await.task_queue.lock().await.pop_front() {
                if SystemTime::now() >= current_task.exec_after {
                    // Execution can proceed, do so...

                    tokio::spawn(async move {
                        match current_task.task_type {
                            // We want to run a routing check to verify if the server is online/offline. If normal, queue a new check task 
                            models::TaskType::CheckStatus => {
                                // Perform task

                                // Add another task for the same delay
                                let execution_delay = match SystemTime::now().checked_add(Duration::new(1, 0)) {
                                    Some(delay) => delay,
                                    None => SystemTime::now(),
                                };
                                
                                // config.lock().await.task_queue.lock().await.push_back(Task {
                                //     task_type: TaskType::CheckStatus,
                                //     // Handing over lookup information 
                                //     action_object: current_task.action_object.to_string(),
                                //     exec_after: execution_delay
                                // });
                            },
                            // We want to add the node to the network and upgrade its status
                            models::TaskType::Instantiate => todo!(),
                            // We want to remove the node from the network and set its status accordingly
                            models::TaskType::Dismiss => todo!(),
                        }
                    });
                }else {
                    // If task cannot be completed, push it to the back of the queue and try process the next one.
                    // This intends to maximize priority tasks by ensuring they are processed first, and that delayed tasks are processed as intended.
                    config.lock().await.task_queue.lock().await.push_back(current_task);
                }
            }else {
                println!("No tasks are queued, skipping...")
            }
        }
    });

    warp::serve(routes)
        .tls()
        .cert_path("cert.pem")
        .key_path("key.pem")
        .run(([0, 0, 0, 0], 443)).await;
}

fn with_config(config: Mesh) -> impl Filter<Extract = (Mesh,), Error = Infallible> + Clone {
    warp::any().map(move || config.clone())
}