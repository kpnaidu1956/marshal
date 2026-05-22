//! Team membership management
//!
//! Handles team membership CRUD operations and syncing from external sources.
//! Teams are defined as a manager + their direct reports.

use chrono::Utc;
use uuid::Uuid;

use super::aggregation_types::*;
use super::storage::AnalyticsDb;
use crate::error::Result;

/// Manages team memberships for analytics
pub struct TeamManager<'a> {
    db: &'a AnalyticsDb,
}

impl<'a> TeamManager<'a> {
    pub fn new(db: &'a AnalyticsDb) -> Self {
        Self { db }
    }

    /// Sync teams from external source (replaces existing team data)
    pub fn sync_teams(&self, request: &TeamSyncRequest) -> Result<SyncResult> {
        let mut members_added = 0;
        let mut teams_synced = 0;

        // Clear existing team memberships for this organization
        self.db.clear_team_memberships(&request.organization_id)?;

        for team_def in &request.teams {
            // Add manager as team member with manager role
            let manager_membership = TeamMembership {
                id: Uuid::new_v4(),
                organization_id: request.organization_id.clone(),
                team_id: team_def.manager_id.clone(),
                team_name: team_def.manager_name.clone(),
                user_id: team_def.manager_id.clone(),
                role: TeamRole::Manager,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };
            self.db.insert_team_membership(&manager_membership)?;
            members_added += 1;

            // Add each direct report as team member
            for member_id in &team_def.member_ids {
                let membership = TeamMembership {
                    id: Uuid::new_v4(),
                    organization_id: request.organization_id.clone(),
                    team_id: team_def.manager_id.clone(),
                    team_name: team_def.manager_name.clone(),
                    user_id: member_id.clone(),
                    role: TeamRole::Member,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                };
                self.db.insert_team_membership(&membership)?;
                members_added += 1;
            }

            teams_synced += 1;
        }

        Ok(SyncResult {
            teams_synced,
            members_added,
        })
    }

    /// List all teams in an organization
    pub fn list_teams(&self, organization_id: &str) -> Result<Vec<TeamInfo>> {
        self.db.list_teams(organization_id)
    }

    /// Get members of a specific team
    pub fn get_team_members(&self, organization_id: &str, team_id: &str) -> Result<Vec<TeamMembership>> {
        self.db.get_team_members(organization_id, team_id)
    }

    /// Get the team a user belongs to
    pub fn get_user_team(&self, organization_id: &str, user_id: &str) -> Result<Option<TeamMembership>> {
        self.db.get_user_team(organization_id, user_id)
    }

    /// Add a single team member
    pub fn add_member(
        &self,
        organization_id: &str,
        team_id: &str,
        team_name: &str,
        user_id: &str,
        role: TeamRole,
    ) -> Result<TeamMembership> {
        let membership = TeamMembership {
            id: Uuid::new_v4(),
            organization_id: organization_id.to_string(),
            team_id: team_id.to_string(),
            team_name: team_name.to_string(),
            user_id: user_id.to_string(),
            role,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        self.db.insert_team_membership(&membership)?;
        Ok(membership)
    }

    /// Remove a team member
    pub fn remove_member(&self, organization_id: &str, team_id: &str, user_id: &str) -> Result<bool> {
        self.db.remove_team_member(organization_id, team_id, user_id)
    }
}

/// Result of team sync operation
#[derive(Debug, Clone)]
pub struct SyncResult {
    pub teams_synced: u32,
    pub members_added: u32,
}

/// Basic team info for listing
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TeamInfo {
    pub team_id: String,
    pub team_name: String,
    pub member_count: u32,
}
