pub use order_volo_gen::order;

pub mod app_config;

pub struct S;

impl order_volo_gen::order::OrderService for S {
    /// 根据 orderId 或 userId 查询订单
    async fn get_order(
        &self,
        _req: ::volo_grpc::Request<order_volo_gen::order::GetOrderRequest>,
    ) -> ::std::result::Result<::volo_grpc::Response<order_volo_gen::order::Order>, ::volo_grpc::Status>
    {
        let req_data = _req.into_inner();
        tracing::info!("get_order: {:?}", req_data);

        if let Some(req_id) = req_data.id {

            // 虚假数据
            let order = order::Order {
                id: req_id,
                user_id: 1,
                name: "示例订单1".into(),
                product_name: "商品1".into(),
                create_at: chrono::Local::now().timestamp(),
                extra: Default::default(),
            };
            return Ok(volo_grpc::Response::new(order));
        } else if let Some(user_id) = req_data.user_id {
            // 虚假数据
            let order = order::Order {
                id: 1,
                user_id,
                name: "示例订单2".into(),
                product_name: "商品2".into(),
                create_at: chrono::Local::now().timestamp(),
                extra: Default::default(),
            };

            return Ok(volo_grpc::Response::new(order));
        } else {
            return Err(::volo_grpc::Status::invalid_argument(
                "id 和 user_id 不能同时为空",
            ));
        }
    }
}
