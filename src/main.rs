use handlers::{with_config, CloudflareReturn};
use rcgen::generate_simple_self_signed;
use warp::{self, Filter};

use std::fs::File;
use std::io::Write;

mod handlers;
mod models;
mod routes;

#[tokio::main]
async fn main() {
    let default_routes = routes::routes();

    let routes = default_routes.with(warp::cors().allow_any_origin());

    intialize().await;

    warp::serve(routes)
        .tls()
        .cert_path("cert.pem")
        .key_path("key.pem")
        .run(([0, 0, 0, 0], 443)).await;
}

async fn intialize() {
    let client = reqwest::Client::new();
    let config = with_config();

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
    
}