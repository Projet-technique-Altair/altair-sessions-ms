use serde::Deserialize;
use url::Url;
use uuid::Uuid;

use crate::error::AppError;

use super::{
    GroupLabSummary, GroupMemberSummary, GroupSummary, LabApiResponse, LabOverview, SessionsService,
};

impl SessionsService {
    async fn fetch_group(
        &self,
        requester_user_id: Uuid,
        roles_header: &str,
        group_id: Uuid,
    ) -> Result<GroupSummary, AppError> {
        let url = self
            .groups_ms_base
            .join(&format!("groups/{group_id}"))
            .map_err(|e| AppError::Internal(format!("Invalid Groups URL: {e}")))?;

        self.get_with_staff_headers(url, requester_user_id, roles_header)
            .await
    }

    async fn fetch_creator_groups(
        &self,
        requester_user_id: Uuid,
        roles_header: &str,
    ) -> Result<Vec<GroupSummary>, AppError> {
        let url = self
            .groups_ms_base
            .join("mygroups")
            .map_err(|e| AppError::Internal(format!("Invalid Groups URL: {e}")))?;

        self.get_with_staff_headers(url, requester_user_id, roles_header)
            .await
    }

    pub(super) async fn fetch_group_members(
        &self,
        requester_user_id: Uuid,
        roles_header: &str,
        group_id: Uuid,
    ) -> Result<Vec<GroupMemberSummary>, AppError> {
        let url = self
            .groups_ms_base
            .join(&format!("groups/{group_id}/members"))
            .map_err(|e| AppError::Internal(format!("Invalid Groups URL: {e}")))?;

        self.get_with_staff_headers(url, requester_user_id, roles_header)
            .await
    }

    async fn fetch_group_labs(
        &self,
        requester_user_id: Uuid,
        roles_header: &str,
        group_id: Uuid,
    ) -> Result<Vec<GroupLabSummary>, AppError> {
        let url = self
            .groups_ms_base
            .join(&format!("groups/{group_id}/labs"))
            .map_err(|e| AppError::Internal(format!("Invalid Groups URL: {e}")))?;

        self.get_with_staff_headers(url, requester_user_id, roles_header)
            .await
    }

    async fn get_with_staff_headers<T>(
        &self,
        url: Url,
        requester_user_id: Uuid,
        roles_header: &str,
    ) -> Result<T, AppError>
    where
        T: for<'de> Deserialize<'de>,
    {
        let response = self
            .client
            .get(url)
            .header("x-altair-user-id", requester_user_id.to_string())
            .header("x-altair-roles", roles_header)
            .send()
            .await
            .map_err(|_| AppError::Internal("Groups MS unreachable".into()))?;

        if response.status() == reqwest::StatusCode::FORBIDDEN {
            return Err(AppError::Forbidden("Cannot access group".into()));
        }
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(AppError::NotFound("Group not found".into()));
        }
        if !response.status().is_success() {
            return Err(AppError::Internal("Groups MS rejected staff lookup".into()));
        }

        let body = response
            .json::<LabApiResponse<T>>()
            .await
            .map_err(|_| AppError::Internal("Invalid Groups response".into()))?;

        Ok(body.data)
    }

    pub(super) async fn student_is_in_creator_group_for_lab(
        &self,
        requester_user_id: Uuid,
        roles_header: &str,
        lab_id: Uuid,
        student_user_id: Uuid,
    ) -> Result<bool, AppError> {
        let groups = self
            .fetch_creator_groups(requester_user_id, roles_header)
            .await?;

        for group in groups {
            if group.creator_id != requester_user_id {
                continue;
            }

            let members = self
                .fetch_group_members(requester_user_id, roles_header, group.group_id)
                .await?;
            if !members
                .iter()
                .any(|member| member.user_id == student_user_id)
            {
                continue;
            }

            let labs = self
                .fetch_group_labs(requester_user_id, roles_header, group.group_id)
                .await?;
            if labs.iter().any(|lab| lab.lab_id == lab_id) {
                return Ok(true);
            }
        }

        Ok(false)
    }

    pub(super) async fn ensure_group_and_lab_allowed(
        &self,
        requester_user_id: Uuid,
        roles_header: &str,
        is_admin: bool,
        lab_id: Uuid,
        lab: &LabOverview,
        group_id: Uuid,
    ) -> Result<(), AppError> {
        let group = self
            .fetch_group(requester_user_id, roles_header, group_id)
            .await?;

        if !is_admin && group.creator_id != requester_user_id {
            return Err(AppError::Forbidden(
                "You are not allowed to inspect this group".into(),
            ));
        }

        if is_admin || lab.creator_id == requester_user_id {
            return Ok(());
        }

        let labs = self
            .fetch_group_labs(requester_user_id, roles_header, group_id)
            .await?;
        if labs.iter().any(|group_lab| group_lab.lab_id == lab_id) {
            Ok(())
        } else {
            Err(AppError::Forbidden(
                "The selected lab is not linked to this group".into(),
            ))
        }
    }
}
