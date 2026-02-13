//! Planning tool request/response types for Product/Project/Plan management.

use serde::{Deserialize, Serialize};

// ============================================================================
// Product Tools
// ============================================================================

/// Request to create a product.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateProductRequest {
    pub name: String,
    pub description: String,
}

/// Response from create_product.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateProductResponse {
    pub product_id: String,
}

/// Product info in list response.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProductInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub status: String,
    pub created_at: String,
}

/// Request to list products.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListProductsRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// Response from list_products.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListProductsResponse {
    pub products: Vec<ProductInfo>,
}

// ============================================================================
// Project Tools
// ============================================================================

/// Request to create a project.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateProjectRequest {
    pub product_id: String,
    pub name: String,
    pub description: String,
}

/// Response from create_project.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateProjectResponse {
    pub project_id: String,
}

/// Project info in list response.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProjectInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub status: String,
    pub product_id: String,
    pub created_at: String,
}

/// Request to list projects.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListProjectsRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// Response from list_projects.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListProjectsResponse {
    pub projects: Vec<ProjectInfo>,
}

// ============================================================================
// Plan Tools
// ============================================================================

/// Request to create a plan.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreatePlanRequest {
    pub project_id: String,
    pub name: String,
    pub strategy: String,
}

/// Response from create_plan.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreatePlanResponse {
    pub plan_id: String,
}

/// Plan info in list response.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlanInfo {
    pub id: String,
    pub name: String,
    pub strategy: String,
    pub status: String,
    pub project_id: String,
    pub created_at: String,
}

/// Request to list plans.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListPlansRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// Response from list_plans.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListPlansResponse {
    pub plans: Vec<PlanInfo>,
}
