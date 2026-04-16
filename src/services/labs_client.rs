use reqwest::Client;
use url::Url;
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

pub async fn fetch_lab_creator_id(labs_ms_url: &str, lab_id: Uuid) -> Result<Uuid, AppError> {
    let base =
        Url::parse(labs_ms_url).map_err(|_| AppError::Internal("Invalid LABS_MS_URL".into()))?;

    let url = base
        .join(&format!("labs/{}", lab_id))
        .map_err(|_| AppError::Internal("Invalid Labs URL join".into()))?;

    let resp = Client::new()
        .get(url)
        .send()
        .await
        .map_err(|_| AppError::Internal("Labs MS unreachable".into()))?;

    if !resp.status().is_success() {
        return Err(AppError::Forbidden("Cannot access lab".into()));
    }

    let body: ApiResponse<LabCreatorData> = resp
        .json()
        .await
        .map_err(|_| AppError::Internal("Invalid Labs response".into()))?;

    Ok(body.data.creator_id)
}
