//! Integration tests for Raft consensus — basic state and configuration tests.
//! Full multi-node tests require a transport layer mock which is TODO.

use ruvector_raft::{RaftNode, RaftNodeConfig, RaftState};

#[test]
fn test_node_creation() {
    let config = RaftNodeConfig::new(
        "node-1".to_string(),
        vec!["node-1".to_string(), "node-2".to_string(), "node-3".to_string()],
    );
    let node = RaftNode::new(config);

    // New node starts as Follower
    assert_eq!(node.current_state(), RaftState::Follower);
    assert_eq!(node.current_term(), 0);
    assert_eq!(node.current_leader(), None);
}

#[test]
fn test_single_node_config() {
    let config = RaftNodeConfig::new("solo".to_string(), vec!["solo".to_string()]);
    assert_eq!(config.node_id, "solo");
    assert_eq!(config.cluster_members.len(), 1);
    assert_eq!(config.election_timeout_min, 150);
    assert_eq!(config.election_timeout_max, 300);
    assert_eq!(config.heartbeat_interval, 50);
}

#[test]
fn test_config_defaults() {
    let config = RaftNodeConfig::new("a".to_string(), vec!["a".to_string(), "b".to_string()]);
    assert!(config.election_timeout_min < config.election_timeout_max);
    assert!(config.heartbeat_interval < config.election_timeout_min);
    assert!(config.max_entries_per_message > 0);
    assert!(config.snapshot_chunk_size > 0);
}
