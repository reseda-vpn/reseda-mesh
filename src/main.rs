use warp;

mod handlers;
mod models;
mod routes;

#[tokio::main]
async fn main() {
    let routes = routes::routes();

    warp::serve(routes)
        .tls()
        .cert_path("cert.pem")
        .key_path("key.pem")
        .run(([0, 0, 0, 0], 443)).await;
}