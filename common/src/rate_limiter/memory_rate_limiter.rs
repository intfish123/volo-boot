//! 基于内存的限流组件-令牌桶算法实现
//!
//! 注意! 此实现中最终用的计算数据不是浮点数，因此不能保证精确的限流控制，只能粗略地控制限流速率，具体可查阅下面的代码。

use crate::rate_limiter::RateLimiter;
use crossbeam::utils::CachePadded;
use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// 令牌桶位结构体
#[derive(Default)]
struct TokenBucket {
    tokens: AtomicU64,
    last_refill: AtomicU64,
}

/// 分片令牌桶位结构体
#[derive(Default)]
pub struct SharedTokenBucket {
    shards: Vec<CachePadded<TokenBucket>>,
    fill_rate: u64,
    capacity: u64,
}

impl SharedTokenBucket {
    /// 创建新的分片token bucket
    /// # param:
    /// * capacity: 总体容量
    /// * fill_rate: 总体填充速率, 单位为秒
    /// * shard_num: 分片大小，默认使用 cpu核数*2
    pub fn new(capacity: u64, fill_rate: u64, shard_num: Option<u32>) -> Self {
        let mut shards_len = num_cpus::get() as u64 * 2;
        if let Some(n) = shard_num {
            shards_len = n as u64;
        }
        if shards_len == 0 {
            tracing::warn!("shard_num cannot be 0, force set to 1");
            shards_len = 1;
        }
        let mut new_fill_rate = fill_rate;
        if new_fill_rate == 0 {
            tracing::warn!("fill_rate cannot be 0, force set to 1");
            new_fill_rate = 1;
        }
        let mut new_capacity = capacity;
        if new_capacity < shards_len {
            tracing::warn!(
                "capacity cannot less than shard_num, force set to {}",
                shards_len
            );
            new_capacity = shards_len;
        }

        let mut per_shard_cap = (new_capacity as f64 / shards_len as f64).round() as u64;
        if per_shard_cap == 0 {
            per_shard_cap = 1
        }

        let now_millis = Self::now_millis();

        let shards = (0..shards_len)
            .map(|_| {
                CachePadded::new(TokenBucket {
                    tokens: AtomicU64::new(per_shard_cap),
                    last_refill: AtomicU64::new(now_millis),
                })
            })
            .collect();
        Self {
            shards,
            fill_rate: new_fill_rate,
            capacity: new_capacity,
        }
    }

    /// 计算每分片的容量，最小为1
    fn per_shard_capacity(&self) -> u64 {
        let cap = (self.capacity as f64 / self.shards.len() as f64).round() as u64;
        if cap <= 0 {
            return 1;
        }
        cap
    }

    /// 获取当前毫秒级时间戳
    #[inline]
    fn now_millis() -> u64 {
        chrono::Utc::now().timestamp_millis() as u64
    }

    /// 填充指定分片中的桶位
    fn refill_shard(&self, shard: &TokenBucket) {
        let current_millis = Self::now_millis();
        let last_refill_time = shard.last_refill.load(Ordering::Acquire);
        let diff_seconds = (current_millis as f64 - last_refill_time as f64) / 1000.0;

        if diff_seconds > 0.0 {
            let need_add_tokens =
                (diff_seconds * self.fill_rate as f64 / self.shards.len() as f64).round() as u64;

            // 获取当前分片的最大容量
            let shard_cap = self.per_shard_capacity();
            // println!("shard_cap: {}, need_add_tokens: {}, diff_seconds: {}", shard_cap, need_add_tokens, diff_seconds);
            if need_add_tokens > 0 {
                if let Ok(_) = shard.last_refill.compare_exchange(
                    last_refill_time,
                    current_millis,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                ) {
                    shard
                        .tokens
                        .fetch_update(Ordering::AcqRel, Ordering::Relaxed, |v| {
                            // Some((v + need_add_tokens).min(shard_cap))
                            Some(v + need_add_tokens)
                        })
                        .ok();
                }
            }
        }
    }

    /// 尝试获取指定数量的令牌
    pub fn try_acquire(&self, seed: String, tokens: u64) -> bool {
        let shards = &self.shards;
        
        let shard;
        if shards.len() == 1 {
            shard = &shards[0];
        } else {
            let hash = mur3::murmurhash3_x86_32(seed.as_bytes(), 0);
            let idx = hash as usize % shards.len();
            shard = &shards[idx];
        }

        // 补充令牌
        self.refill_shard(shard);

        // 扣除令牌
        let mut current_tokens = shard.tokens.load(Ordering::Acquire);
        loop {
            if current_tokens < tokens {
                return false;
            }
            match shard.tokens.compare_exchange_weak(
                current_tokens,
                current_tokens - tokens,
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(actual) => current_tokens = actual,
            }
        }
    }
}

#[cfg(test)]
mod memory_rate_limiter_test {
    use crate::rate_limiter::memory_rate_limiter::SharedTokenBucket;
    use std::sync::Arc;
    use std::time::Instant;
    use std::{thread, time};

    #[test]
    fn test_new() {
        // test bad param
        let rate_limiter = SharedTokenBucket::new(0, 0, Some(0));
        assert_eq!(rate_limiter.capacity, 1);
        assert_eq!(rate_limiter.fill_rate, 1);
        assert_ne!(rate_limiter.shards.len(), 0);

        // test bad param
        let rate_limiter = SharedTokenBucket::new(1, 1, Some(16));
        assert_eq!(rate_limiter.capacity, 16);
        assert_eq!(rate_limiter.fill_rate, 1);
        assert_eq!(rate_limiter.shards.len(), 16);

        // test adepter cpu core
        let rate_limiter = SharedTokenBucket::new(1, 1, None);
        let cpu = num_cpus::get() as u64;
        assert_eq!(rate_limiter.capacity, cpu * 2);
        assert_eq!(rate_limiter.fill_rate, 1);
        assert_eq!(rate_limiter.shards.len() as u64, cpu * 2);
    }
    #[test]
    fn test_try_acquire() {
        let rate_limiter = SharedTokenBucket::new(30, 1, Some(1));
        let mut can_acquire = rate_limiter.try_acquire("k1".to_string(), 1);
        assert_eq!(can_acquire, true);
        can_acquire = rate_limiter.try_acquire("k2".to_string(), 1);
        assert_eq!(can_acquire, true);

        can_acquire = rate_limiter.try_acquire("k3".to_string(), 28);
        assert_eq!(can_acquire, true);

        can_acquire = rate_limiter.try_acquire("k3".to_string(), 1);
        assert_eq!(can_acquire, false);

        thread::sleep(time::Duration::from_secs(1));
        can_acquire = rate_limiter.try_acquire("k3".to_string(), 1);
        assert_eq!(can_acquire, true);

        let rate_limiter = SharedTokenBucket::new(1, 4, Some(16));
        thread::sleep(time::Duration::from_secs(1));
        can_acquire = rate_limiter.try_acquire("k1".to_string(), 1);
        assert_eq!(can_acquire, true);

        thread::sleep(time::Duration::from_secs(3));
        can_acquire = rate_limiter.try_acquire("k1".to_string(), 1);
        assert_eq!(can_acquire, true);
    }

    #[test]
    fn test_async_try_acquire() {
        // 耗时统计可能误差, 不过正常
        let mut dur = 0.0;
        let mut r = false;
        // 第1轮耗时计算 预计耗时 2*10/3 = 6.66s左右
        println!("第1轮");
        dur = async_try_acquire(3, 5, 2, 10);
        r = if (2.0 * 10.0 / 3.0 - dur).abs() < 10.0 {
            true
        } else {
            false
        };
        println!("相差: {}", (2.0 * 10.0 / 3.0 - dur).abs());
        assert_eq!(r, true);

        // 第2轮 预计耗时 3*10/10 = 3s左右
        println!("第2轮");
        dur = async_try_acquire(10, 5, 3, 10);
        r = if (3.0 * 10.0 / 10.0 - dur).abs() < 10.0 {
            true
        } else {
            false
        };
        println!("相差: {}\n", (3.0 * 10.0 / 10.0 - dur).abs());
        assert_eq!(r, true);

        // 第3轮 预计耗时 6*50/20 = 15s左右
        println!("第3轮");
        dur = async_try_acquire(20, 5, 6, 50);
        r = if (6.0 * 50.0 / 20.0 - dur).abs() < 10.0 {
            true
        } else {
            false
        };
        println!("相差: {}\n", (6.0 * 50.0 / 20.0 - dur).abs());
        assert_eq!(r, true);

        // 第4轮 预计耗时 8*50/100 = 4s左右
        println!("第4轮");
        dur = async_try_acquire(100, 16, 8, 50);
        r = if (8.0 * 50.0 / 100.0 - dur).abs() < 10.0 {
            true
        } else {
            false
        };
        println!("相差: {}\n", (8.0 * 50.0 / 100.0 - dur).abs());
        assert_eq!(r, true);

        // 第5轮 预计耗时 16*100/200 = 8s左右
        println!("第5轮");
        dur = async_try_acquire(200, 16, 16, 100);
        r = if (16.0 * 100.0 / 200.0 - dur).abs() < 10.0 {
            true
        } else {
            false
        };
        println!("相差: {}\n", (16.0 * 100.0 / 200.0 - dur).abs());
        assert_eq!(r, true);

        // 第6轮 预计耗时 22*300/200 = 33s左右
        println!("第6轮");
        dur = async_try_acquire(200, 16, 22, 300);
        r = if (22.0 * 300.0 / 200.0 - dur).abs() < 10.0 {
            true
        } else {
            false
        };
        println!("相差: {}\n", (22.0 * 300.0 / 200.0 - dur).abs());
        assert_eq!(r, true);
    }
    fn async_try_acquire(
        fill_rate: u64,
        shard_num: u32,
        thread_num: u32,
        per_thread_cnt: i32,
    ) -> f64 {
        let rate_limiter = Arc::new(SharedTokenBucket::new(1, fill_rate, Some(shard_num)));
        let mut join_handles = Vec::with_capacity(thread_num as usize);

        for i in 0..thread_num {
            let ii = i.clone();
            let rc = rate_limiter.clone();
            join_handles.push(thread::spawn(move || {
                let thread_id = thread::current().id();
                let mut cnt = 0;
                while cnt < per_thread_cnt {
                    if rc.try_acquire(
                        (ii + rand::random::<u32>() % 100000)
                            .to_string()
                            .clone()
                            .to_string(),
                        1,
                    ) {
                        cnt += 1;
                        if cnt % 2 == 0 {
                            // println!("thread_id: {:?}, cnt: {}", thread_id, cnt)
                        }
                    } else {
                        thread::sleep(time::Duration::from_millis(100));
                    }
                }
                // println!("thread_id: {:?} finished", thread_id)
            }));
        }
        let now = Instant::now();
        for x in join_handles {
            x.join().unwrap();
        }
        let dur = now.elapsed().as_millis();
        println!("async_try_acquire duration time: {} s", dur as f64 / 1000.0);
        dur as f64 / 1000.0
    }
}

#[derive(Default)]
pub struct MemoryRateLimiter {
    key_bucket: DashMap<String, Arc<SharedTokenBucket>>,
    init_capacity: u64,
    fill_rate: u64,
    shard_num: Option<u32>,
}
impl MemoryRateLimiter {
    pub fn new(init_capacity: u64, fill_rate: u64, shard_num: Option<u32>) -> Self {
        MemoryRateLimiter {
            key_bucket: DashMap::new(),
            init_capacity,
            fill_rate,
            shard_num: Some(shard_num.unwrap_or(1)),
        }
    }
}

impl RateLimiter for MemoryRateLimiter {
    fn try_acquire(&self, key: String, tokens: u64) -> bool {
        if let Some(v) = self.key_bucket.get(&key) {
            v.try_acquire(key, tokens)
        } else {
            let shard_token_bucket =
                SharedTokenBucket::new(self.init_capacity, self.fill_rate, self.shard_num);
            self.key_bucket.insert(key, Arc::new(shard_token_bucket));
            true
        }
    }
    fn get_config(&self) -> (u64, u64, Option<u32>) {
        (self.init_capacity, self.fill_rate, self.shard_num)
    }
}
