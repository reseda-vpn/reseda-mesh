use dotenv::dotenv;
use rcgen::generate_simple_self_signed;
use sqlx::{Pool, MySql, mysql::MySqlPoolOptions};
use std::collections::{HashMap, VecDeque};
use std::{env, sync::Arc};
use std::fs::File;
use std::io::Write;
use tokio::sync::Mutex;
use reqwest::Client;

use crate::models::TaskQueue;
use crate::{models::{Configuration, Stack}, models::CloudflareReturn};

#[derive(Clone)]
pub struct MeshState {
    pub keys: Configuration,
    pub pool: Pool<MySql>,
    pub client: Client,

    pub instance_stack: Stack,
    pub task_queue: TaskQueue
}

pub fn with_environment() -> Configuration {
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
    let database = match env::var("DATABASE_URL") {
        Ok(val) => val,
        Err(_) => panic!("[err]: Environment variable: $DATABASE_URL not set."),
    };

    Configuration {
        check_key: authentication,
        cloudflare_key: cloudflare,
        database_key: database
    }
}

impl MeshState {
    pub async fn initialize() -> Self {
        let client = reqwest::Client::new();
        let config = with_environment();

        let pool = match MySqlPoolOptions::new()
                .max_connections(5)
                .connect(&config.database_key).await {
                    Ok(pool) => {
                        println!("[service] sqlx::success Successfully started pool.");
                        pool
                    },
                    Err(error) => {
                        panic!("[service] sqlx::error Failed to initialize SQLX pool. Reason: {}", error);
                    }
            };

        let cert = generate_simple_self_signed(vec![format!("mesh.reseda.app")]).unwrap();
        let cert_public = cert.serialize_request_pem().unwrap();
        let cert_private = cert.serialize_private_key_pem();
        let cert_string = cert_public.replace("\r", "").split("\n").collect::<Vec<&str>>().join("\\n");

        let (cert, key) = match client.post("https://api.cloudflare.com/client/v4/certificates")
            .body(format!("
            {{
                \"hostnames\": [
                    \"mesh.reseda.app\"
                ],
                \"requested_validity\": 5475,
                \"request_type\": \"origin-rsa\",
                \"csr\": \"{}\"
            }}", cert_string))
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
                            panic!("[err]: Deserializing Cloudflare Result: {}", err)
                        }
                    }
                },
                Err(err) => {
                    panic!("[err]: Error in setting proxied DNS {}", err);
                },
            };
        
        match File::create("key.pem") {
            Ok(mut output) => {
                match write!(output, "{}", key) {
                    Ok(_) => {},
                    Err(err) => {
                        println!("[err]: Unable to write file file::key.pem; {}", err);
                    },
                }
            },
            Err(err) => {
                println!("[err]: Unable to open file stream for file::key.pem; {}", err)
            },
        };

        match File::create("cert.pem") {
            Ok(mut output) => {
                match write!(output, "{}", cert) {
                    Ok(_) => {},
                    Err(err) => {
                        println!("[err]: Unable to write file file::cert.pem; {}", err);
                    },
                }
            },
            Err(err) => {
                println!("[err]: Unable to open file stream for file::cert.pem; {}", err)
            },
        };

        // Return Configuration
        MeshState {
            keys: config,
            pool: pool,
            client: client,

            instance_stack: Arc::new(Mutex::new(HashMap::new())),
            task_queue: Arc::new(Mutex::new(VecDeque::new()))
        }
    }
}