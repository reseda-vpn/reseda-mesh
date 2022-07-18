use std::time::{Duration};
use std::{convert::Infallible};
use reqwest::{Client};
use uuid::Uuid;
use chrono::Utc;

use warp::Reply;
use warp::reply::{json as json_reply};
use warp::{self, http::StatusCode};
use crate::{Mesh};
use crate::models::{Server, IpResponse, RegistryReturn, Node, NodeState, TaskType, Task, CloudflareDNSRecordCreate, CloudflareReturn};
use rcgen::generate_simple_self_signed;

pub async fn echo() -> Result<Box<dyn warp::Reply>, Infallible> {
    Ok(Box::new(StatusCode::OK))
}

/// As to properly handle the mutex, the scoped approach was taken.
/// Prior to c793e4b94771c7f1b614d416974a580bcc0ab7e1 the non-scoped approach was taken
/// which resulting in multiple instances of deadlocking. This new approach is
/// not recommended and should be changed as it has one fatal flaw:
/// - The configuration cannot be accessed for the duration of MULTIPLE async requests.
/// Thus configuration is unable to be accessed for 10s each time a registration occurs,
/// which if abused can cripple the service. 
pub async fn register_server(
    ip: String,
    authentication_key: Server,
    configuration: Mesh
) -> Result<Box<dyn Reply>, Infallible> {
    if authentication_key.auth != configuration.lock().await.keys.check_key {
        return Ok(Box::new(StatusCode::FORBIDDEN))
    }

    println!("Accepting Registration of {}", ip);

    let node = {
        let config_lock = configuration.lock().await;

        let cloudflare_key = config_lock.keys.cloudflare_key.clone();
        let client = config_lock.client.clone();

        let exists = config_lock.instance_stack.lock().await.contains_key(&ip).clone();

        drop(config_lock);

        match exists {
            true => {
                configuration.lock().await.instance_stack.lock().await.get_mut(&ip).cloned()
            },
            false => {
                println!("No node currently exists, creating and registering a new node.");
    
                println!("[mutex]: Obtaining Client Lock...");
                println!("[mutex]: Obtained Lock on Client.");
    
                let id = Uuid::new_v4();
                let location = match get_location(&client, &ip).await {
                    Ok(val) => val,
                    Err(err) => {
                        return Ok(Box::new(err))
                    }
                };
            
                let identifier = format!("{}-{}", &location.country.to_lowercase(), id.to_string());
    
                println!("Generated Identification: {}", identifier);
            
                let record = match create_dns_records(&cloudflare_key, &client, &identifier, &ip, true).await {
                    Ok(val) => val,
                    Err(err) => {
                        return Ok(Box::new(err))
                    },
                };
    
                println!("Generated DNS Record.");

                let dns_record = match create_dns_records(&cloudflare_key, &client, &identifier, &format!("{}.dns", ip.to_string()), false).await {
                    Ok(val) => val,
                    Err(err) => {
                        return Ok(Box::new(err))
                    },
                };

                println!("Generated DNS Record 2.");
            
                let (cert, key, cert_id) = match create_certificates(&cloudflare_key, &client, &identifier).await {
                    Ok(val) => val,
                    Err(err) => {
                        return Ok(Box::new(err))
                    }
                };
    
                println!("Generated Certificates");
    
                let rr = RegistryReturn {
                    cert, key, ip,
                    record_id: record.result.id, record_dns_id: dns_record.result.id, cert_id,
                    id: identifier.to_string(), res: location
                };
    
                Some(Node {
                    information: rr.clone(),
                    state: NodeState::Registering
                })
            },
        }
    };

    let res: Box<(dyn Reply + 'static)> = match node {
        Some(n) => {
            let config_lock = configuration.lock().await;

            config_lock.instance_stack.lock().await.insert(n.information.ip.clone(), n.clone());

            let exec_time = Utc::now().timestamp_millis() as u128 + Duration::new(30, 0).as_millis();

            config_lock.task_queue.lock().await.push_back(Task {
                task_type: TaskType::Instantiate(0),
                // Handing over lookup information 
                action_object: n.information.ip.to_string(),
                exec_at: exec_time
            });

            println!("Added task to queue.");
            println!("Task Queue: {:?}", &config_lock.task_queue);

            let reply = json_reply(&n.clone().information);

            drop(config_lock);
            drop(n);

            Box::new(reply)
        },
        None => {
            Box::new(StatusCode::INTERNAL_SERVER_ERROR)
        },
    };
    
    Ok(res)
}

async fn create_dns_records(
    cloudflare_key: &String,
    client: &Client,
    identifier: &String,
    ip: &String,
    proxied: bool
) -> Result<CloudflareDNSRecordCreate, StatusCode> {
    println!("Creating DNS Record.");

    let response = match client.post("https://api.cloudflare.com/client/v4/zones/ebb52f1687a35641237774c39391ba2a/dns_records")
        .body(format!("
        {{
            \"type\": \"A\",
            \"name\": \"{}\",
            \"content\": \"{}\",
            \"ttl\": 3600,
            \"priority\": 10,
            \"proxied\": {}
        }}", identifier, ip, proxied))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}",  cloudflare_key))
        .send().await {
            Ok(response) => {
                match response.json::<CloudflareDNSRecordCreate>().await {
                    Ok(r) => Ok(r),
                    Err(err) => {
                        println!("[err]: Deserializing Cloudflare Result: {}", err);
                        return Err(StatusCode::INTERNAL_SERVER_ERROR)
                    },
                }
            },
            Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
        };

    response
}

async fn create_certificates(
    cloudflare_key: &String,
    client: &Client,
    id: &String
) -> Result<(String, String, String), StatusCode> {
    let cert = match generate_simple_self_signed(vec![format!("{}.reseda.app", id.to_string())]) {
        Ok(r) => r,
        Err(err) => {
            println!("[err]: Deserializing Certificate: {}", err);
            return Err(StatusCode::INTERNAL_SERVER_ERROR)
        },
    };

    let cert_public = match cert.serialize_request_pem() {
        Ok(r) => r,
        Err(err) => {
            println!("[err]: Deserializing Certificate: {}", err);
            return Err(StatusCode::INTERNAL_SERVER_ERROR)
        },
    };
    
    let cert_private = cert.serialize_private_key_pem();
    let cert_string = cert_public.replace("\r", "").split("\n").collect::<Vec<&str>>().join("\\n");
    
    let (cert, key, id) = match client.post("https://api.cloudflare.com/client/v4/certificates")
        .body(format!("
        {{
            \"hostnames\": [
                \"{}.reseda.app\"
            ],
            \"requested_validity\": 5475,
            \"request_type\": \"origin-rsa\",
            \"csr\": \"{}\"
        }}", id, cert_string))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", cloudflare_key))
        .send().await {
            Ok(response) => {
                // println!("{:?}", response.text().await);
                let r = response.json::<CloudflareReturn>().await;

                match r {
                    Ok(return_value) => {
                        if return_value.success == false {
                            println!("[err]: cloudflare certificate creation FAILED, return value FAILURE. Reason: {:?}", return_value);
                        }

                        (return_value.result.certificate, cert_private, return_value.result.id)
                    },
                    Err(err) => {
                        println!("[err]: Deserializing Cloudflare Result: {}", err);
                        return Err(StatusCode::INTERNAL_SERVER_ERROR)
                    }
                }
            },
            Err(err) => {
                println!("[err]: Error in setting proxied DNS {}", err);
                return Err(StatusCode::INTERNAL_SERVER_ERROR)
            },
        };

    Ok((cert, key, id))
}

async fn get_location(
    client: &Client,
    ip: &String
) -> Result<IpResponse, StatusCode> {
    let data = match client.get(format!("https://ipgeolocationapi.co/v1/{}", ip))
        .send().await {
            Ok(data) => data,
            Err(_) => {
                return Err(StatusCode::INTERNAL_SERVER_ERROR)
            },
        };

    if !data.status().is_success() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR)
    }

    let val = match data.json::<IpResponse>().await {
        Ok(val) => val,
        Err(_) => {
            return Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    };

    Ok(val)
}