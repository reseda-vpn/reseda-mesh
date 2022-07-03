use warp::{self, Filter};

use crate::handlers;
use crate::models::{Server};

/// All customer routes
pub fn routes(
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    register_server()
    .or(response())
    // .or(registration())
}

fn response(
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path::end()
        .and(warp::get())
        .and_then(handlers::echo)
}

/// POST /register
fn register_server(
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path!("register" / String)
        .and(warp::post())
        .and(json_body())
        .and_then(handlers::register_server)
}

fn json_body() -> impl Filter<Extract = (Server,), Error = warp::Rejection> + Clone {
    warp::body::content_length_limit(1024 * 16).and(warp::body::json())
}