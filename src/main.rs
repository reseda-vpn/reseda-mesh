use models::{NodeStatusResponse, NodeState};
use routes::json_body;
use state::MeshState;
use tokio::sync::Mutex;
use warp::{self, Filter};
use std::{sync::Arc, convert::Infallible, time::SystemTime, time::Duration};
use crate::models::{TaskType, Task};
use futures_timer::Delay;

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
                            models::TaskType::CheckStatus(tries) => {
                                if tries >= 5 {
                                    println!("[task]: CheckStatus->Failed: DeniedRetry");

                                    // If we have been unable to verify the status of the node for more than 5 seconds, we mark it for removal.
                                    let execution_delay = match SystemTime::now().checked_add(Duration::new(1, 0)) {
                                        Some(delay) => delay,
                                        None => SystemTime::now(),
                                    };

                                    config_clone.lock().await.task_queue.lock().await.push_back(Task {
                                        task_type: TaskType::Dismiss(0),
                                        // Handing over lookup information 
                                        action_object: current_task.action_object.to_string(),
                                        exec_after: execution_delay
                                    });

                                    return;
                                }

                                println!("[task]: CheckStatus->Start");

                                let conf_lock = config_clone.lock().await;
                                let stack_lock = conf_lock.instance_stack.lock().await;
                                let node = match stack_lock.get(&current_task.action_object) {
                                    Some(val) => val,
                                    None => {
                                        // There is no matching node. We must close it instead.
                                        let execution_delay = match SystemTime::now().checked_add(Duration::new(1, 0)) {
                                            Some(delay) => delay,
                                            None => SystemTime::now(),
                                        };
    
                                        config_clone.lock().await.task_queue.lock().await.push_back(Task {
                                            task_type: TaskType::Dismiss(0),
                                            // Handing over lookup information 
                                            action_object: current_task.action_object.to_string(),
                                            exec_after: execution_delay
                                        });
    
                                        return;
                                    },
                                };

                                println!("[task]: CheckStatus->Retrieved Node");

                                let request_url = format!("https://{}.dns.reseda.app/health", node.information.id);

                                // Perform task
                                let response = match config_clone.lock().await.client.get(request_url)
                                    .header("Content-Type", "application/json")
                                    .send().await {
                                        Ok(response) => {
                                            let r = response.json::<NodeStatusResponse>().await.unwrap();
                                            
                                            Ok(r)
                                        },
                                        Err(err) => Err(err),
                                    };

                                let tries_count = match response {
                                    Ok(_) => 0,
                                    Err(_) => tries+1
                                };

                                let conf_lock = config_clone.lock().await;
                                let mut stack_lock = conf_lock.instance_stack.lock().await;
                                match stack_lock.get_mut(&current_task.action_object) {
                                    Some(val) => {
                                        val.state = NodeState::Online;
                                    },
                                    None => {},
                                };

                                println!("[task]: CheckStatus->Finished");

                                // Add another task for the same delay
                                let execution_delay = match SystemTime::now().checked_add(Duration::new(1, 0)) {
                                    Some(delay) => delay,
                                    None => SystemTime::now(),
                                };
                                
                                // Readd the task as this will exec every minute
                                config_clone.lock().await.task_queue.lock().await.push_back(Task {
                                    task_type: TaskType::CheckStatus(tries_count),
                                    // Handing over lookup information 
                                    action_object: current_task.action_object.to_string(),
                                    exec_after: execution_delay
                                });
                            },
                            // We want to add the node to the network and upgrade its status
                            models::TaskType::Instantiate(tries) => {
                                if tries >= 6 {
                                    println!("[task]: Instantiate->Failed: DeniedRetry");

                                    // Now we just give up, we've tried 6 times, after 30s initial delay (far more than necessary)
                                    // Thus, the total time by the last try is 1 minute. If the node is offline or sending invalid responses (i.e. constantly rebooting after panic! - wrong information - no state persistance)
                                    // We know that the server has run into issues and we must refuse its request to start.
                                    return;
                                }

                                println!("[task]: Instantiate->Start");

                                let conf_lock = config_clone.lock().await;
                                let stack_lock = conf_lock.instance_stack.lock().await;
                                let node = match stack_lock.get(&current_task.action_object) {
                                    Some(val) => val,
                                    None => {
                                        // There is no matching node. We must close it instead.
                                        let execution_delay = match SystemTime::now().checked_add(Duration::new(1, 0)) {
                                            Some(delay) => delay,
                                            None => SystemTime::now(),
                                        };
    
                                        config_clone.lock().await.task_queue.lock().await.push_back(Task {
                                            task_type: TaskType::Dismiss(0),
                                            // Handing over lookup information 
                                            action_object: current_task.action_object.to_string(),
                                            exec_after: execution_delay
                                        });
    
                                        return;
                                    }
                                };

                                // This is a partial culmination of a check status and a propagation step. 
                                // We need to perform a request to the server, check if it is alive and 'well'
                                // If so, we can give the node the status - online and post it to the reseda database.

                                // If it does not pass the checks, we can queue another instantiate with an instantiation number increase.
                                // If the tries exceeds 6, the node is removed.

                                println!("[task]: Instantiate->Pinging Server");

                                let request_url = format!("https://{}.dns.reseda.app/health", node.information.id);

                                // Perform task
                                let response = match config_clone.lock().await.client.get(request_url)
                                    .header("Content-Type", "application/json")
                                    .send().await {
                                        Ok(response) => {
                                            let r = response.json::<NodeStatusResponse>().await.unwrap();
                                            
                                            Ok(r)
                                        },
                                        Err(err) => Err(err),
                                    };
                                
                                // Unwrap the value
                                let _node_status = match response {
                                    Ok(response) => {
                                        println!("[task]: Instantiate->Ping Successful");

                                        response
                                    },
                                    Err(_) => {
                                        println!("[task]: Instantiate->Ping Failed");

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
    
                                        return;
                                    },
                                };

                                println!("[task]: Instantiate->Publishing Server");

                                // Match the SQLx response for publicizing the server
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
                                        let conf_lock = config_clone.lock().await;
                                        let mut stack_lock = conf_lock.instance_stack.lock().await;
                                        match stack_lock.get_mut(&current_task.action_object) {
                                            Some(val) => {
                                                val.state = NodeState::Online
                                            },
                                            None => {
                                                println!("Was unable to set the state of a node to online in a instantiate task");
                                            },
                                        };

                                        // Once the node has been publicized, we now need to keep monitoring it - we add a new task for 1s time 
                                        // with the CheckStatus task type, this will then continue for the lifetime of the node.
                                        let execution_delay = match SystemTime::now().checked_add(Duration::new(1, 0)) {
                                            Some(delay) => delay,
                                            None => SystemTime::now(),
                                        };

                                        config_clone.lock().await.task_queue.lock().await.push_back(Task {
                                            task_type: TaskType::CheckStatus(0),
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
                                if tries >= 6 { 
                                    println!("[task]: CheckStatus->Failed: DeniedRetry");
                                    return; 
                                }

                                println!("[task]: Dismiss->Start");

                                let conf_lock = config_clone.lock().await;
                                let stack_lock = conf_lock.instance_stack.lock().await;
                                let node = match stack_lock.get(&current_task.action_object) {
                                    Some(val) => val,
                                    None => todo!(),
                                };

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

                                // Now it is no longer publicly advertised - although before we drop the information we best cleanup the cloudflare configuration...
                                match result {
                                    Ok(_) => {
                                        // The node is now removed, we no longer have to monitor it can can safely ignore it.
                                        // We must set its state to offline as the node is no longer active on the mesh.
                                        // If we wish to instantiate it - i.e. we receive a new request from the server later
                                        // as it finishes the initialization after an update -> we can read from this and skip much of the init setup.
                                        let conf_lock = config_clone.lock().await;
                                        let mut stack_lock = conf_lock.instance_stack.lock().await;
                                        match stack_lock.get_mut(&current_task.action_object) {
                                            Some(val) => {
                                                val.state = NodeState::Offline
                                            },
                                            None => {
                                                println!("Was unable to set the state of a node to offline in a dismissal task");
                                            },
                                        };

                                        // We have set the server offline, in the meantime we will count down till its removal. 
                                        // If it comes back on in the meantime, this task will simply be skipped. Task is set for 1h time.
                                        let execution_delay = match SystemTime::now().checked_add(Duration::new(3600, 0)) {
                                            Some(delay) => delay,
                                            None => SystemTime::now(),
                                        };

                                        config_clone.lock().await.task_queue.lock().await.push_back(Task {
                                            task_type: TaskType::Purge,
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
                                            task_type: TaskType::Dismiss(tries+1),
                                            action_object: current_task.action_object.to_string(),
                                            exec_after: execution_delay
                                        });
                                    },
                                }
                            },
                            // We want to remove a server completely from the network and its trace information
                            models::TaskType::Purge => {
                                // Check if this is not necessary
                                let conf_lock = config_clone.lock().await;
                                let stack_lock = conf_lock.instance_stack.lock().await;
                                let node = match stack_lock.get(&current_task.action_object) {
                                    Some(val) => val,
                                    None => {
                                        return;
                                    },
                                };

                                if node.state == NodeState::Online || node.state == NodeState::Registering {
                                    // If the node was brought up in the 1h since this task was queued; we can just skip this task safely.
                                    return;
                                }

                                // After a while, we want to completely erase a server from the mesh as it obviously is not coming back online
                                // Furthermore, it is cluttering the cloudflare configurations, and repeated usages of this dying server that never revives
                                // will leave many upon DNS and SSL records that are 1. not monitored and 2. unregistered by reseda for possibly impersonation 
                                // by another server which will inherit the IP from the dead server. This is a liability and so we must clean it up after a set time period.

                                // First remove the DNS record for the id.
                                let _remove_record = match config_clone.lock().await.client.delete(format!("https://api.cloudflare.com/client/v4/zones/ebb52f1687a35641237774c39391ba2a/dns_records/{}", node.information.record_id))
                                    .header("Authorization", format!("Bearer {}", config_clone.lock().await.keys.cloudflare_key))
                                    .send().await {
                                        Ok(response) => Ok(response),
                                        Err(err) => Err(err),
                                    };

                                let _remove_cert = match config_clone.lock().await.client.delete(format!("https://api.cloudflare.com/client/v4/certificates/{}", node.information.cert_id))
                                    .header("Authorization", format!("Bearer {}", config_clone.lock().await.keys.cloudflare_key))
                                    .send().await {
                                        Ok(response) => Ok(response),
                                        Err(err) => Err(err),
                                    };

                                let conf_lock = config_clone.lock().await;
                                let mut stack_lock = conf_lock.instance_stack.lock().await;
                                stack_lock.remove(&current_task.action_object);
                            }
                        }
                    });
                }else {
                    // If task cannot be completed, push it to the back of the queue and try process the next one.
                    // This intends to maximize priority tasks by ensuring they are processed first, and that delayed tasks are processed as intended.
                    config.lock().await.task_queue.lock().await.push_back(current_task);
                }
            }else {
                println!("No tasks are queued, skipping...");
                
                // If there are no current tasks, we can wait 100ms for the next one to save compute.
                Delay::new(Duration::from_millis(100)).await;
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