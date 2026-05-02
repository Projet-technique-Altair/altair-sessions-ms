use chrono::Utc;
use uuid::Uuid;

use crate::{
    error::AppError,
    models::staff_analysis::{
        AiAnalysisFinalResponse, GroupActivityMetrics, StudentActivityMetrics,
    },
};

use super::SessionsService;

impl SessionsService {
    pub async fn generate_staff_student_ai_analysis(
        &self,
        requester_user_id: Uuid,
        roles_header: &str,
        is_admin: bool,
        lab_id: Uuid,
        student_user_id: Uuid,
    ) -> Result<AiAnalysisFinalResponse<StudentActivityMetrics>, AppError> {
        let activity = self
            .get_staff_student_activity(
                requester_user_id,
                roles_header,
                is_admin,
                lab_id,
                student_user_id,
            )
            .await?;

        let analysis_id = self
            .create_ai_analysis_request(
                "individual_student_activity_report",
                requester_user_id,
                lab_id,
                Some(student_user_id),
                None,
                activity.metrics.session_id,
            )
            .await?;

        let payload = serde_json::json!({
            "report_type": "individual_student_activity_report",
            "lab": activity.lab,
            "student_activity": activity.metrics,
            "constraints": [
                "Do not grade the student",
                "Do not decide success or failure",
                "Do not invent facts outside the payload",
                "Use pedagogical wording for teachers"
            ]
        });

        match self.call_pedagogical_analysis(payload).await {
            Ok(report) => {
                self.finish_ai_analysis_request(analysis_id, "completed")
                    .await?;
                Ok(AiAnalysisFinalResponse {
                    report_type: "individual_student_activity_report".to_string(),
                    generated_at: Utc::now(),
                    metrics: activity.metrics,
                    report,
                })
            }
            Err(error) => {
                let _ = self.finish_ai_analysis_request(analysis_id, "failed").await;
                Err(error)
            }
        }
    }

    pub async fn generate_staff_group_ai_analysis(
        &self,
        requester_user_id: Uuid,
        roles_header: &str,
        is_admin: bool,
        lab_id: Uuid,
        group_id: Uuid,
    ) -> Result<AiAnalysisFinalResponse<GroupActivityMetrics>, AppError> {
        let activity = self
            .get_staff_group_activity(requester_user_id, roles_header, is_admin, lab_id, group_id)
            .await?;

        let analysis_id = self
            .create_ai_analysis_request(
                "group_activity_report",
                requester_user_id,
                lab_id,
                None,
                Some(group_id),
                None,
            )
            .await?;

        let payload = serde_json::json!({
            "report_type": "group_activity_report",
            "lab": activity.lab,
            "group_activity": activity.metrics,
            "students": activity.students,
            "constraints": [
                "Do not rank students",
                "Do not grade students",
                "Keep the report aggregated",
                "Do not invent facts outside the payload"
            ]
        });

        match self.call_pedagogical_analysis(payload).await {
            Ok(report) => {
                self.finish_ai_analysis_request(analysis_id, "completed")
                    .await?;
                Ok(AiAnalysisFinalResponse {
                    report_type: "group_activity_report".to_string(),
                    generated_at: Utc::now(),
                    metrics: activity.metrics,
                    report,
                })
            }
            Err(error) => {
                let _ = self.finish_ai_analysis_request(analysis_id, "failed").await;
                Err(error)
            }
        }
    }

    async fn create_ai_analysis_request(
        &self,
        report_type: &str,
        requested_by_user_id: Uuid,
        lab_id: Uuid,
        student_user_id: Option<Uuid>,
        group_id: Option<Uuid>,
        session_id: Option<Uuid>,
    ) -> Result<Uuid, AppError> {
        sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO ai_analysis_requests (
                report_type,
                requested_by_user_id,
                lab_id,
                student_user_id,
                group_id,
                session_id,
                status,
                model_provider,
                model_name,
                created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, 'running', 'gemini', 'configured', NOW())
            RETURNING analysis_id
            "#,
        )
        .bind(report_type)
        .bind(requested_by_user_id)
        .bind(lab_id)
        .bind(student_user_id)
        .bind(group_id)
        .bind(session_id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))
    }

    async fn finish_ai_analysis_request(
        &self,
        analysis_id: Uuid,
        status: &str,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            UPDATE ai_analysis_requests
            SET status = $1,
                finished_at = NOW()
            WHERE analysis_id = $2
            "#,
        )
        .bind(status)
        .bind(analysis_id)
        .execute(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn call_pedagogical_analysis(
        &self,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, AppError> {
        let url = self
            .ia_ms_base
            .join("internal/ia/pedagogical-analysis")
            .map_err(|e| AppError::Internal(format!("Invalid IA URL: {e}")))?;

        let mut request = self.client.post(url).json(&payload);
        if let Some(token) = &self.internal_worker_token {
            request = request.header("x-internal-worker-token", token);
        }

        let response = request
            .send()
            .await
            .map_err(|_| AppError::Internal("IA MS unreachable".into()))?;
        let status = response.status();
        let body = response
            .json::<serde_json::Value>()
            .await
            .map_err(|_| AppError::Internal("Invalid IA response".into()))?;

        if !status.is_success() {
            let message = body
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(|message| message.as_str())
                .unwrap_or("IA analysis failed");
            return Err(AppError::Internal(message.to_string()));
        }

        body.get("data")
            .cloned()
            .ok_or_else(|| AppError::Internal("IA response data missing".into()))
    }
}
