use warp::{self, Filter};

mod handlers;
mod models;
mod routes;

#[tokio::main]
async fn main() {
    let default_routes = routes::routes();

    let routes = default_routes.with(warp::cors().allow_any_origin());

    warp::serve(routes)
        .tls()
        .cert_path("cert.pem")
        .key_path("key.pem")
        .run(([0, 0, 0, 0], 443)).await;
}