use reqwest::Client;
use uuid::Uuid;

use crate::error::AppError;

#[derive(serde::Deserialize)]
struct ApiResponse<T> {
    data: T,
}

#[derive(serde::Deserialize)]
struct LabCreatorData {
    creator_id: Uuid,
}

// GET {LABS_MS_URL}/labs/{lab_id} et on ne lit que creator_id
pub async fn fetch_lab_creator_id(labs_ms_url: &str, lab_id: Uuid) -> Result<Uuid, AppError> {
    let url = format!("{}/labs/{}", labs_ms_url.trim_end_matches('/'), lab_id);

    let resp = Client::new()
        .get(url)
        .send()
        .await
        .map_err(|_| AppError::Internal("Labs MS unreachable".into()))?;

    if !resp.status().is_success() {
        return Err(AppError::Forbidden("Cannot access lab".into()));
    }

    // Ça parse même si le JSON contient plein d’autres champs : serde ignore le reste
    let body: ApiResponse<LabCreatorData> = resp
        .json()
        .await
        .map_err(|_| AppError::Internal("Invalid Labs response".into()))?;

    Ok(body.data.creator_id)
}
