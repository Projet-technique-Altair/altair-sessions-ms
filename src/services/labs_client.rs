/**
 * @file labs_client — Labs MS client helpers.
 *
 * @remarks
 * Provides small HTTP client helpers used by the Sessions microservice
 * to retrieve lab-related metadata from Labs MS without duplicating that
 * data locally.
 *
 * Responsibilities:
 *
 *  - Build validated Labs MS URLs
 *  - Fetch lab metadata from Labs MS
 *  - Extract the creator identifier of a lab
 *  - Convert unreachable or invalid Labs MS responses into `AppError`
 *  - Enforce access failure when Labs MS does not return a success status
 *
 * Key characteristics:
 *
 *  - Uses `LABS_MS_URL`-compatible base URLs
 *  - Uses `reqwest` for outbound HTTP calls
 *  - Deserializes the standard API response envelope
 *  - Returns UUID-based creator ownership data
 *  - Keeps Labs MS integration logic isolated from route handlers
 *
 * This module acts as a lightweight boundary between Sessions MS and
 * Labs MS for lab ownership lookups.
 *
 * @packageDocumentation
 */

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
