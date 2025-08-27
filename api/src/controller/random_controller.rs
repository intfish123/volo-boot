use crate::controller::R;
use crate::ServiceContext;
use rand::Rng;
use volo_http::request::Request;
use volo_http::{server::extract::Query, utils::Extension};

/// 返回一个随机数
pub async fn get_random(
    Extension(_ctx): Extension<ServiceContext>,
    Query(_param): Query<serde_json::Value>,
    _req: Request,
) -> R<i32> {
    let i = rand::rng().random::<i32>();
    R::ok(i)
}
