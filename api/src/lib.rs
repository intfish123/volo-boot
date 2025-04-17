pub mod app_config;
pub mod consts;
pub mod prometheus;
pub mod router;
pub mod svc_discover;

pub mod controller;

use order::order::OrderServiceClient;
use user::user::UserServiceClient;

/// 这个结构体里面放每个rpc的客户端,目前只放了一个user的
#[derive(Clone)]
pub struct ServiceContext {
    pub rpc_cli_user: Option<UserServiceClient>,
    pub rpc_cli_order: Option<OrderServiceClient>,
}
