use crate::app_config::{AppConfig, DynamicConfig};
use common::rate_limiter::memory_rate_limiter::MemoryRateLimiter;
use common::rate_limiter::RateLimiter;
use common::svc::nacos::NacosNamingAndConfigData;
use dashmap::DashMap;
use lazy_static::lazy_static;
use std::fmt;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::OnceCell;
use tokio_cron_scheduler::{Job, JobScheduler};
use volo_http::context::ServerContext;
use volo_http::http::{StatusCode, Uri};
use volo_http::request::ServerRequest;
use volo_http::response::ServerResponse;
use volo_http::server::IntoResponse;
use volo_http::server::middleware::Next;
use nacos_sdk::api::config::{ConfigChangeListener, ConfigResponse};

pub const DEFAULT_GROUP: &str = "DEFAULT_GROUP";

lazy_static! {
    static ref URL_LIMITER_MAP: DashMap<String, MemoryRateLimiter> = DashMap::new();
}

static JOB_SCHEDULER: OnceCell<JobScheduler> = OnceCell::const_new();

pub struct RateLimiterConfigListener{
    pub data_id: String,
}

impl ConfigChangeListener for RateLimiterConfigListener {
    fn notify(&self, config_resp: ConfigResponse) {
        tracing::info!("config change event={:?}", config_resp.clone());
        if self.data_id.as_str() == config_resp.data_id() {
            let r = serde_yml::from_str(config_resp.content().as_str());
            match r {
                Ok(content) => {
                    let c: DynamicConfig = content;
                    reset_limiter(c)
                }
                Err(e) => {
                    tracing::error!("serde_yml::from_str error {:?}", e);
                }
            }
        }
    }
}

pub async fn get_job_scheduler() -> &'static JobScheduler {
    JOB_SCHEDULER.get_or_init(|| async {
        let sched = JobScheduler::new().await.unwrap();
        sched
    }).await
}

pub async fn init_limiter(
    nacos_naming_and_config_data: Arc<NacosNamingAndConfigData>,
    app_config: AppConfig,
) {
    // let scheduler = get_job_scheduler().await;
    // let data_id = app_config.sd.nacos.service_name.clone();
    // let nnacd_clone = nacos_naming_and_config_data.clone();

    // let _r = scheduler.add(Job::new_async("0/5 * * * * *", move |_uuid, mut _l| {
    //     tracing::info!("schedule for reset limiter");
    //     let n = nnacd_clone.clone();
    //     let di = data_id.clone();
    //     Box::pin(async move {
    //         match n.config_data_map.get(di.as_str()) {
    //             Some(t) => {
    //                 let yml_res = serde_yml::from_str(t.content().as_str());
    //                 if yml_res.is_ok() {
    //                     let yml: DynamicConfig = yml_res.unwrap();
    //                     reset_limiter(yml);
    //                 }
    //             }
    //             None => {
    //                 tracing::info!("get {} with none", di.as_str());
    //             }
    //         }
    //     })
    // }).unwrap()).await;
    // 
    // scheduler.start().await;
    
    
    let content_ret = common::svc::nacos::get_config(
        nacos_naming_and_config_data.clone(),
        app_config.sd.nacos.service_name.clone(),
        DEFAULT_GROUP.to_string(),
    )
    .await;
    let Ok(content) = content_ret else {
        tracing::error!("get config content failed");
        return
    };
    let dynamic_config_ret = serde_yml::from_str(content.as_str());
    if dynamic_config_ret.is_err() {
        tracing::error!("serde_yml::from_str error {:?}", dynamic_config_ret);
        return;
    }
    let dynamic_config: DynamicConfig = dynamic_config_ret.unwrap();
    reset_limiter(dynamic_config);
    
    
}



pub fn reset_limiter(dynamic_config: DynamicConfig) {
    let Some(url_rate) = dynamic_config.url_rate else {
        return;
    };

    let mut valid_keys: Vec<String> = vec![];
    for urc in url_rate {
        for m in urc.method {
            let lower_method = m.to_lowercase();
            
            let key = format!("{}:{}", urc.url, lower_method);
            valid_keys.push(key.clone());

            let r = URL_LIMITER_MAP.get(key.clone().as_str());
            if let Some(r) = r {
                let (init_capacity, fill_rate, shard_num) = r.get_config();
                if init_capacity == 1 && fill_rate == urc.rate && shard_num == Some(1) {
                    tracing::info!("reset_limiter skip for {}", key.clone());
                    continue;
                }
            }
            URL_LIMITER_MAP.insert(key.clone(), MemoryRateLimiter::new(1, urc.rate, Some(1)));
            tracing::info!("reset rate limiter for {}: {}", key, urc.rate);
        }
    }
    URL_LIMITER_MAP.retain(|k, _| valid_keys.contains(k));
}

pub async fn do_rate_limiter(
    uri: Uri,
    cx: &mut ServerContext,
    req: ServerRequest,
    next: Next,
) -> Result<ServerResponse, StatusCode> {
    let path = uri.path();
    let method = req.method().clone();

    let key = format!("{}:{}", path, method.to_string().to_lowercase());
    let limiter = URL_LIMITER_MAP.get(key.as_str());
    if let Some(l) = limiter {
        if !l.try_acquire(key, 1) {
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }
    }

    let response = next.run(cx, req).await;
    let Ok(r) = response else {
        return Ok(response.into_response());
    };
    Ok(r)
}
