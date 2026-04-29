// Background jobs are added here once the domains they depend on exist.
//
// Schedule (matching the TypeScript server):
//   02:00 daily    — expire active employment contracts
//   08:00 daily    — process standing orders + send daily summary email
//   09:00 daily    — send membership renewal reminders (30-day threshold)
//   01:00 Jan 1    — reset membership fund balances, expire memberships
