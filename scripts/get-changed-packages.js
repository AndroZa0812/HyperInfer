const { execFileSync } = require('child_process');
const fs = require('fs');
const path = require('path');

const PUBLISHABLE_PACKAGES = [
  { name: 'hyperinfer-python', dir: 'crates/hyperinfer-python' },
  { name: 'hyperinfer-langchain', dir: 'bindings/hyperinfer-langchain' },
  { name: 'hyperinfer-llamaindex', dir: 'bindings/hyperinfer-llamaindex' },
];

try {
  let lastTag;
  try {
    lastTag = execFileSync('git', ['describe', '--tags', '--abbrev=0', '--match', 'v*']).toString().trim();
  } catch {
    lastTag = execFileSync('git', ['rev-list', '--max-parents=0', 'HEAD']).toString().trim();
  }

  const diffOutput = execFileSync('git', ['diff', '--name-only', lastTag, 'HEAD']).toString();
  const changedFiles = diffOutput.trim().split('\n').filter(Boolean);

  const changedPackages = [];
  for (const pkg of PUBLISHABLE_PACKAGES) {
    if (changedFiles.some(f => f.startsWith(pkg.dir + '/') || f === 'package.json' || f.startsWith('.changeset/'))) {
      const pkgJsonPath = path.join(pkg.dir, 'package.json');
      if (fs.existsSync(pkgJsonPath)) {
        const pkgData = JSON.parse(fs.readFileSync(pkgJsonPath, 'utf8'));
        if (pkgData.name) changedPackages.push(pkgData.name);
      }
    }
  }

  console.log(JSON.stringify(changedPackages));
} catch (error) {
  console.error("Error detecting changed packages:", error);
  process.exit(1);
}
