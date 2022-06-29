use warp;

mod handlers;
mod models;
mod routes;

#[tokio::main]
async fn main() {
    let routes = routes::routes();

    warp::serve(routes)
        .run(([0, 0, 0, 0], 3000))
        .await;
}