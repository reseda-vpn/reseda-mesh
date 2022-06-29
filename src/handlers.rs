
use std::{convert::Infallible, net::SocketAddr};
use std::env;
use dotenv::dotenv;
use uuid::Uuid;

use warp::{self, http::StatusCode};
use crate::models::{Server, IpResponse, Configuration};

fn with_config() -> Configuration {
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

pub async fn echo() -> Result<impl warp::Reply, Infallible> {
    Ok(Box::new(StatusCode::OK))
}

pub async fn register_server(
    ip: std::option::Option<SocketAddr>,
    authentication_key: Server,
) -> Result<impl warp::Reply, Infallible> {
    let config = with_config();

    match ip {
        Some(ip_addr) => {
            println!("{:?} @ {:?}", authentication_key.auth, ip_addr);

            let client =  reqwest::Client::new();

            match client.get(format!("https://ipgeolocationapi.co/v1/{}", ip_addr))
            .send().await {
                Ok(data) => {
                    if data.status().is_success() {
                        let r = match data.json::<IpResponse>().await {
                            Ok(val) => val,
                            Err(_) => {
                                return Ok(Box::new(StatusCode::INTERNAL_SERVER_ERROR))
                            }
                        };

                        println!("{:?}", r);

                        // Assign a name; 
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
                                \"proxied\": false
                            }}", format!("{}-{}.dns", r.country, id.to_string()), ip_addr))
                            .header("Content-Type", "application/json")
                            .header("Authorization", format!("Bearer {}", config.cloudflare_key))
                            .send().await {
                                Ok(_) => {},
                                Err(err) => {
                                    panic!("[err]: Error in setting non-proxied DNS {}", err)
                                },
                            }

                        match client.post("https://api.cloudflare.com/client/v4/zones/ebb52f1687a35641237774c39391ba2a/dns_records")
                            .body(format!("
                            {{
                                \"type\": \"A\",
                                \"name\": \"{}\",
                                \"content\": \"{}\",
                                \"ttl\": 3600,
                                \"priority\": 10,
                                \"proxied\": true
                            }}", format!("{}-{}", r.country, id.to_string()), ip_addr))
                            .header("Content-Type", "application/json")
                            .header("Authorization", format!("Bearer {}", config.cloudflare_key))
                            .send().await {
                                Ok(_) => {},
                                Err(err) => {
                                    panic!("[err]: Error in setting proxied DNS {}", err)
                                },
                            };

                        Ok(Box::new(StatusCode::OK))

                    }else {
                        println!("[err]: Error in getting data for IP geolocation");

                        Ok(Box::new(StatusCode::INTERNAL_SERVER_ERROR))
                    }
                },
                Err(_) => {
                    Ok(Box::new(StatusCode::INTERNAL_SERVER_ERROR))
                },
            }
        },
        None => {
            Ok(Box::new(StatusCode::INTERNAL_SERVER_ERROR))
        },
    }
}