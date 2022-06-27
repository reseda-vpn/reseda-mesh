
use std::{convert::Infallible, net::SocketAddr};

use warp::{self, http::StatusCode, Filter};
use crate::models::Server;

pub async fn register_server(
    ip: std::option::Option<SocketAddr>,
    new_server: Server
) -> Result<impl warp::Reply, Infallible> {
    println!("{:?} @ {:?}", new_server, ip);

    Ok(StatusCode::CREATED)
}

pub async fn registration_list(
    ip: std::option::Option<SocketAddr>
) -> Result<impl warp::Reply, Infallible> {
    println!("{:?}", ip);
    

    Ok(Box::new(StatusCode::NOT_FOUND))
}