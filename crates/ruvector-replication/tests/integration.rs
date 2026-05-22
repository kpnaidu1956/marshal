//! Integration tests for replication: vector clocks, log sync, streams.

use ruvector_replication::conflict::{VectorClock, ClockOrdering};
use ruvector_replication::sync::ReplicationLog;
use ruvector_replication::stream::StreamManager;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Vector Clock Tests
// ---------------------------------------------------------------------------

#[test]
fn test_vector_clock_happens_before_strict() {
    let mut a = VectorClock::new();
    let mut b = VectorClock::new();

    a.increment("node1");
    b.increment("node1");

    // Equal clocks should NOT satisfy happens-before (strict partial order)
    assert!(!a.happens_before(&b), "Equal clocks must not satisfy happens-before");
    assert!(!b.happens_before(&a));
}

#[test]
fn test_vector_clock_causal_order() {
    let mut a = VectorClock::new();
    let mut b = VectorClock::new();

    a.increment("node1"); // a = {node1: 1}
    b.increment("node1"); // b = {node1: 1}
    b.increment("node1"); // b = {node1: 2}

    assert!(a.happens_before(&b));
    assert!(!b.happens_before(&a));
}

#[test]
fn test_vector_clock_concurrent() {
    let mut a = VectorClock::new();
    let mut b = VectorClock::new();

    a.increment("node1");
    b.increment("node2");

    assert!(!a.happens_before(&b));
    assert!(!b.happens_before(&a));
    assert_eq!(a.compare(&b), ClockOrdering::Concurrent);
}

#[test]
fn test_vector_clock_compare_equal() {
    let mut a = VectorClock::new();
    let mut b = VectorClock::new();

    a.increment("node1");
    b.increment("node1");

    assert_eq!(a.compare(&b), ClockOrdering::Equal);
}

#[test]
fn test_vector_clock_merge() {
    let mut a = VectorClock::new();
    let mut b = VectorClock::new();

    a.increment("node1");
    a.increment("node1"); // a = {node1: 2}
    b.increment("node2"); // b = {node2: 1}

    a.merge(&b); // a = {node1: 2, node2: 1}
    assert_eq!(a.get("node1"), 2);
    assert_eq!(a.get("node2"), 1);
}

// ---------------------------------------------------------------------------
// Replication Log Tests
// ---------------------------------------------------------------------------

#[test]
fn test_log_append_and_get() {
    let log = ReplicationLog::new("test-replica");
    let entry = log.append(b"hello".to_vec());

    assert_eq!(entry.sequence, 1);
    assert_eq!(entry.data, b"hello");
    assert!(entry.verify(), "Checksum verification failed");

    let retrieved = log.get(1);
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().data, b"hello");
}

#[test]
fn test_log_sequence_increases() {
    let log = ReplicationLog::new("test-replica");
    let e1 = log.append(b"first".to_vec());
    let e2 = log.append(b"second".to_vec());
    let e3 = log.append(b"third".to_vec());

    assert_eq!(e1.sequence, 1);
    assert_eq!(e2.sequence, 2);
    assert_eq!(e3.sequence, 3);
}

#[test]
fn test_log_entry_checksum_integrity() {
    let log = ReplicationLog::new("test-replica");
    let entry = log.append(b"important data".to_vec());

    assert!(entry.verify(), "Fresh entry should verify");

    let mut tampered = entry.clone();
    tampered.data = b"tampered data".to_vec();
    assert!(!tampered.verify(), "Tampered entry should fail verification");
}

#[test]
fn test_log_truncate_before() {
    let log = ReplicationLog::new("test-replica");
    for _ in 0..10 {
        log.append(b"data".to_vec());
    }

    log.truncate_before(5);
    assert!(log.get(1).is_none(), "Entries before cutoff should be removed");
    assert!(log.get(5).is_some(), "Entries at cutoff should remain");
    assert!(log.get(10).is_some(), "Entries after cutoff should remain");
}

// ---------------------------------------------------------------------------
// Stream Manager Tests
// ---------------------------------------------------------------------------

#[test]
fn test_stream_create_and_remove() {
    let log = Arc::new(ReplicationLog::new("test"));
    let manager = StreamManager::new(log);

    let _stream1 = manager.create_stream("consumer-1");
    let _stream2 = manager.create_stream("consumer-2");
    assert_eq!(manager.stream_count(), 2);

    manager.remove_stream("consumer-1");
    assert_eq!(manager.stream_count(), 1);
}

#[test]
fn test_stream_cleanup_unused() {
    let log = Arc::new(ReplicationLog::new("test"));
    let manager = StreamManager::new(log);

    {
        let _stream = manager.create_stream("temp-consumer");
        assert_eq!(manager.stream_count(), 1);
    }
    // _stream dropped, only manager holds Arc

    manager.cleanup_unused();
    assert_eq!(manager.stream_count(), 0, "Unused streams should be cleaned up");
}
