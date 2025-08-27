pub mod app_config;

pub use user_volo_gen::user;

pub struct S;

impl user::UserService for S {
    async fn get_user(
        &self,
        _req: volo_grpc::Request<user::GetUserRequest>,
    ) -> Result<volo_grpc::Response<user::User>, volo_grpc::Status> {
        let req_data = _req.into_inner();
        tracing::info!("获取用户: {:?}", req_data);
        if let None = req_data.id {
            return Err(volo_grpc::Status::not_found("User not found"));
        }
        let user = user::User {
            id: req_data.id.unwrap_or_default(),
            username: req_data.username.unwrap_or_default(),
            nickname: Some("intfish123".into()),
            phone: Some("12345678".into()),
            extra: Default::default(),
        };
        Ok(volo_grpc::Response::new(user))
    }
}
