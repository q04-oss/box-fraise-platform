// Domains are added incrementally as they are ported from the TypeScript server.
// Each domain follows the pattern:
//   mod.rs        — re-exports
//   routes.rs     — axum handlers, Router<AppState>
//   service.rs    — business logic
//   repository.rs — sqlx queries
//   types.rs      — request/response structs, domain models
