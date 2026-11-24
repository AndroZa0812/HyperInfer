const fs = require('fs');
const path = require('path');
const { execFileSync } = require('child_process');

const rootDir = process.cwd();
const updateLocks = process.argv.includes('--lock');

const RUST_CRATES = [
  { name: 'hyperinfer-core', dir: 'crates/hyperinfer-core' },
  { name: 'hyperinfer-providers', dir: 'crates/hyperinfer-providers' },
  { name: 'hyperinfer-router', dir: 'crates/hyperinfer-router' },
  { name: 'hyperinfer-client', dir: 'crates/hyperinfer-client' },
  { name: 'hyperinfer-server', dir: 'crates/hyperinfer-server' },
  { name: 'hypercode', dir: 'crates/hypercode' },
];

const PYTHON_PACKAGES = [
  { name: 'hyperinfer-python', dir: 'crates/hyperinfer-python' },
  { name: 'hyperinfer-langchain', dir: 'bindings/hyperinfer-langchain' },
  { name: 'hyperinfer-llamaindex', dir: 'bindings/hyperinfer-llamaindex' },
];

function readCargoPackageVersion(cargoPath) {
  const content = fs.readFileSync(cargoPath, 'utf8');
  const pkgSection = content.match(/\[package\][\s\S]*?(?=\n\[|$)/);
  if (!pkgSection) return null;
  const match = pkgSection[0].match(/^version\s*=\s*"([^"]+)"/m);
  return match ? match[1] : null;
}

function writeCargoPackageVersion(cargoPath, version) {
  let content = fs.readFileSync(cargoPath, 'utf8');
  if (/\[package\][\s\S]*?^version\s*=\s*"[^"]+"/m.test(content)) {
    content = content.replace(/(\[package\][\s\S]*?^version\s*=\s*")([^"]+)(")/m, `$1${version}$3`);
  } else if (/\[package\][\s\S]*?^version\s*=\s*\{\s*workspace\s*=\s*true\s*\}/m.test(content)) {
    content = content.replace(
      /(\[package\][\s\S]*?^version\s*=\s*)\{\s*workspace\s*=\s*true\s*\}/m,
      `$1"${version}"`,
    );
  }
  fs.writeFileSync(cargoPath, content);
}

function updateJsonVersion(jsonPath, version) {
  if (!fs.existsSync(jsonPath)) return;
  const raw = fs.readFileSync(jsonPath, 'utf8');
  const updated = raw.replace(/("version"\s*:\s*")([^"]+)(")/, `$1${version}$3`);
  if (updated !== raw) {
    fs.writeFileSync(jsonPath, updated);
  }
}

function updatePyprojectVersion(pyprojectPath, version) {
  if (!fs.existsSync(pyprojectPath)) return;
  const raw = fs.readFileSync(pyprojectPath, 'utf8');
  const updated = raw.replace(/(\[project\][\s\S]*?^version\s*=\s*")([^"]+)(")/m, `$1${version}$3`);
  if (updated !== raw) {
    fs.writeFileSync(pyprojectPath, updated);
  }
}

function syncInternalCargoDeps(cargoPath, crateVersions) {
  if (!fs.existsSync(cargoPath)) return;
  let content = fs.readFileSync(cargoPath, 'utf8');
  let changed = false;

  for (const [crateName, version] of Object.entries(crateVersions)) {
    const escaped = crateName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    // Match single-line or multi-line inline table dependency with path + version
    const depRe = new RegExp(
      `(^${escaped}\\s*=\\s*\\{[^}]*?version\\s*=\\s*")([^"]+)(")`,
      'gm',
    );
    const next = content.replace(depRe, `$1${version}$3`);
    if (next !== content) {
      content = next;
      changed = true;
    }
  }

  if (changed) {
    fs.writeFileSync(cargoPath, content);
  }
}

// 1. Collect authoritative Rust crate versions from Cargo.toml and sync sibling package.json
const crateVersions = {};
for (const crate of RUST_CRATES) {
  const cargoPath = path.join(rootDir, crate.dir, 'Cargo.toml');
  if (!fs.existsSync(cargoPath)) continue;
  const version = readCargoPackageVersion(cargoPath);
  if (!version) continue;
  crateVersions[crate.name] = version;
  console.log(`Syncing ${crate.dir} to version ${version}`);
  updateJsonVersion(path.join(rootDir, crate.dir, 'package.json'), version);
}

// 2. Sync root Cargo.toml [workspace.dependencies] and all crates/*/Cargo.toml internal deps
syncInternalCargoDeps(path.join(rootDir, 'Cargo.toml'), crateVersions);
for (const crate of RUST_CRATES) {
  syncInternalCargoDeps(path.join(rootDir, crate.dir, 'Cargo.toml'), crateVersions);
}

// 3. Sync hyperinfer-server OpenAPI version if present
if (crateVersions['hyperinfer-server']) {
  const routesModPath = path.join(rootDir, 'crates/hyperinfer-server/src/routes/mod.rs');
  if (fs.existsSync(routesModPath)) {
    const raw = fs.readFileSync(routesModPath, 'utf8');
    const updated = raw.replace(
      /(title\s*=\s*"HyperInfer Control Plane API",\s*\n\s*version\s*=\s*")([^"]+)(")/,
      `$1${crateVersions['hyperinfer-server']}$3`,
    );
    if (updated !== raw) {
      fs.writeFileSync(routesModPath, updated);
    }
  }
}

// 4. Sync Python packages from package.json -> pyproject.toml, Cargo.toml (hyperinfer-python), __init__.py
for (const pkg of PYTHON_PACKAGES) {
  const pkgJsonPath = path.join(rootDir, pkg.dir, 'package.json');
  if (!fs.existsSync(pkgJsonPath)) continue;
  const { version } = JSON.parse(fs.readFileSync(pkgJsonPath, 'utf8'));
  console.log(`Syncing ${pkg.dir} to version ${version}`);

  updatePyprojectVersion(path.join(rootDir, pkg.dir, 'pyproject.toml'), version);

  const cargoPath = path.join(rootDir, pkg.dir, 'Cargo.toml');
  if (fs.existsSync(cargoPath)) {
    writeCargoPackageVersion(cargoPath, version);
  }

  if (pkg.name === 'hyperinfer-python') {
    const initPath = path.join(rootDir, pkg.dir, 'python/hyperinfer/__init__.py');
    if (fs.existsSync(initPath)) {
      const raw = fs.readFileSync(initPath, 'utf8');
      const updated = raw.replace(/^(__version__\s*=\s*")([^"]+)(")/m, `$1${version}$3`);
      if (updated !== raw) {
        fs.writeFileSync(initPath, updated);
      }
    }
  }
}

// 5. Optionally refresh Cargo.lock and uv.lock when --lock is passed
if (updateLocks) {
  console.log('Updating Cargo.lock and uv.lock...');
  execFileSync('cargo', ['check', '--workspace', '--offline'], { stdio: 'inherit' });
  execFileSync('uv', ['lock', '--offline'], { stdio: 'inherit' });
}
