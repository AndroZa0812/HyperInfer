const fs = require('fs');
const path = require('path');

const workspaces = [
  'crates/hyperinfer-client',
  'crates/hyperinfer-core',
  'crates/hyperinfer-python',
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
    // Replace version = "..."
    content = content.replace(/version = ".*"/, `version = "${version}"`);
    fs.writeFileSync(pyprojectPath, content);
  }

  // Sync Cargo.toml
  const cargoPath = path.join(process.cwd(), ws, 'Cargo.toml');
  if (fs.existsSync(cargoPath)) {
    let content = fs.readFileSync(cargoPath, 'utf8');
    // Replace version = "..." only at the beginning of the line (package version)
    content = content.replace(/^version = ".*"/m, `version = "${version}"`);
    fs.writeFileSync(cargoPath, content);
  }
}
