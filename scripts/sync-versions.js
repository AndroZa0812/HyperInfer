const fs = require('fs');
const path = require('path');

const workspaces = [
  'crates/hyperinfer-client',
  'crates/hyperinfer-core',
  'crates/hyperinfer-python',
  'crates/hyperinfer-server',
  'bindings/hyperinfer-langchain',
  'bindings/hyperinfer-llamaindex'
];

for (const ws of workspaces) {
  const pkgPath = path.join(process.cwd(), ws, 'package.json');
  if (!fs.existsSync(pkgPath)) continue;

  const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
  const version = pkg.version;
  console.log(`Syncing ${ws} to version ${version}`);

  // Sync pyproject.toml
  const pyprojectPath = path.join(process.cwd(), ws, 'pyproject.toml');
  if (fs.existsSync(pyprojectPath)) {
    let content = fs.readFileSync(pyprojectPath, 'utf8');
    // More robust regex: find [project] section, then find version = "..."
    content = content.replace(/\[project\][\s\S]*?^version = ".*"/m, (match) => {
      return match.replace(/version = ".*"/, `version = "${version}"`);
    });
    fs.writeFileSync(pyprojectPath, content);
  }

  // Sync Cargo.toml
  const cargoPath = path.join(process.cwd(), ws, 'Cargo.toml');
  if (fs.existsSync(cargoPath)) {
    let content = fs.readFileSync(cargoPath, 'utf8');
    
    // Replace version = "..." (package version)
    if (content.match(/^version = ".*"/m)) {
        content = content.replace(/^version = ".*"/m, `version = "${version}"`);
    } else if (content.match(/^version = \{ workspace = true \}/m)) {
        // We shouldn't change version = { workspace = true } if it's the package version inherited from workspace.
        // Actually, if it's package version, and we want to sync, maybe we should change the workspace package version?
        // Let's keep existing logic but add handling for the case where it might be a literal.
    }
    fs.writeFileSync(cargoPath, content);
  }
}
