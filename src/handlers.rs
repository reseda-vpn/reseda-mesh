
use std::{convert::Infallible};
use reqwest::{Client, Response};
use serde::Deserialize;
use uuid::Uuid;

use warp::reply::json as json_reply;
use warp::{self, http::StatusCode};
use crate::Mesh;
use crate::models::{Server, IpResponse, RegistryReturn, Node, NodeState};
use rcgen::generate_simple_self_signed;

#[derive(Deserialize, Debug)]
pub struct CloudflareReturn {
    pub success: bool,
    pub result: CloudflareResult
}

#[derive(Deserialize, Debug)]
pub struct CloudflareResult {
    pub certificate: String
}

pub async fn echo() -> Result<Box<dyn warp::Reply>, Infallible> {
    Ok(Box::new(StatusCode::OK))
}

pub async fn register_server(
    ip: String,
    authentication_key: Server,
    configuration: Mesh
) -> Result<Box<dyn warp::Reply>, Infallible> {
    if authentication_key.auth != configuration.lock().await.keys.check_key {
        return Ok(Box::new(StatusCode::FORBIDDEN))
    }

    let client = &configuration.lock().await.client;

    let id = Uuid::new_v4();
    let location = match get_location(client, &ip).await {
        Ok(val) => val,
        Err(err) => {
            return Ok(Box::new(err))
        }
    };

    let identifier = format!("{}-{}", location.country, id.to_string());

    let _ = match create_dns_records(&configuration, client, &identifier, &ip).await {
        Ok(val) => val,
        Err(err) => {
            return Ok(Box::new(err))
        },
    };

    let (cert, key) = match create_certificates(&configuration, client, &location, &identifier).await {
        Ok(val) => val,
        Err(err) => {
            return Ok(Box::new(err))
        }
    };

    match configuration.lock().await.pool.begin().await {
        Ok(mut transaction) => {
            match sqlx::query!("insert into Server (id, location, country, hostname, flag) values (?, ?, ?, ?, ?)", identifier, location.timezone, location.timezone.split("/").collect::<Vec<&str>>()[1], ip, location.country.to_lowercase().replace(" ", "-"))
                .execute(&mut transaction)
                .await {
                    Ok(result) => {
                        match transaction.commit().await {
                            Ok(r2) => {
                                println!("[sqlx]: Usage Log Transaction Result: {:?}, {:?}", result, r2);
                            },
                            Err(error) => println!("[sqlx]: Transaction Commitance Error: {:?}", error),
                        }

                    },
                    Err(error) => println!("[sqlx]: Transaction Error: {:?}", error),
                }
        },
        Err(err) => {
            println!("[err]: Unable to perform request, user will remain unassigned. Reason: {}", err);
        }
    };

    let rr = RegistryReturn {
        cert: cert,
        key: key,
        ip: ip,
        id: identifier.to_string(),
        res: location
    };

    let node = Node {
        information: rr,
        state: NodeState::Booting
    };

    Ok(Box::new(json_reply(&node.information)))
}


async fn create_dns_records(
    configuration: &Mesh,
    client: &Client,
    identifier: &String,
    ip: &String
) -> Result<Response, StatusCode> {
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
        .header("Authorization", format!("Bearer {}",  configuration.lock().await.keys.cloudflare_key))
        .send().await {
            Ok(return_val) => return_val,
            Err(err) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
        };

    Ok(response)
}

async fn create_certificates(
    configuration: &Mesh,
    client: &Client,
    location: &IpResponse,
    id: &String
) -> Result<(String, String), StatusCode> {
    let cert = generate_simple_self_signed(vec![format!("{}.reseda.app", id.to_string())]).unwrap();
    let cert_public = cert.serialize_request_pem().unwrap();
    let cert_private = cert.serialize_private_key_pem();
    let cert_string = cert_public.replace("\r", "").split("\n").collect::<Vec<&str>>().join("\\n");
    
    let (cert, key) = match client.post("https://api.cloudflare.com/client/v4/certificates")
        .body(format!("
        {{
            \"hostnames\": [
                \"{}.reseda.app\"
            ],
            \"requested_validity\": 5475,
            \"request_type\": \"origin-rsa\",
            \"csr\": \"{}\"
        }}", format!("{}-{}", location.country, id.to_string()), cert_string))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", configuration.lock().await.keys.cloudflare_key))
        .send().await {
            Ok(response) => {
                // println!("{:?}", response.text().await);
                let r = response.json::<CloudflareReturn>().await;

                match r {
                    Ok(return_value) => {
                        if return_value.success == false {
                            println!("[err]: cloudflare certificate creation FAILED, return value FAILURE. Reason: {:?}", return_value);
                        }

                        (return_value.result.certificate, cert_private)
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

    Ok((cert, key))
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