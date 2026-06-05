const fs = require('fs');
const path = require('path');
const { execFileSync } = require('child_process');

const packageNames = process.argv[2];
const bumpType = process.argv[3] || 'patch';

if (!packageNames) {
  console.error('Usage: node bump-versions.js "pkg1,pkg2" [patch|minor|major]');
  process.exit(1);
}

const packages = packageNames.split(',').map(s => s.trim()).filter(Boolean);

const PACKAGE_DIRS = {
  'hyperinfer-python': 'crates/hyperinfer-python',
  'hyperinfer-langchain': 'bindings/hyperinfer-langchain',
  'hyperinfer-llamaindex': 'bindings/hyperinfer-llamaindex',
};

const bumped = [];

for (const name of packages) {
  const dir = PACKAGE_DIRS[name];
  if (!dir) {
    console.error(`Unknown package: ${name}. Available: ${Object.keys(PACKAGE_DIRS).join(', ')}`);
    process.exit(1);
  }

  const pkgJsonPath = path.join(process.cwd(), dir, 'package.json');
  if (!fs.existsSync(pkgJsonPath)) {
    console.error(`package.json not found: ${pkgJsonPath}`);
    continue;
  }

  const pkg = JSON.parse(fs.readFileSync(pkgJsonPath, 'utf8'));
  const [major, minor, patch] = pkg.version.split('.').map(Number);

  let newVersion;
  switch (bumpType) {
    case 'patch': newVersion = `${major}.${minor}.${patch + 1}`; break;
    case 'minor': newVersion = `${major}.${minor + 1}.0`; break;
    case 'major': newVersion = `${major + 1}.0.0`; break;
    default: newVersion = bumpType; break;
  }

  pkg.version = newVersion;
  fs.writeFileSync(pkgJsonPath, JSON.stringify(pkg, null, 2) + '\n');
  console.error(`Bumped ${name}: ${major}.${minor}.${patch} -> ${newVersion}`);
  bumped.push(name);
}

if (bumped.length > 0) {
  const result = execFileSync('bun', ['run', 'scripts/sync-versions.js']).toString();
  console.error(result.trim());
}

console.log(JSON.stringify(bumped));
