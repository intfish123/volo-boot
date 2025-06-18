use crate::controller::R;
use crate::ServiceContext;
use order::order::{GetOrderRequest, Order};
use volo_http::request::ServerRequest;
use volo_http::{http::StatusCode, server::extract::Query, Extension};

/// 通过id获取用户实体
pub async fn get_order(
    Extension(ctx): Extension<ServiceContext>,
    Query(param): Query<serde_json::Value>,
    _req: ServerRequest,
) -> R<Order> {
    // 如果 order rpc 服务为空，直接返回错误码
    let Some(rpc_cli) = ctx.rpc_cli_order else {
        return R::error_status_code(StatusCode::GONE, "Gone");
    };

    // 获取参数
    let id = param.get("id");
    let user_id = param.get("user_id");

    // id 和 user_id 参数不能同时为空
    if id.is_none() && user_id.is_none() {
        return R::error_status_code(StatusCode::BAD_REQUEST, "id 和 user_id 不能同时为空");
    }

    // 如果 load_balance 用的ConsistentHashBalance, 则需要在本地变量（类似于java中的ThreadLocal变量）设置RequestHash
    // 每个请求会自动创建本地变量 METAINFO, 然后在自己的方法里面直接用就行, 参考: https://docs.rs/tokio/latest/tokio/task/struct.LocalKey.html
    // METAINFO.with(|m| {
    //     m.borrow_mut().insert(RequestHash(id as u64));
    // });

    let ret;

    if let Some(id) = id {
        let Some(str_id) = id.as_str() else {
            return R::error_status_code(StatusCode::BAD_REQUEST, "id 解析失败");
        };
        let Ok(id) = str_id.parse() else {
            return R::error_status_code(StatusCode::BAD_REQUEST, "id 解析失败");
        };
        // 请求user rpc服务，然后返回
        ret = rpc_cli
            .get_order(GetOrderRequest {
                id: Some(id),
                user_id: None,
            })
            .await;
    } else if let Some(user_id) = user_id {
        let Some(str_user_id) = user_id.as_str() else {
            return return R::error_status_code(StatusCode::BAD_REQUEST, "user_id 解析失败");
        };
        let Ok(usr_id) = str_user_id.parse() else {
            return return R::error_status_code(StatusCode::BAD_REQUEST, "user_id 解析失败");
        };
        // 请求user rpc服务，然后返回
        ret = rpc_cli
            .get_order(GetOrderRequest {
                id: None,
                user_id: Some(usr_id),
            })
            .await;
    } else {
        return R::error_status_code(StatusCode::BAD_REQUEST, "id 和 user_id 不能同时为空");
    }

    match ret {
        Ok(u) => R::ok(u.into_inner()),
        Err(e) => {
            tracing::error!("get_order error: {:?}", e);
            R::server_error(e.message())
        }
    }
}
