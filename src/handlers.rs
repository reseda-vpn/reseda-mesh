use std::time::{Duration};
use std::{convert::Infallible};
use reqwest::{Client};
use uuid::Uuid;
use chrono::Utc;

use warp::reply::json as json_reply;
use warp::{self, http::StatusCode};
use crate::{Mesh, GuardedMesh};
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
) -> Result<Box<dyn warp::Reply>, Infallible> {
    if authentication_key.auth != configuration.lock().await.keys.check_key {
        return Ok(Box::new(StatusCode::FORBIDDEN))
    }

    println!("Accepting Registration of {}", ip);

    let (conf, node) = {
        let configuration = configuration.lock().await;

        let n = match configuration.instance_stack.lock().await.get_mut(&ip) {
            Some(n) => n.to_owned(), 
            None => {
                println!("No node currently exists, creating and registering a new node.");
    
                println!("[mutex]: Obtaining Client Lock...");
                let client = &configuration.client;
                println!("[mutex]: Obtained Lock on Client.");
    
                let id = Uuid::new_v4();
                let location = match get_location(client, &ip).await {
                    Ok(val) => val,
                    Err(err) => {
                        return Ok(Box::new(err))
                    }
                };
            
                let identifier = format!("{}-{}", &location.country.to_lowercase(), id.to_string());
    
                println!("Generated Identification: {}", identifier);
            
                let dns_record = match create_dns_records(&configuration, client, &identifier, &ip).await {
                    Ok(val) => val,
                    Err(err) => {
                        return Ok(Box::new(err))
                    },
                };
    
                println!("Generated DNS Record.");
            
                let (cert, key, cert_id) = match create_certificates(&configuration, client, &identifier).await {
                    Ok(val) => val,
                    Err(err) => {
                        return Ok(Box::new(err))
                    }
                };
    
                println!("Generated Certificates");
    
                let rr = RegistryReturn {
                    cert, key, ip,
                    record_id: dns_record.result.id, cert_id,
                    id: identifier.to_string(), res: location
                };
    
                Node {
                    information: rr.clone(),
                    state: NodeState::Registering
                }
            },
        };

        (configuration, n)
    };

    conf.instance_stack.lock().await.insert(node.information.ip.clone(), node.clone());

    let exec_time = Utc::now().timestamp_millis() as u128 + Duration::new(30, 0).as_millis();

    conf.task_queue.lock().await.push_back(Task {
        task_type: TaskType::Instantiate(0),
        // Handing over lookup information 
        action_object: node.information.ip.to_string(),
        exec_at: exec_time
    });

    println!("Added task to queue.");
    println!("Task Queue: {:?}", &conf.task_queue);

    let reply = json_reply(&node.clone().information);

    drop(conf);
    drop(node);

    Ok(Box::new(reply))
}

async fn create_dns_records(
    configuration: &GuardedMesh<'_>,
    client: &Client,
    identifier: &String,
    ip: &String
) -> Result<CloudflareDNSRecordCreate, StatusCode> {
    println!("Creating DNS Record.");

    let response = match client.post("https://api.cloudflare.com/client/v4/zones/ebb52f1687a35641237774c39391ba2a/dns_records")
        .body(format!("
        {{
            \"type\": \"A\",
            \"name\": \"{}.dns\",
            \"content\": \"{}\",
            \"ttl\": 3600,
            \"priority\": 10,
            \"proxied\": true
        }}", identifier, ip))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}",  configuration.keys.cloudflare_key))
        .send().await {
            Ok(response) => {
                let r = response.json::<CloudflareDNSRecordCreate>().await.unwrap();
                
                Ok(r)
            },
            Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
        };

    response
}

async fn create_certificates(
    configuration: &GuardedMesh<'_>,
    client: &Client,
    id: &String
) -> Result<(String, String, String), StatusCode> {
    let cert = generate_simple_self_signed(vec![format!("{}.reseda.app", id.to_string())]).unwrap();
    let cert_public = cert.serialize_request_pem().unwrap();
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
        .header("Authorization", format!("Bearer {}", configuration.keys.cloudflare_key))
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