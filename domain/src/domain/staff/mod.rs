/// Database row types, column lists, and HTTP request/response shapes.
pub mod types;
/// Database access — all SQL for staff_roles, staff_visits, quality_assessments,
/// visit_signatures, and business_assessment_history.
pub mod repository;
/// Business logic for role management, visit lifecycle, and quality assessments.
pub mod service;
