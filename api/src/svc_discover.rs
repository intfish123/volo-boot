use anyhow::anyhow;
use async_broadcast::Receiver;
use pd_rs_common::svc::nacos::NacosNamingAndConfigData;
use std::sync::Arc;
use tracing::warn;
use volo::context::Endpoint;
use volo::discovery::{Change, Discover, Instance};
use volo::loadbalance::error::LoadBalanceError;
use volo::net::Address;
use volo::FastStr;

#[derive(Clone)]
pub struct NacosDiscover {
    pub nacos_naming_data: Arc<NacosNamingAndConfigData>,
    pub svc_change_sender: async_broadcast::Sender<Change<FastStr>>,
    pub svc_change_receiver: async_broadcast::Receiver<Change<FastStr>>,
}

impl NacosDiscover {
    pub fn new(inner: Arc<NacosNamingAndConfigData>) -> Self {
        
        let (mut svc_ch_s, svc_ch_r) = async_broadcast::broadcast(100);
        svc_ch_s.set_overflow(true);

        let ret = Self{
            nacos_naming_data: inner,
            svc_change_sender: svc_ch_s,
            svc_change_receiver: svc_ch_r,
        };

        let mut r = ret.nacos_naming_data.event_listener.sub_svc_change_receiver.clone();
        let s = ret.svc_change_sender.clone();
        tokio::spawn(async move {
            loop {
                match r.recv().await {
                    Ok(recv) => {
                        tracing::info!("Received svc change event: {:?}", recv);
                        let key: FastStr = recv.service_name.clone().into();
                        if let Some(is) = recv.instances.clone() {
                            let mut _ins_ret = vec![];
                            for x in is {
                                _ins_ret.push(Arc::new(Instance {
                                    address: volo::net::Address::from(Address::Ip(
                                        format!("{}:{}", x.ip, x.port).parse().unwrap(),
                                    )),
                                    weight: x.weight as u32,
                                    tags: Default::default(),
                                }));
                            }

                            let ch = Change {
                                key: key,
                                all: _ins_ret,
                                added: Default::default(),
                                updated: Default::default(),
                                removed: Default::default(),
                            };
                            let _ = s.try_broadcast(ch);
                        }
                        
                    },
                    Err(err) => warn!("nacos discovering subscription error: {:?}", err),
                }
            }
        });
        
        ret
    }
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
            .event_listener.sub_svc_map
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
        Some(self.svc_change_receiver.clone())
    }
}
