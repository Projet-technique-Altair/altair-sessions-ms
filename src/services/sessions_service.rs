use reqwest::Client;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use url::Url;
use uuid::Uuid;

use crate::{
    error::AppError,
    models::lab_progress::{LabProgress, LabProgressRow},
    models::learner_lab_status::{
        LearnerDashboardLab, LearnerLabStatus, LearnerLabStatusKind, LearnerLabStatusRow,
    },
    models::session::{RuntimeLookup, Session, SessionRow, SessionStatus},
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
}

use serde_json::Value;

#[derive(Serialize)]
pub struct SessionWithSteps {
    #[serde(flatten)]
    pub session: Session,
    pub steps: Vec<Value>,
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

        Self {
            db,
            client: Client::new(),
            lab_api_base,
            labs_ms_base,
        }
    }

    /// Creates or refreshes a TO DO relation for a learner on a public lab.
    pub async fn follow_lab(
        &self,
        user_id: Uuid,
        lab_id: Uuid,
    ) -> Result<LearnerLabStatus, AppError> {
        let lab = self.fetch_lab_overview(lab_id).await?;
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

    /// Marks a learner lab as active when a runtime starts or resumes, but preserves FINISHED.
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
                status = CASE
                    WHEN learner_lab_status.status = 'finished' THEN learner_lab_status.status
                    ELSE 'in_progress'
                END,
                followed_at = learner_lab_status.followed_at,
                started_at = COALESCE(learner_lab_status.started_at, EXCLUDED.started_at),
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
        let mut tx = self
            .db
            .begin()
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        // Keep one active runtime per learner/lab pair and treat a second start as a resume.
        let existing = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM lab_sessions
            WHERE user_id = $1
            AND lab_id = $2
            AND status IN ('created', 'running')
            "#,
        )
        .bind(user_id)
        .bind(lab_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        /*if existing > 0 {
            return Err(AppError::Conflict("Session already active".into()));
        }*/

        if existing > 0 {
            let row = sqlx::query_as::<_, SessionRow>(
                r#"
                SELECT *
                FROM lab_sessions
                WHERE user_id = $1
                AND lab_id = $2
                AND status IN ('created', 'running')
                ORDER BY created_at DESC
                LIMIT 1
                "#,
            )
            .bind(user_id)
            .bind(lab_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

            let session = Session::try_from(row)?;

            if track_learner_status {
                // A resumed session still counts as fresh learner activity for the dashboard.
                self.upsert_lab_status_for_start(&mut tx, user_id, lab_id, session.session_id)
                    .await?;
            }

            tx.commit()
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;

            // "Start" intentionally behaves as "start or resume" in the current session-centric model.
            return Ok(session);
        }

        // Create the runtime session record before calling external services.
        let row = sqlx::query_as::<_, SessionRow>(
            r#"
            INSERT INTO lab_sessions (
                user_id,
                lab_id,
                status
            )
            VALUES ($1, $2, 'created')
            RETURNING *
            "#,
        )
        .bind(user_id)
        .bind(lab_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        let session = Session::try_from(row)?;

        // Progress is still stored per session in the current implementation.
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
        .bind(session.session_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        // The maximum reachable score comes from labs-ms step metadata.
        let url = self
            .labs_ms_base
            .join(&format!("internal/labs/{}/steps", lab_id))
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

        // Persist the computed max score on the freshly created session progress row.
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
        .bind(session.session_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        // Resolve the runtime build metadata just before spawning the lab container.
        let url = self
            .labs_ms_base
            .join(&format!("labs/{}", lab_id))
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

        // The runtime is provisioned only after the DB rows exist.
        /*let spawn_result = self
        .client
        .post(format!("{}/spawn", self.lab_api_url))
        .json(&serde_json::json!({
            "session_id": session.session_id,
            "lab_type": lab_type,
            "template_path": template_path
        }))
        .send()
        .await;*/

        let url = self
            .lab_api_base
            .join("spawn")
            .map_err(|e| AppError::Internal(format!("Invalid lab-api URL: {e}")))?;

        let spawn_result = self
            .client
            .post(url)
            .json(&serde_json::json!({
                "session_id": session.session_id,
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

                // The DB remains the source of truth for runtime session state transitions.
                Self::validate_transition(session.status, SessionStatus::Running)?;

                sqlx::query(
                    r#"
                    UPDATE lab_sessions
                    SET
                        status = 'running',
                        container_id = $1,
                        runtime_kind = $2,
                        webshell_url = $3,
                        app_url = $4
                    WHERE session_id = $5
                    "#,
                )
                .bind(&spawn.data.container_id)
                .bind(&spawn.data.runtime_kind)
                .bind(&spawn.data.webshell_url)
                .bind(&spawn.data.app_url)
                .bind(session.session_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;

                if track_learner_status {
                    // Learner tracking is written only after the runtime is actually usable.
                    self.upsert_lab_status_for_start(&mut tx, user_id, lab_id, session.session_id)
                        .await?;
                }
            }

            Ok(resp) => {
                // Non-success status code
                let error_body = resp.text().await.unwrap_or_default();
                eprintln!("Lab API spawn failed: {}", error_body);

                // Persist the failure so the caller never sees a phantom running session.
                Self::validate_transition(session.status, SessionStatus::Error)?;

                sqlx::query(
                    r#"
                    UPDATE lab_sessions
                    SET status = 'error'
                    WHERE session_id = $1
                    "#,
                )
                .bind(session.session_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;

                tx.commit()
                    .await
                    .map_err(|e| AppError::Internal(e.to_string()))?;

                return Err(AppError::Internal("Failed to spawn container".into()));
            }

            Err(e) => {
                eprintln!("Lab API unreachable: {}", e);

                // Persist the failure so the caller never sees a phantom running session.
                Self::validate_transition(session.status, SessionStatus::Error)?;

                sqlx::query(
                    r#"
                    UPDATE lab_sessions
                    SET status = 'error'
                    WHERE session_id = $1
                    "#,
                )
                .bind(session.session_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;

                tx.commit()
                    .await
                    .map_err(|e| AppError::Internal(e.to_string()))?;

                return Err(AppError::Internal("Lab API service unreachable".into()));
            }
        }

        tx.commit()
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        // Reload after commit to return the final session shape, including runtime URLs.
        self.get_session_by_id(session.session_id).await
    }

    /// DELETE /sessions/:id
    pub async fn stop_session(&self, session_id: Uuid) -> Result<(), AppError> {
        // 1️⃣ Load session
        let row = sqlx::query_as::<_, SessionRow>(
            r#"
            SELECT *
            FROM lab_sessions
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .fetch_one(&self.db)
        .await
        .map_err(|_| AppError::NotFound("Session not found".into()))?;

        let session = Session::try_from(row)?;

        // 2️⃣ Terminal states → idempotent OK
        if Self::is_terminal(session.status) {
            return Ok(());
        }

        // 3️⃣ CREATED → STOPPED interdit
        if session.status == SessionStatus::Created {
            return Err(AppError::Conflict(
                "Cannot stop a session that has not started".into(),
            ));
        }

        // 4️⃣ RUNNING → STOPPED (唯一 transition autorisée ici)
        Self::validate_transition(session.status, SessionStatus::Stopped)?;

        // 5️⃣ Stop container (best effort)
        /*if let Some(container_id) = &session.container_id {
            self.client
                .post(format!("{}/spawn/stop", self.lab_api_url))
                .json(&serde_json::json!({
                    "container_id": container_id
                }))
                .send()
                .await
                .map_err(|e| AppError::Internal(format!("Stop call failed: {e}")))?;
        }*/

        if let Some(container_id) = &session.container_id {
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
        }

        // 6️⃣ Update DB
        sqlx::query(
            r#"
            UPDATE lab_sessions
            SET status = 'stopped'
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .execute(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(())
    }

    /// GET /sessions/:id
    pub async fn get_session_by_id(&self, session_id: Uuid) -> Result<Session, AppError> {
        let row = sqlx::query_as::<_, SessionRow>(
            r#"
            SELECT *
            FROM lab_sessions
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .fetch_one(&self.db)
        .await
        .map_err(|_| AppError::NotFound("Session not found".into()))?;

        Ok(Session::try_from(row)?)
    }

    pub async fn get_active_runtime_by_container_id(
        &self,
        container_id: &str,
    ) -> Result<RuntimeLookup, AppError> {
        let row = sqlx::query_as::<_, SessionRow>(
            r#"
            SELECT *
            FROM lab_sessions
            WHERE container_id = $1
              AND status IN ('created', 'running')
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(container_id)
        .fetch_one(&self.db)
        .await
        .map_err(|_| AppError::NotFound("Runtime not found".into()))?;

        let session = Session::try_from(row)?;
        let runtime_kind = session
            .runtime_kind
            .clone()
            .ok_or_else(|| AppError::NotFound("Runtime kind missing".into()))?;
        let container_id = session
            .container_id
            .clone()
            .ok_or_else(|| AppError::NotFound("Container id missing".into()))?;

        // The web bootstrap only needs the live runtime ownership fields, not
        // the full learner session payload.
        Ok(RuntimeLookup {
            session_id: session.session_id,
            user_id: session.user_id,
            container_id,
            status: session.status,
            runtime_kind,
            app_url: session.app_url,
        })
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

    // GET /sessions/lab/:id
    pub async fn get_sessions_by_lab(&self, lab_id: Uuid) -> Result<Vec<Session>, AppError> {
        let rows = sqlx::query_as::<_, SessionRow>(
            r#"
            SELECT *
            FROM lab_sessions
            WHERE lab_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(lab_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        rows.into_iter().map(Session::try_from).collect()
    }

    //GET /sessions/user/:id
    pub async fn get_sessions_by_user(&self, user_id: Uuid) -> Result<Vec<Session>, AppError> {
        let rows = sqlx::query_as::<_, SessionRow>(
            r#"
            SELECT *
            FROM lab_sessions
            WHERE user_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        rows.into_iter().map(Session::try_from).collect()
    }

    fn validate_transition(from: SessionStatus, to: SessionStatus) -> Result<(), AppError> {
        use SessionStatus::*;

        let allowed = matches!(
            (from, to),
            (Created, Running)
                | (Created, Error)
                | (Running, Stopped)
                | (Running, Expired)
                | (Running, Error)
        );

        if allowed {
            Ok(())
        } else {
            Err(AppError::Conflict(format!(
                "Invalid session state transition: {:?} -> {:?}",
                from, to
            )))
        }
    }

    fn is_terminal(status: SessionStatus) -> bool {
        matches!(
            status,
            SessionStatus::Stopped | SessionStatus::Expired | SessionStatus::Error
        )
    }

    //EXPIRE SESSION
    pub async fn expire_session(&self, session_id: Uuid) -> Result<(), AppError> {
        // 1️⃣ Load session
        let row = sqlx::query_as::<_, SessionRow>(
            r#"
            SELECT *
            FROM lab_sessions
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .fetch_one(&self.db)
        .await
        .map_err(|_| AppError::NotFound("Session not found".into()))?;

        let session = Session::try_from(row)?;

        // 2️⃣ Terminal states → idempotent OK
        if Self::is_terminal(session.status) {
            return Ok(());
        }

        // 3️⃣ Only RUNNING → EXPIRED allowed
        if session.status != SessionStatus::Running {
            return Ok(()); // idempotence (CREATED, etc.)
        }

        // 4️⃣ Validate transition
        Self::validate_transition(session.status, SessionStatus::Expired)?;

        // 5️⃣ Stop container (best effort)
        /*if let Some(container_id) = &session.container_id {
            let _ = self
                .client
                .post(format!("{}/spawn/stop", self.lab_api_url))
                .json(&serde_json::json!({
                    "container_id": container_id
                }))
                .send()
                .await;
            // ⚠️ best effort: expiration must proceed even if runtime is down
        }*/

        if let Some(container_id) = &session.container_id {
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
        }

        // 6️⃣ Update DB
        sqlx::query(
            r#"
            UPDATE lab_sessions
            SET status = 'expired',
                expires_at = NOW()
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .execute(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(())
    }

    // CRON
    pub async fn expire_all_expired_sessions(&self) -> Result<usize, AppError> {
        // 1️⃣ Sélectionner les sessions RUNNING dépassant le timeout
        let rows = sqlx::query_as::<_, SessionRow>(
            r#"
            SELECT *
            FROM lab_sessions
            WHERE status = 'running'
            AND (
                created_at + INTERVAL '2 hours'
            ) < NOW()
            "#,
        )
        .fetch_all(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        let mut expired_count = 0;

        // 2️⃣ Appliquer expiration une par une (idempotent)
        for row in rows {
            let session = Session::try_from(row)?;

            // sécurité supplémentaire
            if session.status != SessionStatus::Running {
                continue;
            }

            // réutilise ta logique existante
            if self.expire_session(session.session_id).await.is_ok() {
                expired_count += 1;
            }
        }

        Ok(expired_count)
    }

    // ======================================================
    // GET /sessions/:id/progress
    // ======================================================
    pub async fn get_progress(&self, session_id: Uuid) -> Result<LabProgress, AppError> {
        // 1️⃣ Charger la progression
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

        // 2️⃣ Charger created_at de la session
        let session_created_at = sqlx::query_scalar::<_, chrono::NaiveDateTime>(
            r#"
            SELECT created_at
            FROM lab_sessions
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .fetch_one(&self.db)
        .await
        .map_err(|_| AppError::NotFound("Session not found".into()))?;

        // 3️⃣ Construire la réponse API
        Ok(LabProgress::from_row(progress_row, session_created_at))
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
        // 1️⃣ Charger la session
        let session_row = sqlx::query_as::<_, SessionRow>(
            r#"
            SELECT *
            FROM lab_sessions
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .fetch_one(&self.db)
        .await
        .map_err(|_| AppError::NotFound("Session not found".into()))?;

        let session = Session::try_from(session_row)?;

        // 2️⃣ Charger la progression
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

        // 3️⃣ Récupérer le nombre total de steps depuis Labs MS
        // Hypothèse: GET /labs/:lab_id/steps -> renvoie { "steps": [ ... ] }
        /*let lab = self.client
        .get(format!("{}/labs/{}/steps", self.lab_api_url, session.lab_id))
        .send()
        .await
        .map_err(|_| AppError::Internal("Labs MS unreachable".into()))?
        .json::<serde_json::Value>()
        .await
        .map_err(|_| AppError::Internal("Invalid Labs response".into()))?;*/

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

        // 4️⃣ Vérifier completion: toutes les steps 1..=total_steps doivent être dans completed_steps
        let completed: std::collections::HashSet<i32> =
            progress.completed_steps.iter().cloned().collect();

        let all_done = (1..=total_steps).all(|s| completed.contains(&s));

        if !all_done {
            return Err(AppError::Conflict("Lab not completed yet".into()));
        }

        // 5️⃣ Stop pod (best effort)
        /*if let Some(container_id) = &session.container_id {
            let _ = self
                .client
                .post(format!("{}/spawn/stop", self.lab_api_url))
                .json(&serde_json::json!({ "container_id": container_id }))
                .send()
                .await;
        }*/

        if let Some(container_id) = &session.container_id {
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
        }

        // Runtime completion and learner completion are two different concepts:
        // the session is stopped, while the learner-lab relation becomes FINISHED.
        if session.status == SessionStatus::Running {
            sqlx::query(
                r#"
                UPDATE lab_sessions
                SET status = 'stopped'
                WHERE session_id = $1
                "#,
            )
            .bind(session_id)
            .execute(&self.db)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
        }

        // Persist the product-level completion state after the completion checks passed.
        self.mark_lab_finished(session.user_id, session.lab_id, session.session_id)
            .await?;

        // 7️⃣ Stats
        // total_attempts = somme des valeurs de attempts_per_step
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

        let now = chrono::Utc::now().naive_utc();
        let completion_seconds = (now - session.created_at).num_seconds().max(0);

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
    runtime_kind: String,
    webshell_url: Option<String>,
    app_url: Option<String>,
    #[allow(dead_code)]
    status: String,
}

#[cfg(test)]
mod tests {
    use super::extract_runtime_app_port;
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
}
