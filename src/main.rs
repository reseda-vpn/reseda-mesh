use warp;

mod handlers;
mod models;
mod routes;

#[tokio::main]
async fn main() {
    let routes = routes::routes();

    warp::serve(routes)
        .run(([127, 0, 0, 1], 3000))
        .await;
}