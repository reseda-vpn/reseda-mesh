use std::convert::Infallible;
use std::net::SocketAddr;

use warp::{self, Filter};

use crate::handlers;
use crate::models::{Server, Configuration};

/// All customer routes
pub fn routes(
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    register_server()
    // .or(registration())
}

// /// GET /register
// fn registration(
// ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
//     warp::path("register")
//         .and(warp::get())
//         .and(with_route())
//         .and(with_config())
//         .and_then(handlers::registration_list)
// }

/// POST /register
fn register_server(
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path("register")
        .and(warp::post())
        .and(with_route())
        .and(json_body())
        .and_then(handlers::register_server)
}

fn json_body() -> impl Filter<Extract = (Server,), Error = warp::Rejection> + Clone {
    warp::body::content_length_limit(1024 * 16).and(warp::body::json())
}

fn with_route() -> impl Filter<Extract = (std::option::Option<SocketAddr>,), Error = Infallible> + Clone {
    warp::addr::remote()
}