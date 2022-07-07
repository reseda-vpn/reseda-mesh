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

                    let config_clone = config.clone();

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
                                
                                // Readd the task as this will exec every minute
                                config_clone.lock().await.task_queue.lock().await.push_back(Task {
                                    task_type: TaskType::CheckStatus,
                                    // Handing over lookup information 
                                    action_object: current_task.action_object.to_string(),
                                    exec_after: execution_delay
                                });
                            },
                            // We want to add the node to the network and upgrade its status
                            models::TaskType::Instantiate(tries) => {
                                if tries >= 6 {
                                    // Now we just give up, we've tried 6 times, after 30s initial delay (far more than necessary)
                                    // Thus, the total time by the last try is 1 minute. If the node is offline or sending invalid responses (i.e. constantly rebooting after panic! - wrong information - no state persistance)
                                    // We know that the server has run into issues and we must refuse its request to start.
                                    return;
                                }

                                let conf_lock = config_clone.lock().await;
                                let stack_lock = conf_lock.instance_stack.lock().await;
                                let node = match stack_lock.get(&current_task.action_object) {
                                    Some(val) => val,
                                    None => todo!(),
                                };

                                // This is a partial culmination of a check status and a propagation step. 
                                // We need to perform a request to the server, check if it is alive and 'well'
                                // If so, we can give the node the status - online and post it to the reseda database.

                                // If it does not pass the checks, we can queue another instantiate with an instantiation number increase.
                                // If the tries exceeds 6, the node is removed.

                                // REQUEST START
                                // ...
                                // REQUEST END

                                let result = match config_clone.lock().await.pool.begin().await {
                                    Ok(mut transaction) => {
                                        match sqlx::query!("insert into Server (id, location, country, hostname, flag) values (?, ?, ?, ?, ?)", node.information.id, node.information.res.timezone, node.information.res.timezone.split("/").collect::<Vec<&str>>()[1], node.information.ip, node.information.res.country.to_lowercase().replace(" ", "-"))
                                            .execute(&mut transaction)
                                            .await {
                                                Ok(result) => {
                                                    match transaction.commit().await {
                                                        Ok(_) => {
                                                            Ok(result)
                                                        },
                                                        Err(error) => { 
                                                            Err(error) 
                                                        }
                                                    }
                                                },
                                                Err(error) => {
                                                    Err(error)
                                                }
                                            }
                                    },
                                    Err(error) => {
                                        Err(error)
                                    }
                                };

                                match result {
                                    Ok(_) => {
                                        // Once the node has been publicized, we now need to keep monitoring it - we add a new task for 1s time 
                                        // with the CheckStatus task type, this will then continue for the lifetime of the node.
                                        let execution_delay = match SystemTime::now().checked_add(Duration::new(1, 0)) {
                                            Some(delay) => delay,
                                            None => SystemTime::now(),
                                        };

                                        config_clone.lock().await.task_queue.lock().await.push_back(Task {
                                            task_type: TaskType::CheckStatus,
                                            // Handing over lookup information 
                                            action_object: current_task.action_object.to_string(),
                                            exec_after: execution_delay
                                        });
                                    },
                                    Err(_) => {
                                        // Uh oh, something went wrong. Thats okay, we can just requeue this task for 5s time and increment the try counter.
                                        let execution_delay = match SystemTime::now().checked_add(Duration::new(5, 0)) {
                                            Some(delay) => delay,
                                            None => SystemTime::now(),
                                        };

                                        config_clone.lock().await.task_queue.lock().await.push_back(Task {
                                            task_type: TaskType::Instantiate(tries+1),
                                            action_object: current_task.action_object.to_string(),
                                            exec_after: execution_delay
                                        });
                                    },
                                }
                            },
                            // We want to remove the node from the network and set its status accordingly
                            models::TaskType::Dismiss(tries) => {
                                if tries >= 6 { return; }

                                let conf_lock = config_clone.lock().await;
                                let stack_lock = conf_lock.instance_stack.lock().await;
                                let node = match stack_lock.get(&current_task.action_object) {
                                    Some(val) => val,
                                    None => todo!(),
                                };

                                // REQUEST START
                                // ...
                                // REQUEST END

                                let result = match config_clone.lock().await.pool.begin().await {
                                    Ok(mut transaction) => {
                                        match sqlx::query!("delete from Server where id = ?", node.information.id)
                                            .execute(&mut transaction)
                                            .await {
                                                Ok(result) => {
                                                    match transaction.commit().await {
                                                        Ok(_) => {
                                                            Ok(result)
                                                        },
                                                        Err(error) => { 
                                                            Err(error) 
                                                        }
                                                    }
                                                },
                                                Err(error) => {
                                                    Err(error)
                                                }
                                            }
                                    },
                                    Err(error) => {
                                        Err(error)
                                    }
                                };

                                // Now it is no longer publically advertised - although before we drop the information we best cleanup the cloudflare configuration...


                                match result {
                                    Ok(_) => {
                                        // The node is now removed, we no longer have to monitor it can can safely ignore it.
                                    },
                                    Err(_) => {
                                        // Uh oh, something went wrong. Thats okay, we can just requeue this task for 5s time and increment the try counter.
                                        let execution_delay = match SystemTime::now().checked_add(Duration::new(5, 0)) {
                                            Some(delay) => delay,
                                            None => SystemTime::now(),
                                        };

                                        config_clone.lock().await.task_queue.lock().await.push_back(Task {
                                            task_type: TaskType::Dismiss(tries+1),
                                            action_object: current_task.action_object.to_string(),
                                            exec_after: execution_delay
                                        });
                                    },
                                }
                            },
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