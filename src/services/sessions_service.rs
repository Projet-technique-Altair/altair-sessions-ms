use reqwest::Client;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Row, Transaction};
use url::Url;
use uuid::Uuid;

use crate::{
    error::AppError,
    models::lab_progress::{LabProgress, LabProgressRow},
    models::learner_lab_status::{
        LearnerDashboardLab, LearnerLabStatus, LearnerLabStatusKind, LearnerLabStatusRow,
    },
    models::session::{Session, SessionRow},
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum ValidationType {
    ExactMatch,
    Regex,
    Contains,
}

#[derive(Clone)]
pub struct SessionsService {
    db: PgPool,
    client: Client,
    /// URL for lab-api-service (Kubernetes/container management)
    lab_api_base: Url,
    /// URL for labs-ms (lab metadata, steps, hints)
    labs_ms_base: Url,
    /// URL for groups-ms (private access checks)
    groups_ms_base: Url,
}

use serde_json::Value;

#[derive(Serialize)]
pub struct AdminSessionsAnalytics {
    pub total_sessions: i64,
    pub launched_sessions: i64,
    pub completed_sessions: i64,
    pub active_sessions: i64,
    pub active_runtimes: i64,
    pub completion_rate: f64,
    pub launches_last_7d: i64,
    pub completions_last_7d: i64,
}

#[derive(Serialize)]
pub struct SessionWithSteps {
    #[serde(flatten)]
    pub session: Session,
    pub steps: Vec<Value>,
}

#[derive(Clone)]
pub struct WebRuntimeSession {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub runtime_kind: String,
    pub container_id: String,
    pub status: String,
}

// =====================================
// Résultat métier de validate-step
// =====================================
pub struct ValidateStepResult {
    pub correct: bool,
    pub attempts: i32,
    pub points_earned: i32,
    pub current_step: i32,
    pub next_step: Option<serde_json::Value>,
}

const RUNTIME_TTL_SECS: i64 = 7_200;

const SESSION_SELECT: &str = r#"
    SELECT
        s.session_id,
        s.user_id,
        s.lab_id,
        s.current_runtime_id,
        s.status,
        r.container_id,
        r.runtime_kind,
        r.webshell_url,
        r.app_url,
        r.expires_at,
        s.created_at,
        s.completed_at,
        s.last_activity_at
    FROM lab_sessions s
    LEFT JOIN lab_session_runtimes r
      ON r.runtime_id = s.current_runtime_id
"#;

#[derive(Debug, Clone, Deserialize)]
struct LabApiResponse<T> {
    data: T,
}

#[derive(Debug, Clone, Deserialize)]
struct LabOverview {
    lab_id: Uuid,
    name: String,
    description: Option<String>,
    difficulty: Option<String>,
    category: Option<String>,
    visibility: Option<String>,
    content_status: Option<String>,
    lab_delivery: Option<String>,
    estimated_duration: Option<String>,
    template_path: Option<String>,
}

fn extract_runtime_app_port(
    lab_data: &serde_json::Value,
    lab_delivery: &str,
) -> Result<Option<i64>, AppError> {
    let app_port =
        match lab_data
            .get("runtime")
            .and_then(|runtime| runtime.get("app_port"))
        {
            Some(serde_json::Value::Null) | None => None,
            // The spawn payload needs an integer port value that downstream services can
            // forward as-is to Kubernetes Service targetPort.
            Some(value) => Some(value.as_i64().ok_or_else(|| {
                AppError::Internal("Lab runtime.app_port must be an integer".into())
            })?),
        };

    // Web runtimes need their HTTP entrypoint before sessions-ms can ask lab-api
    // to provision the matching Kubernetes Service.
    if lab_delivery == "web" && app_port.is_none() {
        return Err(AppError::Internal(
            "Lab runtime.app_port missing for web delivery".into(),
        ));
    }

    Ok(app_port)
}

impl SessionsService {
    pub fn new(db: PgPool) -> Self {
        let raw_api =
            std::env::var("LAB_API_URL").unwrap_or_else(|_| "http://localhost:8085/".to_string());

        let lab_api_base = Url::parse(&raw_api).expect("Invalid LAB_API_URL");

        let raw_labs =
            std::env::var("LABS_MS_URL").unwrap_or_else(|_| "http://localhost:3002/".to_string());

        let labs_ms_base = Url::parse(&raw_labs).expect("Invalid LABS_MS_URL");

        let raw_groups =
            std::env::var("GROUPS_MS_URL").unwrap_or_else(|_| "http://localhost:3006/".to_string());

        let groups_ms_base = Url::parse(&raw_groups).expect("Invalid GROUPS_MS_URL");

        Self {
            db,
            client: Client::new(),
            lab_api_base,
            labs_ms_base,
            groups_ms_base,
        }
    }

    pub async fn get_admin_analytics(&self) -> Result<AdminSessionsAnalytics, AppError> {
        let row = sqlx::query(
            r#"
            SELECT
                COUNT(*)::BIGINT AS total_sessions,
                COUNT(*) FILTER (WHERE status IN ('created', 'in_progress', 'completed'))::BIGINT AS launched_sessions,
                COUNT(*) FILTER (WHERE status = 'completed')::BIGINT AS completed_sessions,
                COUNT(*) FILTER (WHERE status IN ('created', 'in_progress'))::BIGINT AS active_sessions,
                COUNT(*) FILTER (
                    WHERE current_runtime_id IS NOT NULL
                      AND status IN ('created', 'in_progress')
                )::BIGINT AS active_runtimes,
                COUNT(*) FILTER (WHERE created_at >= NOW() - INTERVAL '7 days')::BIGINT AS launches_last_7d,
                COUNT(*) FILTER (
                    WHERE completed_at >= NOW() - INTERVAL '7 days'
                      AND status = 'completed'
                )::BIGINT AS completions_last_7d
            FROM lab_sessions
            "#,
        )
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        let launched_sessions: i64 = row.try_get("launched_sessions").unwrap_or(0);
        let completed_sessions: i64 = row.try_get("completed_sessions").unwrap_or(0);
        let completion_rate = if launched_sessions > 0 {
            completed_sessions as f64 / launched_sessions as f64
        } else {
            0.0
        };

        Ok(AdminSessionsAnalytics {
            total_sessions: row.try_get("total_sessions").unwrap_or(0),
            launched_sessions,
            completed_sessions,
            active_sessions: row.try_get("active_sessions").unwrap_or(0),
            active_runtimes: row.try_get("active_runtimes").unwrap_or(0),
            completion_rate,
            launches_last_7d: row.try_get("launches_last_7d").unwrap_or(0),
            completions_last_7d: row.try_get("completions_last_7d").unwrap_or(0),
        })
    }

    fn runtime_namespace(runtime_kind: &str) -> &'static str {
        match runtime_kind {
            "web" => "labs-web",
            _ => "default",
        }
    }

    async fn load_session_row(&self, session_id: Uuid) -> Result<SessionRow, AppError> {
        let sql = format!("{SESSION_SELECT} WHERE s.session_id = $1");

        sqlx::query_as::<_, SessionRow>(&sql)
            .bind(session_id)
            .fetch_one(&self.db)
            .await
            .map_err(|_| AppError::NotFound("Session not found".into()))
    }

    async fn load_session_row_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        session_id: Uuid,
    ) -> Result<SessionRow, AppError> {
        let sql = format!("{SESSION_SELECT} WHERE s.session_id = $1");

        sqlx::query_as::<_, SessionRow>(&sql)
            .bind(session_id)
            .fetch_one(&mut **tx)
            .await
            .map_err(|_| AppError::NotFound("Session not found".into()))
    }

    async fn next_restart_index(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        session_id: Uuid,
    ) -> Result<i32, AppError> {
        let next = sqlx::query_scalar::<_, i32>(
            r#"
            SELECT COALESCE(MAX(restart_index), 0) + 1
            FROM lab_session_runtimes
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(next)
    }

    async fn create_runtime_row(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        session_id: Uuid,
        runtime_kind: &str,
    ) -> Result<Uuid, AppError> {
        let restart_index = self.next_restart_index(tx, session_id).await?;
        let namespace = Self::runtime_namespace(runtime_kind);

        let runtime_id = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO lab_session_runtimes (
                session_id,
                runtime_kind,
                status,
                namespace,
                expires_at,
                restart_index
            )
            VALUES (
                $1,
                $2,
                'starting',
                $3,
                NOW() + ($4 * INTERVAL '1 second'),
                $5
            )
            RETURNING runtime_id
            "#,
        )
        .bind(session_id)
        .bind(runtime_kind)
        .bind(namespace)
        .bind(RUNTIME_TTL_SECS)
        .bind(restart_index)
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(runtime_id)
    }

    async fn mark_session_runtime_active(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        session_id: Uuid,
        runtime_id: Uuid,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            UPDATE lab_sessions
            SET
                status = 'in_progress',
                current_runtime_id = $1,
                last_activity_at = NOW()
            WHERE session_id = $2
            "#,
        )
        .bind(runtime_id)
        .bind(session_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn clear_current_runtime(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        session_id: Uuid,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            UPDATE lab_sessions
            SET
                current_runtime_id = NULL,
                last_activity_at = NOW()
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn mark_session_in_progress(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        session_id: Uuid,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            UPDATE lab_sessions
            SET
                status = 'in_progress',
                last_activity_at = NOW()
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn finalize_runtime_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        runtime_id: Uuid,
        status: &str,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            UPDATE lab_session_runtimes
            SET
                status = $1,
                stopped_at = COALESCE(stopped_at, NOW()),
                last_seen_at = NOW()
            WHERE runtime_id = $2
            "#,
        )
        .bind(status)
        .bind(runtime_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn total_runtime_seconds(&self, session_id: Uuid) -> Result<i64, AppError> {
        let total = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COALESCE(
                FLOOR(SUM(
                    EXTRACT(
                        EPOCH FROM (
                            COALESCE(stopped_at, NOW()) - created_at
                        )
                    )
                ))::BIGINT,
                0::BIGINT
            )
            FROM lab_session_runtimes
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(total.max(0))
    }

    async fn provision_runtime_for_session(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        session_id: Uuid,
        lab_id: Uuid,
    ) -> Result<(), AppError> {
        let url = self
            .labs_ms_base
            .join(&format!("labs/{lab_id}"))
            .map_err(|e| AppError::Internal(format!("Invalid Labs URL: {e}")))?;

        let lab_resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| AppError::Internal("Labs MS unreachable".into()))?
            .json::<serde_json::Value>()
            .await
            .map_err(|_| AppError::Internal("Invalid Labs response".into()))?;

        let lab_data = lab_resp
            .get("data")
            .ok_or_else(|| AppError::Internal("Lab data missing".into()))?;

        let lab_type = lab_data["lab_type"]
            .as_str()
            .unwrap_or("ctf_terminal_guided")
            .to_string();

        let template_path = lab_data["template_path"]
            .as_str()
            .ok_or_else(|| AppError::Internal("Lab template_path missing".into()))?
            .to_string();
        let lab_delivery = lab_data["lab_delivery"]
            .as_str()
            .ok_or_else(|| AppError::Internal("Lab lab_delivery missing".into()))?
            .to_string();
        let app_port = extract_runtime_app_port(lab_data, &lab_delivery)?;

        let runtime_id = self
            .create_runtime_row(tx, session_id, &lab_delivery)
            .await?;

        let url = self
            .lab_api_base
            .join("spawn")
            .map_err(|e| AppError::Internal(format!("Invalid lab-api URL: {e}")))?;

        let spawn_result = self
            .client
            .post(url)
            .json(&serde_json::json!({
                "session_id": session_id,
                "runtime_id": runtime_id,
                "lab_type": lab_type,
                "template_path": template_path,
                "lab_delivery": lab_delivery,
                "app_port": app_port
            }))
            .send()
            .await;

        match spawn_result {
            Ok(resp) if resp.status().is_success() => {
                let spawn: SpawnResponse = resp.json().await.map_err(|_| {
                    AppError::Internal("Invalid response from lab-api-service".into())
                })?;

                let persist_runtime = async {
                    sqlx::query(
                        r#"
                        UPDATE lab_session_runtimes
                        SET
                            container_id = $1,
                            status = 'running',
                            webshell_url = $2,
                            app_url = $3,
                            last_seen_at = NOW()
                        WHERE runtime_id = $4
                        "#,
                    )
                    .bind(&spawn.data.container_id)
                    .bind(&spawn.data.webshell_url)
                    .bind(&spawn.data.app_url)
                    .bind(runtime_id)
                    .execute(&mut **tx)
                    .await
                    .map_err(|e| AppError::Internal(e.to_string()))?;

                    self.mark_session_runtime_active(tx, session_id, runtime_id)
                        .await
                }
                .await;

                // Stop the freshly created runtime if sessions-ms cannot persist it.
                if let Err(error) = persist_runtime {
                    let _ = self
                        .stop_runtime_container_best_effort(&spawn.data.container_id)
                        .await;
                    return Err(error);
                }
            }
            Ok(resp) => {
                let error_body = resp.text().await.unwrap_or_default();
                eprintln!("Lab API spawn failed: {}", error_body);

                self.finalize_runtime_in_tx(tx, runtime_id, "error").await?;
                self.clear_current_runtime(tx, session_id).await?;
                self.mark_session_in_progress(tx, session_id).await?;

                return Err(AppError::Internal("Failed to spawn container".into()));
            }
            Err(error) => {
                eprintln!("Lab API unreachable: {}", error);

                self.finalize_runtime_in_tx(tx, runtime_id, "error").await?;
                self.clear_current_runtime(tx, session_id).await?;
                self.mark_session_in_progress(tx, session_id).await?;

                return Err(AppError::Internal("Lab API service unreachable".into()));
            }
        }

        Ok(())
    }

    async fn stop_runtime_container_best_effort(&self, container_id: &str) -> Result<(), AppError> {
        let url = self
            .lab_api_base
            .join("spawn/stop")
            .map_err(|e| AppError::Internal(format!("Invalid lab-api URL: {e}")))?;

        let _ = self
            .client
            .post(url)
            .json(&serde_json::json!({
                "container_id": container_id
            }))
            .send()
            .await;

        Ok(())
    }

    /// Creates or refreshes a TO DO relation for a learner on a public lab.
    pub async fn follow_lab(
        &self,
        user_id: Uuid,
        lab_id: Uuid,
    ) -> Result<LearnerLabStatus, AppError> {
        let lab = self.fetch_lab_overview(lab_id).await?;
        if lab.content_status.as_deref() == Some("archived") {
            return Err(AppError::Forbidden(
                "Archived labs cannot be followed".into(),
            ));
        }

        if lab.visibility.as_deref() != Some("PUBLIC") {
            return Err(AppError::Forbidden(
                "Only public labs can be followed".into(),
            ));
        }

        let now = chrono::Utc::now().naive_utc();

        // Re-following an existing TO DO refreshes its timestamps. Higher states keep their
        // original lifecycle so we do not accidentally downgrade IN_PROGRESS or FINISHED.
        let row = sqlx::query_as::<_, LearnerLabStatusRow>(
            r#"
            INSERT INTO learner_lab_status (
                user_id,
                lab_id,
                status,
                followed_at,
                last_activity_at
            )
            VALUES ($1, $2, 'todo', $3, $3)
            ON CONFLICT (user_id, lab_id)
            DO UPDATE
            SET
                followed_at = CASE
                    WHEN learner_lab_status.status = 'todo' THEN EXCLUDED.followed_at
                    ELSE learner_lab_status.followed_at
                END,
                last_activity_at = CASE
                    WHEN learner_lab_status.status = 'todo' THEN EXCLUDED.last_activity_at
                    ELSE learner_lab_status.last_activity_at
                END
            RETURNING *
            "#,
        )
        .bind(user_id)
        .bind(lab_id)
        .bind(now)
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        LearnerLabStatus::try_from(row)
    }

    /// Removes the learner-lab relation only while it still represents a simple saved item.
    pub async fn unfollow_lab(&self, user_id: Uuid, lab_id: Uuid) -> Result<(), AppError> {
        let existing = sqlx::query_as::<_, LearnerLabStatusRow>(
            r#"
            SELECT *
            FROM learner_lab_status
            WHERE user_id = $1 AND lab_id = $2
            "#,
        )
        .bind(user_id)
        .bind(lab_id)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        let Some(row) = existing else {
            return Ok(());
        };

        let current = LearnerLabStatus::try_from(row)?;

        if current.status != LearnerLabStatusKind::Todo {
            return Err(AppError::Conflict(
                "Only TO DO labs can be removed from follow".into(),
            ));
        }

        sqlx::query(
            r#"
            DELETE FROM learner_lab_status
            WHERE user_id = $1 AND lab_id = $2
            "#,
        )
        .bind(user_id)
        .bind(lab_id)
        .execute(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(())
    }

    /// Builds the learner dashboard view by combining learner status rows with live lab metadata.
    pub async fn get_dashboard_labs(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<LearnerDashboardLab>, AppError> {
        let rows = sqlx::query_as::<_, LearnerLabStatusRow>(
            r#"
            SELECT *
            FROM learner_lab_status
            WHERE user_id = $1
            ORDER BY
                CASE status
                    WHEN 'in_progress' THEN 0
                    WHEN 'todo' THEN 1
                    WHEN 'finished' THEN 2
                    ELSE 3
                END,
                last_activity_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        let mut result = Vec::with_capacity(rows.len());

        for row in rows {
            let status = LearnerLabStatus::try_from(row)?.clone();
            let lab = self.fetch_lab_overview(status.lab_id).await?;
            let progress = self
                .compute_dashboard_progress(status.status, status.last_session_id, status.lab_id)
                .await?;

            result.push(LearnerDashboardLab {
                lab_id: lab.lab_id,
                name: lab.name,
                description: lab.description,
                difficulty: lab.difficulty,
                category: lab.category,
                visibility: lab.visibility,
                lab_delivery: lab.lab_delivery,
                estimated_duration: lab.estimated_duration,
                template_path: lab.template_path,
                status: status.status,
                started_at: status.started_at,
                finished_at: status.finished_at,
                last_activity_at: status.last_activity_at,
                progress,
            });
        }

        Ok(result)
    }

    /// Marks a learner lab as active when a runtime starts or resumes.
    async fn upsert_lab_status_for_start(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        user_id: Uuid,
        lab_id: Uuid,
        session_id: Uuid,
    ) -> Result<(), AppError> {
        let now = chrono::Utc::now().naive_utc();

        sqlx::query(
            r#"
            INSERT INTO learner_lab_status (
                user_id,
                lab_id,
                status,
                followed_at,
                started_at,
                last_activity_at,
                last_session_id
            )
            VALUES ($1, $2, 'in_progress', $3, $3, $3, $4)
            ON CONFLICT (user_id, lab_id)
            DO UPDATE
            SET
                status = 'in_progress',
                followed_at = learner_lab_status.followed_at,
                started_at = CASE
                    WHEN learner_lab_status.status = 'finished' THEN EXCLUDED.started_at
                    ELSE COALESCE(learner_lab_status.started_at, EXCLUDED.started_at)
                END,
                finished_at = NULL,
                last_activity_at = EXCLUDED.last_activity_at,
                last_session_id = EXCLUDED.last_session_id
            "#,
        )
        .bind(user_id)
        .bind(lab_id)
        .bind(now)
        .bind(session_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(())
    }

    /// Persists the product-level FINISHED state independently from the runtime session state.
    async fn mark_lab_finished(
        &self,
        user_id: Uuid,
        lab_id: Uuid,
        session_id: Uuid,
    ) -> Result<(), AppError> {
        let now = chrono::Utc::now().naive_utc();

        sqlx::query(
            r#"
            INSERT INTO learner_lab_status (
                user_id,
                lab_id,
                status,
                followed_at,
                started_at,
                finished_at,
                last_activity_at,
                last_session_id
            )
            VALUES ($1, $2, 'finished', $3, $3, $3, $3, $4)
            ON CONFLICT (user_id, lab_id)
            DO UPDATE
            SET
                status = 'finished',
                started_at = COALESCE(learner_lab_status.started_at, EXCLUDED.started_at),
                finished_at = EXCLUDED.finished_at,
                last_activity_at = EXCLUDED.last_activity_at,
                last_session_id = EXCLUDED.last_session_id
            "#,
        )
        .bind(user_id)
        .bind(lab_id)
        .bind(now)
        .bind(session_id)
        .execute(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(())
    }

    /// Reads the catalog entry from labs-ms instead of duplicating lab metadata in sessions DB.
    async fn fetch_lab_overview(&self, lab_id: Uuid) -> Result<LabOverview, AppError> {
        let url = self
            .labs_ms_base
            .join(&format!("labs/{}", lab_id))
            .map_err(|e| AppError::Internal(format!("Invalid Labs URL: {e}")))?;

        let body = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| AppError::Internal("Labs MS unreachable".into()))?
            .json::<LabApiResponse<LabOverview>>()
            .await
            .map_err(|_| AppError::Internal("Invalid Labs response".into()))?;

        Ok(body.data)
    }

    async fn user_has_group_access_to_lab(
        &self,
        user_id: Uuid,
        lab_id: Uuid,
    ) -> Result<bool, AppError> {
        let url = self
            .groups_ms_base
            .join(&format!(
                "internal/access/lab?user_id={user_id}&lab_id={lab_id}"
            ))
            .map_err(|e| AppError::Internal(format!("Invalid Groups URL: {e}")))?;

        let body = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| AppError::Internal("Groups MS unreachable".into()))?
            .json::<serde_json::Value>()
            .await
            .map_err(|_| AppError::Internal("Invalid Groups response".into()))?;

        Ok(body
            .get("data")
            .and_then(|value| value.as_bool())
            .unwrap_or(false))
    }

    async fn ensure_lab_can_start(&self, user_id: Uuid, lab_id: Uuid) -> Result<(), AppError> {
        let lab = self.fetch_lab_overview(lab_id).await?;

        if lab.content_status.as_deref() == Some("archived") {
            return Err(AppError::Forbidden(
                "This lab is archived and cannot be started".into(),
            ));
        }

        if lab.visibility.as_deref() == Some("PUBLIC") {
            return Ok(());
        }

        if self.user_has_group_access_to_lab(user_id, lab_id).await? {
            return Ok(());
        }

        Err(AppError::Forbidden(
            "You are not allowed to start this private lab".into(),
        ))
    }

    /// Uses labs-ms as the source of truth for step count when rendering learner progress.
    async fn fetch_lab_steps_count(&self, lab_id: Uuid) -> Result<i32, AppError> {
        let url = self
            .labs_ms_base
            .join(&format!("labs/{}/steps", lab_id))
            .map_err(|e| AppError::Internal(format!("Invalid Labs URL: {e}")))?;

        let body = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| AppError::Internal("Labs MS unreachable".into()))?
            .json::<serde_json::Value>()
            .await
            .map_err(|_| AppError::Internal("Invalid Labs response".into()))?;

        let steps = body["data"]
            .as_array()
            .ok_or_else(|| AppError::Internal("Labs steps missing".into()))?;

        Ok(steps.len() as i32)
    }

    /// Converts the learner-level status into a dashboard percentage.
    async fn compute_dashboard_progress(
        &self,
        status: LearnerLabStatusKind,
        last_session_id: Option<Uuid>,
        lab_id: Uuid,
    ) -> Result<i32, AppError> {
        match status {
            LearnerLabStatusKind::Todo => Ok(0),
            LearnerLabStatusKind::Finished => Ok(100),
            LearnerLabStatusKind::InProgress => {
                // Without a tracked session we cannot derive a partial percentage yet.
                let Some(session_id) = last_session_id else {
                    return Ok(0);
                };

                let progress = sqlx::query_as::<_, LabProgressRow>(
                    r#"
                    SELECT *
                    FROM lab_progress
                    WHERE session_id = $1
                    "#,
                )
                .bind(session_id)
                .fetch_optional(&self.db)
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;

                let Some(progress) = progress else {
                    return Ok(0);
                };

                let total_steps = self.fetch_lab_steps_count(lab_id).await?;
                if total_steps <= 0 {
                    return Ok(0);
                }

                let completed = progress.completed_steps.len() as i32;
                Ok(((completed * 100) / total_steps).clamp(0, 99))
            }
        }
    }

    /// POST /labs/:id/start
    pub async fn start_session(
        &self,
        user_id: Uuid,
        lab_id: Uuid,
        track_learner_status: bool,
    ) -> Result<Session, AppError> {
        self.ensure_lab_can_start(user_id, lab_id).await?;

        let mut tx = self
            .db
            .begin()
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let existing_session_id = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT session_id
            FROM lab_sessions
            WHERE user_id = $1
              AND lab_id = $2
              AND status IN ('created', 'in_progress')
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(user_id)
        .bind(lab_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        let session_id = if let Some(session_id) = existing_session_id {
            let row = self.load_session_row_in_tx(&mut tx, session_id).await?;
            let row = self.reconcile_runtime_row_in_tx(&mut tx, row).await?;

            if row.current_runtime_id.is_none() {
                self.provision_runtime_for_session(&mut tx, session_id, lab_id)
                    .await?;
            } else {
                sqlx::query(
                    r#"
                    UPDATE lab_sessions
                    SET last_activity_at = NOW()
                    WHERE session_id = $1
                    "#,
                )
                .bind(session_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;
            }

            session_id
        } else {
            let session_id = sqlx::query_scalar::<_, Uuid>(
                r#"
                INSERT INTO lab_sessions (
                    user_id,
                    lab_id,
                    status,
                    last_activity_at
                )
                VALUES ($1, $2, 'created', NOW())
                RETURNING session_id
                "#,
            )
            .bind(user_id)
            .bind(lab_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

            sqlx::query(
                r#"
                INSERT INTO lab_progress (
                    session_id,
                    current_step,
                    completed_steps,
                    hints_used,
                    attempts_per_step,
                    score,
                    max_score
                )
                VALUES (
                    $1,
                    1,
                    '{}',
                    '[]',
                    '{}',
                    0,
                    0
                )
                "#,
            )
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

            let url = self
                .labs_ms_base
                .join(&format!("internal/labs/{lab_id}/steps"))
                .map_err(|e| AppError::Internal(format!("Invalid Labs URL: {e}")))?;

            let steps_resp = self
                .client
                .get(url)
                .send()
                .await
                .map_err(|_| AppError::Internal("Labs MS unreachable".into()))?
                .json::<serde_json::Value>()
                .await
                .map_err(|_| AppError::Internal("Invalid Labs response".into()))?;

            let steps = steps_resp["data"]
                .as_array()
                .ok_or_else(|| AppError::Internal("Labs steps missing".into()))?;

            let max_score = steps
                .iter()
                .filter_map(|s| s["points"].as_i64())
                .sum::<i64>() as i32;

            sqlx::query(
                r#"
                UPDATE lab_progress
                SET
                    max_score = $1,
                    score = 0
                WHERE session_id = $2
                "#,
            )
            .bind(max_score)
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

            self.provision_runtime_for_session(&mut tx, session_id, lab_id)
                .await?;

            session_id
        };

        if track_learner_status {
            self.upsert_lab_status_for_start(&mut tx, user_id, lab_id, session_id)
                .await?;
        }

        tx.commit()
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        self.get_session_by_id(session_id).await
    }

    /// DELETE /sessions/:id
    pub async fn stop_session(&self, session_id: Uuid) -> Result<(), AppError> {
        let row = self.load_session_row(session_id).await?;
        let row = self.reconcile_runtime_row(row).await?;

        let Some(runtime_id) = row.current_runtime_id else {
            return Ok(());
        };

        if let Some(container_id) = row.container_id.as_deref() {
            self.stop_runtime_container_best_effort(container_id)
                .await?;
        }

        let mut tx = self
            .db
            .begin()
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        self.finalize_runtime_in_tx(&mut tx, runtime_id, "stopped")
            .await?;
        self.clear_current_runtime(&mut tx, session_id).await?;

        if row.status != "completed" {
            self.mark_session_in_progress(&mut tx, session_id).await?;
        }

        tx.commit()
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(())
    }

    /// GET /sessions/:id
    pub async fn get_session_by_id(&self, session_id: Uuid) -> Result<Session, AppError> {
        let row = self.load_session_row(session_id).await?;
        let row = self.reconcile_runtime_row(row).await?;

        Session::try_from(row)
    }

    pub async fn get_session_with_steps(
        &self,
        session_id: Uuid,
    ) -> Result<SessionWithSteps, AppError> {
        // 1) Session DB
        let session = self.get_session_by_id(session_id).await?;

        // 2) Steps via Labs MS
        let url = self
            .labs_ms_base
            .join(&format!("internal/labs/{}/steps/runtime", session.lab_id))
            .map_err(|e| AppError::Internal(format!("Invalid Labs URL: {e}")))?;

        let steps_resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| AppError::Internal("Labs MS unreachable".into()))?
            .json::<serde_json::Value>()
            .await
            .map_err(|_| AppError::Internal("Invalid Labs response".into()))?;

        let steps = steps_resp["data"]
            .as_array()
            .ok_or_else(|| AppError::Internal("Labs steps missing".into()))?
            .clone();

        Ok(SessionWithSteps { session, steps })
    }

    pub async fn get_web_runtime(&self, session_id: Uuid) -> Result<WebRuntimeSession, AppError> {
        let session = self.get_session_by_id(session_id).await?;

        if session.current_runtime_id.is_none() {
            return Err(AppError::Conflict("Web runtime not ready yet".into()));
        }

        let runtime_kind = session
            .runtime_kind
            .clone()
            .ok_or_else(|| AppError::Conflict("Runtime kind not ready yet".into()))?;

        if runtime_kind != "web" {
            return Err(AppError::BadRequest(
                "Session does not expose a web runtime".into(),
            ));
        }

        let container_id = session
            .container_id
            .clone()
            .ok_or_else(|| AppError::Conflict("Web runtime container not ready yet".into()))?;

        Ok(WebRuntimeSession {
            session_id: session.session_id,
            user_id: session.user_id,
            runtime_kind,
            container_id,
            status: "running".to_string(),
        })
    }

    // GET /sessions/lab/:id
    pub async fn get_sessions_by_lab(&self, lab_id: Uuid) -> Result<Vec<Session>, AppError> {
        let sql = format!("{SESSION_SELECT} WHERE s.lab_id = $1 ORDER BY s.created_at DESC");

        let rows = sqlx::query_as::<_, SessionRow>(&sql)
            .bind(lab_id)
            .fetch_all(&self.db)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let mut sessions = Vec::with_capacity(rows.len());
        for row in rows {
            let row = self.reconcile_runtime_row(row).await?;
            sessions.push(Session::try_from(row)?);
        }

        Ok(sessions)
    }

    //GET /sessions/user/:id
    pub async fn get_sessions_by_user(&self, user_id: Uuid) -> Result<Vec<Session>, AppError> {
        let sql = format!("{SESSION_SELECT} WHERE s.user_id = $1 ORDER BY s.created_at DESC");

        let rows = sqlx::query_as::<_, SessionRow>(&sql)
            .bind(user_id)
            .fetch_all(&self.db)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let mut sessions = Vec::with_capacity(rows.len());
        for row in rows {
            let row = self.reconcile_runtime_row(row).await?;
            sessions.push(Session::try_from(row)?);
        }

        Ok(sessions)
    }

    //EXPIRE SESSION
    pub async fn expire_session(&self, session_id: Uuid) -> Result<(), AppError> {
        let row = self.load_session_row(session_id).await?;
        let row = self.reconcile_runtime_row(row).await?;

        let Some(runtime_id) = row.current_runtime_id else {
            return Ok(());
        };

        if let Some(container_id) = row.container_id.as_deref() {
            self.stop_runtime_container_best_effort(container_id)
                .await?;
        }

        let mut tx = self
            .db
            .begin()
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        self.finalize_runtime_in_tx(&mut tx, runtime_id, "expired")
            .await?;
        self.clear_current_runtime(&mut tx, session_id).await?;

        if row.status != "completed" {
            self.mark_session_in_progress(&mut tx, session_id).await?;
        }

        tx.commit()
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(())
    }

    // CRON
    pub async fn expire_all_expired_sessions(&self) -> Result<usize, AppError> {
        let session_ids = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT DISTINCT session_id
            FROM lab_session_runtimes
            WHERE status = 'running'
              AND expires_at < NOW()
            "#,
        )
        .fetch_all(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        let mut expired_count = 0;

        for session_id in session_ids {
            if self.expire_session(session_id).await.is_ok() {
                expired_count += 1;
            }
        }

        Ok(expired_count)
    }

    // ======================================================
    // GET /sessions/:id/progress
    // ======================================================
    pub async fn get_progress(&self, session_id: Uuid) -> Result<LabProgress, AppError> {
        let progress_row = sqlx::query_as::<_, LabProgressRow>(
            r#"
            SELECT *
            FROM lab_progress
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .fetch_one(&self.db)
        .await
        .map_err(|_| AppError::NotFound("Progress not found".into()))?;

        let time_elapsed = self.total_runtime_seconds(session_id).await?;

        Ok(LabProgress::from_row(progress_row, time_elapsed))
    }

    // ======================================================
    // Helpers Labs MS
    // ======================================================

    /*async fn fetch_lab_steps(
        &self,
        lab_id: Uuid,
    ) -> Result<Vec<serde_json::Value>, AppError> {
        let resp = self.client
            .get(format!("{}/internal/labs/{}/steps", self.lab_api_url, lab_id))
            .send()
            .await
            .map_err(|_| AppError::Internal("Labs MS unreachable".into()))?
            .json::<serde_json::Value>()
            .await
            .map_err(|_| AppError::Internal("Invalid Labs response".into()))?;

        resp["data"]
            .as_array()
            .cloned()
            .ok_or_else(|| AppError::Internal("Labs steps missing".into()))
    }

    fn find_step_by_number(
        steps: &[serde_json::Value],
        step_number: i32,
    ) -> Result<&serde_json::Value, AppError> {
        steps
            .iter()
            .find(|s| s["step_number"].as_i64() == Some(step_number as i64))
            .ok_or_else(|| AppError::NotFound("Step not found in Labs".into()))
    }*/

    async fn fetch_step_internal(
        &self,
        lab_id: Uuid,
        step_number: i32,
    ) -> Result<serde_json::Value, AppError> {
        let url = self
            .labs_ms_base
            .join(&format!("internal/labs/{}/steps/{}", lab_id, step_number))
            .map_err(|e| AppError::Internal(format!("Invalid Labs URL: {e}")))?;

        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| AppError::Internal("Labs MS unreachable".into()))?
            .json::<serde_json::Value>()
            .await
            .map_err(|_| AppError::Internal("Invalid Labs response".into()))?;

        resp.get("data")
            .cloned()
            .ok_or_else(|| AppError::Internal("Lab step missing".into()))
    }

    pub async fn validate_step(
        &self,
        session_id: Uuid,
        step_number: i32,
        user_answer: String,
    ) -> Result<ValidateStepResult, AppError> {
        // 1️⃣ Charger la progression
        let progress = sqlx::query_as::<_, LabProgressRow>(
            r#"
            SELECT *
            FROM lab_progress
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .fetch_one(&self.db)
        .await
        .map_err(|_| AppError::NotFound("Progress not found".into()))?;

        // 2️⃣ Vérifier progression linéaire
        if step_number != progress.current_step {
            return Err(AppError::Conflict("Invalid step order".into()));
        }

        // 3️⃣ Incrémenter attempts_per_step
        let mut attempts = progress.attempts_per_step;
        let key = step_number.to_string();
        let new_attempts = attempts.get(&key).and_then(|v| v.as_i64()).unwrap_or(0) + 1;

        attempts[key.clone()] = serde_json::json!(new_attempts);

        // 4️⃣ Charger lab_id
        let lab_id = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT lab_id
            FROM lab_sessions
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .fetch_one(&self.db)
        .await
        .map_err(|_| AppError::NotFound("Session not found".into()))?;

        // 5️⃣ Charger les steps depuis Labs MS
        let step = self.fetch_step_internal(lab_id, step_number).await?;

        // 6️⃣ Extraire les données de validation
        /*let validation_type: ValidationType = serde_json::from_value(
            step["validation_type"].clone()
        ).map_err(|_| AppError::Internal("Invalid validation_type".into()))?;*/

        let validation_type = match step["validation_type"].as_str() {
            Some("exact_match") => ValidationType::ExactMatch,
            Some("regex") => ValidationType::Regex,
            Some("contains") => ValidationType::Contains,
            other => {
                return Err(AppError::Internal(format!(
                    "Invalid validation_type: {:?}",
                    other
                )))
            }
        };

        let expected_answer = step["expected_answer"].as_str().unwrap_or("");

        let validation_pattern = step["validation_pattern"].as_str().unwrap_or("");

        let points = step["points"].as_i64().unwrap_or(0) as i32;

        let step_id = step["step_id"]
            .as_str()
            .ok_or_else(|| AppError::Internal("Missing step_id".into()))?;

        let hints_url = self
            .labs_ms_base
            .join(&format!("labs/{}/steps/{}/hints", lab_id, step_id))
            .map_err(|e| AppError::Internal(format!("Invalid Labs URL: {e}")))?;

        let hints_resp = self
            .client
            .get(hints_url)
            .send()
            .await
            .map_err(|_| AppError::Internal("Labs MS unreachable".into()))?
            .json::<serde_json::Value>()
            .await
            .map_err(|_| AppError::Internal("Invalid Labs response".into()))?;

        let lab_hints = hints_resp["data"]
            .as_array()
            .ok_or_else(|| AppError::Internal("Labs hints missing".into()))?;

        let used_hint_keys = progress.hints_used.as_array().cloned().unwrap_or_default();

        let total_hint_cost = lab_hints
            .iter()
            .filter(|hint| {
                let hint_number = hint["hint_number"].as_i64().unwrap_or_default();
                let hint_key = format!("{}_{}", step_number, hint_number);

                used_hint_keys
                    .iter()
                    .any(|used| used.as_str() == Some(hint_key.as_str()))
            })
            .map(|hint| hint["cost"].as_i64().unwrap_or(0) as i32)
            .sum::<i32>();

        let effective_points = (points - total_hint_cost).max(0);

        // 7️⃣ Validation
        let correct = match validation_type {
            ValidationType::ExactMatch => user_answer.trim() == expected_answer,
            ValidationType::Contains => user_answer.contains(validation_pattern),
            ValidationType::Regex => regex::Regex::new(validation_pattern)
                .map(|r| r.is_match(&user_answer))
                .unwrap_or(false),
        };

        // 7️⃣ Mise à jour DB
        if correct {
            sqlx::query(
                r#"
                UPDATE lab_progress
                SET
                    completed_steps = array_append(completed_steps, $1),
                    current_step = current_step + 1,
                    attempts_per_step = $2,
                    score = score + $3
                WHERE session_id = $4
                "#,
            )
            .bind(step_number)
            .bind(&attempts)
            .bind(effective_points)
            .bind(session_id)
            .execute(&self.db)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
        } else {
            sqlx::query(
                r#"
                UPDATE lab_progress
                SET attempts_per_step = $1
                WHERE session_id = $2
                "#,
            )
            .bind(&attempts)
            .bind(session_id)
            .execute(&self.db)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
        }

        // 8️⃣ Construire le résultat métier
        Ok(ValidateStepResult {
            correct,
            attempts: new_attempts as i32,
            points_earned: if correct { effective_points } else { 0 },
            current_step: if correct {
                progress.current_step + 1
            } else {
                progress.current_step
            },
            next_step: if correct {
                step.get("next_step").cloned()
            } else {
                None
            },
        })
    }

    pub async fn request_hint(
        &self,
        session_id: Uuid,
        step_number: i32,
        hint_number: i32,
    ) -> Result<(String, i32, i32), AppError> {
        // 1️⃣ Charger la progression
        let progress = sqlx::query_as::<_, LabProgressRow>(
            r#"
            SELECT *
            FROM lab_progress
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .fetch_one(&self.db)
        .await
        .map_err(|_| AppError::NotFound("Progress not found".into()))?;

        // 2️⃣ Clé unique pour cette astuce
        let hint_key = format!("{}_{}", step_number, hint_number);

        // 3️⃣ Convertir hints_used en Vec<Value>
        let mut hints = progress.hints_used.as_array().cloned().unwrap_or_default();

        // 4️⃣ Vérifier si déjà utilisée
        if hints.iter().any(|h| h.as_str() == Some(&hint_key)) {
            return Err(AppError::Conflict("Hint already used".into()));
        }

        // 5️⃣ Charger lab_id
        let lab_id = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT lab_id
            FROM lab_sessions
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .fetch_one(&self.db)
        .await
        .map_err(|_| AppError::NotFound("Session not found".into()))?;

        // 6️⃣ Charger les steps
        let step = self.fetch_step_internal(lab_id, step_number).await?;

        let step_id = step["step_id"]
            .as_str()
            .ok_or_else(|| AppError::Internal("Missing step_id".into()))?;

        // 7️⃣ Charger les hints de la step
        let url = self
            .labs_ms_base
            .join(&format!("labs/{}/steps/{}/hints", lab_id, step_id))
            .map_err(|e| AppError::Internal(format!("Invalid Labs URL: {e}")))?;

        let hints_resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| AppError::Internal("Labs MS unreachable".into()))?
            .json::<serde_json::Value>()
            .await
            .map_err(|_| AppError::Internal("Invalid Labs response".into()))?;

        let lab_hints = hints_resp["data"]
            .as_array()
            .ok_or_else(|| AppError::Internal("Labs hints missing".into()))?;

        let hint = lab_hints
            .iter()
            .find(|h| h["hint_number"].as_i64() == Some(hint_number as i64))
            .ok_or_else(|| AppError::NotFound("Hint not found".into()))?;

        let hint_text = hint["text"].as_str().unwrap_or("No hint").to_string();
        let cost = hint["cost"].as_i64().unwrap_or(0) as i32;

        // 7️⃣ Mise à jour score + hints_used
        hints.push(serde_json::json!(hint_key));

        sqlx::query(
            r#"
            UPDATE lab_progress
            SET
                hints_used = $1
            WHERE session_id = $2
            "#,
        )
        .bind(serde_json::Value::Array(hints.clone()))
        .bind(session_id)
        .execute(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok((hint_text, cost, progress.score))
    }

    pub async fn complete_session(&self, session_id: Uuid) -> Result<serde_json::Value, AppError> {
        let session_row = self.load_session_row(session_id).await?;
        let session_row = self.reconcile_runtime_row(session_row).await?;
        let session = Session::try_from(session_row)?;

        let progress = sqlx::query_as::<_, LabProgressRow>(
            r#"
            SELECT *
            FROM lab_progress
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .fetch_one(&self.db)
        .await
        .map_err(|_| AppError::NotFound("Progress not found".into()))?;

        let url = self
            .labs_ms_base
            .join(&format!("labs/{}/steps", session.lab_id))
            .map_err(|e| AppError::Internal(format!("Invalid Labs URL: {e}")))?;

        let steps_resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| AppError::Internal("Labs MS unreachable".into()))?
            .json::<serde_json::Value>()
            .await
            .map_err(|_| AppError::Internal("Invalid Labs response".into()))?;

        let steps = steps_resp["data"]
            .as_array()
            .ok_or_else(|| AppError::Internal("Labs steps missing".into()))?;

        let total_steps = steps.len() as i32;

        if total_steps <= 0 {
            return Err(AppError::Internal("Labs returned no steps".into()));
        }

        let completed: std::collections::HashSet<i32> =
            progress.completed_steps.iter().cloned().collect();

        let all_done = (1..=total_steps).all(|s| completed.contains(&s));

        if !all_done {
            return Err(AppError::Conflict("Lab not completed yet".into()));
        }

        if let Some(container_id) = session.container_id.as_deref() {
            self.stop_runtime_container_best_effort(container_id)
                .await?;
        }

        let mut tx = self
            .db
            .begin()
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        if let Some(runtime_id) = session.current_runtime_id {
            self.finalize_runtime_in_tx(&mut tx, runtime_id, "stopped")
                .await?;
            self.clear_current_runtime(&mut tx, session_id).await?;
        }

        sqlx::query(
            r#"
            UPDATE lab_sessions
            SET
                status = 'completed',
                completed_at = COALESCE(completed_at, NOW()),
                last_activity_at = NOW()
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        self.mark_lab_finished(session.user_id, session.lab_id, session.session_id)
            .await?;

        let total_attempts = progress
            .attempts_per_step
            .as_object()
            .map(|m| m.values().filter_map(|v| v.as_i64()).sum::<i64>())
            .unwrap_or(0);

        let hints_used = progress
            .hints_used
            .as_array()
            .map(|a| a.len() as i64)
            .unwrap_or(0);

        let completion_seconds = self.total_runtime_seconds(session_id).await?;

        Ok(serde_json::json!({
            "completed": true,
            "final_score": progress.score,
            "max_score": progress.max_score,
            "completion_time_seconds": completion_seconds,
            "hints_used": hints_used,
            "total_attempts": total_attempts
        }))
    }

    pub async fn fetch_lab_creator_id(&self, lab_id: Uuid) -> Result<Uuid, AppError> {
        crate::services::labs_client::fetch_lab_creator_id(self.labs_ms_base.as_str(), lab_id).await
    }

    async fn reconcile_runtime_row(&self, row: SessionRow) -> Result<SessionRow, AppError> {
        let Some(runtime_id) = row.current_runtime_id else {
            return Ok(row);
        };

        let Some(container_id) = row.container_id.as_deref() else {
            let mut tx = self
                .db
                .begin()
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;

            self.finalize_runtime_in_tx(&mut tx, runtime_id, "error")
                .await?;
            self.clear_current_runtime(&mut tx, row.session_id).await?;

            if row.status != "completed" {
                self.mark_session_in_progress(&mut tx, row.session_id)
                    .await?;
            }

            tx.commit()
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;

            return self.load_session_row(row.session_id).await;
        };

        let Some(runtime_status) = self.fetch_runtime_status(container_id).await? else {
            return Ok(row);
        };

        if Self::runtime_status_is_active(&runtime_status) {
            return Ok(row);
        }

        let mut tx = self
            .db
            .begin()
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        self.finalize_runtime_in_tx(
            &mut tx,
            runtime_id,
            Self::runtime_status_to_final_state(&runtime_status),
        )
        .await?;
        self.clear_current_runtime(&mut tx, row.session_id).await?;

        if row.status != "completed" {
            self.mark_session_in_progress(&mut tx, row.session_id)
                .await?;
        }

        tx.commit()
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        self.load_session_row(row.session_id).await
    }

    async fn reconcile_runtime_row_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        row: SessionRow,
    ) -> Result<SessionRow, AppError> {
        let Some(runtime_id) = row.current_runtime_id else {
            return Ok(row);
        };

        let Some(container_id) = row.container_id.as_deref() else {
            self.finalize_runtime_in_tx(tx, runtime_id, "error").await?;
            self.clear_current_runtime(tx, row.session_id).await?;

            if row.status != "completed" {
                self.mark_session_in_progress(tx, row.session_id).await?;
            }

            return self.load_session_row_in_tx(tx, row.session_id).await;
        };

        let Some(runtime_status) = self.fetch_runtime_status(container_id).await? else {
            return Ok(row);
        };

        if Self::runtime_status_is_active(&runtime_status) {
            return Ok(row);
        }

        self.finalize_runtime_in_tx(
            tx,
            runtime_id,
            Self::runtime_status_to_final_state(&runtime_status),
        )
        .await?;
        self.clear_current_runtime(tx, row.session_id).await?;

        if row.status != "completed" {
            self.mark_session_in_progress(tx, row.session_id).await?;
        }

        self.load_session_row_in_tx(tx, row.session_id).await
    }

    async fn fetch_runtime_status(&self, container_id: &str) -> Result<Option<String>, AppError> {
        let url = self
            .lab_api_base
            .join(&format!("spawn/status/{container_id}"))
            .map_err(|e| AppError::Internal(format!("Invalid lab-api URL: {e}")))?;

        let response = match self.client.get(url).send().await {
            Ok(response) => response,
            Err(_) => return Ok(None),
        };

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(Some("Unknown".to_string()));
        }

        if !response.status().is_success() {
            eprintln!(
                "Runtime status lookup failed for {}: HTTP {}",
                container_id,
                response.status()
            );
            return Ok(None);
        }

        let payload = match response.json::<RuntimeStatusResponse>().await {
            Ok(payload) => payload,
            Err(error) => {
                eprintln!(
                    "Runtime status lookup returned invalid payload for {}: {}",
                    container_id, error
                );
                return Ok(None);
            }
        };

        Ok(Some(payload.status))
    }

    fn runtime_status_is_active(status: &str) -> bool {
        status.eq_ignore_ascii_case("running")
    }

    fn runtime_status_to_final_state(status: &str) -> &'static str {
        if status.eq_ignore_ascii_case("succeeded") {
            "stopped"
        } else {
            "error"
        }
    }
}

#[derive(Deserialize)]
struct SpawnResponse {
    #[allow(dead_code)]
    success: bool,
    data: SpawnResponseData,
}

#[derive(Deserialize)]
struct SpawnResponseData {
    container_id: String,
    webshell_url: Option<String>,
    app_url: Option<String>,
    #[allow(dead_code)]
    status: String,
}

#[derive(Deserialize)]
struct RuntimeStatusResponse {
    status: String,
}

#[cfg(test)]
mod tests {
    use super::{extract_runtime_app_port, SessionsService};
    use serde_json::json;

    #[test]
    fn web_delivery_requires_runtime_app_port() {
        // Web sessions must fail early here if labs-ms data is still incomplete.
        let lab_data = json!({
            "runtime": {
                "app_port": null
            }
        });

        let error = extract_runtime_app_port(&lab_data, "web").unwrap_err();

        assert_eq!(
            error.to_string(),
            "Internal error: Lab runtime.app_port missing for web delivery"
        );
    }

    #[test]
    fn terminal_delivery_keeps_optional_runtime_app_port() {
        // Terminal sessions do not depend on a public HTTP entrypoint.
        let lab_data = json!({
            "runtime": {
                "app_port": null
            }
        });

        assert_eq!(
            extract_runtime_app_port(&lab_data, "terminal").unwrap(),
            None
        );
    }

    #[test]
    fn integer_runtime_app_port_is_forwarded() {
        // sessions-ms only needs to preserve the explicit integer port from labs-ms.
        let lab_data = json!({
            "runtime": {
                "app_port": 3000
            }
        });

        assert_eq!(
            extract_runtime_app_port(&lab_data, "web").unwrap(),
            Some(3000)
        );
    }

    #[test]
    fn running_runtime_status_is_considered_active() {
        assert!(SessionsService::runtime_status_is_active("Running"));
    }

    #[test]
    fn unknown_runtime_status_is_not_considered_active() {
        assert!(!SessionsService::runtime_status_is_active("Unknown"));
    }
}
