//! Lock-free data structures for high-concurrency operations
//!
//! This module provides lock-free implementations of common data structures
//! to minimize contention and improve scalability.

use crossbeam::queue::{ArrayQueue, SegQueue};
use crossbeam::utils::CachePadded;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

/// Lock-free counter with cache padding to prevent false sharing
#[repr(align(64))]
pub struct LockFreeCounter {
    value: CachePadded<AtomicU64>,
}

impl LockFreeCounter {
    /// Creates a new counter with the specified initial value.
    pub fn new(initial: u64) -> Self {
        Self {
            value: CachePadded::new(AtomicU64::new(initial)),
        }
    }

    /// Atomically increments the counter by 1, returning the previous value.
    #[inline]
    pub fn increment(&self) -> u64 {
        self.value.fetch_add(1, Ordering::Relaxed)
    }

    /// Returns the current value of the counter.
    #[inline]
    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }

    /// Atomically adds delta to the counter, returning the previous value.
    #[inline]
    pub fn add(&self, delta: u64) -> u64 {
        self.value.fetch_add(delta, Ordering::Relaxed)
    }
}

/// Lock-free statistics collector
pub struct LockFreeStats {
    /// Count of query operations
    queries: CachePadded<AtomicU64>,
    /// Count of insert operations
    inserts: CachePadded<AtomicU64>,
    /// Count of delete operations
    deletes: CachePadded<AtomicU64>,
    /// Cumulative latency in nanoseconds
    total_latency_ns: CachePadded<AtomicU64>,
}

impl LockFreeStats {
    /// Creates a new statistics collector with all counters initialized to zero.
    pub fn new() -> Self {
        Self {
            queries: CachePadded::new(AtomicU64::new(0)),
            inserts: CachePadded::new(AtomicU64::new(0)),
            deletes: CachePadded::new(AtomicU64::new(0)),
            total_latency_ns: CachePadded::new(AtomicU64::new(0)),
        }
    }

    /// Records a query operation with the given latency in nanoseconds.
    #[inline]
    pub fn record_query(&self, latency_ns: u64) {
        self.queries.fetch_add(1, Ordering::Relaxed);
        self.total_latency_ns
            .fetch_add(latency_ns, Ordering::Relaxed);
    }

    /// Records an insert operation.
    #[inline]
    pub fn record_insert(&self) {
        self.inserts.fetch_add(1, Ordering::Relaxed);
    }

    /// Records a delete operation.
    #[inline]
    pub fn record_delete(&self) {
        self.deletes.fetch_add(1, Ordering::Relaxed);
    }

    /// Returns a snapshot of the current statistics.
    pub fn snapshot(&self) -> StatsSnapshot {
        let queries = self.queries.load(Ordering::Relaxed);
        let total_latency = self.total_latency_ns.load(Ordering::Relaxed);

        StatsSnapshot {
            queries,
            inserts: self.inserts.load(Ordering::Relaxed),
            deletes: self.deletes.load(Ordering::Relaxed),
            avg_latency_ns: if queries > 0 {
                total_latency / queries
            } else {
                0
            },
        }
    }
}

impl Default for LockFreeStats {
    fn default() -> Self {
        Self::new()
    }
}

/// A point-in-time snapshot of statistics.
#[derive(Debug, Clone)]
pub struct StatsSnapshot {
    /// Total number of query operations recorded.
    pub queries: u64,
    /// Total number of insert operations recorded.
    pub inserts: u64,
    /// Total number of delete operations recorded.
    pub deletes: u64,
    /// Average latency per query in nanoseconds.
    pub avg_latency_ns: u64,
}

/// Lock-free object pool for reducing allocations
pub struct ObjectPool<T> {
    /// Queue holding available pooled objects
    queue: Arc<SegQueue<T>>,
    /// Factory function to create new objects
    factory: Arc<dyn Fn() -> T + Send + Sync>,
    /// Maximum number of objects in the pool
    capacity: usize,
    /// Current count of allocated objects
    allocated: AtomicUsize,
}

impl<T> ObjectPool<T> {
    /// Creates a new object pool with the given capacity and factory function.
    pub fn new<F>(capacity: usize, factory: F) -> Self
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        Self {
            queue: Arc::new(SegQueue::new()),
            factory: Arc::new(factory),
            capacity,
            allocated: AtomicUsize::new(0),
        }
    }

    /// Get an object from the pool or create a new one
    pub fn acquire(&self) -> PooledObject<T> {
        let object = self.queue.pop().unwrap_or_else(|| {
            let current = self.allocated.fetch_add(1, Ordering::Relaxed);
            if current < self.capacity {
                (self.factory)()
            } else {
                self.allocated.fetch_sub(1, Ordering::Relaxed);
                // Wait for an object to be returned
                loop {
                    if let Some(obj) = self.queue.pop() {
                        break obj;
                    }
                    std::hint::spin_loop();
                }
            }
        });

        PooledObject {
            object: Some(object),
            pool: Arc::clone(&self.queue),
        }
    }
}

/// RAII wrapper for pooled objects
pub struct PooledObject<T> {
    /// The wrapped object, returned to pool on drop
    object: Option<T>,
    /// Reference to the parent pool for returning the object
    pool: Arc<SegQueue<T>>,
}

impl<T> PooledObject<T> {
    /// Returns a reference to the pooled object.
    pub fn get(&self) -> &T {
        self.object.as_ref().unwrap()
    }

    /// Returns a mutable reference to the pooled object.
    pub fn get_mut(&mut self) -> &mut T {
        self.object.as_mut().unwrap()
    }
}

impl<T> Drop for PooledObject<T> {
    fn drop(&mut self) {
        if let Some(object) = self.object.take() {
            self.pool.push(object);
        }
    }
}

impl<T> std::ops::Deref for PooledObject<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.object.as_ref().unwrap()
    }
}

impl<T> std::ops::DerefMut for PooledObject<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.object.as_mut().unwrap()
    }
}

/// Lock-free ring buffer for work distribution
pub struct LockFreeWorkQueue<T> {
    /// Bounded lock-free queue backing this work queue
    queue: ArrayQueue<T>,
}

impl<T> LockFreeWorkQueue<T> {
    /// Creates a new work queue with the specified capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            queue: ArrayQueue::new(capacity),
        }
    }

    /// Attempts to push an item to the queue, returning the item on failure.
    #[inline]
    pub fn try_push(&self, item: T) -> Result<(), T> {
        self.queue.push(item)
    }

    /// Attempts to pop an item from the queue, returning None if empty.
    #[inline]
    pub fn try_pop(&self) -> Option<T> {
        self.queue.pop()
    }

    /// Returns the current number of items in the queue.
    #[inline]
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Returns true if the queue contains no items.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_lockfree_counter() {
        let counter = Arc::new(LockFreeCounter::new(0));
        let mut handles = vec![];

        for _ in 0..10 {
            let counter_clone = Arc::clone(&counter);
            handles.push(thread::spawn(move || {
                for _ in 0..1000 {
                    counter_clone.increment();
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(counter.get(), 10000);
    }

    #[test]
    fn test_object_pool() {
        let pool = ObjectPool::new(4, || Vec::<u8>::with_capacity(1024));

        let mut obj1 = pool.acquire();
        obj1.push(1);
        assert_eq!(obj1.len(), 1);

        drop(obj1);

        let obj2 = pool.acquire();
        // Object should be reused (but cleared state is not guaranteed)
        assert!(obj2.capacity() >= 1024);
    }

    #[test]
    fn test_stats_collector() {
        let stats = LockFreeStats::new();

        stats.record_query(1000);
        stats.record_query(2000);
        stats.record_insert();

        let snapshot = stats.snapshot();
        assert_eq!(snapshot.queries, 2);
        assert_eq!(snapshot.inserts, 1);
        assert_eq!(snapshot.avg_latency_ns, 1500);
    }
}
