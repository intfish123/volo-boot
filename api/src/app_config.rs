use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub port: u32,
    pub metric_port: Option<u32>,
    pub timeout: Option<u64>,
    // 订阅服务列表
    pub subscribe_service: Vec<String>,
    // 服务注册中心配置
    pub sd: ServerDiscover,
}

/// 服务发现配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerDiscover {
    pub nacos: NacosConfig,
}

/// nacos
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NacosConfig {
    pub server_addr: String,
    pub namespace: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub service_name: String,
}

/// 从nacos中获取的配置
#[derive(Default, Debug, Clone, Deserialize, Serialize)]
pub struct DynamicConfig {
    pub url_rate: Option<Vec<UrlRateConfig>>,
}

#[derive(Default, Debug, Clone, Deserialize, Serialize)]
pub struct UrlRateConfig {
    pub url: String,
    pub method: Vec<String>,
    pub rate: u64,
}
