use routes::json_body;
use state::MeshState;
use tokio::sync::Mutex;
use warp::{self, Filter};
use std::{sync::Arc, convert::Infallible};

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

    warp::serve(routes)
        .tls()
        .cert_path("cert.pem")
        .key_path("key.pem")
        .run(([0, 0, 0, 0], 443)).await;
}

fn with_config(config: Mesh) -> impl Filter<Extract = (Mesh,), Error = Infallible> + Clone {
    warp::any().map(move || config.clone())
}