//! nacos服务注册与发现

use anyhow::{anyhow, Result};
use dashmap::DashMap;
use nacos_sdk::api::constants;
use nacos_sdk::api::naming::{
    NamingChangeEvent, NamingEventListener, NamingService, NamingServiceBuilder, ServiceInstance,
};
use nacos_sdk::api::props::ClientProps;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Debug)]
pub struct NacosNamingData {
    naming: NamingService,

    state: RwLock<NamingState>,

    pub sub_svc_map: DashMap<String, Vec<ServiceInstance>>,
}

#[derive(Clone, Debug, Default)]
pub struct NamingState {
    service_name: String,
    group_name: Option<String>,
    service_instance: Vec<ServiceInstance>,
}

impl NacosNamingData {
    pub fn get_state(&self) -> NamingState {
        self.state.read().unwrap().clone()
    }
    pub fn update_state(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instance: Vec<ServiceInstance>,
    ) {
        let mut state = self.state.write().unwrap();
        state.service_name = service_name;
        state.group_name = group_name;
        state.service_instance = service_instance;
    }
}

/// 构建naming service
pub async fn build_naming_server(
    server_addr: String,
    namespace: String,
    app_name: String,
    user_name: Option<String>,
    password: Option<String>,
) -> Result<NacosNamingData> {
    // 因为它内部会初始化与服务端的长链接，后续的数据交互及变更订阅，都是实时地通过长链接告知客户端的。

    let mut client_props = ClientProps::new()
        // eg. "127.0.0.1:8848"
        .server_addr(server_addr)
        .namespace(namespace)
        .app_name(app_name.clone());

    let mut enable_http_login = false;
    if let Some(user_name) = user_name {
        if !user_name.is_empty() {
            client_props = client_props.auth_username(user_name);
            enable_http_login = true;
        }
    }
    if let Some(password) = password {
        if !password.is_empty() {
            client_props = client_props.auth_password(password);
            enable_http_login = true;
        }
    }
    let naming_service = if enable_http_login {
        NamingServiceBuilder::new(client_props)
            .enable_auth_plugin_http()
            .build()?
    } else {
        NamingServiceBuilder::new(client_props).build()?
    };
    Ok(NacosNamingData {
        naming: naming_service,
        state: RwLock::new(NamingState {
            service_name: "".to_string(),
            group_name: None,
            service_instance: Vec::new(),
        }),
        sub_svc_map: DashMap::new(),
    })
}

/// 向nacos注册自己
pub async fn register_service(
    nacos_naming_data: Arc<NacosNamingData>,
    service_name: String,
    service_port: i32,
    service_metadata: HashMap<String, String>,
) -> Result<Vec<ServiceInstance>> {
    // 请注意！一般情况下，应用下仅需一个 Naming 客户端，而且需要长期持有直至应用停止。
    // 因为它内部会初始化与服务端的长链接，后续的数据交互及变更订阅，都是实时地通过长链接告知客户端的。

    // 注册服务
    let local_ip = local_ip_address::local_ip()?;
    let svc_inst = ServiceInstance {
        ip: local_ip.to_string(),
        port: service_port,
        metadata: service_metadata,
        ..Default::default()
    };

    let group_name = Some(constants::DEFAULT_GROUP.to_string());

    let _register_inst_ret = nacos_naming_data
        .naming
        .register_instance(service_name.clone(), group_name.clone(), svc_inst.clone())
        .await;
    match _register_inst_ret {
        Ok(_) => {
            tracing::info!(
                "Register service {}@{} to nacos successfully",
                service_name.clone(),
                local_ip.to_string()
            );
            nacos_naming_data.update_state(service_name, group_name, vec![svc_inst.clone()]);
            Ok(vec![svc_inst])
        }
        Err(e) => {
            tracing::error!(
                "Failed to register service {}@{} to nacos: {}",
                service_name.clone(),
                local_ip.to_string(),
                e
            );
            Err(anyhow!(e))
        }
    }
}

/// 从nacos注销
pub async fn unregister_service(nacos_naming_data: Arc<NacosNamingData>) -> Result<()> {
    let naming_service = nacos_naming_data.naming.clone();
    let state = nacos_naming_data.get_state();
    let service_name = state.service_name;
    let group_name = state.group_name;
    let svc_inst = state.service_instance;

    let mut errors = Vec::new();
    let mut insts = Vec::new();

    if !svc_inst.is_empty() {
        for inst in svc_inst {
            match naming_service
                .deregister_instance(service_name.clone(), group_name.clone(), inst.clone())
                .await
            {
                Ok(_) => insts.push(format!("{}@{}", service_name.clone(), inst.ip.clone())),
                Err(e) => errors.push(e.to_string()),
            }
        }
    }

    if !errors.is_empty() {
        Err(anyhow!(
            "Failed to deregister instances: {}",
            errors.join(", ")
        ))
    } else {
        tracing::info!("Deregister instances: {}", insts.join(", "));
        Ok(())
    }
}

impl NamingEventListener for NacosNamingData {
    fn event(&self, event: Arc<NamingChangeEvent>) {
        tracing::info!("subscriber notify event={:?}", event.clone());
        let inst_list = event.instances.clone().unwrap_or_default();
        self.sub_svc_map
            .insert(event.service_name.clone(), inst_list);
    }
}

pub async fn subscribe_service(
    nacos_naming_data: Arc<NacosNamingData>,
    sub_service_name: String,
) -> Result<()> {
    let temp_naming = nacos_naming_data;
    let naming_service = temp_naming.naming.clone();

    let state = temp_naming.get_state();
    let group_name = state.group_name;
    match naming_service
        .subscribe(sub_service_name, group_name, Vec::default(), temp_naming)
        .await
    {
        Ok(_) => Ok(()),
        Err(e) => Err(anyhow!("subscribe_service error: {}", e)),
    }
}
