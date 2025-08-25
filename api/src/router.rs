use crate::prometheus::{setup_metrics_recorder, track_metrics};
use crate::rate_limiter::do_rate_limiter;
use crate::{controller, ServiceContext};
use std::future::ready;
use volo_http::{
    response::Response,
    server::{middleware, route::get, IntoResponse},
    Router, utils::Extension
};

/// 构建路由
pub fn build_router(cxt: ServiceContext) -> Router {
    let record_handler = setup_metrics_recorder();

    Router::new()
        .route("/metrics", get(move || ready(record_handler.render())))
        .route(
            "/user/query-one",
            get(controller::user_controller::get_user),
        )
        .route(
            "/order/query-one",
            get(controller::order_controller::get_order),
        )
        .layer(middleware::from_fn(track_metrics))
        .layer(middleware::from_fn(do_rate_limiter))
        .layer(middleware::map_response(headers_map_response))
        .layer(Extension(cxt))
}

async fn headers_map_response(response: Response) -> impl IntoResponse {
    (
        [
            ("Access-Control-Allow-Origin", "*"),
            ("Access-Control-Allow-Headers", "*"),
            ("Access-Control-Allow-Method", "*"),
        ],
        response,
    )
}
