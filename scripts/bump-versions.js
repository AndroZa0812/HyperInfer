const fs = require('fs');
const path = require('path');
const { execFileSync } = require('child_process');

const packageNames = process.argv[2];
const bumpType = process.argv[3] || 'patch';
const extraArgs = process.argv.slice(4);

if (!packageNames) {
  console.error(
    'Usage: node scripts/bump-versions.js "pkg1,pkg2|all" [patch|minor|major|x.y.z] [--lock]',
  );
  process.exit(1);
}

// Authoritative source for each package:
// - Rust crates use Cargo.toml as source of truth
// - Python packages use package.json as source of truth (compatible with release.yml)
const PACKAGES = {
  'hyperinfer-core': { dir: 'crates/hyperinfer-core', source: 'cargo' },
  'hyperinfer-providers': { dir: 'crates/hyperinfer-providers', source: 'cargo' },
  'hyperinfer-router': { dir: 'crates/hyperinfer-router', source: 'cargo' },
  'hyperinfer-client': { dir: 'crates/hyperinfer-client', source: 'cargo' },
  'hyperinfer-server': { dir: 'crates/hyperinfer-server', source: 'cargo' },
  hypercode: { dir: 'crates/hypercode', source: 'cargo' },
  'hyperinfer-python': { dir: 'crates/hyperinfer-python', source: 'package' },
  'hyperinfer-langchain': { dir: 'bindings/hyperinfer-langchain', source: 'package' },
  'hyperinfer-llamaindex': { dir: 'bindings/hyperinfer-llamaindex', source: 'package' },
};

function computeNextVersion(currentVersion, bump) {
  const parts = currentVersion.split('-')[0].split('.').map(Number);
  if (parts.length !== 3 || parts.some(Number.isNaN)) {
    throw new Error(`Invalid semver version: ${currentVersion}`);
  }
  const [major, minor, patch] = parts;
  switch (bump) {
    case 'patch':
      return `${major}.${minor}.${patch + 1}`;
    case 'minor':
      return `${major}.${minor + 1}.0`;
    case 'major':
      return `${major + 1}.0.0`;
    default:
      if (!/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(bump)) {
        throw new Error(`Invalid bump type or version: ${bump}`);
      }
      return bump;
  }
}

function readCargoPackageVersion(cargoPath) {
  const content = fs.readFileSync(cargoPath, 'utf8');
  const pkgSection = content.match(/\[package\][\s\S]*?(?=\n\[|$)/);
  if (!pkgSection) {
    throw new Error(`No [package] section found in ${cargoPath}`);
  }
  const versionMatch = pkgSection[0].match(/^version\s*=\s*"([^"]+)"/m);
  if (!versionMatch) {
    throw new Error(`No package version found in ${cargoPath}`);
  }
  return versionMatch[1];
}

function writeCargoPackageVersion(cargoPath, newVersion) {
  const content = fs.readFileSync(cargoPath, 'utf8');
  const updated = content.replace(/(\[package\][\s\S]*?^version\s*=\s*")([^"]+)(")/m, `$1${newVersion}$3`);
  fs.writeFileSync(cargoPath, updated);
}

const requested =
  packageNames.trim() === 'all'
    ? Object.keys(PACKAGES)
    : packageNames
        .split(',')
        .map((s) => s.trim())
        .filter(Boolean);

const bumped = [];

for (const name of requested) {
  const spec = PACKAGES[name];
  if (!spec) {
    console.error(
      `Unknown package: ${name}. Available: ${Object.keys(PACKAGES).join(', ')}, or "all"`,
    );
    process.exit(1);
  }

  const pkgDir = path.join(process.cwd(), spec.dir);
  if (spec.source === 'cargo') {
    const cargoPath = path.join(pkgDir, 'Cargo.toml');
    if (!fs.existsSync(cargoPath)) {
      console.error(`Cargo.toml not found: ${cargoPath}`);
      process.exit(1);
    }
    const oldVersion = readCargoPackageVersion(cargoPath);
    const newVersion = computeNextVersion(oldVersion, bumpType);
    writeCargoPackageVersion(cargoPath, newVersion);
    console.error(`Bumped ${name}: ${oldVersion} -> ${newVersion}`);
    bumped.push(name);
  } else {
    const pkgJsonPath = path.join(pkgDir, 'package.json');
    if (!fs.existsSync(pkgJsonPath)) {
      console.error(`package.json not found: ${pkgJsonPath}`);
      process.exit(1);
    }
    const pkg = JSON.parse(fs.readFileSync(pkgJsonPath, 'utf8'));
    const oldVersion = pkg.version;
    const newVersion = computeNextVersion(oldVersion, bumpType);
    pkg.version = newVersion;
    fs.writeFileSync(pkgJsonPath, JSON.stringify(pkg, null, 2) + '\n');
    console.error(`Bumped ${name}: ${oldVersion} -> ${newVersion}`);
    bumped.push(name);
  }
}

if (bumped.length > 0) {
  const syncArgs = ['scripts/sync-versions.js', ...extraArgs.filter((a) => a === '--lock')];
  const result = execFileSync(process.execPath, syncArgs).toString();
  if (result.trim()) {
    console.error(result.trim());
  }
}

console.log(JSON.stringify(bumped));
