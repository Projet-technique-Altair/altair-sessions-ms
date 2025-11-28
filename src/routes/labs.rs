use axum::{Router, routing::get, Json};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
pub struct Lab {
    pub lab_id: String,
    pub name: String,
    pub description: String,
}

pub fn labs_routes() -> Router {
    Router::new().route("/labs", get(get_labs))
}

async fn get_labs() -> Json<Vec<Lab>> {
    Json(vec![
        Lab {
            lab_id: "lab-1".into(),
            name: "Linux Basics".into(),
            description: "A basic Linux lab".into(),
        }
    ])
}
