pub mod memory_rate_limiter;


pub trait RateLimiter {
    fn try_acquire(&self, key: String, tokens: u64) -> bool;
    fn get_config(&self) -> (u64, u64, Option<u32>);
}
