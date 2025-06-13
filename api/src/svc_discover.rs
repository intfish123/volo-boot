use anyhow::anyhow;
use async_broadcast::Receiver;
use common::svc::nacos::NacosNamingAndConfigData;
use std::sync::Arc;
use volo::context::Endpoint;
use volo::discovery::{Change, Discover, Instance};
use volo::loadbalance::error::LoadBalanceError;
use volo::net::Address;
use volo::FastStr;

#[derive(Clone)]
pub struct NacosDiscover {
    pub nacos_naming_data: Arc<NacosNamingAndConfigData>,
}
impl Discover for NacosDiscover {
    type Key = FastStr;
    type Error = LoadBalanceError;

    async fn discover<'s>(
        &'s self,
        endpoint: &'s Endpoint,
    ) -> Result<Vec<Arc<Instance>>, Self::Error> {
        let inst_list = self
            .nacos_naming_data
            .sub_svc_map
            .get(endpoint.service_name.as_str());
        if let Some(inst_list) = inst_list {
            let mut _ins_ret = vec![];
            for x in inst_list.iter() {
                _ins_ret.push(Arc::new(Instance {
                    address: volo::net::Address::from(Address::Ip(
                        format!("{}:{}", x.ip, x.port).parse().unwrap(),
                    )),
                    weight: x.weight as u32,
                    tags: Default::default(),
                }));
            }
            Ok(_ins_ret)
        } else {
            let ee = anyhow!("no instances for {}", endpoint.service_name.to_string()).into();
            Err(LoadBalanceError::Discover(ee))
        }
    }

    fn key(&self, endpoint: &Endpoint) -> Self::Key {
        endpoint.service_name.clone()
    }

    fn watch(&self, _keys: Option<&[Self::Key]>) -> Option<Receiver<Change<Self::Key>>> {
        None
    }
}
