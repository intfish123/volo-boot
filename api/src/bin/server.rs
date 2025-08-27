use api::rate_limiter::{init_limiter, RateLimiterConfigListener, DEFAULT_GROUP};
use api::{consts, router, svc_discover, ServiceContext};
use clap::Parser;
use order::order::OrderServiceClient;
use pd_rs_common::load_config::LoadConfig;
use pd_rs_common::svc::nacos::NacosNamingAndConfigData;
use std::sync::Arc;
use std::{net::SocketAddr, time::Duration};
use std::collections::HashMap;
use user::user::UserServiceClient;
use volo_http::{
    context::ServerContext,
    http::StatusCode,
    server::{layer::TimeoutLayer, Router, Server},
    Address,
};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    config: String,
}

fn timeout_handler(_: &ServerContext) -> (StatusCode, &'static str) {
    (StatusCode::REQUEST_TIMEOUT, "Timeout!\n")
}

#[volo::main]
async fn main() {
    // 解析命令行参数, 启动命令如: cargo run --package api --bin server -- --config=/volo-boot/api/config/app_config.toml
    let args = Args::parse();

    // 全局日志模块初始化
    let _logger_guard = pd_rs_common::logger::init_tracing(Some(2), None);

    // 加载配置
    let config_file_path = args.config;
    let app_config = api::app_config::AppConfig::load_toml(config_file_path.as_str()).unwrap();
    let app_config_clone = app_config.clone();

    // 注册服务
    let nacos_config = app_config.sd.nacos;

    // 获取nacos naming service
    let nacos_naming_data = Arc::new(
        NacosNamingAndConfigData::new(
            nacos_config.server_addr,
            nacos_config.namespace.unwrap_or("".to_string()),
            nacos_config.service_name.clone(),
            nacos_config.username,
            nacos_config.password,
        )
        .unwrap(),
    );

    // 指标抓取端口
    let metric_port = app_config.metric_port.unwrap_or(app_config.port);
    let mut need_standard_metrics = false;
    if metric_port != app_config.port {
        need_standard_metrics = true;
        // 注册用于抓取指标的服务实例
        let _metrics_nacos_svc_inst = nacos_naming_data
            .register_service(
                nacos_config.service_name.clone() + "_metrics",
                metric_port as i32,
                None,
                None,
                Default::default(),
            )
            .await;
    }

    // 注册基础服务
    let mut meta_map = HashMap::<String, String>::new();
    // 如果metrics端口是单独的则, 需要屏蔽服务端口的指标抓取
    if need_standard_metrics {
        meta_map.insert("disable_metrics".to_string(), "true".to_string());
    }
    let _nacos_svc_inst = nacos_naming_data
        .register_service(
            nacos_config.service_name.clone(),
            app_config.port as i32,
            None,
            None,
            meta_map,
        )
        .await;

    // 订阅rpc服务
    let service_context =
        subscribe_service(nacos_naming_data.clone(), app_config.subscribe_service).await;

    // 获取配置
    init_limiter(nacos_naming_data.clone(), app_config_clone.clone()).await;

    // 监听配置
    let rate_limiter_lis = Arc::new(RateLimiterConfigListener {
        data_id: nacos_config.service_name.clone(),
    });
    match nacos_naming_data
        .add_config_listener(
            nacos_config.service_name.clone(),
            DEFAULT_GROUP.to_string(),
            rate_limiter_lis,
        )
        .await
    {
        Ok(_) => tracing::info!(
            "add config listener: {} {}",
            nacos_config.service_name,
            DEFAULT_GROUP.to_string()
        ),
        Err(e) => tracing::error!("add config listener err: {}", e),
    }

    if need_standard_metrics {
        tokio::spawn(async move {
            // 启动metrics端口
            let addr: SocketAddr = format!("[::]:{}", metric_port).parse().unwrap();
            let addr = Address::from(addr);

            tracing::info!("Metrics port listening on {addr}");
            Server::new(router::build_metrics_router()).run(addr).await.unwrap();
        });
    }

    // 启动http服务
    let biz_app = Router::new()
        .merge(router::build_biz_router(service_context, !need_standard_metrics))
        .layer(TimeoutLayer::new(
            Duration::from_secs(app_config.timeout.unwrap_or(10)),
            timeout_handler,
        ));

    let addr: SocketAddr = format!("[::]:{}", app_config.port).parse().unwrap();
    let addr = Address::from(addr);

    tracing::info!("Listening on {addr}, need standard metrics is {need_standard_metrics}");

    Server::new(biz_app).run(addr).await.unwrap();
}

async fn subscribe_service(
    nacos_naming_data: Arc<NacosNamingAndConfigData>,
    service_names: Vec<String>,
) -> ServiceContext {
    let mut ret: ServiceContext = Default::default();

    if !service_names.is_empty() {
        let discover = svc_discover::NacosDiscover::new(nacos_naming_data.clone());

        tracing::info!("subscribe services: {}", service_names.join(", "));
        for sub_svc in service_names {
            let sub_ret = nacos_naming_data.subscribe_service(sub_svc.clone()).await;
            match sub_ret {
                Ok(_) => {
                    tracing::info!("subscribe service: {} success.", sub_svc.clone());
                }
                Err(e) => {
                    tracing::error!("subscribe service: {} field, error: {}", sub_svc.clone(), e);
                }
            }

            // 构建grpc客户端
            match sub_svc.as_str() {
                consts::RPC_USER_KEY => {
                    let user_client: UserServiceClient =
                        user::user::UserServiceClientBuilder::new(sub_svc)
                            .discover(discover.clone())
                            // .load_balance(volo::loadbalance::random::WeightedRandomBalance::new())
                            .load_balance(
                                volo::loadbalance::consistent_hash::ConsistentHashBalance::new(
                                    Default::default(),
                                ),
                            )
                            .build();
                    ret.rpc_cli_user = Some(user_client);
                }
                consts::RPC_ORDER_KEY => {
                    let order_client: OrderServiceClient =
                        order::order::OrderServiceClientBuilder::new(sub_svc)
                            .discover(discover.clone())
                            .load_balance(volo::loadbalance::random::WeightedRandomBalance::new())
                            .build();
                    ret.rpc_cli_order = Some(order_client);
                }
                _ => {}
            }
        }
    }

    ret
}
