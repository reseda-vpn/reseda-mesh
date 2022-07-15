use warp::{self, Filter};

use crate::models::{Server};

pub fn json_body() -> impl Filter<Extract = (Server,), Error = warp::Rejection> + Clone {
    warp::body::content_length_limit(1024 * 16).and(warp::body::json())
}