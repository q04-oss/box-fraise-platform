/// Staff web app — a PWA served from the API binary.
///
/// Staff authenticate with their PIN at /staff/login, receive an HttpOnly
/// session cookie (StaffClaims JWT), then use /staff/scan to stamp customer
/// QR codes. The cookie is never readable by JavaScript.
///
/// Installable as a homescreen app via the PWA manifest at /staff/manifest.
pub mod routes;
