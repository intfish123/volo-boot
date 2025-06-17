use anyhow::anyhow;
use async_broadcast::{Receiver, RecvError};
use pd_rs_common::svc::nacos::NacosNamingAndConfigData;
use std::sync::Arc;
use dashmap::DashMap;
use tracing::warn;
use volo::context::Endpoint;
use volo::discovery::{Change, Discover, Instance, diff_address};
use volo::loadbalance::error::LoadBalanceError;
use volo::net::Address;
use volo::FastStr;

#[derive(Clone)]
pub struct NacosDiscover {
    pub nacos_naming_data: Arc<NacosNamingAndConfigData>,
    pub svc_change_sender: async_broadcast::Sender<Change<FastStr>>,
    pub svc_change_receiver: async_broadcast::Receiver<Change<FastStr>>,
    pub current_svc_instance: Arc<DashMap<FastStr, Vec<Arc<Instance>>>>
}

impl NacosDiscover {
    pub fn new(inner: Arc<NacosNamingAndConfigData>) -> Self {
        
        let (mut svc_ch_s, svc_ch_r) = async_broadcast::broadcast(100);
        svc_ch_s.set_overflow(true);

        let ret = Self{
            nacos_naming_data: inner,
            svc_change_sender: svc_ch_s,
            svc_change_receiver: svc_ch_r,
            current_svc_instance: Arc::new(DashMap::new()),
        };

        let mut r = ret.nacos_naming_data.event_listener.sub_svc_change_receiver.clone();
        let s = ret.svc_change_sender.clone();
        let current_svc_instance = ret.current_svc_instance.clone();
        tokio::spawn(async move {
            loop {
                match r.recv().await {
                    Ok(recv) => {
                        tracing::info!("Received svc change event: {:?}", recv);
                        let key: FastStr = recv.service_name.clone().into();
                        if let Some(is) = recv.instances.clone() {
                            let mut new_instance = Vec::with_capacity(is.len());
                            for x in is {
                                match format!("{}:{}", x.ip, x.port).parse() {
                                    Ok(addr) => new_instance.push(Arc::new(Instance {
                                        address: volo::net::Address::from(Address::Ip(addr)),
                                        weight: x.weight as u32,
                                        tags: Default::default(),
                                    })),
                                    Err(e) => tracing::error!("failed to parse instance address: {:?}, err: {}", x, e),
                                }
                            }
                            
                            let mut pre_svc_instance = vec![];
                            match current_svc_instance.get(key.as_str()) {
                                Some(instance) => {
                                    pre_svc_instance.extend(instance.value().iter().cloned());
                                }
                                None => {}
                            }
                            
                            let (ch, is_change) = diff_address(key.clone(), pre_svc_instance, new_instance.clone());
                            if is_change {
                                current_svc_instance.insert(key, new_instance);
                            }
                            
                            // always broadcast
                            let _ = s.try_broadcast(ch);
                        }
                        
                    },
                    Err(err) => {
                        match err { 
                            // if the channel is closed, break
                            RecvError::Closed => break,
                            _ => warn!("nacos discovering subscription error: {:?}", err)
                        }
                    },
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
        let key = endpoint.service_name.clone();
        
        let inst_list = self
            .nacos_naming_data
            .event_listener.sub_svc_map
            .get(key.as_str());
        if let Some(inst_list) = inst_list {
            let mut new_instance = Vec::with_capacity(inst_list.len());
            for x in inst_list.iter() {
                match format!("{}:{}", x.ip, x.port).parse() {
                    Ok(addr) => new_instance.push(Arc::new(Instance {
                        address: volo::net::Address::from(Address::Ip(addr)),
                        weight: x.weight as u32,
                        tags: Default::default(),
                    })),
                    Err(e) => tracing::error!("failed to parse instance address: {:?}, err: {}", x, e),
                }
            }
            
            self.current_svc_instance.insert(key, new_instance.clone());
            Ok(new_instance)
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
