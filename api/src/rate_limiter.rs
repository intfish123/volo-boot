use crate::app_config::{AppConfig, DynamicConfig};
use dashmap::DashMap;
use lazy_static::lazy_static;
use nacos_sdk::api::config::{ConfigChangeListener, ConfigResponse};
use pd_rs_common::rate_limiter::memory_rate_limiter::MemoryRateLimiter;
use pd_rs_common::rate_limiter::RateLimiter;
use pd_rs_common::svc::nacos::NacosNamingAndConfigData;
use regex::Regex;
use std::sync::Arc;
use tokio::sync::OnceCell;
use tokio_cron_scheduler::JobScheduler;
use volo_http::context::ServerContext;
use volo_http::http::{StatusCode, Uri};
use volo_http::request::Request;
use volo_http::response::Response;
use volo_http::server::middleware::Next;
use volo_http::server::IntoResponse;

pub const DEFAULT_GROUP: &str = "DEFAULT_GROUP";

lazy_static! {
    static ref URL_LIMITER_MAP: DashMap<String, ReteLimiterData> = DashMap::new();
}

static JOB_SCHEDULER: OnceCell<JobScheduler> = OnceCell::const_new();

pub struct ReteLimiterData {
    pub url: String,
    pub url_regex: Regex,
    pub method: String,
    pub memory_rate_limiter: MemoryRateLimiter,
}

pub struct RateLimiterConfigListener {
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
    JOB_SCHEDULER
        .get_or_init(|| async {
            let sched = JobScheduler::new().await.unwrap();
            sched
        })
        .await
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

    let content_ret = nacos_naming_and_config_data
        .get_config(
            app_config.sd.nacos.service_name.clone(),
            DEFAULT_GROUP.to_string(),
        )
        .await;
    let Ok(content) = content_ret else {
        tracing::error!("get config content failed");
        return;
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
        let urc_clone = urc.clone();
        for m in urc.method {
            let lower_method = m.to_lowercase();
            let key = format!("{}:{}", urc.url, lower_method);
            valid_keys.push(key.clone());

            let mut skip = false;
            for x in URL_LIMITER_MAP.iter() {
                if x.url == urc.url && x.method == lower_method {
                    let (a, b, c) = x.memory_rate_limiter.get_config();
                    if a == 1 && b == urc.rate && c == Some(1) {
                        skip = true;
                        tracing::info!("skip reset limiter for {:?}", urc_clone);
                        break;
                    }
                }
            }
            if skip {
                continue;
            }

            let reg_ret = Regex::new(urc.url.clone().as_str());
            if let Ok(reg) = reg_ret {
                let data = ReteLimiterData {
                    url: urc.url.clone(),
                    url_regex: reg,
                    method: lower_method,
                    memory_rate_limiter: MemoryRateLimiter::new(1, urc.rate, Some(1)),
                };
                URL_LIMITER_MAP.insert(key.clone(), data);
                tracing::info!("reset rate limiter for {}: {}", key, urc.rate);
            } else if let Err(e) = reg_ret {
                tracing::error!("parse regex for {}: {}", urc.url.clone(), e);
            }
        }
    }
    URL_LIMITER_MAP.retain(|k, _| valid_keys.contains(k));
}

pub async fn do_rate_limiter(
    uri: Uri,
    cx: &mut ServerContext,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let path = uri.path();
    let method = req.method().to_string().to_lowercase();

    for m in URL_LIMITER_MAP.iter() {
        if m.url_regex.is_match(path) && m.method == method {
            if !m.memory_rate_limiter.try_acquire(path.to_string(), 1) {
                return Err(StatusCode::TOO_MANY_REQUESTS);
            }
        }
    }

    let response = next.run(cx, req).await;
    let Ok(r) = response else {
        return Ok(response.into_response());
    };
    Ok(r)
}

#[cfg(test)]
mod regex_test {
    use regex::Regex;

    #[test]
    fn regex_test() {
        let re = Regex::new("/user.*").unwrap();
        assert_eq!(re.is_match("/user/query-one"), true);
    }
}
