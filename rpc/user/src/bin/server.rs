use anyhow::anyhow;
use clap::Parser;
use pd_rs_common::load_config::LoadConfig;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use user::app_config::AppConfig;
use user::S;
use volo_grpc::codegen::futures::TryFutureExt;
use volo_grpc::server::{Server, ServiceBuilder};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    config: String,
}

#[volo::main]
async fn main() {
    let args = Args::parse();

    // 这里不要使用 `let _ = xxx;` 的形式来接受返回结果，避免被立即drop掉导致日志声明周期有问题
    let _logger_guard = pd_rs_common::logger::init_tracing();

    let config_file_path = args.config;
    let app_config = AppConfig::load_toml(config_file_path.as_str()).unwrap();

    let addr: SocketAddr = format!("[::]:{}", app_config.port).parse().unwrap();
    let addr = volo::net::Address::from(addr);

    // 注册服务
    let nacos_config = app_config.sd.nacos;

    let nacos_naming_data = Arc::new(
        pd_rs_common::svc::nacos::NacosNamingAndConfigData::new(
            nacos_config.server_addr,
            nacos_config.namespace.unwrap_or("".to_string()),
            nacos_config.service_name.clone(),
            nacos_config.username,
            nacos_config.password,
        )
        .unwrap(),
    );

    let nacos_svc_inst = nacos_naming_data
        .register_service(
            nacos_config.service_name,
            app_config.port as i32,
            None,
            None,
            Default::default(),
        )
        .await;

    // 优雅停机
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(());

    let signal_task = tokio::spawn(async move {
        let mut term = signal::unix::signal(signal::unix::SignalKind::terminate())
            .map_err(|e| anyhow!("Failed to create SIGTERM handler: {}", e))?;
        let int = signal::ctrl_c().map_err(|e| anyhow!("Failed to register CTRL-C handler: {}", e));
        tokio::select! {
            _ = term.recv() => tracing::info!("receive sigterm"),
            _ = int => tracing::info!("receive ctrl_c")
        }

        if let Ok(_) = nacos_svc_inst {
            let _ret = nacos_naming_data.deregister_service().await;

            tokio::time::sleep(Duration::from_secs(3)).await;
        }
        shutdown_tx.send(()).ok();
        Ok::<_, anyhow::Error>(())
    });

    let server_task = tokio::spawn(async move {
        Server::new()
            .add_service(
                ServiceBuilder::new(user_volo_gen::user::UserServiceServer::new(S)).build(),
            )
            .run_with_shutdown(addr, async {
                let _ = shutdown_rx.changed().await;
                Ok(())
            })
            .await
            .unwrap()
    });

    let _tasks = tokio::join!(server_task, signal_task);
}
