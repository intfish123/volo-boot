pub mod app_config;
pub mod consts;
pub mod prometheus;
pub mod router;
pub mod svc_discover;

pub mod controller;
pub mod rate_limiter;

use order::order::OrderServiceClient;
use user::user::UserServiceClient;

/// 这个结构体里面放每个rpc的客户端
#[derive(Clone, Default)]
pub struct ServiceContext {
    pub rpc_cli_user: Option<UserServiceClient>,
    pub rpc_cli_order: Option<OrderServiceClient>,
}
