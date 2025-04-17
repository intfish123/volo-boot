use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub port: u32,
    pub sd: ServerDiscover,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerDiscover {
    pub nacos: NacosConfig,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NacosConfig {
    pub server_addr: String,
    pub namespace: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub service_name: String,
}
