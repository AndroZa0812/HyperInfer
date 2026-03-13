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

const rootCargoPath = path.join(process.cwd(), 'Cargo.toml');

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
    // Targeted replacement within [project] section
    content = content.replace(/\[project\][\s\S]*?^version = ".*"/m, (match) => {
      return match.replace(/version = ".*"/, `version = "${version}"`);
    });
    fs.writeFileSync(pyprojectPath, content);
  }

  // Sync Cargo.toml
  const cargoPath = path.join(process.cwd(), ws, 'Cargo.toml');
  if (fs.existsSync(cargoPath)) {
    let content = fs.readFileSync(cargoPath, 'utf8');
    
    // 1. Try replacing literal version
    if (content.match(/^version = ".*"/m)) {
        content = content.replace(/^version = ".*"/m, `version = "${version}"`);
    } 
    // 2. Fallback: replace version = { workspace = true }
    else if (content.match(/^version = \{ workspace = true \}/m)) {
        // Option A: Update root workspace package version (affects all)
        if (fs.existsSync(rootCargoPath)) {
            let rootContent = fs.readFileSync(rootCargoPath, 'utf8');
            rootContent = rootContent.replace(/\[workspace\.package\][\s\S]*?^version = ".*"/m, (match) => {
                return match.replace(/version = ".*"/, `version = "${version}"`);
            });
            fs.writeFileSync(rootCargoPath, rootContent);
        }
        // Option B: Optionally convert to literal in module (commented out as per instruction "or" choice)
        // content = content.replace(/^version = \{ workspace = true \}/m, `version = "${version}"`);
    }
    fs.writeFileSync(cargoPath, content);
  }
}
