use crate::consts::BINCODE_CONFIG_STANDARD;
use crate::controller::R;
use crate::ServiceContext;
use rand::prelude::*;
use user::user::{GetUserRequest, User};
use volo::loadbalance::RequestHash;
use volo::METAINFO;
use volo_http::request::Request;
use volo_http::{http::StatusCode, server::extract::Query, utils::Extension};

/// 通过id获取用户实体
pub async fn get_user(
    Extension(ctx): Extension<ServiceContext>,
    Query(param): Query<serde_json::Value>,
    _req: Request,
) -> R<User> {
    let Some(rpc_cli) = ctx.rpc_cli_user else {
        return R::error_status_code(StatusCode::GONE, "Gone");
    };

    let Some(id) = param.get("id") else {
        return R::error_status_code(StatusCode::BAD_REQUEST, "id 不能为空");
    };

    let Some(str_id) = id.as_str() else {
        return R::error_status_code(StatusCode::BAD_REQUEST, "id 解析失败");
    };

    let Ok(id) = str_id.parse() else {
        return R::error_status_code(StatusCode::BAD_REQUEST, "id 解析失败");
    };

    // 如果 load_balance 用的ConsistentHashBalance, 则需要在本地变量（类似于java中的ThreadLocal变量）设置RequestHash
    // 每个请求会自动创建本地变量 METAINFO, 然后在自己的方法里面直接用就行, 参考: https://docs.rs/tokio/latest/tokio/task/struct.LocalKey.html
    METAINFO.with(|m| {
        // 使用user_id进行hash
        // let bytes = bincode::encode_to_vec(id, BINCODE_CONFIG_STANDARD).unwrap();

        // 随机数
        let random_id = rand::rng().random::<i32>();
        let bytes = bincode::encode_to_vec(random_id, BINCODE_CONFIG_STANDARD).unwrap();

        let hash = mur3::murmurhash3_x64_128(bytes.as_slice(), 0).0;
        m.borrow_mut().insert(RequestHash(hash));
    });

    // 请求user rpc服务，然后返回
    let ret_user = rpc_cli
        .get_user(GetUserRequest {
            id: Some(id),
            username: Some("some name123".into()),
        })
        .await;
    match ret_user {
        Ok(u) => R::ok(u.into_inner()),
        Err(e) => {
            tracing::error!("get_user error: {:?}", e);
            R::server_error(e.message())
        }
    }
}
