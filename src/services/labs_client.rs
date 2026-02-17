use reqwest::Client;
use uuid::Uuid;
use url::Url;

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
/*pub async fn fetch_lab_creator_id(labs_ms_url: &str, lab_id: Uuid) -> Result<Uuid, AppError> {
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
}*/

pub async fn fetch_lab_creator_id(
    labs_ms_url: &str,
    lab_id: Uuid,
) -> Result<Uuid, AppError> {

    // 1️⃣ Parse base URL
    let base = Url::parse(labs_ms_url)
        .map_err(|_| AppError::Internal("Invalid LABS_MS_URL".into()))?;

    // 2️⃣ Join path safely
    let url = base
        .join(&format!("labs/{}", lab_id))
        .map_err(|_| AppError::Internal("Invalid Labs URL join".into()))?;

    // 3️⃣ HTTP request
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

