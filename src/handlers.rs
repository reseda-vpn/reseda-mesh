
use std::{convert::Infallible};
use std::env;
use dotenv::dotenv;
use serde::Deserialize;
use uuid::Uuid;

use warp::reply::json as json_reply;
use warp::{self, http::StatusCode};
use crate::models::{Server, IpResponse, Configuration, RegistryReturn};
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

pub fn with_config() -> Configuration {
    dotenv().expect(".env file not found");

    println!("Environment Keys:");
    for argument in env::args() {
        println!("{}", argument);
    }

    let authentication = match env::var("AUTHENTICATION_KEY") {
        Ok(val) => val,
        Err(_) => panic!("[err]: Environment variable: $AUTHENTICATION_KEY not set."),
    };
    let cloudflare = match env::var("CLOUDFLARE_KEY") {
        Ok(val) => val,
        Err(_) => panic!("[err]: Environment variable: $CLOUDFLARE_KEY not set."),
    };
    let database = match env::var("DATABASE_KEY") {
        Ok(val) => val,
        Err(_) => panic!("[err]: Environment variable: $DATABASE_KEY not set."),
    };

    Configuration {
        check_key: authentication,
        cloudflare_key: cloudflare,
        database_key: database
    }
}

pub async fn echo() -> Result<Box<dyn warp::Reply>, Infallible> {
    Ok(Box::new(StatusCode::OK))
}

pub async fn register_server(
    ip: String,
    authentication_key: Server,
) -> Result<Box<dyn warp::Reply>, Infallible> {
    let config = with_config();

    if authentication_key.auth != config.check_key {
        return Ok(Box::new(StatusCode::FORBIDDEN))
    }

    let client =  reqwest::Client::new();

    let data = match client.get(format!("https://ipgeolocationapi.co/v1/{}", ip))
    .send().await {
        Ok(data) => data,
        Err(_) => {
            return Ok(Box::new(StatusCode::INTERNAL_SERVER_ERROR))
        },
    };

    if !data.status().is_success() {
        return Ok(Box::new(StatusCode::INTERNAL_SERVER_ERROR))
    }

    let r = match data.json::<IpResponse>().await {
        Ok(val) => val,
        Err(_) => {
            return Ok(Box::new(StatusCode::INTERNAL_SERVER_ERROR))
        }
    };

    println!("{:?}", r);

    let id = Uuid::new_v4();

    let client = reqwest::Client::new();

    match client.post("https://api.cloudflare.com/client/v4/zones/ebb52f1687a35641237774c39391ba2a/dns_records")
        .body(format!("
        {{
            \"type\": \"A\",
            \"name\": \"{}.dns\",
            \"content\": \"{}\",
            \"ttl\": 3600,
            \"priority\": 10,
            \"proxied\": true
        }}", format!("{}-{}", r.country, id.to_string()), ip))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", config.cloudflare_key))
        .send().await {
            Ok(return_val) => {
                println!("{:?}", return_val);
            },
            Err(err) => {
                panic!("[err]: Error in setting proxied DNS {}", err)
            },
        };

    match client.post("https://reseda.app/api/server/register")
        .body(format!("
        {{
            \"id\": \"{}\",
            \"location\": \"{}\",
            \"country\": \"{}\",
            \"hostname\": \"{}\",
            \"virtual\": \"false\",
            \"flag\": \"{}\",
            \"override\": \"false\"
        }}", format!("{}-{}", r.country, id.to_string()), r.timezone, r.city, ip, r.city.to_lowercase().replace(" ", "-")))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", config.cloudflare_key))
        .send().await {
            Ok(res) => {
                println!("Reseda Returned: {:?}", res.text().await);
            },
            Err(err) => {
                panic!("[err]: Error in registering server; {}", err)
            },
        };
    
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
        }}", format!("{}-{}", r.country, id.to_string()), cert_string))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", config.cloudflare_key))
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
                        return Ok(Box::new(StatusCode::INTERNAL_SERVER_ERROR))
                    }
                }
            },
            Err(err) => {
                println!("[err]: Error in setting proxied DNS {}", err);
                return Ok(Box::new(StatusCode::INTERNAL_SERVER_ERROR))
            },
        };

    let rr = RegistryReturn {
        cert: cert,
        key: key,
        ip: ip,
        id: format!("{}-{}", r.country, id.to_string()),
        res: r
    };

    Ok(Box::new(json_reply(&rr)))
}