//! Participation network analysis and centrality metrics
//!
//! Builds communication graphs from interactions and computes:
//! - Degree centrality (number of connections)
//! - Betweenness centrality (bridge nodes between others)
//! - Closeness centrality (average distance to all nodes)
//! - Connector identification (cross-team bridges)
//! - Bottleneck detection (many dependencies)

use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet, VecDeque};
use uuid::Uuid;

use super::aggregation_types::*;
use super::storage::AnalyticsDb;
use super::types::InteractionClassification;
use crate::error::Result;

/// Network analyzer for participation graphs
pub struct NetworkAnalyzer<'a> {
    db: &'a AnalyticsDb,
}

impl<'a> NetworkAnalyzer<'a> {
    pub fn new(db: &'a AnalyticsDb) -> Self {
        Self { db }
    }

    /// Build participation network from classifications
    pub fn build_network(
        &self,
        organization_id: &str,
        classifications: &[InteractionClassification],
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
    ) -> Result<ParticipationNetwork> {
        let mut edges: HashMap<(String, String), EdgeData> = HashMap::new();
        let mut nodes: HashSet<String> = HashSet::new();

        for classification in classifications {
            let from_user = &classification.sender_id;
            nodes.insert(from_user.clone());

            // Extract mentioned users from entities
            for to_user in &classification.entities.mentioned_users {
                if to_user != from_user {
                    nodes.insert(to_user.clone());

                    let key = (from_user.clone(), to_user.clone());
                    let entry = edges.entry(key).or_insert_with(|| EdgeData {
                        interaction_count: 0,
                        sentiments: Vec::new(),
                        type_breakdown: HashMap::new(),
                    });

                    entry.interaction_count += 1;
                    entry.sentiments.push(classification.sentiment);
                    *entry
                        .type_breakdown
                        .entry(classification.interaction_type.as_str().to_string())
                        .or_insert(0) += 1;
                }
            }
        }

        // Find max interaction count for normalization
        let max_count = edges.values().map(|e| e.interaction_count).max().unwrap_or(1);

        // Build participation edges
        let mut participation_edges = Vec::new();
        for ((from, to), data) in edges {
            let avg_sentiment = if data.sentiments.is_empty() {
                0.0
            } else {
                data.sentiments.iter().sum::<f32>() / data.sentiments.len() as f32
            };

            let weight = data.interaction_count as f32 / max_count as f32;

            // Determine team_id (if both users in same team)
            let from_team = self.db.get_user_team(organization_id, &from)?;
            let to_team = self.db.get_user_team(organization_id, &to)?;
            let team_id = match (&from_team, &to_team) {
                (Some(ft), Some(tt)) if ft.team_id == tt.team_id => Some(ft.team_id.clone()),
                _ => None, // Cross-team or unknown
            };

            participation_edges.push(ParticipationEdge {
                id: Uuid::new_v4(),
                organization_id: organization_id.to_string(),
                team_id,
                from_user_id: from,
                to_user_id: to,
                interaction_count: data.interaction_count,
                avg_sentiment,
                type_breakdown: data.type_breakdown,
                period_start,
                period_end,
                weight,
                computed_at: Utc::now(),
            });
        }

        Ok(ParticipationNetwork {
            organization_id: organization_id.to_string(),
            nodes: nodes.into_iter().collect(),
            edges: participation_edges,
            period_start,
            period_end,
        })
    }

    /// Compute participation metrics for all users in the network
    pub fn compute_metrics(
        &self,
        organization_id: &str,
        network: &ParticipationNetwork,
        period_type: PeriodType,
    ) -> Result<Vec<ParticipationMetrics>> {
        let mut metrics = Vec::new();

        // Build adjacency structures for centrality calculations
        let mut outgoing: HashMap<&str, Vec<&str>> = HashMap::new();
        let mut incoming: HashMap<&str, Vec<&str>> = HashMap::new();
        let mut interaction_counts: HashMap<&str, (u32, u32)> = HashMap::new();

        for node in &network.nodes {
            outgoing.insert(node, Vec::new());
            incoming.insert(node, Vec::new());
            interaction_counts.insert(node, (0, 0));
        }

        for edge in &network.edges {
            outgoing.entry(&edge.from_user_id).or_default().push(&edge.to_user_id);
            incoming.entry(&edge.to_user_id).or_default().push(&edge.from_user_id);

            let from_entry = interaction_counts.entry(&edge.from_user_id).or_insert((0, 0));
            from_entry.0 += edge.interaction_count;

            let to_entry = interaction_counts.entry(&edge.to_user_id).or_insert((0, 0));
            to_entry.1 += edge.interaction_count;
        }

        let n = network.nodes.len() as f32;
        if n <= 1.0 {
            return Ok(metrics);
        }

        // Compute all-pairs shortest paths (Floyd-Warshall simplified)
        let distances = self.compute_shortest_paths(&network.nodes, &network.edges);

        for user_id in &network.nodes {
            let out_neighbors = outgoing.get(user_id.as_str()).map(|v| v.len()).unwrap_or(0);
            let in_neighbors = incoming.get(user_id.as_str()).map(|v| v.len()).unwrap_or(0);
            let unique_collaborators = {
                let mut collabs: HashSet<&str> = HashSet::new();
                if let Some(out) = outgoing.get(user_id.as_str()) {
                    collabs.extend(out.iter().copied());
                }
                if let Some(inc) = incoming.get(user_id.as_str()) {
                    collabs.extend(inc.iter().copied());
                }
                collabs.len() as u32
            };

            // Degree centrality: normalized unique connections
            let degree_centrality = unique_collaborators as f32 / (n - 1.0);

            // Closeness centrality: 1 / avg distance to all other nodes
            let closeness_centrality = self.compute_closeness(user_id, &distances, n);

            // Betweenness centrality: fraction of shortest paths through this node
            let betweenness_centrality =
                self.compute_betweenness(user_id, &network.nodes, &distances);

            // Determine if user is a connector (bridges multiple teams)
            let is_connector = self.is_cross_team_connector(organization_id, user_id, &network.edges)?;

            // Determine if user is a bottleneck (many incoming dependencies)
            let is_bottleneck = in_neighbors > out_neighbors * 2 && in_neighbors > 5;

            // Get user's team
            let team_membership = self.db.get_user_team(organization_id, user_id)?;
            let team_id = team_membership.map(|t| t.team_id);

            let counts = interaction_counts.get(user_id.as_str()).copied().unwrap_or((0, 0));

            metrics.push(ParticipationMetrics {
                id: Uuid::new_v4(),
                organization_id: organization_id.to_string(),
                team_id,
                user_id: user_id.clone(),
                period_start: network.period_start,
                period_end: network.period_end,
                period_type,
                degree_centrality,
                betweenness_centrality,
                closeness_centrality,
                total_interactions_sent: counts.0,
                total_interactions_received: counts.1,
                unique_collaborators,
                is_connector,
                is_bottleneck,
                computed_at: Utc::now(),
            });
        }

        Ok(metrics)
    }

    /// Persist network edges and metrics
    pub fn persist_network(
        &self,
        network: &ParticipationNetwork,
        metrics: &[ParticipationMetrics],
    ) -> Result<()> {
        for edge in &network.edges {
            self.db.upsert_participation_edge(edge)?;
        }

        for metric in metrics {
            self.db.upsert_participation_metrics(metric)?;
        }

        Ok(())
    }

    /// Get top connectors (cross-team bridge users)
    pub fn get_top_connectors(
        &self,
        organization_id: &str,
        network: &ParticipationNetwork,
        limit: usize,
    ) -> Result<Vec<ConnectorInfo>> {
        let mut connectors: Vec<ConnectorInfo> = Vec::new();

        for user_id in &network.nodes {
            if self.is_cross_team_connector(organization_id, user_id, &network.edges)? {
                let teams_connected = self.count_teams_connected(organization_id, user_id, &network.edges)?;
                let cross_team_interactions = self.count_cross_team_interactions(organization_id, user_id, &network.edges)?;

                connectors.push(ConnectorInfo {
                    user_id: user_id.clone(),
                    teams_connected,
                    cross_team_interactions,
                });
            }
        }

        // Sort by teams_connected descending, then by cross_team_interactions
        connectors.sort_by(|a, b| {
            b.teams_connected
                .cmp(&a.teams_connected)
                .then(b.cross_team_interactions.cmp(&a.cross_team_interactions))
        });

        connectors.truncate(limit);
        Ok(connectors)
    }

    /// Maximum number of nodes to analyze (prevents O(n²) memory issues)
    const MAX_NETWORK_NODES: usize = 1000;

    /// Helper: compute shortest paths using BFS (simplified, unweighted)
    /// Uses VecDeque for O(1) queue operations instead of Vec's O(n) insert(0, x)
    fn compute_shortest_paths(
        &self,
        nodes: &[String],
        edges: &[ParticipationEdge],
    ) -> HashMap<(String, String), u32> {
        // Guard against excessive memory usage for large networks
        if nodes.len() > Self::MAX_NETWORK_NODES {
            tracing::warn!(
                "Network has {} nodes, exceeding limit of {}. Analysis truncated.",
                nodes.len(),
                Self::MAX_NETWORK_NODES
            );
            // Only analyze first MAX_NETWORK_NODES nodes
            let truncated_nodes: Vec<_> = nodes.iter().take(Self::MAX_NETWORK_NODES).collect();
            return self.compute_shortest_paths_internal(&truncated_nodes, edges);
        }

        let node_refs: Vec<_> = nodes.iter().collect();
        self.compute_shortest_paths_internal(&node_refs, edges)
    }

    /// Internal BFS implementation with proper VecDeque usage
    fn compute_shortest_paths_internal(
        &self,
        nodes: &[&String],
        edges: &[ParticipationEdge],
    ) -> HashMap<(String, String), u32> {
        let mut distances: HashMap<(String, String), u32> = HashMap::new();

        // Build adjacency list (undirected graph for shortest paths)
        let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
        for node in nodes {
            adj.insert(node.as_str(), Vec::new());
        }
        for edge in edges {
            // Check if both nodes are in our analysis set
            if adj.contains_key(edge.from_user_id.as_str()) && adj.contains_key(edge.to_user_id.as_str()) {
                adj.entry(&edge.from_user_id).or_default().push(&edge.to_user_id);
                adj.entry(&edge.to_user_id).or_default().push(&edge.from_user_id);
            }
        }

        // BFS from each node using VecDeque for O(1) push_back/pop_front
        for start in nodes {
            let mut visited: HashMap<&str, u32> = HashMap::new();
            let mut queue: VecDeque<&str> = VecDeque::new();

            queue.push_back(start.as_str());
            visited.insert(start.as_str(), 0);

            while let Some(current) = queue.pop_front() {
                let current_dist = visited[current];
                if let Some(neighbors) = adj.get(current) {
                    for neighbor in neighbors {
                        if !visited.contains_key(neighbor) {
                            visited.insert(neighbor, current_dist + 1);
                            queue.push_back(neighbor);
                        }
                    }
                }
            }

            for (dest, dist) in visited {
                distances.insert(((*start).clone(), dest.to_string()), dist);
            }
        }

        distances
    }

    /// Helper: compute closeness centrality
    /// Closeness = (number of reachable nodes) / (sum of distances to reachable nodes)
    /// This measures how "close" a node is to all other reachable nodes.
    /// Higher values indicate more central positions in the network.
    fn compute_closeness(
        &self,
        user_id: &str,
        distances: &HashMap<(String, String), u32>,
        _n: f32,
    ) -> f32 {
        let mut total_dist: u32 = 0;
        let mut reachable: u32 = 0;

        for ((from, _to), dist) in distances {
            if from == user_id && *dist > 0 {
                total_dist += dist;
                reachable += 1;
            }
        }

        if reachable == 0 || total_dist == 0 {
            return 0.0;
        }

        // Closeness centrality: reachable nodes / sum of distances
        // This is the harmonic mean interpretation, suitable for disconnected graphs
        (reachable as f32) / (total_dist as f32)
    }

    // Helper: compute betweenness centrality (simplified)
    fn compute_betweenness(
        &self,
        user_id: &str,
        nodes: &[String],
        distances: &HashMap<(String, String), u32>,
    ) -> f32 {
        let mut betweenness: f32 = 0.0;
        let n = nodes.len();

        if n <= 2 {
            return 0.0;
        }

        // For each pair (s, t) where s != t != user_id
        // Check if user_id is on the shortest path
        for s in nodes {
            if s == user_id {
                continue;
            }
            for t in nodes {
                if t == user_id || t == s {
                    continue;
                }

                let d_st = distances.get(&(s.clone(), t.clone())).copied();
                let d_sv = distances.get(&(s.clone(), user_id.to_string())).copied();
                let d_vt = distances.get(&(user_id.to_string(), t.clone())).copied();

                if let (Some(st), Some(sv), Some(vt)) = (d_st, d_sv, d_vt) {
                    if st > 0 && sv + vt == st {
                        // user_id is on the shortest path
                        betweenness += 1.0;
                    }
                }
            }
        }

        // Normalize
        let pairs = ((n - 1) * (n - 2)) as f32;
        if pairs > 0.0 {
            betweenness / pairs
        } else {
            0.0
        }
    }

    // Helper: check if user is a cross-team connector
    fn is_cross_team_connector(
        &self,
        organization_id: &str,
        user_id: &str,
        edges: &[ParticipationEdge],
    ) -> Result<bool> {
        let teams_connected = self.count_teams_connected(organization_id, user_id, edges)?;
        Ok(teams_connected >= 2)
    }

    // Helper: count unique teams a user connects with
    fn count_teams_connected(
        &self,
        organization_id: &str,
        user_id: &str,
        edges: &[ParticipationEdge],
    ) -> Result<u32> {
        let mut teams: HashSet<String> = HashSet::new();

        // Add user's own team
        if let Some(membership) = self.db.get_user_team(organization_id, user_id)? {
            teams.insert(membership.team_id);
        }

        // Add teams of users they interact with
        for edge in edges {
            let other_user = if edge.from_user_id == user_id {
                &edge.to_user_id
            } else if edge.to_user_id == user_id {
                &edge.from_user_id
            } else {
                continue;
            };

            if let Some(membership) = self.db.get_user_team(organization_id, other_user)? {
                teams.insert(membership.team_id);
            }
        }

        Ok(teams.len() as u32)
    }

    // Helper: count cross-team interactions
    fn count_cross_team_interactions(
        &self,
        organization_id: &str,
        user_id: &str,
        edges: &[ParticipationEdge],
    ) -> Result<u32> {
        let user_team = self.db.get_user_team(organization_id, user_id)?;
        let user_team_id = user_team.map(|t| t.team_id);

        let mut count: u32 = 0;

        for edge in edges {
            let (is_involved, other_user) = if edge.from_user_id == user_id {
                (true, &edge.to_user_id)
            } else if edge.to_user_id == user_id {
                (true, &edge.from_user_id)
            } else {
                (false, &edge.from_user_id)
            };

            if !is_involved {
                continue;
            }

            let other_team = self.db.get_user_team(organization_id, other_user)?;
            let other_team_id = other_team.map(|t| t.team_id);

            // Cross-team if teams differ (or one is unknown)
            if user_team_id != other_team_id {
                count += edge.interaction_count;
            }
        }

        Ok(count)
    }
}

/// Intermediate structure for building edges
struct EdgeData {
    interaction_count: u32,
    sentiments: Vec<f32>,
    type_breakdown: HashMap<String, u32>,
}

/// Participation network structure
#[derive(Debug, Clone)]
pub struct ParticipationNetwork {
    pub organization_id: String,
    pub nodes: Vec<String>,
    pub edges: Vec<ParticipationEdge>,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
}

/// Information about a cross-team connector
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConnectorInfo {
    pub user_id: String,
    pub teams_connected: u32,
    pub cross_team_interactions: u32,
}
