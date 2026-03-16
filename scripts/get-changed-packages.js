const { execSync } = require('child_process');

// Get changes since last tag
const statusOutput = execSync('bunx changeset status --since=origin/main --output json').toString();
const status = JSON.parse(statusOutput);

// Extract changed packages
const changedPackages = status.releases.map(r => r.name);
// Ensure it's a valid JSON array for GHA matrix
console.log(JSON.stringify(changedPackages));
